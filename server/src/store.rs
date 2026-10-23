pub mod access;
pub mod export;
pub mod import;
pub mod logbook;
pub mod membership;
pub mod passkey;
pub mod qsl;
pub mod qso;
pub mod reference;
pub mod registration;
pub mod session;
pub mod settings;
pub mod station;
pub mod token;
pub mod user;

use std::{
    ops::{Deref, DerefMut},
    sync::Arc,
};

use sha2::{Digest, Sha256};
use sqlx::{Sqlite, SqliteConnection, Transaction};

use crate::{db::Db, domain::qso::QsoError};

pub(crate) use crate::db::{new_id, now, timestamp, today};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("{0} not found")]
    NotFound(&'static str),
    #[error("not signed in")]
    Unauthenticated,
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

pub struct WriteTx {
    transaction: Transaction<'static, Sqlite>,
    scope: Arc<()>,
}

impl WriteTx {
    pub async fn begin(db: &Db) -> Result<WriteTx, StoreError> {
        Ok(WriteTx {
            transaction: db.begin_with("BEGIN IMMEDIATE").await?,
            scope: Arc::new(()),
        })
    }

    pub async fn commit(self) -> Result<(), StoreError> {
        let WriteTx { transaction, scope } = self;
        drop(scope);
        Ok(transaction.commit().await?)
    }
}

impl Deref for WriteTx {
    type Target = SqliteConnection;

    fn deref(&self) -> &SqliteConnection {
        &self.transaction
    }
}

impl DerefMut for WriteTx {
    fn deref_mut(&mut self) -> &mut SqliteConnection {
        &mut self.transaction
    }
}

pub(crate) fn random_token() -> String {
    let mut bytes = [0u8; 32];
    rand::fill(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub(crate) fn hash_token(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

pub(crate) const MAX_TEXT_BYTES: usize = 2048;

pub(crate) fn name<'a>(what: &str, value: &'a str) -> Result<&'a str, StoreError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(StoreError::Invalid(format!("{what} needs a name")));
    }
    if value.len() > MAX_TEXT_BYTES {
        return Err(StoreError::Invalid(format!(
            "the name of {what} is {} bytes long; at most {MAX_TEXT_BYTES} are kept",
            value.len()
        )));
    }
    Ok(value)
}

pub(crate) fn is_unique_violation(e: &sqlx::Error) -> bool {
    e.as_database_error().is_some_and(|e| e.is_unique_violation())
}

#[cfg(test)]
pub(crate) mod testing {
    use std::collections::BTreeMap;

    use tempfile::TempDir;

    use super::{
        WriteTx,
        access::{self, AdminAccess, LogbookAccess, LogbookRole, Principal, UserAccess},
        logbook::{self, Logbook},
        membership,
        station::{self, Station, StationInput},
        user::{self, Role, User},
    };
    use crate::{
        db::{self, Db},
        domain::qso::Qso,
    };

    pub fn contact(call: &str, time_on: &str) -> Qso {
        Qso::from_fields([
            ("CALL", call),
            ("QSO_DATE", "20261022"),
            ("TIME_ON", time_on),
            ("BAND", "40m"),
            ("MODE", "CW"),
        ])
        .unwrap()
    }

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

        pub async fn user_access(&self, user: &User) -> UserAccess {
            let mut conn = self.db.acquire().await.unwrap();
            access::user(&mut conn, &Principal::user(&user.id)).await.unwrap()
        }

        pub async fn access(&self, user: &User) -> LogbookAccess {
            let mut conn = self.db.acquire().await.unwrap();
            access::logbook(&mut conn, &Principal::user(&user.id), &self.logbook.id)
                .await
                .unwrap()
        }

        pub async fn station(&self, name: &str) -> Station {
            let access = self.access(&self.owner).await;
            let mut tx = WriteTx::begin(&self.db).await.unwrap();
            let station = station::create(&mut tx, &access.owner().unwrap(), &station_input(name))
                .await
                .unwrap();
            tx.commit().await.unwrap();
            station
        }

        pub async fn other_logbook(&self) -> (Logbook, LogbookAccess) {
            let admin = self.admin_access().await;
            let mut tx = WriteTx::begin(&self.db).await.unwrap();
            let other = logbook::create(&mut tx, &admin, "Other", &self.owner.id).await.unwrap();
            let access = access::logbook(&mut tx, &Principal::user(&self.owner.id), &other.id)
                .await
                .unwrap();
            tx.commit().await.unwrap();
            (other, access)
        }
    }

    pub fn station_input(name: &str) -> StationInput {
        StationInput {
            name: name.into(),
            callsign: "jl1his/1".into(),
            wavelog_id: None,
            fields: BTreeMap::from([
                ("my_city".to_owned(), "Minato".to_owned()),
                ("MY_CNTY".to_owned(), "100103".to_owned()),
                ("MY_RIG".to_owned(), String::new()),
            ]),
        }
    }
}
