use std::collections::BTreeMap;

use chrono::{NaiveDate, TimeDelta};
use clap::{Args, Subcommand, ValueEnum};
use url::Url;

use crate::{
    cli::{CliResult, Context, days, parse_day, pick, registration_url},
    config::parse_public_origin,
    db,
    store::{
        WriteTx,
        access::{self, LogbookRole},
        logbook, membership, registration,
        station::{self, StationInput},
        token::{self, TokenInput, TokenKind},
        user::{self, Role, User},
    },
};

#[derive(Args)]
pub struct SetupArgs {
    #[arg(long, help = "Personal callsign of the first admin")]
    callsign: String,
    #[command(flatten)]
    link: LinkArgs,
}

#[derive(Args)]
pub struct LinkArgs {
    #[arg(
        long,
        env = "PUBLIC_ORIGIN",
        value_parser = parse_public_origin,
        help = "URL the app is reached at; the registration link points there"
    )]
    public_origin: Url,
    #[arg(
        long,
        default_value_t = 24,
        value_parser = clap::value_parser!(u32).range(1..),
        help = "Hours the registration link stays valid"
    )]
    ttl_hours: u32,
}

impl LinkArgs {
    fn print(&self, user: &User, token: &str) {
        println!(
            "Open this link within {} hours to register a passkey for {}:",
            self.ttl_hours, user.callsign
        );
        println!("{}", registration_url(&self.public_origin, token));
    }

    fn ttl(&self) -> TimeDelta {
        TimeDelta::hours(i64::from(self.ttl_hours))
    }
}

pub async fn setup(context: &Context<'_>, args: &SetupArgs) -> CliResult<()> {
    let mut tx = WriteTx::begin(context.db).await?;
    let user = match user::find_by_callsign(&mut tx, &args.callsign).await? {
        Some(user) => user,
        None => user::create_initial_admin(&mut tx, &args.callsign).await?,
    };
    let token = registration::issue_for_setup(&mut tx, &user.id, args.link.ttl()).await?;
    tx.commit().await?;
    args.link.print(&user, &token);
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum RoleArg {
    Admin,
    User,
}

impl From<RoleArg> for Role {
    fn from(arg: RoleArg) -> Role {
        match arg {
            RoleArg::Admin => Role::Admin,
            RoleArg::User => Role::User,
        }
    }
}

#[derive(Subcommand)]
pub enum UserCommand {
    #[command(about = "Adds a user; register their passkey with `user invite`")]
    Add {
        callsign: String,
        #[arg(long, help = "Make the user an admin")]
        admin: bool,
    },
    #[command(about = "Lists the users")]
    List,
    #[command(about = "Prints a link for the user to register a passkey")]
    Invite {
        callsign: String,
        #[command(flatten)]
        link: LinkArgs,
    },
    #[command(about = "Disables a user, deleting their sessions and API tokens")]
    Disable { callsign: String },
    #[command(about = "Enables a disabled user")]
    Enable { callsign: String },
    #[command(about = "Changes the instance role of a user")]
    Role {
        callsign: String,
        #[arg(value_enum)]
        role: RoleArg,
    },
}

async fn find_user(conn: &mut sqlx::SqliteConnection, callsign: &str) -> CliResult<User> {
    Ok(user::find_by_callsign(conn, callsign)
        .await?
        .ok_or_else(|| format!("no user has the callsign {callsign}"))?)
}

fn print_user(user: &User) {
    let state = match &user.disabled_at {
        Some(at) => format!("disabled {at}"),
        None => "active".to_owned(),
    };
    println!("{}\t{}\t{state}", user.callsign, user.role.as_str());
}

pub async fn user(context: &Context<'_>, command: UserCommand) -> CliResult<()> {
    let principal = context.principal().await?;
    if let UserCommand::List = command {
        let mut conn = context.db.acquire().await?;
        let admin = access::admin(&mut conn, &principal).await?;
        user::list(&mut conn, &admin).await?.iter().for_each(print_user);
        return Ok(());
    }
    let mut tx = WriteTx::begin(context.db).await?;
    let admin = access::admin(&mut tx, &principal).await?;
    match command {
        UserCommand::List => unreachable!("listed above"),
        UserCommand::Add {
            callsign,
            admin: is_admin,
        } => {
            let role = if is_admin { Role::Admin } else { Role::User };
            let added = user::create(&mut tx, &admin, &callsign, role).await?;
            tx.commit().await?;
            print_user(&added);
        }
        UserCommand::Invite { callsign, link } => {
            let invited = find_user(&mut tx, &callsign).await?;
            let token = registration::issue(&mut tx, &admin, &invited.id, link.ttl()).await?;
            tx.commit().await?;
            link.print(&invited, &token);
        }
        UserCommand::Disable { callsign: ref key } | UserCommand::Enable { callsign: ref key } => {
            let disabled = matches!(command, UserCommand::Disable { .. });
            let target = find_user(&mut tx, key).await?;
            let changed = user::set_disabled(&mut tx, &admin, &target.id, disabled).await?;
            tx.commit().await?;
            print_user(&changed);
        }
        UserCommand::Role { callsign, role } => {
            let target = find_user(&mut tx, &callsign).await?;
            let changed = user::set_role(&mut tx, &admin, &target.id, role.into()).await?;
            tx.commit().await?;
            print_user(&changed);
        }
    }
    Ok(())
}

#[derive(Subcommand)]
pub enum LogbookCommand {
    #[command(about = "Creates a Logbook with its first owner (admin)")]
    Add {
        name: String,
        #[arg(long, help = "Callsign of the owner")]
        owner: String,
    },
    #[command(about = "Lists your Logbooks, or every Logbook for an admin")]
    List,
    #[command(about = "Deletes a Logbook and everything in it after a snapshot (admin)")]
    Delete {
        logbook: String,
        #[arg(long, help = "Confirm the deletion")]
        yes: bool,
    },
    #[command(about = "Makes a user an owner of a Logbook as a recorded rescue (admin)")]
    AddOwner {
        logbook: String,
        callsign: String,
        #[arg(long, help = "Why the rescue is needed; recorded with it")]
        reason: String,
    },
}

pub async fn logbook(context: &Context<'_>, command: LogbookCommand) -> CliResult<()> {
    let principal = context.principal().await?;
    let mut conn = context.db.acquire().await?;
    let admin = access::admin(&mut conn, &principal).await;
    match command {
        LogbookCommand::List => match admin {
            Ok(admin) => {
                for logbook in logbook::list_all(&mut conn, &admin).await? {
                    println!("{}\t{}\t{} QSOs inserted", logbook.id, logbook.name, logbook.last_seq);
                }
            }
            Err(_) => {
                let user = access::user(&mut conn, &principal).await?;
                for (logbook, role) in logbook::list(&mut conn, &user).await? {
                    println!("{}\t{}\t{}", logbook.id, logbook.name, role.as_str());
                }
            }
        },
        LogbookCommand::Add { name, owner } => {
            drop(conn);
            let mut tx = WriteTx::begin(context.db).await?;
            let admin = access::admin(&mut tx, &principal).await?;
            let owner = find_user(&mut tx, &owner).await?;
            let created = logbook::create(&mut tx, &admin, &name, &owner.id).await?;
            tx.commit().await?;
            println!("{}\t{}", created.id, created.name);
        }
        LogbookCommand::Delete { logbook: key, yes } => {
            let admin = admin?;
            let logbooks = logbook::list_all(&mut conn, &admin).await?;
            let target = pick(&logbooks, Some(&key), "Logbook", |l| &l.id, |l| &l.name)?.clone();
            drop(conn);
            if !yes {
                return Err(format!(
                    "this deletes {} with its {} QSOs, Stations and Memberships; add --yes to confirm",
                    target.name, target.last_seq
                )
                .into());
            }
            let snapshot = db::snapshot(context.db, &db::snapshot_dir(context.database), "before-delete").await?;
            println!("Snapshot written to {}", snapshot.display());
            let mut tx = WriteTx::begin(context.db).await?;
            logbook::delete(&mut tx, &admin, &target.id).await?;
            tx.commit().await?;
            println!("Deleted {}", target.name);
        }
        LogbookCommand::AddOwner {
            logbook: key,
            callsign,
            reason,
        } => {
            let admin = admin?;
            let logbooks = logbook::list_all(&mut conn, &admin).await?;
            let target = pick(&logbooks, Some(&key), "Logbook", |l| &l.id, |l| &l.name)?.clone();
            drop(conn);
            let mut tx = WriteTx::begin(context.db).await?;
            let owner = find_user(&mut tx, &callsign).await?;
            logbook::add_owner(&mut tx, &admin, &target.id, &owner.id, &reason).await?;
            tx.commit().await?;
            println!("{} is now an owner of {}", owner.callsign, target.name);
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum MemberRoleArg {
    Owner,
    Operator,
}

impl From<MemberRoleArg> for LogbookRole {
    fn from(arg: MemberRoleArg) -> LogbookRole {
        match arg {
            MemberRoleArg::Owner => LogbookRole::Owner,
            MemberRoleArg::Operator => LogbookRole::Operator,
        }
    }
}

#[derive(Subcommand)]
pub enum MemberCommand {
    #[command(about = "Adds a member or changes their role")]
    Set {
        callsign: String,
        #[arg(value_enum)]
        role: MemberRoleArg,
        #[arg(long, help = "Logbook ID or name; may be left out if you have only one")]
        logbook: Option<String>,
    },
    #[command(about = "Removes a member")]
    Remove {
        callsign: String,
        #[arg(long, help = "Logbook ID or name; may be left out if you have only one")]
        logbook: Option<String>,
    },
}

pub async fn member(context: &Context<'_>, command: MemberCommand) -> CliResult<()> {
    let (MemberCommand::Set { logbook, callsign, .. } | MemberCommand::Remove { logbook, callsign }) = &command;
    let access = context.logbook_access(logbook.as_deref()).await?;
    let mut tx = WriteTx::begin(context.db).await?;
    let member = find_user(&mut tx, callsign).await?;
    let owner = access.owner()?;
    match command {
        MemberCommand::Set { role, .. } => {
            membership::set(&mut tx, &owner, &member.id, role.into()).await?;
            tx.commit().await?;
            println!(
                "{} is now {} of the Logbook",
                member.callsign,
                LogbookRole::from(role).as_str()
            );
        }
        MemberCommand::Remove { .. } => {
            membership::remove(&mut tx, &owner, &member.id).await?;
            tx.commit().await?;
            println!("Removed {} from the Logbook", member.callsign);
        }
    }
    Ok(())
}

fn parse_field(s: &str) -> Result<(String, String), String> {
    s.split_once('=')
        .map(|(name, value)| (name.trim().to_owned(), value.to_owned()))
        .filter(|(name, _)| !name.is_empty())
        .ok_or_else(|| format!("{s:?} is not NAME=VALUE"))
}

#[derive(Args)]
pub struct StationFields {
    #[arg(long, help = "Old Wavelog station ID kept for /wl2 clients")]
    wavelog_id: Option<i64>,
    #[arg(
        long = "field",
        value_name = "NAME=VALUE",
        value_parser = parse_field,
        help = "A MY_* default baked into new QSOs; repeat it, and give an empty value to remove one"
    )]
    fields: Vec<(String, String)>,
}

#[derive(Subcommand)]
pub enum StationCommand {
    #[command(about = "Adds a Station")]
    Add {
        name: String,
        #[arg(long, help = "STATION_CALLSIGN baked into new QSOs")]
        callsign: String,
        #[command(flatten)]
        fields: StationFields,
        #[arg(long, help = "Logbook ID or name; may be left out if you have only one")]
        logbook: Option<String>,
    },
    #[command(about = "Lists the Stations")]
    List {
        #[arg(long, help = "Logbook ID or name; may be left out if you have only one")]
        logbook: Option<String>,
    },
    #[command(about = "Changes a Station; past QSOs keep their values until `station rewrite`")]
    Update {
        station: String,
        #[arg(long, help = "New name")]
        name: Option<String>,
        #[arg(long, help = "New STATION_CALLSIGN")]
        callsign: Option<String>,
        #[command(flatten)]
        fields: StationFields,
        #[arg(long, help = "Logbook ID or name; may be left out if you have only one")]
        logbook: Option<String>,
    },
    #[command(about = "Archives a Station")]
    Archive {
        station: String,
        #[arg(long, help = "Logbook ID or name; may be left out if you have only one")]
        logbook: Option<String>,
    },
    #[command(about = "Brings an archived Station back")]
    Unarchive {
        station: String,
        #[arg(long, help = "Logbook ID or name; may be left out if you have only one")]
        logbook: Option<String>,
    },
    #[command(about = "Rewrites the past QSOs of a Station with its current values")]
    Rewrite {
        station: String,
        #[arg(long, value_parser = parse_day, help = "First QSO day (UTC), such as 2026-10-01")]
        from: Option<NaiveDate>,
        #[arg(long, value_parser = parse_day, help = "Last QSO day (UTC), inclusive")]
        until: Option<NaiveDate>,
        #[arg(long, help = "The number of QSOs the preview reported; without it only previews")]
        expect: Option<usize>,
        #[arg(long, help = "Logbook ID or name; may be left out if you have only one")]
        logbook: Option<String>,
    },
}

fn print_station(station: &station::Station) {
    let wavelog = station.wavelog_id.map(|id| format!("wavelog {id}")).unwrap_or_default();
    let archived = station.archived_at.as_ref().map(|_| "archived").unwrap_or("");
    println!(
        "{}\t{}\t{}\t{wavelog}\t{archived}",
        station.id, station.name, station.callsign
    );
    for (name, value) in &station.fields {
        println!("\t{name}={value}");
    }
}

fn apply_fields(input: &mut StationInput, fields: &StationFields) {
    if let Some(id) = fields.wavelog_id {
        input.wavelog_id = Some(id);
    }
    for (name, value) in &fields.fields {
        let name = name.to_ascii_uppercase();
        match value.is_empty() {
            true => input.fields.remove(&name),
            false => input.fields.insert(name, value.clone()),
        };
    }
}

pub async fn station(context: &Context<'_>, command: StationCommand) -> CliResult<()> {
    let logbook = match &command {
        StationCommand::Add { logbook, .. }
        | StationCommand::List { logbook }
        | StationCommand::Update { logbook, .. }
        | StationCommand::Archive { logbook, .. }
        | StationCommand::Unarchive { logbook, .. }
        | StationCommand::Rewrite { logbook, .. } => logbook.clone(),
    };
    let access = context.logbook_access(logbook.as_deref()).await?;
    match command {
        StationCommand::List { .. } => {
            let mut conn = context.db.acquire().await?;
            station::list(&mut conn, &access).await?.iter().for_each(print_station);
        }
        StationCommand::Add {
            name, callsign, fields, ..
        } => {
            let mut input = StationInput {
                name,
                callsign,
                wavelog_id: None,
                fields: BTreeMap::new(),
            };
            apply_fields(&mut input, &fields);
            let mut tx = WriteTx::begin(context.db).await?;
            let created = station::create(&mut tx, &access.owner()?, &input).await?;
            tx.commit().await?;
            print_station(&created);
        }
        StationCommand::Update {
            station: key,
            name,
            callsign,
            fields,
            ..
        } => {
            let current = context.station(&access, Some(&key)).await?;
            let mut input = StationInput {
                name: name.unwrap_or(current.name),
                callsign: callsign.unwrap_or(current.callsign),
                wavelog_id: current.wavelog_id,
                fields: current.fields,
            };
            apply_fields(&mut input, &fields);
            let mut tx = WriteTx::begin(context.db).await?;
            let updated = station::update(&mut tx, &access.owner()?, &current.id, &input).await?;
            tx.commit().await?;
            print_station(&updated);
        }
        StationCommand::Archive { station: ref key, .. } | StationCommand::Unarchive { station: ref key, .. } => {
            let archived = matches!(command, StationCommand::Archive { .. });
            let target = context.station(&access, Some(key)).await?;
            let mut tx = WriteTx::begin(context.db).await?;
            let changed = station::set_archived(&mut tx, &access.owner()?, &target.id, archived).await?;
            tx.commit().await?;
            print_station(&changed);
        }
        StationCommand::Rewrite {
            station: key,
            from,
            until,
            expect,
            ..
        } => {
            let target = context.station(&access, Some(&key)).await?;
            let range = days(from, until);
            let owner = access.owner()?;
            match expect {
                None => {
                    let mut conn = context.db.acquire().await?;
                    let count = station::preview_rewrite(&mut conn, &owner, &target.id, range).await?;
                    println!(
                        "{count} QSOs of {} would change; rerun with --expect {count}",
                        target.name
                    );
                }
                Some(expected) => {
                    let mut tx = WriteTx::begin(context.db).await?;
                    let count = station::rewrite_qsos(&mut tx, &owner, &target.id, range, expected).await?;
                    tx.commit().await?;
                    println!("Rewrote {count} QSOs of {}", target.name);
                }
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum TokenKindArg {
    Native,
    Wl2,
}

impl From<TokenKindArg> for TokenKind {
    fn from(arg: TokenKindArg) -> TokenKind {
        match arg {
            TokenKindArg::Native => TokenKind::Native,
            TokenKindArg::Wl2 => TokenKind::Wl2,
        }
    }
}

#[derive(Subcommand)]
pub enum TokenCommand {
    #[command(about = "Issues an API token and prints it once")]
    Issue {
        name: String,
        #[arg(long, value_enum, default_value_t = TokenKindArg::Native, help = "The API the token opens")]
        kind: TokenKindArg,
        #[arg(long, help = "Prefix a wl2 token with wl2_, as grayline expects")]
        wl2_prefix: bool,
        #[arg(
            long = "logbook",
            required = true,
            help = "Logbook ID or name in the token's scope; repeat it"
        )]
        logbooks: Vec<String>,
    },
    #[command(about = "Lists your API tokens, or another user's for an admin")]
    List { callsign: Option<String> },
    #[command(about = "Revokes an API token")]
    Revoke { id: String },
}

pub async fn token(context: &Context<'_>, command: TokenCommand) -> CliResult<()> {
    let principal = context.principal().await?;
    match command {
        TokenCommand::Issue {
            name,
            kind,
            wl2_prefix,
            logbooks,
        } => {
            let mut logbook_ids = vec![];
            for key in &logbooks {
                logbook_ids.push(context.logbook_access(Some(key)).await?.logbook_id().to_owned());
            }
            let mut tx = WriteTx::begin(context.db).await?;
            let user = access::user(&mut tx, &principal).await?;
            let input = TokenInput {
                kind: kind.into(),
                name,
                logbook_ids,
                wl2_prefix,
            };
            let issued = token::issue(&mut tx, &user, &input).await?;
            tx.commit().await?;
            println!(
                "Token {} ({}) issued; it is shown only once:",
                issued.token.name, issued.token.id
            );
            println!("{}", issued.plaintext);
        }
        TokenCommand::List { callsign } => {
            let mut conn = context.db.acquire().await?;
            let user = access::user(&mut conn, &principal).await?;
            let user_id = match callsign {
                Some(callsign) => find_user(&mut conn, &callsign).await?.id,
                None => user.user_id().to_owned(),
            };
            for token in token::list(&mut conn, &user, &user_id).await? {
                println!(
                    "{}\t{}\t{}\t{}\tlast used {}",
                    token.id,
                    token.name,
                    token.kind.as_str(),
                    token.logbook_ids.join(","),
                    token.last_used_at.as_deref().unwrap_or("never")
                );
            }
        }
        TokenCommand::Revoke { id } => {
            let mut tx = WriteTx::begin(context.db).await?;
            let user = access::user(&mut tx, &principal).await?;
            token::revoke(&mut tx, &user, &id).await?;
            tx.commit().await?;
            println!("Revoked {id}");
        }
    }
    Ok(())
}
