use std::{
    fs,
    path::{Path, PathBuf},
};

use chrono::Utc;
use clap::{Args, Subcommand, ValueEnum};
use sha2::{Digest, Sha256};

use crate::{
    cli::{CliResult, Context},
    domain::reference::{parse_cty, parse_dxcc, parse_parks, parse_subdivisions},
    store::{
        WriteTx, access,
        reference::{self, ReferenceKind, ReferenceVersion},
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
    #[arg(help = "cty.dat, or the TOML table for dxcc, subdivisions or parks")]
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
    Dxcc,
    Subdivisions,
    Parks,
}

impl From<KindArg> for ReferenceKind {
    fn from(kind: KindArg) -> ReferenceKind {
        match kind {
            KindArg::Cty => ReferenceKind::Cty,
            KindArg::Dxcc => ReferenceKind::Dxcc,
            KindArg::Subdivisions => ReferenceKind::Subdivisions,
            KindArg::Parks => ReferenceKind::Parks,
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
            println!(
                "Installed {} {} at {}",
                installed.kind.as_str(),
                installed.version,
                path.display()
            );
        }
    }
    Ok(())
}

async fn install(context: &Context<'_>, args: &InstallArgs) -> CliResult<(ReferenceVersion, PathBuf)> {
    let bytes = fs::read(&args.file).map_err(|e| format!("{}: {e}", args.file.display()))?;
    let text = std::str::from_utf8(&bytes).map_err(|e| format!("{}: {e}", args.file.display()))?;
    let invalid = |e: &dyn std::fmt::Display| format!("{}: {e}", args.file.display());
    let paths = &context.config.reference;
    let (detected, entries, path) = match args.kind {
        KindArg::Cty => {
            let cty = parse_cty(text).map_err(|e| invalid(&e))?;
            (cty.version, cty.database.len(), &paths.cty)
        }
        KindArg::Dxcc => (None, parse_dxcc(text).map_err(|e| invalid(&e))?.len(), &paths.dxcc),
        KindArg::Subdivisions => {
            let subdivisions = parse_subdivisions(text).map_err(|e| invalid(&e))?;
            (
                None,
                subdivisions.values().map(|names| names.len()).sum(),
                &paths.subdivisions,
            )
        }
        KindArg::Parks => (None, parse_parks(text).map_err(|e| invalid(&e))?.len(), &paths.parks),
    };
    println!("{} has {entries} entries", args.file.display());
    let version = args
        .version
        .clone()
        .or(detected)
        .unwrap_or_else(|| Utc::now().format("%Y-%m-%d").to_string());

    let principal = context.principal().await?;
    let mut tx = WriteTx::begin(context.db).await?;
    let admin = access::admin(&mut tx, &principal).await?;
    let checksum = Sha256::digest(&bytes);
    let installed = reference::set(&mut tx, &admin, args.kind.into(), &version, Some(checksum.as_slice())).await?;
    let previous = match fs::read(path) {
        Ok(bytes) => Some(bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(format!("{}: {e}", path.display()).into()),
    };
    replace(path, &bytes)?;
    if let Err(error) = tx.commit().await {
        let restored = match previous {
            Some(bytes) => replace(path, &bytes),
            None => fs::remove_file(path).map_err(|e| format!("{}: {e}", path.display()).into()),
        };
        if let Err(restore_error) = restored {
            return Err(format!("{error}; restoring {} failed: {restore_error}", path.display()).into());
        }
        return Err(error.into());
    }
    Ok((installed, path.clone()))
}

fn replace(path: &Path, bytes: &[u8]) -> CliResult<()> {
    if let Some(dir) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let mut staging = path.as_os_str().to_owned();
    staging.push(".installing");
    let staging = PathBuf::from(staging);
    fs::write(&staging, bytes).map_err(|e| format!("{}: {e}", staging.display()))?;
    fs::rename(&staging, path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::{
        config::{Config, ReferenceConfig},
        domain::reference::tests::CTY,
        store::testing::World,
    };

    #[tokio::test]
    async fn installs_checked_files_as_an_admin() {
        let world = World::new().await;
        let dir = tempfile::tempdir().unwrap();
        let config = Config {
            reference: ReferenceConfig {
                cty: dir.path().join("data").join("cty.dat"),
                ..ReferenceConfig::default()
            },
            ..Config::default()
        };
        let database = dir.path().join("unused.db");
        let source = dir.path().join("download.dat");
        fs::write(&source, CTY).unwrap();
        let args = InstallArgs {
            kind: KindArg::Cty,
            file: source.clone(),
            version: None,
        };
        let context = |actor| Context {
            db: &world.db,
            database: &database,
            config: &config,
            actor: Some(actor),
        };

        assert!(install(&context("JA1OWN"), &args).await.is_err());
        assert!(!config.reference.cty.exists());

        let (installed, path) = install(&context("JL1HIS"), &args).await.unwrap();
        assert_eq!(path, config.reference.cty);
        assert_eq!(fs::read_to_string(&path).unwrap(), CTY);
        assert_eq!(
            (installed.kind, installed.version.as_str()),
            (ReferenceKind::Cty, "20260915")
        );
        assert_eq!(installed.checksum.as_deref(), Some(Sha256::digest(CTY).as_slice()));

        fs::write(&source, "Japan\n").unwrap();
        assert!(install(&context("JL1HIS"), &args).await.is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), CTY);

        let updated = CTY.replace("Japan:", "Nippon:");
        fs::write(&source, &updated).unwrap();
        let args = InstallArgs {
            version: Some(" 2 ".into()),
            ..args
        };
        let (installed, _) = install(&context("JL1HIS"), &args).await.unwrap();
        assert_eq!(installed.version, "2");
        assert_eq!(fs::read_to_string(&path).unwrap(), updated);
        assert_eq!(installed.checksum.as_deref(), Some(Sha256::digest(&updated).as_slice()));
    }

    #[rstest]
    #[case::invalid_version("version")]
    #[case::file_write("file")]
    #[case::database_commit("commit")]
    #[tokio::test]
    async fn preserves_reference_data_on_failed_install(#[case] failure: &str, #[values(false, true)] existing: bool) {
        let world = World::new().await;
        let dir = tempfile::tempdir().unwrap();
        let config = Config {
            reference: ReferenceConfig {
                cty: dir.path().join("cty.dat"),
                ..ReferenceConfig::default()
            },
            ..Config::default()
        };
        let database = dir.path().join("unused.db");
        let source = dir.path().join("source.dat");
        fs::write(&source, CTY).unwrap();
        let context = Context {
            db: &world.db,
            database: &database,
            config: &config,
            actor: Some("JL1HIS"),
        };
        let mut args = InstallArgs {
            kind: KindArg::Cty,
            file: source.clone(),
            version: None,
        };
        if existing {
            install(&context, &args).await.unwrap();
        }
        let mut conn = world.db.acquire().await.unwrap();
        let previous = reference::current(&mut conn, ReferenceKind::Cty).await.unwrap();
        drop(conn);
        fs::write(&source, CTY.replace("Japan:", "Nippon:")).unwrap();
        args.version = Some("new".into());
        match failure {
            "version" => args.version = Some(" ".into()),
            "file" => fs::create_dir(dir.path().join("cty.dat.installing")).unwrap(),
            "commit" => {
                sqlx::raw_sql(
                    "CREATE TABLE install_failure (
                        user_id TEXT REFERENCES users(id) DEFERRABLE INITIALLY DEFERRED
                     );
                     CREATE TRIGGER fail_reference_insert AFTER INSERT ON reference_versions
                     BEGIN INSERT INTO install_failure VALUES ('missing'); END;
                     CREATE TRIGGER fail_reference_update AFTER UPDATE ON reference_versions
                     BEGIN INSERT INTO install_failure VALUES ('missing'); END;",
                )
                .execute(&world.db)
                .await
                .unwrap();
            }
            _ => unreachable!(),
        }
        let error = install(&context, &args).await.unwrap_err().to_string();
        match failure {
            "version" => assert!(error.contains("version cannot be empty"), "{error}"),
            "file" => assert!(error.contains("cty.dat.installing"), "{error}"),
            "commit" => assert!(error.contains("FOREIGN KEY constraint failed"), "{error}"),
            _ => unreachable!(),
        }
        if existing {
            assert_eq!(fs::read(&config.reference.cty).unwrap(), CTY.as_bytes());
        } else {
            assert!(!config.reference.cty.exists());
        }
        let mut conn = world.db.acquire().await.unwrap();
        assert_eq!(
            reference::current(&mut conn, ReferenceKind::Cty).await.unwrap(),
            previous
        );
    }
}
