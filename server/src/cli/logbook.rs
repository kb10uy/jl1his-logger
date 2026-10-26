use clap::{Subcommand, ValueEnum};

use crate::{
    cli::{CliResult, Context, LogbookArg, admin_tx, find_user, pick_logbook},
    db,
    store::{
        StoreError, WriteTx,
        access::{self, AdminAccess, LogbookRole, Principal},
        logbook::{self, Logbook, membership},
    },
};

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
