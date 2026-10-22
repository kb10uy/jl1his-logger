use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use chrono::Utc;
use sqlx::{
    migrate::{Migrate, MigrateError, Migrator},
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions},
};

pub type Db = SqlitePool;

pub static MIGRATOR: Migrator = sqlx::migrate!();

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("failed to create {}: {source}", path.display())]
    CreateDir { path: PathBuf, source: std::io::Error },
    #[error("{} is not valid UTF-8", path.display())]
    NonUtf8Path { path: PathBuf },
    #[error("database error: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("migration error: {0}")]
    Migrate(#[from] MigrateError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationStatus {
    pub applied: Vec<i64>,
    pub pending: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Migrated {
    pub applied: Vec<i64>,
    pub snapshot: Option<PathBuf>,
}

pub async fn open(path: &Path) -> Result<Db, DbError> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        create_dir(dir)?;
    }
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));
    Ok(SqlitePoolOptions::new().connect_with(options).await?)
}

pub async fn migration_status(db: &Db) -> Result<MigrationStatus, DbError> {
    status_with(&MIGRATOR, db).await
}

pub async fn migrate(db: &Db, snapshot_dir: &Path) -> Result<Migrated, DbError> {
    migrate_with(&MIGRATOR, db, snapshot_dir).await
}

async fn status_with(migrator: &Migrator, db: &Db) -> Result<MigrationStatus, DbError> {
    let mut conn = db.acquire().await?;
    conn.ensure_migrations_table(&migrator.table_name).await?;
    if let Some(version) = conn.dirty_version(&migrator.table_name).await? {
        return Err(MigrateError::Dirty(version).into());
    }
    let applied = conn.list_applied_migrations(&migrator.table_name).await?;
    let ups = || migrator.iter().filter(|m| m.migration_type.is_up_migration());
    for done in &applied {
        match ups().find(|m| m.version == done.version) {
            None => return Err(MigrateError::VersionMissing(done.version).into()),
            Some(m) if m.checksum != done.checksum => return Err(MigrateError::VersionMismatch(done.version).into()),
            Some(_) => {}
        }
    }
    let pending = ups()
        .map(|m| m.version)
        .filter(|v| !applied.iter().any(|done| done.version == *v))
        .collect();
    Ok(MigrationStatus {
        applied: applied.into_iter().map(|done| done.version).collect(),
        pending,
    })
}

async fn migrate_with(migrator: &Migrator, db: &Db, snapshot_dir: &Path) -> Result<Migrated, DbError> {
    let status = status_with(migrator, db).await?;
    let Some(&first) = status.pending.first() else {
        return Ok(Migrated {
            applied: vec![],
            snapshot: None,
        });
    };
    let snapshot = match status.applied.is_empty() {
        true => None,
        false => Some(snapshot(db, snapshot_dir, first).await?),
    };
    migrator.run(db).await?;
    Ok(Migrated {
        applied: status.pending,
        snapshot,
    })
}

async fn snapshot(db: &Db, dir: &Path, before: i64) -> Result<PathBuf, DbError> {
    create_dir(dir)?;
    let path = dir.join(format!("{}-before-{before:04}.db", Utc::now().format("%Y%m%dT%H%M%SZ")));
    let target = path
        .to_str()
        .ok_or_else(|| DbError::NonUtf8Path { path: path.clone() })?;
    sqlx::query("VACUUM INTO ?").bind(target).execute(db).await?;
    Ok(path)
}

fn create_dir(path: &Path) -> Result<(), DbError> {
    std::fs::create_dir_all(path).map_err(|source| DbError::CreateDir {
        path: path.to_owned(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use sqlx::{
        AssertSqlSafe, SqlSafeStr,
        migrate::{Migration, MigrationType},
    };
    use tempfile::TempDir;

    use super::*;

    async fn temp_db() -> (TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let db = open(&dir.path().join("data").join("test.db")).await.unwrap();
        (dir, db)
    }

    fn migration(version: i64, sql: &str) -> Migration {
        Migration::new(
            version,
            Cow::Borrowed("test"),
            MigrationType::Simple,
            AssertSqlSafe(sql.to_owned()).into_sql_str(),
            false,
        )
    }

    fn with_extra(extra: Migration) -> Migrator {
        let mut migrations: Vec<_> = MIGRATOR.iter().cloned().collect();
        migrations.push(extra);
        Migrator::with_migrations(migrations)
    }

    #[tokio::test]
    async fn sets_the_connection_pragmas() {
        let (_dir, db) = temp_db().await;
        let journal: String = sqlx::query_scalar("PRAGMA journal_mode").fetch_one(&db).await.unwrap();
        let foreign_keys: i64 = sqlx::query_scalar("PRAGMA foreign_keys").fetch_one(&db).await.unwrap();
        let busy_timeout: i64 = sqlx::query_scalar("PRAGMA busy_timeout").fetch_one(&db).await.unwrap();
        assert_eq!((journal.as_str(), foreign_keys, busy_timeout), ("wal", 1, 5000));
    }

    #[tokio::test]
    async fn migrates_a_new_database_without_a_snapshot() {
        let (dir, db) = temp_db().await;
        let snapshots = dir.path().join("snapshots");
        let all: Vec<i64> = MIGRATOR.iter().map(|m| m.version).collect();

        let first = migrate(&db, &snapshots).await.unwrap();
        assert_eq!(first.applied, all);
        assert_eq!(first.snapshot, None);
        assert!(!snapshots.exists());

        let again = migrate(&db, &snapshots).await.unwrap();
        assert!(again.applied.is_empty());
        let status = migration_status(&db).await.unwrap();
        assert_eq!((status.applied, status.pending), (all, vec![]));
    }

    #[tokio::test]
    async fn snapshots_before_applying_to_an_existing_schema() {
        let (dir, db) = temp_db().await;
        let snapshots = dir.path().join("snapshots");
        migrate(&db, &snapshots).await.unwrap();
        sqlx::query("INSERT INTO settings (key, value, updated_at) VALUES ('k', 'v', '2026-10-22T00:00:00Z')")
            .execute(&db)
            .await
            .unwrap();

        let migrator = with_extra(migration(9999, "CREATE TABLE added (x INTEGER)"));
        let migrated = migrate_with(&migrator, &db, &snapshots).await.unwrap();
        assert_eq!(migrated.applied, vec![9999]);
        let snapshot = migrated.snapshot.unwrap();
        assert!(snapshot.to_str().unwrap().ends_with("-before-9999.db"));

        let old = open(&snapshot).await.unwrap();
        let value: String = sqlx::query_scalar("SELECT value FROM settings WHERE key = 'k'")
            .fetch_one(&old)
            .await
            .unwrap();
        let added: i64 = sqlx::query_scalar("SELECT count(*) FROM sqlite_schema WHERE name = 'added'")
            .fetch_one(&old)
            .await
            .unwrap();
        assert_eq!((value.as_str(), added), ("v", 0));
    }

    #[tokio::test]
    async fn refuses_an_edited_migration_before_snapshotting() {
        let (dir, db) = temp_db().await;
        let snapshots = dir.path().join("snapshots");
        migrate(&db, &snapshots).await.unwrap();

        let mut migrations: Vec<_> = MIGRATOR.iter().cloned().collect();
        migrations[0] = migration(migrations[0].version, "CREATE TABLE edited (x INTEGER)");
        migrations.push(migration(9999, "CREATE TABLE added (x INTEGER)"));
        let edited = Migrator::with_migrations(migrations);

        let err = migrate_with(&edited, &db, &snapshots).await.unwrap_err();
        assert!(
            matches!(err, DbError::Migrate(MigrateError::VersionMismatch(1))),
            "{err}"
        );
        assert!(!snapshots.exists());
    }

    #[tokio::test]
    async fn refuses_a_database_from_a_newer_binary() {
        let (dir, db) = temp_db().await;
        let newer = with_extra(migration(9999, "CREATE TABLE added (x INTEGER)"));
        migrate_with(&newer, &db, &dir.path().join("snapshots")).await.unwrap();

        let err = migration_status(&db).await.unwrap_err();
        assert!(
            matches!(err, DbError::Migrate(MigrateError::VersionMissing(9999))),
            "{err}"
        );
    }
}
