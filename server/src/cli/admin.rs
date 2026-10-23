use std::collections::BTreeMap;

use chrono::TimeDelta;
use clap::{Args, Subcommand, ValueEnum};
use url::Url;

use crate::{
    cli::{CliResult, Context, DayRange, LogbookArg, find_user, pick_logbook, registration_url},
    db,
    store::{
        StoreError, WriteTx,
        access::{self, AdminAccess, LogbookAccess, LogbookRole, Principal},
        logbook::{self, Logbook},
        membership, registration, settings,
        station::{self, Station, StationInput},
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
        default_value_t = 24,
        value_parser = clap::value_parser!(u32).range(1..),
        help = "Hours the registration link stays valid"
    )]
    ttl_hours: u32,
}

impl LinkArgs {
    fn print(&self, origin: &Url, user: &User, token: &str) {
        println!(
            "Open this link within {} hours to register a passkey for {}:",
            self.ttl_hours, user.callsign
        );
        println!("{}", registration_url(origin, token));
    }

    fn ttl(&self) -> TimeDelta {
        TimeDelta::hours(i64::from(self.ttl_hours))
    }
}

pub async fn setup(context: &Context<'_>, args: &SetupArgs) -> CliResult<()> {
    let origin = context.public_origin()?;
    let mut tx = WriteTx::begin(context.db).await?;
    let user = match user::find_by_callsign(&mut tx, &args.callsign).await? {
        Some(user) => user,
        None => user::create_initial_admin(&mut tx, &args.callsign).await?,
    };
    let token = registration::issue_for_setup(&mut tx, &user.id, args.link.ttl()).await?;
    tx.commit().await?;
    args.link.print(origin, &user, &token);
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

fn print_user(user: &User) {
    let state = match &user.disabled_at {
        Some(at) => format!("disabled {at}"),
        None => "active".to_owned(),
    };
    println!("{}\t{}\t{state}", user.callsign, user.role.as_str());
}

async fn admin_tx(context: &Context<'_>, principal: &Principal) -> CliResult<(WriteTx, AdminAccess)> {
    let mut tx = WriteTx::begin(context.db).await?;
    let admin = access::admin(&mut tx, principal).await?;
    Ok((tx, admin))
}

pub async fn user(context: &Context<'_>, command: UserCommand) -> CliResult<()> {
    let principal = context.principal().await?;
    match command {
        UserCommand::List => {
            let mut conn = context.db.acquire().await?;
            let admin = access::admin(&mut conn, &principal).await?;
            user::list(&mut conn, &admin).await?.iter().for_each(print_user);
        }
        UserCommand::Add {
            callsign,
            admin: is_admin,
        } => {
            let (mut tx, admin) = admin_tx(context, &principal).await?;
            let role = if is_admin { Role::Admin } else { Role::User };
            let added = user::create(&mut tx, &admin, &callsign, role).await?;
            tx.commit().await?;
            print_user(&added);
        }
        UserCommand::Invite { callsign, link } => {
            let origin = context.public_origin()?;
            let (mut tx, admin) = admin_tx(context, &principal).await?;
            let invited = find_user(&mut tx, &callsign).await?;
            let token = registration::issue(&mut tx, &admin, &invited.id, link.ttl()).await?;
            tx.commit().await?;
            link.print(origin, &invited, &token);
        }
        UserCommand::Disable { callsign } => set_disabled(context, &principal, &callsign, true).await?,
        UserCommand::Enable { callsign } => set_disabled(context, &principal, &callsign, false).await?,
        UserCommand::Role { callsign, role } => {
            let (mut tx, admin) = admin_tx(context, &principal).await?;
            let target = find_user(&mut tx, &callsign).await?;
            let changed = user::set_role(&mut tx, &admin, &target.id, role.into()).await?;
            tx.commit().await?;
            print_user(&changed);
        }
    }
    Ok(())
}

async fn set_disabled(context: &Context<'_>, principal: &Principal, callsign: &str, disabled: bool) -> CliResult<()> {
    let (mut tx, admin) = admin_tx(context, principal).await?;
    let target = find_user(&mut tx, callsign).await?;
    let changed = user::set_disabled(&mut tx, &admin, &target.id, disabled).await?;
    tx.commit().await?;
    print_user(&changed);
    Ok(())
}

#[derive(Subcommand)]
pub enum SettingsCommand {
    #[command(about = "Lists the instance settings")]
    List,
    #[command(about = "Prints the value of an instance setting")]
    Get { key: String },
    #[command(about = "Sets an instance setting")]
    Set { key: String, value: String },
}

pub async fn settings(context: &Context<'_>, command: SettingsCommand) -> CliResult<()> {
    let principal = context.principal().await?;
    match command {
        SettingsCommand::List => {
            let mut conn = context.db.acquire().await?;
            let admin = access::admin(&mut conn, &principal).await?;
            for setting in settings::list(&mut conn, &admin).await? {
                println!("{}\t{}\t{}", setting.key, setting.value, setting.updated_at);
            }
        }
        SettingsCommand::Get { key } => {
            let mut conn = context.db.acquire().await?;
            access::admin(&mut conn, &principal).await?;
            let value = settings::get(&mut conn, key.trim())
                .await?
                .ok_or_else(|| format!("the setting {key:?} is not set"))?;
            println!("{value}");
        }
        SettingsCommand::Set { key, value } => {
            let (mut tx, admin) = admin_tx(context, &principal).await?;
            settings::set(&mut tx, &admin, &key, &value).await?;
            tx.commit().await?;
            println!("{} = {value}", key.trim());
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

async fn any_logbook(context: &Context<'_>, principal: &Principal, key: &str) -> CliResult<(AdminAccess, Logbook)> {
    let mut conn = context.db.acquire().await?;
    let admin = access::admin(&mut conn, principal).await?;
    let logbooks = logbook::list_all(&mut conn, &admin).await?;
    let found = pick_logbook(&logbooks, Some(key))?.clone();
    Ok((admin, found))
}

pub async fn logbook(context: &Context<'_>, command: LogbookCommand) -> CliResult<()> {
    let principal = context.principal().await?;
    match command {
        LogbookCommand::List => {
            let mut conn = context.db.acquire().await?;
            match access::admin(&mut conn, &principal).await {
                Ok(admin) => {
                    for logbook in logbook::list_all(&mut conn, &admin).await? {
                        println!("{}\t{}\t{} QSOs inserted", logbook.id, logbook.name, logbook.last_seq);
                    }
                }
                Err(StoreError::Forbidden(_)) => {
                    let user = access::user(&mut conn, &principal).await?;
                    for (logbook, role) in logbook::list(&mut conn, &user).await? {
                        println!("{}\t{}\t{}", logbook.id, logbook.name, role.as_str());
                    }
                }
                Err(e) => return Err(e.into()),
            }
        }
        LogbookCommand::Add { name, owner } => {
            let (mut tx, admin) = admin_tx(context, &principal).await?;
            let owner = find_user(&mut tx, &owner).await?;
            let created = logbook::create(&mut tx, &admin, &name, &owner.id).await?;
            tx.commit().await?;
            println!("{}\t{}", created.id, created.name);
        }
        LogbookCommand::Delete { logbook: key, yes } => {
            let (admin, target) = any_logbook(context, &principal, &key).await?;
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
            let (admin, target) = any_logbook(context, &principal, &key).await?;
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
    #[command(about = "Lists the members of a Logbook")]
    List {
        #[command(flatten)]
        logbook: LogbookArg,
    },
    #[command(about = "Adds a member or changes their role")]
    Set {
        callsign: String,
        #[arg(value_enum)]
        role: MemberRoleArg,
        #[command(flatten)]
        logbook: LogbookArg,
    },
    #[command(about = "Removes a member")]
    Remove {
        callsign: String,
        #[command(flatten)]
        logbook: LogbookArg,
    },
}

pub async fn member(context: &Context<'_>, command: MemberCommand) -> CliResult<()> {
    match command {
        MemberCommand::List { logbook } => {
            let access = context.logbook_access(&logbook).await?;
            let mut conn = context.db.acquire().await?;
            for member in membership::list(&mut conn, &access).await? {
                let state = if member.disabled { "disabled" } else { "active" };
                println!("{}\t{}\t{state}", member.callsign, member.role.as_str());
            }
        }
        MemberCommand::Set {
            callsign,
            role,
            logbook,
        } => {
            let access = context.logbook_access(&logbook).await?;
            let role = LogbookRole::from(role);
            let mut tx = WriteTx::begin(context.db).await?;
            let member = find_user(&mut tx, &callsign).await?;
            membership::set(&mut tx, &access.owner()?, &member.id, role).await?;
            tx.commit().await?;
            println!("{} is now {} of the Logbook", member.callsign, role.as_str());
        }
        MemberCommand::Remove { callsign, logbook } => {
            let access = context.logbook_access(&logbook).await?;
            let mut tx = WriteTx::begin(context.db).await?;
            let member = find_user(&mut tx, &callsign).await?;
            membership::remove(&mut tx, &access.owner()?, &member.id).await?;
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

impl StationFields {
    fn apply(&self, input: &mut StationInput) {
        if let Some(id) = self.wavelog_id {
            input.wavelog_id = Some(id);
        }
        for (name, value) in &self.fields {
            let name = name.to_ascii_uppercase();
            match value.is_empty() {
                true => input.fields.remove(&name),
                false => input.fields.insert(name, value.clone()),
            };
        }
    }
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
        #[command(flatten)]
        logbook: LogbookArg,
    },
    #[command(about = "Lists the Stations")]
    List {
        #[command(flatten)]
        logbook: LogbookArg,
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
        #[command(flatten)]
        logbook: LogbookArg,
    },
    #[command(about = "Archives a Station")]
    Archive {
        station: String,
        #[command(flatten)]
        logbook: LogbookArg,
    },
    #[command(about = "Brings an archived Station back")]
    Unarchive {
        station: String,
        #[command(flatten)]
        logbook: LogbookArg,
    },
    #[command(about = "Rewrites the past QSOs of a Station with its current values")]
    Rewrite {
        station: String,
        #[command(flatten)]
        days: DayRange,
        #[arg(long, help = "The number of QSOs the preview reported; without it only previews")]
        expect: Option<usize>,
        #[command(flatten)]
        logbook: LogbookArg,
    },
}

impl StationCommand {
    fn logbook(&self) -> &LogbookArg {
        match self {
            StationCommand::Add { logbook, .. }
            | StationCommand::List { logbook }
            | StationCommand::Update { logbook, .. }
            | StationCommand::Archive { logbook, .. }
            | StationCommand::Unarchive { logbook, .. }
            | StationCommand::Rewrite { logbook, .. } => logbook,
        }
    }
}

fn print_station(station: &Station) {
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

pub async fn station(context: &Context<'_>, command: StationCommand) -> CliResult<()> {
    let access = context.logbook_access(command.logbook()).await?;
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
            fields.apply(&mut input);
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
            fields.apply(&mut input);
            let mut tx = WriteTx::begin(context.db).await?;
            let updated = station::update(&mut tx, &access.owner()?, &current.id, &input).await?;
            tx.commit().await?;
            print_station(&updated);
        }
        StationCommand::Archive { station: key, .. } => set_archived(context, &access, &key, true).await?,
        StationCommand::Unarchive { station: key, .. } => set_archived(context, &access, &key, false).await?,
        StationCommand::Rewrite {
            station: key,
            days,
            expect,
            ..
        } => {
            let target = context.station(&access, Some(&key)).await?;
            let owner = access.owner()?;
            match expect {
                None => {
                    let mut conn = context.db.acquire().await?;
                    let count = station::preview_rewrite(&mut conn, &owner, &target.id, days.range()).await?;
                    println!(
                        "{count} QSOs of {} would change; rerun with --expect {count}",
                        target.name
                    );
                }
                Some(expected) => {
                    let mut tx = WriteTx::begin(context.db).await?;
                    let count = station::rewrite_qsos(&mut tx, &owner, &target.id, days.range(), expected).await?;
                    tx.commit().await?;
                    println!("Rewrote {count} QSOs of {}", target.name);
                }
            }
        }
    }
    Ok(())
}

async fn set_archived(context: &Context<'_>, access: &LogbookAccess, key: &str, archived: bool) -> CliResult<()> {
    let target = context.station(access, Some(key)).await?;
    let mut tx = WriteTx::begin(context.db).await?;
    let changed = station::set_archived(&mut tx, &access.owner()?, &target.id, archived).await?;
    tx.commit().await?;
    print_station(&changed);
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
        #[arg(long, help = "Prefix a wl2 token with wl2_")]
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
    match command {
        TokenCommand::Issue {
            name,
            kind,
            wl2_prefix,
            logbooks: keys,
        } => {
            let (principal, logbooks) = context.logbooks().await?;
            let logbook_ids = keys
                .iter()
                .map(|key| Ok(pick_logbook(&logbooks, Some(key))?.id.clone()))
                .collect::<CliResult<_>>()?;
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
            let principal = context.principal().await?;
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
            let principal = context.principal().await?;
            let mut tx = WriteTx::begin(context.db).await?;
            let user = access::user(&mut tx, &principal).await?;
            token::revoke(&mut tx, &user, &id).await?;
            tx.commit().await?;
            println!("Revoked {id}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case("MY_CITY=Minato", Ok(("MY_CITY", "Minato")))]
    #[case(" my_rig =IC-705=x", Ok(("my_rig", "IC-705=x")))]
    #[case("MY_RIG=", Ok(("MY_RIG", "")))]
    #[case("=Minato", Err(()))]
    #[case("MY_CITY", Err(()))]
    fn reads_name_value_fields(#[case] text: &str, #[case] expected: Result<(&str, &str), ()>) {
        let read = parse_field(text);
        assert_eq!(
            read.as_ref().map(|(n, v)| (n.as_str(), v.as_str())).map_err(|_| ()),
            expected
        );
    }

    #[test]
    fn applies_station_fields_onto_an_input() {
        let mut input = StationInput {
            fields: BTreeMap::from([("MY_CITY".to_owned(), "Minato".to_owned())]),
            ..StationInput::default()
        };
        let fields = StationFields {
            wavelog_id: Some(3),
            fields: vec![
                ("my_city".to_owned(), String::new()),
                ("my_rig".to_owned(), "IC-705".to_owned()),
            ],
        };
        fields.apply(&mut input);
        assert_eq!(input.wavelog_id, Some(3));
        assert_eq!(
            input.fields,
            BTreeMap::from([("MY_RIG".to_owned(), "IC-705".to_owned())])
        );
    }
}
