use clap::Subcommand;

use crate::{
    cli::{CliResult, Context, find_user, pick_logbook},
    store::{
        WriteTx, access,
        account::token::{self, TokenInput},
    },
};

#[derive(Subcommand)]
pub enum TokenCommand {
    #[command(about = "Issues an API token and prints it once")]
    Issue {
        name: String,
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
        TokenCommand::Issue { name, logbooks: keys } => {
            let (principal, logbooks) = context.logbooks().await?;
            let logbook_ids = keys
                .iter()
                .map(|key| Ok(pick_logbook(&logbooks, Some(key))?.id.clone()))
                .collect::<CliResult<_>>()?;
            let mut tx = WriteTx::begin(context.db).await?;
            let user = access::user(&mut tx, &principal).await?;
            let input = TokenInput { name, logbook_ids };
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
                    "{}\t{}\t{}\tlast used {}",
                    token.id,
                    token.name,
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
