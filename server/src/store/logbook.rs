use sqlx::{AssertSqlSafe, Row, SqliteConnection, sqlite::SqliteRow};

use crate::store::{
    StoreError, WriteTx,
    access::{self, AdminAccess, LogbookAccess, LogbookRole, UserAccess},
    membership, new_id, now, user,
};

const SELECT: &str = "SELECT id, name, last_seq, last_updated_seq, cursor_generation FROM logbooks";

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
    if !user::is_active(tx, owner_id).await? {
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

pub async fn delete(tx: &mut WriteTx, admin: &AdminAccess, logbook_id: &str) -> Result<(), StoreError> {
    admin.revalidate(tx).await?;
    let deleted = sqlx::query("DELETE FROM logbooks WHERE id = ?")
        .bind(logbook_id)
        .execute(&mut **tx)
        .await?;
    match deleted.rows_affected() {
        0 => Err(StoreError::NotFound("Logbook")),
        _ => Ok(()),
    }
}

pub async fn add_owner(
    tx: &mut WriteTx,
    admin: &AdminAccess,
    logbook_id: &str,
    user_id: &str,
    reason: &str,
) -> Result<(), StoreError> {
    let rescue = access::rescue(tx, admin, logbook_id, "add owner", reason).await?;
    membership::set(tx, &rescue.owner()?, user_id, LogbookRole::Owner).await
}

pub async fn list_all(conn: &mut SqliteConnection, admin: &AdminAccess) -> Result<Vec<Logbook>, StoreError> {
    admin.revalidate(conn).await?;
    sqlx::query(AssertSqlSafe(format!("{SELECT} ORDER BY name, id")))
        .fetch_all(conn)
        .await?
        .iter()
        .map(Logbook::from_row)
        .collect()
}

pub async fn list(conn: &mut SqliteConnection, user: &UserAccess) -> Result<Vec<(Logbook, LogbookRole)>, StoreError> {
    let user = user.revalidate(conn).await?;
    sqlx::query(
        "SELECT l.id, l.name, l.last_seq, l.last_updated_seq, l.cursor_generation, m.role \
         FROM logbooks l JOIN memberships m ON m.logbook_id = l.id \
         WHERE m.user_id = ? ORDER BY l.name, l.id",
    )
    .bind(user.user_id())
    .fetch_all(conn)
    .await?
    .iter()
    .map(|row| Ok((Logbook::from_row(row)?, LogbookRole::parse(row.try_get("role")?)?)))
    .collect()
}

pub async fn get(conn: &mut SqliteConnection, access: &LogbookAccess) -> Result<Logbook, StoreError> {
    access.revalidate(conn).await?;
    let row = sqlx::query(AssertSqlSafe(format!("{SELECT} WHERE id = ?")))
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
    use crate::{
        domain::qso::QsoId,
        store::{
            qso::{self, QsoInput},
            testing::{World, contact},
        },
    };

    #[tokio::test]
    async fn deletes_a_logbook_with_everything_it_owns() {
        let world = World::new().await;
        let access = world.access(&world.owner).await;
        let station = world.station("Home").await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        let input = QsoInput {
            station_id: station.id,
            qso: contact("JA1XYZ", "0405"),
        };
        qso::create(&mut tx, &access, &QsoId::generate(), input).await.unwrap();
        let admin = world.admin_access().await;
        access::rescue(&mut tx, &admin, &world.logbook.id, "read", "audit")
            .await
            .unwrap();

        delete(&mut tx, &admin, &world.logbook.id).await.unwrap();
        for table in ["memberships", "stations", "qsos", "rescue_operations"] {
            let count: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
                .fetch_one(&mut *tx)
                .await
                .unwrap();
            assert_eq!(count, 0, "{table}");
        }
        assert!(matches!(
            delete(&mut tx, &admin, &world.logbook.id).await,
            Err(StoreError::NotFound("Logbook"))
        ));
    }

    #[tokio::test]
    async fn adds_an_owner_as_a_recorded_rescue() {
        let world = World::new().await;
        let admin = world.admin_access().await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        assert!(matches!(
            add_owner(&mut tx, &admin, &world.logbook.id, &world.outsider.id, " ").await,
            Err(StoreError::Invalid(_))
        ));
        add_owner(&mut tx, &admin, &world.logbook.id, &world.outsider.id, "the owner left")
            .await
            .unwrap();
        let roles: Vec<(String, String)> =
            sqlx::query_as("SELECT user_id, role FROM memberships WHERE role = 'owner' ORDER BY user_id")
                .fetch_all(&mut *tx)
                .await
                .unwrap();
        let mut expected = vec![
            (world.owner.id.clone(), "owner".to_owned()),
            (world.outsider.id.clone(), "owner".to_owned()),
        ];
        expected.sort();
        assert_eq!(roles, expected);
        let (action, reason): (String, String) = sqlx::query_as("SELECT action, reason FROM rescue_operations")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!((action.as_str(), reason.as_str()), ("add owner", "the owner left"));
    }

    #[tokio::test]
    async fn lists_logbooks_for_admins_and_members() {
        let world = World::new().await;
        let admin = world.admin_access().await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        let other = create(&mut tx, &admin, "Alpha", &world.operator.id).await.unwrap();
        let names: Vec<_> = list_all(&mut tx, &admin)
            .await
            .unwrap()
            .into_iter()
            .map(|l| l.name)
            .collect();
        assert_eq!(names, ["Alpha", "Club"]);

        let operator = access::user(&mut tx, &access::Principal::user(&world.operator.id))
            .await
            .unwrap();
        assert_eq!(
            list(&mut tx, &operator).await.unwrap(),
            [
                (other, LogbookRole::Owner),
                (world.logbook.clone(), LogbookRole::Operator)
            ]
        );
        let outsider = access::user(&mut tx, &access::Principal::user(&world.outsider.id))
            .await
            .unwrap();
        assert!(list(&mut tx, &outsider).await.unwrap().is_empty());
    }

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
