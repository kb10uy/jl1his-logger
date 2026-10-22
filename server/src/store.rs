pub mod access;
pub mod logbook;
pub mod membership;
pub mod station;
pub mod user;

use std::ops::{Deref, DerefMut};

use chrono::{SecondsFormat, Utc};
use sqlx::{Sqlite, SqliteConnection, Transaction};
use uuid::Uuid;

use crate::{db::Db, domain::qso::QsoError};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("{0} not found")]
    NotFound(&'static str),
    #[error("{0}")]
    Forbidden(&'static str),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Qso(#[from] QsoError),
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

pub struct WriteTx(Transaction<'static, Sqlite>);

impl WriteTx {
    pub async fn begin(db: &Db) -> Result<WriteTx, StoreError> {
        Ok(WriteTx(db.begin_with("BEGIN IMMEDIATE").await?))
    }

    pub async fn commit(self) -> Result<(), StoreError> {
        Ok(self.0.commit().await?)
    }
}

impl Deref for WriteTx {
    type Target = SqliteConnection;

    fn deref(&self) -> &SqliteConnection {
        &self.0
    }
}

impl DerefMut for WriteTx {
    fn deref_mut(&mut self) -> &mut SqliteConnection {
        &mut self.0
    }
}

pub(crate) fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

pub(crate) fn new_id() -> String {
    Uuid::now_v7().to_string()
}

pub(crate) fn is_unique_violation(e: &sqlx::Error) -> bool {
    e.as_database_error().is_some_and(|e| e.is_unique_violation())
}

#[cfg(test)]
pub(crate) mod testing {
    use tempfile::TempDir;

    use super::{
        WriteTx,
        access::{self, AdminAccess, LogbookAccess, LogbookRole, Principal},
        logbook::{self, Logbook},
        membership,
        user::{self, Role, User},
    };
    use crate::db::{self, Db};

    pub struct World {
        pub _dir: TempDir,
        pub db: Db,
        pub admin: User,
        pub owner: User,
        pub operator: User,
        pub outsider: User,
        pub logbook: Logbook,
    }

    impl World {
        pub async fn new() -> World {
            let dir = tempfile::tempdir().unwrap();
            let db = db::open(&dir.path().join("test.db")).await.unwrap();
            db::migrate(&db, &dir.path().join("snapshots")).await.unwrap();

            let mut tx = WriteTx::begin(&db).await.unwrap();
            let admin = user::create_initial_admin(&mut tx, "JL1HIS").await.unwrap();
            let admin_access = access::admin(&mut tx, &Principal::user(&admin.id)).await.unwrap();
            let owner = user::create(&mut tx, &admin_access, "JA1OWN", Role::User)
                .await
                .unwrap();
            let operator = user::create(&mut tx, &admin_access, "JA1OPR", Role::User)
                .await
                .unwrap();
            let outsider = user::create(&mut tx, &admin_access, "JA1OUT", Role::User)
                .await
                .unwrap();
            let logbook = logbook::create(&mut tx, &admin_access, "Club", &owner.id)
                .await
                .unwrap();
            let owner_access = access::logbook(&mut tx, &Principal::user(&owner.id), &logbook.id)
                .await
                .unwrap();
            membership::set(
                &mut tx,
                &owner_access.owner().unwrap(),
                &operator.id,
                LogbookRole::Operator,
            )
            .await
            .unwrap();
            tx.commit().await.unwrap();

            World {
                _dir: dir,
                db,
                admin,
                owner,
                operator,
                outsider,
                logbook,
            }
        }

        pub async fn admin_access(&self) -> AdminAccess {
            let mut conn = self.db.acquire().await.unwrap();
            access::admin(&mut conn, &Principal::user(&self.admin.id))
                .await
                .unwrap()
        }

        pub async fn access(&self, user: &User) -> LogbookAccess {
            let mut conn = self.db.acquire().await.unwrap();
            access::logbook(&mut conn, &Principal::user(&user.id), &self.logbook.id)
                .await
                .unwrap()
        }
    }
}
