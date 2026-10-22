use std::path::{Path, PathBuf};

use clap::Args;

use crate::{
    cli::{CliResult, Context},
    db::backup::{self, backup_dir, backup_path},
};

#[derive(Args)]
pub struct BackupArgs {
    #[arg(
        short,
        long,
        conflicts_with = "keep",
        help = "File to write [default: a timestamped file in the backups directory next to the database]"
    )]
    output: Option<PathBuf>,
    #[arg(
        long,
        value_parser = clap::value_parser!(u64).range(1..),
        help = "Keep only this many backups in the backups directory"
    )]
    keep: Option<u64>,
}

#[derive(Args)]
pub struct RestoreArgs {
    #[arg(help = "Backup to restore; it is copied, never modified")]
    file: PathBuf,
    #[arg(long, help = "Confirm replacing the database")]
    yes: bool,
}

pub async fn backup(context: &Context<'_>, args: &BackupArgs) -> CliResult<()> {
    let dir = backup_dir(context.database);
    let path = args.output.clone().unwrap_or_else(|| backup_path(&dir));
    backup::backup(context.db, &path).await?;
    println!("Backup written to {}", path.display());
    if let Some(keep) = args.keep {
        for removed in backup::prune(&dir, usize::try_from(keep)?)? {
            println!("Removed {}", removed.display());
        }
    }
    Ok(())
}

pub async fn restore(database: &Path, args: &RestoreArgs) -> CliResult<()> {
    let lock = backup::lock(database)?;
    if !args.yes {
        return Err(format!(
            "this replaces {} with {}, signs everyone out and forces every client to sync again; add --yes to confirm",
            database.display(),
            args.file.display()
        )
        .into());
    }
    let restored = backup::restore(&lock, database, &args.file).await?;
    if let Some(snapshot) = &restored.snapshot {
        println!("The replaced database was saved to {}", snapshot.display());
    }
    if !restored.migrated.applied.is_empty() {
        println!("Applied migrations {:?}", restored.migrated.applied);
    }
    println!(
        "Restored {}. {} Logbooks will sync again; {} sessions and {} unused registration links were deleted.",
        args.file.display(),
        restored.logbooks,
        restored.sessions,
        restored.registrations
    );
    println!("API tokens in the restored database; revoke any that were revoked after the backup:");
    for token in &restored.tokens {
        println!(
            "  {}\t{}\t{}\tcreated {}\tlast used {}",
            token.callsign,
            token.name,
            token.kind,
            token.created_at,
            token.last_used_at.as_deref().unwrap_or("never")
        );
    }
    Ok(())
}
