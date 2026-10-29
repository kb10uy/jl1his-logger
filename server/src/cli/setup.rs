use chrono::TimeDelta;
use clap::Args;
use url::Url;

use crate::{
    cli::{CliResult, Context, registration_url},
    store::{
        WriteTx,
        account::{
            registration,
            user::{self, User},
        },
    },
};

#[derive(Args)]
pub struct SetupArgs {
    #[arg(
        long,
        help = "Personal callsign of the first admin, or of an admin who lost their passkeys"
    )]
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
    pub(super) fn print(&self, origin: &Url, user: &User, token: &str) {
        println!(
            "Open this link within {} hours to register a passkey for {}:",
            self.ttl_hours, user.callsign
        );
        println!("{}", registration_url(origin, token));
    }

    pub(super) fn ttl(&self) -> TimeDelta {
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
