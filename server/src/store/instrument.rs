use std::collections::BTreeMap;

use sqlx::{AssertSqlSafe, Row, SqliteConnection, sqlite::SqliteRow};

use crate::{
    domain::qso::{field_value, tx_power},
    store::{StoreError, WriteTx, access::LogbookAccess, name as store_name, new_id, now, station::defaults},
};

const NOT_DEFAULTS: &[&str] = &[
    "CALL",
    "QSO_DATE",
    "TIME_ON",
    "BAND",
    "MODE",
    "STATION_CALLSIGN",
    "MY_RIG",
    "MY_ANTENNA",
    "TX_PWR",
];

const SELECT: &str = "SELECT id, name, my_rig, my_antenna, tx_pwr, fields, archived_at, created_at, updated_at \
                      FROM instruments";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InstrumentInput {
    pub name: String,
    pub my_rig: Option<String>,
    pub my_antenna: Option<String>,
    pub tx_pwr: Option<String>,
    pub fields: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instrument {
    pub id: String,
    pub name: String,
    pub my_rig: Option<String>,
    pub my_antenna: Option<String>,
    pub tx_pwr: Option<String>,
    pub fields: BTreeMap<String, String>,
    pub archived_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl Instrument {
    fn from_row(row: &SqliteRow) -> Result<Instrument, StoreError> {
        let fields: String = row.try_get("fields")?;
        Ok(Instrument {
            id: row.try_get("id")?,
            name: row.try_get("name")?,
            my_rig: row.try_get("my_rig")?,
            my_antenna: row.try_get("my_antenna")?,
            tx_pwr: row.try_get("tx_pwr")?,
            fields: serde_json::from_str(&fields)
                .map_err(|e| StoreError::Invalid(format!("stored Instrument fields are not a string map: {e}")))?,
            archived_at: row.try_get("archived_at")?,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
        })
    }

    pub fn defaults(&self) -> BTreeMap<String, String> {
        let mut defaults = self.fields.clone();
        for (name, value) in [
            ("MY_RIG", &self.my_rig),
            ("MY_ANTENNA", &self.my_antenna),
            ("TX_PWR", &self.tx_pwr),
        ] {
            if let Some(value) = value {
                defaults.insert(name.to_owned(), value.clone());
            }
        }
        defaults
    }
}

struct Normalized {
    name: String,
    my_rig: Option<String>,
    my_antenna: Option<String>,
    tx_pwr: Option<String>,
    fields: String,
}

fn fixed(name: &str, value: Option<&str>) -> Result<Option<String>, StoreError> {
    let Some(value) = value.map(str::trim).filter(|v| !v.is_empty()) else {
        return Ok(None);
    };
    field_value(name, value)?;
    Ok(Some(value.to_owned()))
}

fn normalize(input: &InstrumentInput) -> Result<Normalized, StoreError> {
    let name = store_name("an Instrument", &input.name)?;
    let tx_pwr = fixed("TX_PWR", input.tx_pwr.as_deref())?;
    if let Some(watts) = &tx_pwr {
        tx_power(watts)?;
    }
    let fields = defaults(&input.fields, NOT_DEFAULTS, "in the fields of an Instrument")?;
    Ok(Normalized {
        name: name.to_owned(),
        my_rig: fixed("MY_RIG", input.my_rig.as_deref())?,
        my_antenna: fixed("MY_ANTENNA", input.my_antenna.as_deref())?,
        tx_pwr,
        fields: serde_json::to_string(&fields).expect("a string map serializes"),
    })
}

pub async fn create(
    tx: &mut WriteTx,
    access: &LogbookAccess,
    input: &InstrumentInput,
) -> Result<Instrument, StoreError> {
    access.revalidate(tx).await?;
    let normalized = normalize(input)?;
    let id = new_id();
    let now = now();
    sqlx::query(
        "INSERT INTO instruments \
         (id, logbook_id, name, my_rig, my_antenna, tx_pwr, fields, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(access.logbook_id())
    .bind(&normalized.name)
    .bind(&normalized.my_rig)
    .bind(&normalized.my_antenna)
    .bind(&normalized.tx_pwr)
    .bind(&normalized.fields)
    .bind(&now)
    .bind(&now)
    .execute(&mut **tx)
    .await?;
    fetch(tx, access, &id).await
}

pub async fn update(
    tx: &mut WriteTx,
    access: &LogbookAccess,
    id: &str,
    input: &InstrumentInput,
) -> Result<Instrument, StoreError> {
    access.revalidate(tx).await?;
    let normalized = normalize(input)?;
    let updated = sqlx::query(
        "UPDATE instruments SET name = ?, my_rig = ?, my_antenna = ?, tx_pwr = ?, fields = ?, updated_at = ? \
         WHERE logbook_id = ? AND id = ?",
    )
    .bind(&normalized.name)
    .bind(&normalized.my_rig)
    .bind(&normalized.my_antenna)
    .bind(&normalized.tx_pwr)
    .bind(&normalized.fields)
    .bind(now())
    .bind(access.logbook_id())
    .bind(id)
    .execute(&mut **tx)
    .await?;
    if updated.rows_affected() == 0 {
        return Err(StoreError::NotFound("Instrument"));
    }
    fetch(tx, access, id).await
}

pub async fn set_archived(
    tx: &mut WriteTx,
    access: &LogbookAccess,
    id: &str,
    archived: bool,
) -> Result<Instrument, StoreError> {
    access.revalidate(tx).await?;
    let now = now();
    let updated = sqlx::query(
        "UPDATE instruments SET archived_at = CASE WHEN ? THEN coalesce(archived_at, ?) END, updated_at = ? \
         WHERE logbook_id = ? AND id = ?",
    )
    .bind(archived)
    .bind(&now)
    .bind(&now)
    .bind(access.logbook_id())
    .bind(id)
    .execute(&mut **tx)
    .await?;
    if updated.rows_affected() == 0 {
        return Err(StoreError::NotFound("Instrument"));
    }
    fetch(tx, access, id).await
}

pub async fn get(conn: &mut SqliteConnection, access: &LogbookAccess, id: &str) -> Result<Instrument, StoreError> {
    access.revalidate(conn).await?;
    fetch(conn, access, id).await
}

pub(crate) async fn fetch(
    conn: &mut SqliteConnection,
    access: &LogbookAccess,
    id: &str,
) -> Result<Instrument, StoreError> {
    let row = sqlx::query(AssertSqlSafe(format!("{SELECT} WHERE logbook_id = ? AND id = ?")))
        .bind(access.logbook_id())
        .bind(id)
        .fetch_optional(conn)
        .await?
        .ok_or(StoreError::NotFound("Instrument"))?;
    Instrument::from_row(&row)
}

pub async fn list(conn: &mut SqliteConnection, access: &LogbookAccess) -> Result<Vec<Instrument>, StoreError> {
    access.revalidate(conn).await?;
    sqlx::query(AssertSqlSafe(format!(
        "{SELECT} WHERE logbook_id = ? ORDER BY archived_at IS NOT NULL, name, id"
    )))
    .bind(access.logbook_id())
    .fetch_all(conn)
    .await?
    .iter()
    .map(Instrument::from_row)
    .collect()
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::store::testing::{World, instrument_input as input};

    #[tokio::test]
    async fn operators_create_instruments_with_normalized_values() {
        let world = World::new().await;
        let access = world.access(&world.operator).await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        let mut given = input(" IC-705 ");
        given.my_antenna = Some("  ".into());
        given.fields.insert("my_sig".into(), "POTA".into());
        let instrument = create(&mut tx, &access, &given).await.unwrap();
        assert_eq!(instrument.name, "IC-705");
        assert_eq!(
            (instrument.my_rig.as_deref(), instrument.my_antenna.as_deref()),
            (Some("IC-705"), None)
        );
        assert_eq!(instrument.tx_pwr.as_deref(), Some("0.5"));
        assert_eq!(
            instrument.defaults(),
            BTreeMap::from([
                ("MY_RIG".to_owned(), "IC-705".to_owned()),
                ("MY_SIG".to_owned(), "POTA".to_owned()),
                ("TX_PWR".to_owned(), "0.5".to_owned()),
            ])
        );
    }

    #[rstest]
    #[case::power("TX_PWR", "QRP")]
    #[case::no_power("TX_PWR", "0")]
    #[case::fixed_field("MY_RIG", "IC-705")]
    #[case::station_callsign("STATION_CALLSIGN", "JL1HIS")]
    #[case::bad_name("My:Rig", "x")]
    #[tokio::test]
    async fn refuses_bad_values(#[case] name: &str, #[case] value: &str) {
        let world = World::new().await;
        let access = world.access(&world.operator).await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        let mut bad = input("IC-705");
        match name {
            "TX_PWR" => bad.tx_pwr = Some(value.into()),
            _ => {
                bad.fields.insert(name.into(), value.into());
            }
        }
        let refused = create(&mut tx, &access, &bad).await;
        assert!(
            matches!(refused, Err(StoreError::Invalid(_) | StoreError::Qso(_))),
            "{refused:?}"
        );
    }

    #[tokio::test]
    async fn updates_and_archives_only_its_own_logbook_instruments() {
        let world = World::new().await;
        let instrument = world.instrument("IC-705").await;
        let access = world.access(&world.operator).await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();

        let mut changed = input("IC-705");
        changed.tx_pwr = Some("5".into());
        let updated = update(&mut tx, &access, &instrument.id, &changed).await.unwrap();
        assert_eq!(updated.tx_pwr.as_deref(), Some("5"));

        let archived = set_archived(&mut tx, &access, &instrument.id, true).await.unwrap();
        assert!(archived.archived_at.is_some());
        let restored = set_archived(&mut tx, &access, &instrument.id, false).await.unwrap();
        assert_eq!(restored.archived_at, None);
        tx.commit().await.unwrap();

        let (_, other) = world.other_logbook().await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        assert!(matches!(
            update(&mut tx, &other, &instrument.id, &changed).await,
            Err(StoreError::NotFound("Instrument"))
        ));
        assert!(matches!(
            get(&mut tx, &other, &instrument.id).await,
            Err(StoreError::NotFound("Instrument"))
        ));
    }

    #[tokio::test]
    async fn lists_active_instruments_first() {
        let world = World::new().await;
        let access = world.access(&world.owner).await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        let old = create(&mut tx, &access, &input("FT-817")).await.unwrap();
        create(&mut tx, &access, &input("IC-905")).await.unwrap();
        create(&mut tx, &access, &input("IC-705")).await.unwrap();
        set_archived(&mut tx, &access, &old.id, true).await.unwrap();
        tx.commit().await.unwrap();

        let operator = world.access(&world.operator).await;
        let mut conn = world.db.acquire().await.unwrap();
        let names: Vec<_> = list(&mut conn, &operator)
            .await
            .unwrap()
            .into_iter()
            .map(|i| i.name)
            .collect();
        assert_eq!(names, ["IC-705", "IC-905", "FT-817"]);
    }
}
