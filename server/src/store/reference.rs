use std::{
    io::ErrorKind,
    path::{Path, PathBuf},
};

use chrono::Utc;
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{AssertSqlSafe, Row, SqliteConnection, sqlite::SqliteRow};
use tokio::fs;

use crate::{
    db::Db,
    domain::reference::{self as data, ReferenceError},
    store::{
        Fault, StoreError, WriteTx,
        access::{AdminAccess, UserAccess},
        new_id, now,
    },
};

const SELECT: &str = "SELECT kind, version, checksum, updated_at FROM reference_versions";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceKind {
    Cty,
    Subdivisions,
    Parks,
}

impl ReferenceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ReferenceKind::Cty => "cty",
            ReferenceKind::Subdivisions => "subdivisions",
            ReferenceKind::Parks => "parks",
        }
    }

    fn parse(s: &str) -> Result<ReferenceKind, StoreError> {
        match s {
            "cty" => Ok(ReferenceKind::Cty),
            "subdivisions" => Ok(ReferenceKind::Subdivisions),
            "parks" => Ok(ReferenceKind::Parks),
            _ => Err(StoreError::Invalid(format!("unknown reference data {s:?}").into())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceVersion {
    pub kind: ReferenceKind,
    pub version: String,
    pub checksum: Option<Vec<u8>>,
    pub updated_at: String,
}

impl ReferenceVersion {
    fn from_row(row: &SqliteRow) -> Result<ReferenceVersion, StoreError> {
        Ok(ReferenceVersion {
            kind: ReferenceKind::parse(row.try_get("kind")?)?,
            version: row.try_get("version")?,
            checksum: row.try_get("checksum")?,
            updated_at: row.try_get("updated_at")?,
        })
    }
}

pub async fn set(
    tx: &mut WriteTx,
    admin: &AdminAccess,
    kind: ReferenceKind,
    version: &str,
    checksum: Option<&[u8]>,
) -> Result<ReferenceVersion, StoreError> {
    admin.revalidate(tx).await?;
    let version = version.trim();
    if version.is_empty() {
        return Err(StoreError::Invalid("a reference data version cannot be empty".into()));
    }
    let row = sqlx::query(
        "INSERT INTO reference_versions (kind, version, checksum, updated_at) VALUES (?, ?, ?, ?) \
         ON CONFLICT (kind) DO UPDATE SET \
         version = excluded.version, checksum = excluded.checksum, updated_at = excluded.updated_at \
         RETURNING kind, version, checksum, updated_at",
    )
    .bind(kind.as_str())
    .bind(version)
    .bind(checksum)
    .bind(now())
    .fetch_one(&mut **tx)
    .await?;
    ReferenceVersion::from_row(&row)
}

pub async fn list(conn: &mut SqliteConnection, user: &UserAccess) -> Result<Vec<ReferenceVersion>, StoreError> {
    user.revalidate(conn).await?;
    sqlx::query(AssertSqlSafe(format!("{SELECT} ORDER BY kind")))
        .fetch_all(conn)
        .await?
        .iter()
        .map(ReferenceVersion::from_row)
        .collect()
}

pub async fn current(conn: &mut SqliteConnection, kind: ReferenceKind) -> Result<Option<ReferenceVersion>, StoreError> {
    sqlx::query(AssertSqlSafe(format!("{SELECT} WHERE kind = ?")))
        .bind(kind.as_str())
        .fetch_optional(conn)
        .await?
        .as_ref()
        .map(ReferenceVersion::from_row)
        .transpose()
}

#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("{0}")]
    File(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    pub version: ReferenceVersion,
    pub entries: usize,
}

fn check(kind: ReferenceKind, bytes: &[u8]) -> Result<(Option<String>, usize), ReferenceError> {
    match kind {
        ReferenceKind::Cty => {
            let cty = data::parse_cty_archive(bytes)?;
            Ok((cty.version, cty.database.len()))
        }
        ReferenceKind::Subdivisions => {
            let subdivisions = data::parse_subdivisions(data::text(bytes)?)?;
            Ok((None, subdivisions.values().map(|names| names.len()).sum()))
        }
        ReferenceKind::Parks => Ok((None, data::parse_parks(data::text(bytes)?)?.len())),
    }
}

pub async fn install(
    db: &Db,
    admin: &AdminAccess,
    kind: ReferenceKind,
    path: &Path,
    bytes: &[u8],
    version: Option<&str>,
) -> Result<Installed, InstallError> {
    let (detected, entries) =
        check(kind, bytes).map_err(|e| StoreError::Invalid(Fault::coded("reference_invalid", e.to_string())))?;
    let version = version
        .map(str::to_owned)
        .or(detected)
        .unwrap_or_else(|| Utc::now().format("%Y-%m-%d").to_string());
    let (db, admin, path, bytes) = (db.clone(), admin.clone(), path.to_owned(), bytes.to_vec());
    let installed = tokio::spawn(async move { swap(&db, &admin, kind, &path, &bytes, &version).await })
        .await
        .map_err(|e| InstallError::File(format!("installing {} stopped: {e}", kind.as_str())))??;
    Ok(Installed {
        version: installed,
        entries,
    })
}

async fn swap(
    db: &Db,
    admin: &AdminAccess,
    kind: ReferenceKind,
    path: &Path,
    bytes: &[u8],
    version: &str,
) -> Result<ReferenceVersion, InstallError> {
    let mut tx = WriteTx::begin(db).await?;
    let checksum = Sha256::digest(bytes);
    let installed = set(&mut tx, admin, kind, version, Some(checksum.as_slice())).await?;
    let previous = match fs::read(path).await {
        Ok(bytes) => Some(bytes),
        Err(e) if e.kind() == ErrorKind::NotFound => None,
        Err(e) => return Err(file_error(path, e)),
    };
    replace(path, bytes).await?;
    if let Err(error) = tx.commit().await {
        let restored = match previous {
            Some(bytes) => replace(path, &bytes).await,
            None => fs::remove_file(path).await.map_err(|e| file_error(path, e)),
        };
        if let Err(restore_error) = restored {
            return Err(InstallError::File(format!(
                "{error}; restoring {} failed: {restore_error}",
                path.display()
            )));
        }
        return Err(error.into());
    }
    Ok(installed)
}

fn file_error(path: &Path, error: std::io::Error) -> InstallError {
    InstallError::File(format!("{}: {error}", path.display()))
}

async fn replace(path: &Path, bytes: &[u8]) -> Result<(), InstallError> {
    if let Some(dir) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
        fs::create_dir_all(dir).await.map_err(|e| file_error(dir, e))?;
    }
    let mut staging = path.as_os_str().to_owned();
    staging.push(format!(".{}.installing", new_id()));
    let staging = PathBuf::from(staging);
    let written = match fs::write(&staging, bytes).await {
        Ok(()) => fs::rename(&staging, path).await.map_err(|e| file_error(path, e)),
        Err(e) => Err(file_error(&staging, e)),
    };
    if written.is_err() {
        let _ = fs::remove_file(&staging).await;
    }
    written
}

#[cfg(test)]
mod tests {
    use std::fs as sync_fs;

    use rstest::rstest;
    use tempfile::TempDir;

    use super::*;
    use crate::{
        domain::reference::tests::{CTY, CTY_CSV, cty_archive},
        store::{access, testing::World},
    };

    fn leftovers(dir: &Path) -> Vec<String> {
        sync_fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".installing"))
            .collect()
    }

    async fn current_cty(world: &World) -> Option<ReferenceVersion> {
        let mut conn = world.db.acquire().await.unwrap();
        current(&mut conn, ReferenceKind::Cty).await.unwrap()
    }

    #[tokio::test]
    async fn installs_a_checked_archive() {
        let world = World::new().await;
        let admin = world.admin_access().await;
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("reference").join("cty.zip");
        let archive = cty_archive(CTY, CTY_CSV);

        let installed = install(&world.db, &admin, ReferenceKind::Cty, &path, &archive, None)
            .await
            .unwrap();
        assert_eq!(installed.entries, 2);
        assert_eq!(
            (installed.version.kind, installed.version.version.as_str()),
            (ReferenceKind::Cty, "20260915")
        );
        assert_eq!(
            installed.version.checksum.as_deref(),
            Some(Sha256::digest(&archive).as_slice())
        );
        assert_eq!(sync_fs::read(&path).unwrap(), archive);

        let updated = cty_archive(&CTY.replace("Japan:", "Nippon:"), CTY_CSV);
        let installed = install(&world.db, &admin, ReferenceKind::Cty, &path, &updated, Some(" 2 "))
            .await
            .unwrap();
        assert_eq!(installed.version.version, "2");
        assert_eq!(sync_fs::read(&path).unwrap(), updated);
        assert_eq!(current_cty(&world).await, Some(installed.version));
        assert!(leftovers(path.parent().unwrap()).is_empty());
    }

    #[rstest]
    #[case::invalid_archive("archive")]
    #[case::invalid_version("version")]
    #[case::database_commit("commit")]
    #[tokio::test]
    async fn preserves_reference_data_on_failed_install(#[case] failure: &str, #[values(false, true)] existing: bool) {
        let world = World::new().await;
        let admin = world.admin_access().await;
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("cty.zip");
        let archive = cty_archive(CTY, CTY_CSV);
        if existing {
            install(&world.db, &admin, ReferenceKind::Cty, &path, &archive, None)
                .await
                .unwrap();
        }
        let previous = current_cty(&world).await;
        let mut update = cty_archive(&CTY.replace("Japan:", "Nippon:"), CTY_CSV);
        let mut version = "new";
        match failure {
            "archive" => update = cty_archive(CTY, "JA,Japan,339,AS,25,45,36.40,-138.38,-9.0,JA;\n"),
            "version" => version = " ",
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
        let error = install(&world.db, &admin, ReferenceKind::Cty, &path, &update, Some(version))
            .await
            .unwrap_err()
            .to_string();
        match failure {
            "archive" => assert!(error.contains("cty.csv has no row for JD/o"), "{error}"),
            "version" => assert!(error.contains("version cannot be empty"), "{error}"),
            "commit" => assert!(error.contains("FOREIGN KEY constraint failed"), "{error}"),
            _ => unreachable!(),
        }
        match existing {
            true => assert_eq!(sync_fs::read(&path).unwrap(), archive),
            false => assert!(!path.exists()),
        }
        assert_eq!(current_cty(&world).await, previous);
        assert!(leftovers(dir.path()).is_empty());
    }

    #[tokio::test]
    async fn leaves_nothing_behind_when_the_file_cannot_be_written() {
        let world = World::new().await;
        let admin = world.admin_access().await;
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("cty.zip");
        sync_fs::create_dir(&path).unwrap();
        sync_fs::write(path.join("kept"), "kept").unwrap();

        let archive = cty_archive(CTY, CTY_CSV);
        let error = install(&world.db, &admin, ReferenceKind::Cty, &path, &archive, None)
            .await
            .unwrap_err();
        assert!(matches!(error, InstallError::File(_)), "{error}");
        assert_eq!(sync_fs::read_to_string(path.join("kept")).unwrap(), "kept");
        assert_eq!(current_cty(&world).await, None);
        assert!(leftovers(dir.path()).is_empty());
    }

    #[tokio::test]
    async fn gives_concurrent_installs_their_own_staging_files() {
        let world = World::new().await;
        let admin = world.admin_access().await;
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("cty.zip");
        let first = cty_archive(CTY, CTY_CSV);
        let second = cty_archive(&CTY.replace("Japan:", "Nippon:"), CTY_CSV);
        let (a, b) = tokio::join!(
            install(&world.db, &admin, ReferenceKind::Cty, &path, &first, Some("1")),
            install(&world.db, &admin, ReferenceKind::Cty, &path, &second, Some("2")),
        );
        let (a, b) = (a.unwrap(), b.unwrap());
        let installed = current_cty(&world).await.unwrap();
        let bytes = sync_fs::read(&path).unwrap();
        assert_eq!(installed.checksum.as_deref(), Some(Sha256::digest(&bytes).as_slice()));
        assert!(installed == a.version || installed == b.version);
        assert!(leftovers(dir.path()).is_empty());
    }

    #[tokio::test]
    async fn finishes_an_install_whose_caller_gives_up_after_the_swap() {
        let world = World::new().await;
        let admin = world.admin_access().await;
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("cty.zip");
        let first = cty_archive(CTY, CTY_CSV);
        install(&world.db, &admin, ReferenceKind::Cty, &path, &first, Some("1"))
            .await
            .unwrap();

        let second = cty_archive(&CTY.replace("Japan:", "Nippon:"), CTY_CSV);
        let swapped = async {
            while sync_fs::read(&path).ok().as_deref() != Some(second.as_slice()) {
                tokio::task::yield_now().await;
            }
        };
        tokio::select! {
            biased;
            _ = swapped => {}
            _ = install(&world.db, &admin, ReferenceKind::Cty, &path, &second, Some("2")) => {
                panic!("the install finished before the swap was seen")
            }
        }

        let committed = async {
            loop {
                if let Some(installed) = current_cty(&world).await.filter(|installed| installed.version == "2") {
                    return installed;
                }
                tokio::task::yield_now().await;
            }
        };
        let installed = tokio::time::timeout(std::time::Duration::from_secs(10), committed)
            .await
            .expect("the abandoned install was rolled back");
        let bytes = sync_fs::read(&path).unwrap();
        assert_eq!(bytes, second);
        assert_eq!(installed.checksum.as_deref(), Some(Sha256::digest(&bytes).as_slice()));
        assert!(leftovers(dir.path()).is_empty());
    }

    #[tokio::test]
    async fn installs_toml_tables_and_refuses_other_text() {
        let world = World::new().await;
        let admin = world.admin_access().await;
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("parks.toml");
        let parks = "JP-0001 = { name_en = \"Rishiri\" }\n";
        let installed = install(&world.db, &admin, ReferenceKind::Parks, &path, parks.as_bytes(), None)
            .await
            .unwrap();
        assert_eq!(installed.entries, 1);
        assert_eq!(sync_fs::read_to_string(&path).unwrap(), parks);
        let error = install(&world.db, &admin, ReferenceKind::Parks, &path, b"\xff", None)
            .await
            .unwrap_err();
        assert!(matches!(error, InstallError::Store(StoreError::Invalid(_))), "{error}");
    }

    #[tokio::test]
    async fn records_versions_for_signed_in_users_to_read() {
        let world = World::new().await;
        let admin = world.admin_access().await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        set(&mut tx, &admin, ReferenceKind::Cty, "2026-10-01", None)
            .await
            .unwrap();
        let cty = set(&mut tx, &admin, ReferenceKind::Cty, " 2026-10-15 ", Some(&[1, 2]))
            .await
            .unwrap();
        set(&mut tx, &admin, ReferenceKind::Parks, "2026-09", None)
            .await
            .unwrap();
        assert!(matches!(
            set(&mut tx, &admin, ReferenceKind::Subdivisions, " ", None).await,
            Err(StoreError::Invalid(_))
        ));
        let not_admin = access::admin(&mut tx, &access::Principal::user(&world.operator.id)).await;
        assert!(matches!(not_admin, Err(StoreError::Forbidden(_))));

        let user = world.user_access(&world.operator).await;
        let versions = list(&mut tx, &user).await.unwrap();
        assert_eq!(versions.len(), 2);
        assert_eq!(versions[0], cty);
        assert_eq!(
            (versions[0].version.as_str(), versions[0].checksum.as_deref()),
            ("2026-10-15", Some(&[1u8, 2][..]))
        );
        assert_eq!(versions[1].kind, ReferenceKind::Parks);
    }
}
