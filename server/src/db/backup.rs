use std::{
    ffi::OsString,
    fs::{self, File, TryLockError},
    io::{self, Read},
    path::{Path, PathBuf},
};

use chrono::Utc;

use crate::db::{self, Db, DbError, Migrated, create_dir, new_id, now, vacuum_into};

const PREFIX: &str = "jl1his-logger-";

#[derive(Debug, thiserror::Error)]
pub enum BackupError {
    #[error("{}: {source}", path.display())]
    Io { path: PathBuf, source: io::Error },
    #[error("{} is locked by another process; stop the server first", path.display())]
    Locked { path: PathBuf },
    #[error("{} already exists", path.display())]
    Exists { path: PathBuf },
    #[error("{} is the database itself", path.display())]
    SameFile { path: PathBuf },
    #[error("{} failed the integrity check: {detail}", path.display())]
    Integrity { path: PathBuf, detail: String },
    #[error("{} has no applied migrations; it is not a jl1his-logger database", path.display())]
    NotLogger { path: PathBuf },
    #[error(transparent)]
    Db(#[from] DbError),
    #[error("database error: {0}")]
    Sqlx(#[from] sqlx::Error),
}

#[derive(Debug)]
pub struct Lock {
    _file: File,
}

#[derive(Debug)]
pub struct Restored {
    pub snapshot: Option<PathBuf>,
    pub migrated: Migrated,
    pub logbooks: usize,
    pub sessions: u64,
    pub registrations: u64,
    pub tokens: Vec<RestoredToken>,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct RestoredToken {
    pub callsign: String,
    pub name: String,
    pub created_at: String,
    pub last_used_at: Option<String>,
}

fn io_error(path: &Path) -> impl FnOnce(io::Error) -> BackupError + '_ {
    move |source| BackupError::Io {
        path: path.to_owned(),
        source,
    }
}

fn sibling(database: &Path, suffix: &str) -> PathBuf {
    let mut name = OsString::from(database.as_os_str());
    name.push(suffix);
    PathBuf::from(name)
}

pub fn lock(database: &Path) -> Result<Lock, BackupError> {
    if let Some(dir) = db::parent(database) {
        create_dir(dir)?;
    }
    let path = sibling(database, ".lock");
    let file = File::options()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&path)
        .map_err(io_error(&path))?;
    match file.try_lock() {
        Ok(()) => Ok(Lock { _file: file }),
        Err(TryLockError::WouldBlock) => Err(BackupError::Locked { path }),
        Err(TryLockError::Error(source)) => Err(BackupError::Io { path, source }),
    }
}

pub fn backup_dir(database: &Path) -> PathBuf {
    db::parent(database).unwrap_or(Path::new("")).join("backups")
}

pub fn backup_path(dir: &Path) -> PathBuf {
    dir.join(format!("{PREFIX}{}.db", db::file_stamp(Utc::now())))
}

pub async fn backup(db: &Db, path: &Path) -> Result<(), BackupError> {
    if path.exists() {
        return Err(BackupError::Exists { path: path.to_owned() });
    }
    if let Some(dir) = db::parent(path) {
        create_dir(dir)?;
    }
    Ok(vacuum_into(db, path).await?)
}

pub fn prune(dir: &Path, keep: usize) -> Result<Vec<PathBuf>, BackupError> {
    let mut backups = vec![];
    for entry in fs::read_dir(dir).map_err(io_error(dir))? {
        let entry = entry.map_err(io_error(dir))?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(PREFIX) && name.ends_with(".db") && entry.path().is_file() {
            backups.push(entry.path());
        }
    }
    backups.sort();
    let excess = backups.len().saturating_sub(keep);
    let removed: Vec<PathBuf> = backups.drain(..excess).collect();
    for path in &removed {
        fs::remove_file(path).map_err(io_error(path))?;
    }
    Ok(removed)
}

pub async fn restore(_lock: &Lock, database: &Path, source: &Path) -> Result<Restored, BackupError> {
    if fs::canonicalize(source)
        .ok()
        .is_some_and(|s| fs::canonicalize(database).ok() == Some(s))
    {
        return Err(BackupError::SameFile {
            path: source.to_owned(),
        });
    }
    let mut header = [0; 16];
    File::open(source)
        .and_then(|mut file| file.read_exact(&mut header))
        .map_err(io_error(source))?;
    if &header != b"SQLite format 3\0" {
        return Err(BackupError::Integrity {
            path: source.to_owned(),
            detail: "not an SQLite database".into(),
        });
    }
    let staging = sibling(database, ".restoring");
    remove_database_files(&staging)?;
    fs::copy(source, &staging).map_err(io_error(source))?;
    let restored = async {
        let restored = prepare(&staging, source).await?;
        let snapshot = match database.exists() {
            true => {
                let current = db::open(database).await?;
                let snapshot = db::snapshot(&current, &db::snapshot_dir(database), "before-restore").await;
                current.close().await;
                Some(snapshot?)
            }
            false => None,
        };
        fs::rename(&staging, database).map_err(io_error(database))?;
        Ok(Restored { snapshot, ..restored })
    }
    .await;
    if restored.is_err() {
        let _ = remove_database_files(&staging);
    }
    restored
}

async fn prepare(staging: &Path, source: &Path) -> Result<Restored, BackupError> {
    let db = db::open(staging).await?;
    let checked = async {
        let rows: Vec<String> = sqlx::query_scalar("PRAGMA integrity_check").fetch_all(&db).await?;
        if rows != ["ok"] {
            return Err(BackupError::Integrity {
                path: source.to_owned(),
                detail: rows.join("; "),
            });
        }
        if db::migration_status(&db).await?.applied.is_empty() {
            return Err(BackupError::NotLogger {
                path: source.to_owned(),
            });
        }
        reset(&db, staging).await
    }
    .await;
    db.close().await;
    let restored = checked?;
    let wal = sibling(staging, "-wal");
    if wal.exists() {
        return Err(BackupError::Exists { path: wal });
    }
    Ok(restored)
}

async fn reset(db: &Db, database: &Path) -> Result<Restored, BackupError> {
    let migrated = db::migrate(db, &db::snapshot_dir(database)).await?;
    let now = now();
    let mut tx = db.begin_with("BEGIN IMMEDIATE").await?;
    let logbooks: Vec<String> = sqlx::query_scalar("SELECT id FROM logbooks")
        .fetch_all(&mut *tx)
        .await?;
    for id in &logbooks {
        sqlx::query("UPDATE logbooks SET cursor_generation = ?, updated_at = ? WHERE id = ?")
            .bind(new_id())
            .bind(&now)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    let sessions = sqlx::query("DELETE FROM sessions").execute(&mut *tx).await?;
    let registrations = sqlx::query("DELETE FROM registration_tokens WHERE used_at IS NULL")
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    let tokens = sqlx::query_as(
        "SELECT u.callsign, t.name, t.created_at, t.last_used_at \
         FROM api_tokens t JOIN users u ON u.id = t.user_id ORDER BY u.callsign, t.created_at, t.id",
    )
    .fetch_all(db)
    .await?;
    Ok(Restored {
        snapshot: None,
        migrated,
        logbooks: logbooks.len(),
        sessions: sessions.rows_affected(),
        registrations: registrations.rows_affected(),
        tokens,
    })
}

fn remove_database_files(path: &Path) -> Result<(), BackupError> {
    for path in [path.to_owned(), sibling(path, "-wal"), sibling(path, "-shm")] {
        match fs::remove_file(&path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(BackupError::Io { path, source: e }),
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use sqlx::migrate::Migrator;
    use tempfile::TempDir;

    use super::*;

    const NOW: &str = "2026-10-22T00:00:00Z";

    async fn database() -> (TempDir, PathBuf, Db) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data").join("logger.db");
        let db = db::open(&path).await.unwrap();
        db::migrate(&db, &db::snapshot_dir(&path)).await.unwrap();
        sqlx::raw_sql(
            "INSERT INTO users (id, callsign, role, created_at, updated_at)
             VALUES ('user', 'JL1HIS', 'admin', '2026-10-22T00:00:00Z', '2026-10-22T00:00:00Z');
             INSERT INTO logbooks (id, name, cursor_generation, created_at, updated_at)
             VALUES ('log', 'Club', 'generation', '2026-10-22T00:00:00Z', '2026-10-22T00:00:00Z');
             INSERT INTO sessions (id, token_hash, user_id, created_at, last_used_at, expires_at)
             VALUES ('session', X'01', 'user', '2026-10-22T00:00:00Z', '2026-10-22T00:00:00Z', '2099-01-01T00:00:00Z');
             INSERT INTO registration_tokens (token_hash, user_id, created_at, expires_at, used_at)
             VALUES (X'02', 'user', '2026-10-22T00:00:00Z', '2099-01-01T00:00:00Z', NULL),
                    (X'03', 'user', '2026-10-22T00:00:00Z', '2099-01-01T00:00:00Z', '2026-10-22T01:00:00Z');
             INSERT INTO api_tokens (id, user_id, name, token_hash, created_at)
             VALUES ('token', 'user', 'grayline', X'04', '2026-10-22T00:00:00Z');",
        )
        .execute(&db)
        .await
        .unwrap();
        (dir, path, db)
    }

    async fn marks(db: &Db) -> Vec<String> {
        sqlx::query_scalar("SELECT callsign FROM users WHERE id = callsign ORDER BY callsign")
            .fetch_all(db)
            .await
            .unwrap()
    }

    async fn mark(db: &Db, key: &str) {
        sqlx::query("INSERT INTO users (id, callsign, role, created_at, updated_at) VALUES (?1, ?1, 'user', ?2, ?2)")
            .bind(key)
            .bind(NOW)
            .execute(db)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn locks_the_database_once() {
        let dir = tempfile::tempdir().unwrap();
        let database = dir.path().join("data").join("logger.db");
        let held = lock(&database).unwrap();
        assert!(matches!(lock(&database), Err(BackupError::Locked { .. })));
        drop(held);
        lock(&database).unwrap();
    }

    #[tokio::test]
    async fn backs_up_while_open_and_prunes_old_backups() {
        let (dir, path, db) = database().await;
        mark(&db, "before").await;
        let backups = backup_dir(&path);
        for name in [
            "jl1his-logger-20261001T000000Z.db",
            "jl1his-logger-20261002T000000Z.db",
            "other.db",
        ] {
            fs::create_dir_all(&backups).unwrap();
            fs::write(backups.join(name), b"").unwrap();
        }
        let target = backup_path(&backups);
        backup(&db, &target).await.unwrap();
        assert!(matches!(backup(&db, &target).await, Err(BackupError::Exists { .. })));

        let copy = db::open(&target).await.unwrap();
        assert_eq!(marks(&copy).await, ["before"]);
        copy.close().await;

        let removed = prune(&backups, 2).unwrap();
        assert_eq!(removed, [backups.join("jl1his-logger-20261001T000000Z.db")]);
        let mut left: Vec<_> = fs::read_dir(&backups)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .filter(|n| n.ends_with(".db"))
            .collect();
        left.sort();
        assert_eq!(left.len(), 3);
        assert!(left.contains(&"other.db".to_owned()));
        drop(dir);
    }

    #[tokio::test]
    async fn restores_a_backup_and_forces_a_resync() {
        let (dir, path, db) = database().await;
        mark(&db, "in backup").await;
        let source = dir.path().join("saved.db");
        backup(&db, &source).await.unwrap();
        let saved = fs::read(&source).unwrap();
        mark(&db, "after backup").await;
        db.close().await;

        let held = lock(&path).unwrap();
        let restored = restore(&held, &path, &source).await.unwrap();
        assert!(restored.snapshot.as_ref().unwrap().is_file());
        assert!(restored.migrated.applied.is_empty());
        assert_eq!(
            (restored.logbooks, restored.sessions, restored.registrations),
            (1, 1, 1)
        );
        assert_eq!(
            restored.tokens,
            [RestoredToken {
                callsign: "JL1HIS".into(),
                name: "grayline".into(),
                created_at: NOW.into(),
                last_used_at: None,
            }]
        );
        assert_eq!(fs::read(&source).unwrap(), saved);
        assert!(!sibling(&path, ".restoring").exists());

        let db = db::open(&path).await.unwrap();
        assert_eq!(marks(&db).await, ["in backup"]);
        let generation: String = sqlx::query_scalar("SELECT cursor_generation FROM logbooks")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_ne!(generation, "generation");
        let left: (i64, i64) =
            sqlx::query_as("SELECT (SELECT count(*) FROM sessions), (SELECT count(*) FROM registration_tokens)")
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(left, (0, 1));

        let snapshot = db::open(restored.snapshot.as_ref().unwrap()).await.unwrap();
        assert_eq!(marks(&snapshot).await, ["after backup", "in backup"]);
    }

    #[rstest]
    #[case::migration("migration")]
    #[case::reset("reset")]
    #[case::snapshot("snapshot")]
    #[tokio::test]
    async fn preserves_the_database_when_restore_fails(#[case] phase: &str) {
        let (dir, path, db) = database().await;
        mark(&db, "current").await;
        db.close().await;
        let saved = fs::read(&path).unwrap();
        let source = dir.path().join("saved.db");
        let copy = db::open(&source).await.unwrap();
        if phase == "migration" {
            let initial = Migrator::with_migrations(vec![db::MIGRATOR.iter().next().unwrap().clone()]);
            initial.run(&copy).await.unwrap();
            sqlx::query("INSERT INTO reference_versions (kind, version, updated_at) VALUES ('legacy', 'v1', ?)")
                .bind(NOW)
                .execute(&copy)
                .await
                .unwrap();
        } else {
            db::migrate(&copy, &db::snapshot_dir(&source)).await.unwrap();
            if phase == "reset" {
                sqlx::query("DROP TABLE sessions").execute(&copy).await.unwrap();
            } else {
                fs::write(db::snapshot_dir(&path), b"blocked").unwrap();
            }
        }
        mark(&copy, "backup").await;
        copy.close().await;
        let source_saved = fs::read(&source).unwrap();

        let held = lock(&path).unwrap();
        let error = restore(&held, &path, &source).await.unwrap_err();
        match phase {
            "migration" => assert!(matches!(error, BackupError::Db(DbError::Migrate(_)))),
            "reset" => assert!(matches!(error, BackupError::Sqlx(_))),
            _ => assert!(matches!(error, BackupError::Db(DbError::CreateDir { .. }))),
        }
        assert_eq!(fs::read(&path).unwrap(), saved);
        assert_eq!(fs::read(&source).unwrap(), source_saved);
        for suffix in [".restoring", ".restoring-wal", ".restoring-shm"] {
            assert!(!sibling(&path, suffix).exists());
        }
        let current = db::open(&path).await.unwrap();
        assert_eq!(marks(&current).await, ["current"]);
        let state: (String, i64, i64) = sqlx::query_as(
            "SELECT cursor_generation, (SELECT count(*) FROM sessions), \
             (SELECT count(*) FROM registration_tokens) FROM logbooks",
        )
        .fetch_one(&current)
        .await
        .unwrap();
        assert_eq!(state, ("generation".into(), 1, 2));
        current.close().await;
    }

    #[tokio::test]
    async fn restores_a_backup_with_pending_migrations() {
        let (dir, path, db) = database().await;
        mark(&db, "current").await;
        db.close().await;
        let source = dir.path().join("old.db");
        let old = db::open(&source).await.unwrap();
        let initial = Migrator::with_migrations(vec![db::MIGRATOR.iter().next().unwrap().clone()]);
        initial.run(&old).await.unwrap();
        mark(&old, "backup").await;
        old.close().await;
        let saved = fs::read(&source).unwrap();

        let held = lock(&path).unwrap();
        let restored = restore(&held, &path, &source).await.unwrap();
        assert_eq!(restored.migrated.applied, [2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14]);
        assert!(restored.migrated.snapshot.unwrap().is_file());
        assert!(restored.snapshot.unwrap().is_file());
        assert_eq!(fs::read(&source).unwrap(), saved);
        let current = db::open(&path).await.unwrap();
        assert!(db::migration_status(&current).await.unwrap().pending.is_empty());
        assert_eq!(marks(&current).await, ["backup"]);
        current.close().await;
    }

    #[tokio::test]
    async fn refuses_a_file_that_is_not_a_logger_database() {
        let (dir, path, db) = database().await;
        mark(&db, "current").await;
        db.close().await;

        let other = dir.path().join("other.db");
        let foreign = db::open(&other).await.unwrap();
        sqlx::query("CREATE TABLE t (x INTEGER)")
            .execute(&foreign)
            .await
            .unwrap();
        foreign.close().await;
        let text = dir.path().join("notes.txt");
        fs::write(&text, b"not a database at all, just some text that is long enough").unwrap();

        let held = lock(&path).unwrap();
        assert!(matches!(
            restore(&held, &path, &other).await,
            Err(BackupError::NotLogger { .. })
        ));
        assert!(matches!(
            restore(&held, &path, &text).await,
            Err(BackupError::Integrity { .. })
        ));
        assert!(matches!(
            restore(&held, &path, &path).await,
            Err(BackupError::SameFile { .. })
        ));
        assert!(!sibling(&path, ".restoring").exists());

        let db = db::open(&path).await.unwrap();
        assert_eq!(marks(&db).await, ["current"]);
    }
}
