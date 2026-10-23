use std::{error::Error, net::SocketAddr, path::PathBuf, process::ExitCode, sync::Arc};

use clap::{Parser, Subcommand};
use jl1his_logger::{
    cli,
    config::Config,
    db::{self, backup},
    web::{AppState, router, serve},
};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(version, about = "The QSO log of record for JL1HIS")]
struct Cli {
    #[arg(
        short,
        long,
        default_value = "config.toml",
        global = true,
        help = "TOML configuration (see assets/config.example.toml); relative paths in it are relative to its directory"
    )]
    config: PathBuf,
    #[arg(
        long = "as",
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
        #[arg(long, help = "Address to listen on, in place of server.listen")]
        listen: Option<SocketAddr>,
        #[arg(long, help = "The built client (client/build), in place of server.web_dir")]
        web_dir: Option<PathBuf>,
    },
    #[command(flatten)]
    Cli(cli::Command),
}

#[tokio::main]
async fn main() -> ExitCode {
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
    let config = Config::load(&cli.config)?;
    let database = config.database.path.clone();
    match cli.command {
        Command::Serve { listen, web_dir } => {
            let listen = listen.unwrap_or(config.server.listen);
            let web_dir = web_dir.or_else(|| config.server.web_dir.clone());
            let public_origin = config
                .server
                .public_origin
                .clone()
                .ok_or("set server.public_origin in the configuration")?;
            match &web_dir {
                Some(dir) if !dir.join("index.html").is_file() => {
                    return Err(format!("{} has no index.html; build the client first", dir.display()).into());
                }
                Some(dir) => tracing::info!("serving the client from {}", dir.display()),
                None => tracing::warn!("server.web_dir is not set; serving the API only"),
            }

            let _lock = backup::lock(&database)?;
            let db = db::open(&database).await?;
            if config.server.auto_migrate {
                let migrated = db::migrate(&db, &db::snapshot_dir(&database)).await?;
                if let Some(snapshot) = &migrated.snapshot {
                    tracing::info!("snapshot written to {}", snapshot.display());
                }
                if !migrated.applied.is_empty() {
                    tracing::info!("applied migrations {:?}", migrated.applied);
                }
            } else {
                db::require_migrated(&db).await?;
            }

            let state = Arc::new(AppState::new(db.clone(), config, public_origin.clone())?);
            let listener = tokio::net::TcpListener::bind(listen).await?;
            tracing::info!("listening on http://{} for {public_origin}", listener.local_addr()?);
            let closing = state.clone();
            serve(listener, router(state, web_dir.as_deref()), async move {
                shutdown_signal().await;
                closing.close_streams();
            })
            .await?;
            db.close().await;
        }
        Command::Cli(command) => cli::run(&database, &config, cli.actor.as_deref(), command).await?,
    }
    Ok(())
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
