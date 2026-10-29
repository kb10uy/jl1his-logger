mod backup;
mod setup;

use std::{error::Error, path::Path};

use url::Url;

use crate::{
    config::Config,
    db::{self, Db, Migrated},
};

pub type CliResult<T> = Result<T, Box<dyn Error>>;

struct Context<'a> {
    db: &'a Db,
    database: &'a Path,
    config: &'a Config,
}

#[derive(clap::Subcommand)]
pub enum Command {
    #[command(about = "Applies pending migrations after snapshotting the database")]
    Migrate,
    #[command(about = "Creates the first admin, or lets an admin register a passkey again, and prints the link")]
    Setup(setup::SetupArgs),
    #[command(about = "Writes a copy of the database while the server runs")]
    Backup(backup::BackupArgs),
    #[command(about = "Replaces the database with a backup; the server must be stopped")]
    Restore(backup::RestoreArgs),
}

pub async fn run(database: &Path, config: &Config, command: Command) -> CliResult<()> {
    if let Command::Restore(args) = &command {
        return backup::restore(database, args).await;
    }
    let db = db::open(database).await?;
    let result = async {
        if !matches!(command, Command::Migrate | Command::Setup(_)) {
            db::require_migrated(&db).await?;
        }
        let context = Context {
            db: &db,
            database,
            config,
        };
        match command {
            Command::Migrate => {
                let migrated = context.migrate().await?;
                match migrated.applied.is_empty() {
                    true => println!("The database is up to date."),
                    false => println!("Applied migrations {:?}", migrated.applied),
                }
                Ok(())
            }
            Command::Setup(args) => {
                context.migrate().await?;
                setup::setup(&context, &args).await
            }
            Command::Backup(args) => backup::backup(&context, &args).await,
            Command::Restore(_) => unreachable!("restore runs without an open database"),
        }
    }
    .await;
    db.close().await;
    result
}

impl Context<'_> {
    async fn migrate(&self) -> CliResult<Migrated> {
        let migrated = db::migrate(self.db, &db::snapshot_dir(self.database)).await?;
        if let Some(snapshot) = &migrated.snapshot {
            println!("Snapshot written to {}", snapshot.display());
        }
        Ok(migrated)
    }

    fn public_origin(&self) -> CliResult<&Url> {
        Ok(self
            .config
            .server
            .public_origin
            .as_ref()
            .ok_or("set server.public_origin in the configuration; the registration link points there")?)
    }
}

fn registration_url(origin: &Url, token: &str) -> Url {
    let mut url = origin.join("/register").expect("an origin takes a path");
    url.set_fragment(Some(token));
    url
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_registration_token_out_of_the_request() {
        let origin = Url::parse("https://log.example.com").unwrap();
        let url = registration_url(&origin, "abc");
        assert_eq!(url.as_str(), "https://log.example.com/register#abc");
    }
}
