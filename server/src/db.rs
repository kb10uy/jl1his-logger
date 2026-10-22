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
    async fn upgrades_the_initial_schema_without_losing_export_rows() {
        let (dir, db) = temp_db().await;
        let initial = Migrator::with_migrations(vec![MIGRATOR.iter().next().unwrap().clone()]);
        initial.run(&db).await.unwrap();
        sqlx::raw_sql(
            "INSERT INTO users (id, callsign, role, created_at, updated_at)
             VALUES ('user', 'JL1HIS', 'admin', '2026-10-22T00:00:00Z', '2026-10-22T00:00:00Z');
             INSERT INTO logbooks (id, name, cursor_generation, created_at, updated_at)
             VALUES ('log', 'Club', 'generation', '2026-10-22T00:00:00Z', '2026-10-22T00:00:00Z');
             INSERT INTO stations (id, logbook_id, name, callsign, created_at, updated_at)
             VALUES ('station', 'log', 'Home', 'JL1HIS', '2026-10-22T00:00:00Z', '2026-10-22T00:00:00Z');
             INSERT INTO qsos (
                 id, logbook_id, station_id, seq, updated_seq, revision, created_by, created_at,
                 updated_by, updated_at, call, qso_date, time_on, band, mode, station_callsign, call_base, qso_time
             ) VALUES (
                 'qso', 'log', 'station', 1, 1, 3, 'user', '2026-10-22T00:00:00Z',
                 'user', '2026-10-22T00:00:00Z', 'JA1XYZ', '20261022', '0000', '40m', 'CW', 'JL1HIS', 'JA1XYZ', 1792627200
             );
             INSERT INTO export_batches (id, logbook_id, kind, created_by, created_at, applied_at)
             VALUES ('batch', 'log', 'eqsl', 'user', '2026-10-22T00:00:00Z', '2026-10-22T01:00:00Z');
             INSERT INTO export_batch_qsos (logbook_id, batch_id, qso_id, revision)
             VALUES ('log', 'batch', 'qso', 2);
             INSERT INTO reference_versions (kind, version, checksum, updated_at)
             VALUES ('cty', '20261022', X'abcd', '2026-10-22T00:00:00Z');",
        )
        .execute(&db)
        .await
        .unwrap();

        let migrated = migrate(&db, &dir.path().join("snapshots")).await.unwrap();
        assert_eq!(migrated.applied, [2]);
        assert!(migrated.snapshot.unwrap().is_file());
        let batch: (String, String, String, String, String, Option<String>) =
            sqlx::query_as("SELECT id, logbook_id, kind, created_by, created_at, applied_at FROM export_batches")
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(
            batch,
            (
                "batch".into(),
                "log".into(),
                "eqsl".into(),
                "user".into(),
                "2026-10-22T00:00:00Z".into(),
                Some("2026-10-22T01:00:00Z".into())
            )
        );
        let target: (String, String, String, i64) =
            sqlx::query_as("SELECT logbook_id, batch_id, qso_id, revision FROM export_batch_qsos")
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(target, ("log".into(), "batch".into(), "qso".into(), 2));
        let reference: (String, String, Vec<u8>, String) =
            sqlx::query_as("SELECT kind, version, checksum, updated_at FROM reference_versions")
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(
            reference,
            (
                "cty".into(),
                "20261022".into(),
                vec![0xab, 0xcd],
                "2026-10-22T00:00:00Z".into()
            )
        );
        assert!(
            sqlx::query("PRAGMA foreign_key_check")
                .fetch_all(&db)
                .await
                .unwrap()
                .is_empty()
        );
        for sql in [
            "UPDATE export_batches SET kind = 'unknown' WHERE id = 'batch'",
            "UPDATE reference_versions SET kind = 'unknown' WHERE kind = 'cty'",
            "UPDATE export_batch_qsos SET batch_id = 'missing' WHERE batch_id = 'batch'",
        ] {
            assert!(sqlx::query(sql).execute(&db).await.is_err(), "{sql}");
        }
        sqlx::query("DELETE FROM export_batches WHERE id = 'batch'")
            .execute(&db)
            .await
            .unwrap();
        let remaining: i64 = sqlx::query_scalar("SELECT count(*) FROM export_batch_qsos")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(remaining, 0);
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
