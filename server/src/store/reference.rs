use sqlx::{Row, SqliteConnection, sqlite::SqliteRow};

use crate::store::{
    StoreError, WriteTx,
    access::{AdminAccess, UserAccess},
    now,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
            _ => Err(StoreError::Invalid(format!("unknown reference data {s:?}"))),
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
    sqlx::query("SELECT kind, version, checksum, updated_at FROM reference_versions ORDER BY kind")
        .fetch_all(conn)
        .await?
        .iter()
        .map(ReferenceVersion::from_row)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{access, testing::World};

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
