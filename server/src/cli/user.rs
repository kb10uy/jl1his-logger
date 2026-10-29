use clap::{Subcommand, ValueEnum};

use crate::{
    cli::{CliResult, Context, admin_tx, find_user, setup::LinkArgs},
    store::{
        access::{self, Principal},
        account::{
            registration,
            user::{self, Role, User},
        },
    },
};

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

pub async fn user(context: &Context<'_>, command: UserCommand) -> CliResult<()> {
    let principal = context.principal().await?;
    match command {
        UserCommand::List => {
            let mut conn = context.db.acquire().await?;
            let admin = access::admin(&mut conn, &principal).await?;
            user::list(&mut conn, &admin)
                .await?
                .iter()
                .for_each(|account| print_user(&account.user));
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
