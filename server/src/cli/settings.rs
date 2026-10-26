use clap::Subcommand;

use crate::{
    cli::{CliResult, Context, admin_tx},
    store::{access, settings},
};

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
