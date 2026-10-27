use std::{fs, path::PathBuf};

use clap::{Args, Subcommand, ValueEnum};

use crate::{
    cli::{CliResult, Context},
    store::{
        access,
        reference::{self, Installed, ReferenceKind},
    },
};

#[derive(Subcommand)]
pub enum ReferenceCommand {
    #[command(about = "Lists the installed reference data versions")]
    List,
    #[command(about = "Checks a reference data file and installs it where the configuration points (admin)")]
    Install(InstallArgs),
}

#[derive(Args)]
pub struct InstallArgs {
    #[arg(value_enum, help = "Which reference data the file holds")]
    kind: KindArg,
    #[arg(help = "A CTY zip archive (Big CTY or the contest files)")]
    file: PathBuf,
    #[arg(
        long,
        help = "Version to record [default: the =VER entry of cty.dat, else the UTC date of today]"
    )]
    version: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum KindArg {
    Cty,
}

impl From<KindArg> for ReferenceKind {
    fn from(kind: KindArg) -> ReferenceKind {
        match kind {
            KindArg::Cty => ReferenceKind::Cty,
        }
    }
}

pub async fn run(context: &Context<'_>, command: ReferenceCommand) -> CliResult<()> {
    match command {
        ReferenceCommand::List => {
            let principal = context.principal().await?;
            let mut conn = context.db.acquire().await?;
            let user = access::user(&mut conn, &principal).await?;
            for version in reference::list(&mut conn, &user).await? {
                println!("{}\t{}\t{}", version.kind.as_str(), version.version, version.updated_at);
            }
        }
        ReferenceCommand::Install(args) => {
            let (installed, path) = install(context, &args).await?;
            println!("{} has {} entries", args.file.display(), installed.entries);
            println!(
                "Installed {} {} at {}",
                installed.version.kind.as_str(),
                installed.version.version,
                path.display()
            );
        }
    }
    Ok(())
}

async fn install(context: &Context<'_>, args: &InstallArgs) -> CliResult<(Installed, PathBuf)> {
    let principal = context.principal().await?;
    let mut conn = context.db.acquire().await?;
    let admin = access::admin(&mut conn, &principal).await?;
    drop(conn);
    let bytes = fs::read(&args.file).map_err(|e| format!("{}: {e}", args.file.display()))?;
    let paths = &context.config.reference;
    let path = match args.kind {
        KindArg::Cty => &paths.cty,
    };
    let installed = reference::install(
        context.db,
        &admin,
        args.kind.into(),
        path,
        &bytes,
        args.version.as_deref(),
    )
    .await
    .map_err(|e| format!("{}: {e}", args.file.display()))?;
    Ok((installed, path.clone()))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use tempfile::TempDir;

    use super::*;
    use crate::{
        config::{Config, ReferenceConfig},
        domain::reference::tests::{CTY, CTY_CSV, cty_archive},
        store::testing::World,
    };

    #[tokio::test]
    async fn installs_a_cty_archive_as_an_admin() {
        let world = World::new().await;
        let dir = TempDir::new().unwrap();
        let config = Config {
            reference: ReferenceConfig {
                cty: dir.path().join("data").join("cty.zip"),
            },
            ..Config::default()
        };
        let source = dir.path().join("bigcty.zip");
        let archive = cty_archive(CTY, CTY_CSV);
        fs::write(&source, &archive).unwrap();
        let context = |actor| Context {
            db: &world.db,
            database: Path::new("unused.db"),
            config: &config,
            actor: Some(actor),
        };
        let args = InstallArgs {
            kind: KindArg::Cty,
            file: source.clone(),
            version: None,
        };

        let refused = install(&context("JA1OWN"), &args).await.unwrap_err();
        assert!(refused.to_string().contains("only an admin"), "{refused}");
        assert!(!config.reference.cty.exists());

        let (installed, path) = install(&context("JL1HIS"), &args).await.unwrap();
        assert_eq!(path, config.reference.cty);
        assert_eq!(fs::read(&path).unwrap(), archive);
        assert_eq!(
            (
                installed.version.kind,
                installed.version.version.as_str(),
                installed.entries
            ),
            (ReferenceKind::Cty, "20260915", 2)
        );

        fs::write(&source, CTY).unwrap();
        let refused = install(&context("JL1HIS"), &args).await.unwrap_err();
        assert!(refused.to_string().contains("not a zip archive"), "{refused}");
        assert_eq!(fs::read(&path).unwrap(), archive);
    }
}
