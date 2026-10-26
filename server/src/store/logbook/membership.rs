use sqlx::SqliteConnection;

use crate::store::{
    Fault, StoreError, WriteTx,
    access::{LogbookAccess, LogbookRole, OwnerAccess},
    account::user,
    now,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub user_id: String,
    pub callsign: String,
    pub role: LogbookRole,
    pub disabled: bool,
}

pub async fn list(conn: &mut SqliteConnection, access: &LogbookAccess) -> Result<Vec<Member>, StoreError> {
    access.revalidate(conn).await?;
    let rows: Vec<(String, String, String, bool)> = sqlx::query_as(
        "SELECT u.id, u.callsign, m.role, u.disabled_at IS NOT NULL FROM memberships m \
         JOIN users u ON u.id = m.user_id WHERE m.logbook_id = ? ORDER BY u.callsign",
    )
    .bind(access.logbook_id())
    .fetch_all(conn)
    .await?;
    rows.into_iter()
        .map(|(user_id, callsign, role, disabled)| {
            Ok(Member {
                user_id,
                callsign,
                role: LogbookRole::parse(&role)?,
                disabled,
            })
        })
        .collect()
}

pub async fn set(
    tx: &mut WriteTx,
    owner: &OwnerAccess<'_>,
    user_id: &str,
    role: LogbookRole,
) -> Result<(), StoreError> {
    owner.revalidate(tx).await?;
    if !user::is_active(tx, user_id).await? {
        return Err(StoreError::NotFound("user"));
    }
    if role == LogbookRole::Operator {
        keep_another_owner(tx, owner, user_id).await?;
    }
    sqlx::query(
        "INSERT INTO memberships (logbook_id, user_id, role, created_at) VALUES (?, ?, ?, ?) \
         ON CONFLICT (logbook_id, user_id) DO UPDATE SET role = excluded.role",
    )
    .bind(owner.logbook_id())
    .bind(user_id)
    .bind(role.as_str())
    .bind(now())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn remove(tx: &mut WriteTx, owner: &OwnerAccess<'_>, user_id: &str) -> Result<(), StoreError> {
    owner.revalidate(tx).await?;
    keep_another_owner(tx, owner, user_id).await?;
    let removed = sqlx::query("DELETE FROM memberships WHERE logbook_id = ? AND user_id = ?")
        .bind(owner.logbook_id())
        .bind(user_id)
        .execute(&mut **tx)
        .await?;
    match removed.rows_affected() {
        0 => Err(StoreError::NotFound("Membership")),
        _ => Ok(()),
    }
}

async fn keep_another_owner(tx: &mut WriteTx, owner: &OwnerAccess<'_>, user_id: &str) -> Result<(), StoreError> {
    let others: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM memberships m JOIN users u ON u.id = m.user_id \
             WHERE m.logbook_id = ? AND m.role = 'owner' AND m.user_id <> ? AND u.disabled_at IS NULL",
    )
    .bind(owner.logbook_id())
    .bind(user_id)
    .fetch_one(&mut **tx)
    .await?;
    match others {
        0 => Err(StoreError::Conflict(Fault::coded(
            "last_owner",
            "a Logbook keeps at least one active owner",
        ))),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::testing::World;

    #[tokio::test]
    async fn adds_and_changes_members() {
        let world = World::new().await;
        let access = world.access(&world.owner).await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        set(
            &mut tx,
            &access.owner().unwrap(),
            &world.outsider.id,
            LogbookRole::Operator,
        )
        .await
        .unwrap();
        set(
            &mut tx,
            &access.owner().unwrap(),
            &world.operator.id,
            LogbookRole::Owner,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        assert_eq!(world.access(&world.outsider).await.role(), LogbookRole::Operator);
        assert_eq!(world.access(&world.operator).await.role(), LogbookRole::Owner);

        let mut conn = world.db.acquire().await.unwrap();
        let members = list(&mut conn, &world.access(&world.outsider).await).await.unwrap();
        let members: Vec<_> = members
            .iter()
            .map(|m| (m.callsign.as_str(), m.role, m.disabled))
            .collect();
        assert_eq!(
            members,
            [
                ("JA1OPR", LogbookRole::Owner, false),
                ("JA1OUT", LogbookRole::Operator, false),
                ("JA1OWN", LogbookRole::Owner, false),
            ]
        );
    }

    #[tokio::test]
    async fn keeps_the_last_owner() {
        let world = World::new().await;
        let access = world.access(&world.owner).await;
        let owner = access.owner().unwrap();
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        assert!(matches!(
            set(&mut tx, &owner, &world.owner.id, LogbookRole::Operator).await,
            Err(StoreError::Conflict(_))
        ));
        assert!(matches!(
            remove(&mut tx, &owner, &world.owner.id).await,
            Err(StoreError::Conflict(_))
        ));
        remove(&mut tx, &owner, &world.operator.id).await.unwrap();
        assert!(matches!(
            remove(&mut tx, &owner, &world.operator.id).await,
            Err(StoreError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn counts_only_active_owners() {
        let world = World::new().await;
        let access = world.access(&world.owner).await;
        let owner = access.owner().unwrap();
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        set(&mut tx, &owner, &world.operator.id, LogbookRole::Owner)
            .await
            .unwrap();
        sqlx::query("UPDATE users SET disabled_at = ? WHERE id = ?")
            .bind(now())
            .bind(&world.operator.id)
            .execute(&mut *tx)
            .await
            .unwrap();
        assert!(matches!(
            remove(&mut tx, &owner, &world.owner.id).await,
            Err(StoreError::Conflict(_))
        ));
    }
}
