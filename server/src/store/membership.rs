use crate::store::{
    StoreError, WriteTx,
    access::{LogbookRole, OwnerAccess},
    now,
};

pub async fn set(
    tx: &mut WriteTx,
    owner: &OwnerAccess<'_>,
    user_id: &str,
    role: LogbookRole,
) -> Result<(), StoreError> {
    let user_exists: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users WHERE id = ? AND disabled_at IS NULL)")
            .bind(user_id)
            .fetch_one(&mut **tx)
            .await?;
    if !user_exists {
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
    let others: i64 =
        sqlx::query_scalar("SELECT count(*) FROM memberships WHERE logbook_id = ? AND role = 'owner' AND user_id <> ?")
            .bind(owner.logbook_id())
            .bind(user_id)
            .fetch_one(&mut **tx)
            .await?;
    match others {
        0 => Err(StoreError::Conflict("a Logbook keeps at least one owner".into())),
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
}
