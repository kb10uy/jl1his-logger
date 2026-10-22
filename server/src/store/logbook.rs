use sqlx::{Row, SqliteConnection, sqlite::SqliteRow};

use crate::store::{
    StoreError, WriteTx,
    access::{AdminAccess, LogbookAccess, LogbookRole},
    new_id, now,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Logbook {
    pub id: String,
    pub name: String,
    pub last_seq: i64,
    pub last_updated_seq: i64,
    pub cursor_generation: String,
}

impl Logbook {
    fn from_row(row: &SqliteRow) -> Result<Logbook, StoreError> {
        Ok(Logbook {
            id: row.try_get("id")?,
            name: row.try_get("name")?,
            last_seq: row.try_get("last_seq")?,
            last_updated_seq: row.try_get("last_updated_seq")?,
            cursor_generation: row.try_get("cursor_generation")?,
        })
    }
}

pub async fn create(tx: &mut WriteTx, admin: &AdminAccess, name: &str, owner_id: &str) -> Result<Logbook, StoreError> {
    admin.revalidate(tx).await?;
    let name = name.trim();
    if name.is_empty() {
        return Err(StoreError::Invalid("a Logbook needs a name".into()));
    }
    let owner_exists: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users WHERE id = ? AND disabled_at IS NULL)")
            .bind(owner_id)
            .fetch_one(&mut **tx)
            .await?;
    if !owner_exists {
        return Err(StoreError::NotFound("user"));
    }

    let logbook = Logbook {
        id: new_id(),
        name: name.to_owned(),
        last_seq: 0,
        last_updated_seq: 0,
        cursor_generation: new_id(),
    };
    let now = now();
    sqlx::query("INSERT INTO logbooks (id, name, cursor_generation, created_at, updated_at) VALUES (?, ?, ?, ?, ?)")
        .bind(&logbook.id)
        .bind(&logbook.name)
        .bind(&logbook.cursor_generation)
        .bind(&now)
        .bind(&now)
        .execute(&mut **tx)
        .await?;
    sqlx::query("INSERT INTO memberships (logbook_id, user_id, role, created_at) VALUES (?, ?, ?, ?)")
        .bind(&logbook.id)
        .bind(owner_id)
        .bind(LogbookRole::Owner.as_str())
        .bind(&now)
        .execute(&mut **tx)
        .await?;
    Ok(logbook)
}

pub async fn get(conn: &mut SqliteConnection, access: &LogbookAccess) -> Result<Logbook, StoreError> {
    access.revalidate(conn).await?;
    let row = sqlx::query("SELECT id, name, last_seq, last_updated_seq, cursor_generation FROM logbooks WHERE id = ?")
        .bind(access.logbook_id())
        .fetch_optional(conn)
        .await?
        .ok_or(StoreError::NotFound("Logbook"))?;
    Logbook::from_row(&row)
}

pub(crate) struct Sequence {
    pub seq: i64,
    pub updated_seq: i64,
}

pub(crate) async fn next_insert(tx: &mut WriteTx, logbook_id: &str) -> Result<Sequence, StoreError> {
    let (seq, updated_seq) = sqlx::query_as(
        "UPDATE logbooks SET last_seq = last_seq + 1, last_updated_seq = last_updated_seq + 1 \
         WHERE id = ? RETURNING last_seq, last_updated_seq",
    )
    .bind(logbook_id)
    .fetch_one(&mut **tx)
    .await?;
    Ok(Sequence { seq, updated_seq })
}

pub(crate) async fn next_update(tx: &mut WriteTx, logbook_id: &str) -> Result<i64, StoreError> {
    Ok(sqlx::query_scalar(
        "UPDATE logbooks SET last_updated_seq = last_updated_seq + 1 WHERE id = ? RETURNING last_updated_seq",
    )
    .bind(logbook_id)
    .fetch_one(&mut **tx)
    .await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{access, testing::World};

    #[tokio::test]
    async fn makes_the_creator_given_owner_an_owner() {
        let world = World::new().await;
        let access = world.access(&world.owner).await;
        let mut conn = world.db.acquire().await.unwrap();
        let logbook = get(&mut conn, &access).await.unwrap();
        assert_eq!(logbook, world.logbook);
        assert_eq!((logbook.name.as_str(), logbook.last_seq), ("Club", 0));
        assert_eq!(access.role(), LogbookRole::Owner);
    }

    #[tokio::test]
    async fn refuses_a_blank_name_or_an_unknown_owner() {
        let world = World::new().await;
        let admin = world.admin_access().await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        assert!(matches!(
            create(&mut tx, &admin, " ", &world.owner.id).await,
            Err(StoreError::Invalid(_))
        ));
        assert!(matches!(
            create(&mut tx, &admin, "Other", "no-such-user").await,
            Err(StoreError::NotFound("user"))
        ));
    }

    #[tokio::test]
    async fn hands_out_sequences_that_only_grow() {
        let world = World::new().await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        let first = next_insert(&mut tx, &world.logbook.id).await.unwrap();
        let updated = next_update(&mut tx, &world.logbook.id).await.unwrap();
        let second = next_insert(&mut tx, &world.logbook.id).await.unwrap();
        assert_eq!((first.seq, first.updated_seq), (1, 1));
        assert_eq!(updated, 2);
        assert_eq!((second.seq, second.updated_seq), (2, 3));

        let other = {
            let admin = access::admin(&mut tx, &access::Principal::user(&world.admin.id))
                .await
                .unwrap();
            create(&mut tx, &admin, "Other", &world.owner.id).await.unwrap()
        };
        let separate = next_insert(&mut tx, &other.id).await.unwrap();
        assert_eq!((separate.seq, separate.updated_seq), (1, 1));
    }
}
