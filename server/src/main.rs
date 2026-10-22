use std::{
    error::Error,
    net::SocketAddr,
    path::{Path, PathBuf},
    process::ExitCode,
    sync::Arc,
};

use clap::{ArgAction, Parser, Subcommand};
use jl1his_logger::{
    cli,
    config::{Config, parse_public_origin},
    db::{self, Db, Migrated, backup},
    web::{AppState, router},
};
use tracing_subscriber::EnvFilter;
use url::Url;

#[derive(Parser)]
#[command(version, about = "The QSO log of record for JL1HIS")]
struct Cli {
    #[arg(
        long,
        env = "DATABASE_PATH",
        default_value = "data/jl1his-logger.db",
        global = true,
        help = "SQLite database file; snapshots go to the snapshots directory next to it"
    )]
    database: PathBuf,
    #[arg(
        short,
        long,
        env = "CONFIG_PATH",
        global = true,
        help = "TOML configuration (see assets/config.example.toml); defaults apply without it"
    )]
    config: Option<PathBuf>,
    #[arg(
        long = "as",
        env = "CLI_CALLSIGN",
        global = true,
        value_name = "CALLSIGN",
        help = "Callsign of the user the CLI acts as"
    )]
    actor: Option<String>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    #[command(about = "Serves the web app and the API")]
    Serve {
        #[arg(
            long,
            env = "LISTEN",
            default_value = "127.0.0.1:8080",
            help = "Address to listen on"
        )]
        listen: SocketAddr,
        #[arg(
            long,
            env = "PUBLIC_ORIGIN",
            value_parser = parse_public_origin,
            help = "URL the app is reached at, such as https://log.example.com; passkeys are bound to its host"
        )]
        public_origin: Url,
        #[arg(
            long,
            env = "WEB_DIR",
            help = "The built client (client/build); without it only the API is served"
        )]
        web_dir: Option<PathBuf>,
        #[arg(
            long,
            env = "AUTO_MIGRATE",
            default_value_t = true,
            action = ArgAction::Set,
            help = "Apply pending migrations at startup; with false, pending migrations stop the server"
        )]
        auto_migrate: bool,
    },
    #[command(about = "Applies pending migrations after snapshotting the database")]
    Migrate,
    #[command(flatten)]
    Cli(cli::Command),
}

#[tokio::main]
async fn main() -> ExitCode {
    if let Err(e) = dotenvy::dotenv()
        && !e.not_found()
    {
        eprintln!("error: failed to load .env: {e}");
        return ExitCode::FAILURE;
    }

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with_writer(std::io::stderr)
        .init();

    match run(Cli::parse()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> Result<(), Box<dyn Error>> {
    match cli.command {
        Command::Serve {
            listen,
            public_origin,
            web_dir,
            auto_migrate,
        } => {
            let config = load_config(cli.config.as_deref())?;
            match &web_dir {
                Some(dir) if !dir.join("index.html").is_file() => {
                    return Err(format!("{} has no index.html; build the client first", dir.display()).into());
                }
                Some(dir) => tracing::info!("serving the client from {}", dir.display()),
                None => tracing::warn!("WEB_DIR is not set; serving the API only"),
            }

            let _lock = backup::lock(&cli.database)?;
            let db = db::open(&cli.database).await?;
            if auto_migrate {
                let migrated = migrate(&db, &cli.database).await?;
                if let Some(snapshot) = &migrated.snapshot {
                    tracing::info!("snapshot written to {}", snapshot.display());
                }
                if !migrated.applied.is_empty() {
                    tracing::info!("applied migrations {:?}", migrated.applied);
                }
            } else {
                let status = db::migration_status(&db).await?;
                if !status.pending.is_empty() {
                    return Err(format!(
                        "migrations {:?} are pending; run the migrate subcommand first",
                        status.pending
                    )
                    .into());
                }
            }

            let state = Arc::new(AppState {
                db: db.clone(),
                config,
                public_origin: public_origin.clone(),
            });
            let listener = tokio::net::TcpListener::bind(listen).await?;
            tracing::info!("listening on http://{} for {public_origin}", listener.local_addr()?);
            axum::serve(listener, router(state, web_dir.as_deref()))
                .with_graceful_shutdown(shutdown_signal())
                .await?;
            db.close().await;
        }
        Command::Migrate => {
            let db = db::open(&cli.database).await?;
            let migrated = migrate(&db, &cli.database).await?;
            db.close().await;
            if let Some(snapshot) = &migrated.snapshot {
                println!("Snapshot written to {}", snapshot.display());
            }
            match migrated.applied.is_empty() {
                true => println!("The database is up to date."),
                false => println!("Applied migrations {:?}", migrated.applied),
            }
        }
        Command::Cli(command) => {
            let config = load_config(cli.config.as_deref())?;
            cli::run(&cli.database, &config, cli.actor.as_deref(), command).await?;
        }
    }
    Ok(())
}

fn load_config(path: Option<&Path>) -> Result<Config, Box<dyn Error>> {
    Ok(match path {
        Some(path) => Config::load(path)?,
        None => Config::default(),
    })
}

async fn migrate(db: &Db, database: &Path) -> Result<Migrated, db::DbError> {
    db::migrate(db, &db::snapshot_dir(database)).await
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(_) => std::future::pending().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::*;

    #[test]
    fn cli_is_consistent() {
        Cli::command().debug_assert();
    }
}
