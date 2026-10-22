use std::collections::BTreeMap;

use sqlx::{Row, SqliteConnection, sqlite::SqliteRow};

use crate::{
    domain::qso::field_name,
    store::{
        StoreError, WriteTx,
        access::{LogbookAccess, OwnerAccess},
        is_unique_violation, new_id, now,
        user::normalize_callsign,
    },
};

const NOT_DEFAULTS: &[&str] = &["CALL", "QSO_DATE", "TIME_ON", "BAND", "MODE", "STATION_CALLSIGN"];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StationInput {
    pub name: String,
    pub callsign: String,
    pub wavelog_id: Option<i64>,
    pub fields: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Station {
    pub id: String,
    pub wavelog_id: Option<i64>,
    pub name: String,
    pub callsign: String,
    pub fields: BTreeMap<String, String>,
    pub archived_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl Station {
    fn from_row(row: &SqliteRow) -> Result<Station, StoreError> {
        let fields: String = row.try_get("fields")?;
        Ok(Station {
            id: row.try_get("id")?,
            wavelog_id: row.try_get("wavelog_id")?,
            name: row.try_get("name")?,
            callsign: row.try_get("callsign")?,
            fields: serde_json::from_str(&fields)
                .map_err(|e| StoreError::Invalid(format!("stored Station fields are not a string map: {e}")))?,
            archived_at: row.try_get("archived_at")?,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
        })
    }
}

struct Normalized {
    name: String,
    callsign: String,
    fields: String,
}

fn normalize(input: &StationInput) -> Result<Normalized, StoreError> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(StoreError::Invalid("a Station needs a name".into()));
    }
    let mut fields = BTreeMap::new();
    for (name, value) in &input.fields {
        let name = field_name(name)?;
        if NOT_DEFAULTS.contains(&name.as_str()) {
            return Err(StoreError::Invalid(format!("{name} cannot be a Station default")));
        }
        if !value.is_empty() && fields.insert(name.clone(), value.clone()).is_some() {
            return Err(StoreError::Invalid(format!("{name} is given more than once")));
        }
    }
    Ok(Normalized {
        name: name.to_owned(),
        callsign: normalize_callsign(&input.callsign)?,
        fields: serde_json::to_string(&fields).expect("a string map serializes"),
    })
}

fn wavelog_id_taken(e: sqlx::Error, wavelog_id: Option<i64>) -> StoreError {
    match (is_unique_violation(&e), wavelog_id) {
        (true, Some(id)) => StoreError::Conflict(format!("another Station has the Wavelog ID {id}")),
        _ => e.into(),
    }
}

pub async fn create(tx: &mut WriteTx, owner: &OwnerAccess<'_>, input: &StationInput) -> Result<Station, StoreError> {
    owner.revalidate(tx).await?;
    let normalized = normalize(input)?;
    let id = new_id();
    let now = now();
    sqlx::query(
        "INSERT INTO stations (id, logbook_id, wavelog_id, name, callsign, fields, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(owner.logbook_id())
    .bind(input.wavelog_id)
    .bind(&normalized.name)
    .bind(&normalized.callsign)
    .bind(&normalized.fields)
    .bind(&now)
    .bind(&now)
    .execute(&mut **tx)
    .await
    .map_err(|e| wavelog_id_taken(e, input.wavelog_id))?;
    get(tx, owner, &id).await
}

pub async fn update(
    tx: &mut WriteTx,
    owner: &OwnerAccess<'_>,
    id: &str,
    input: &StationInput,
) -> Result<Station, StoreError> {
    owner.revalidate(tx).await?;
    let normalized = normalize(input)?;
    let updated = sqlx::query(
        "UPDATE stations SET wavelog_id = ?, name = ?, callsign = ?, fields = ?, updated_at = ? \
         WHERE logbook_id = ? AND id = ?",
    )
    .bind(input.wavelog_id)
    .bind(&normalized.name)
    .bind(&normalized.callsign)
    .bind(&normalized.fields)
    .bind(now())
    .bind(owner.logbook_id())
    .bind(id)
    .execute(&mut **tx)
    .await
    .map_err(|e| wavelog_id_taken(e, input.wavelog_id))?;
    if updated.rows_affected() == 0 {
        return Err(StoreError::NotFound("Station"));
    }
    get(tx, owner, id).await
}

pub async fn set_archived(
    tx: &mut WriteTx,
    owner: &OwnerAccess<'_>,
    id: &str,
    archived: bool,
) -> Result<Station, StoreError> {
    owner.revalidate(tx).await?;
    let now = now();
    let updated = sqlx::query(
        "UPDATE stations SET archived_at = CASE WHEN ? THEN coalesce(archived_at, ?) END, updated_at = ? \
         WHERE logbook_id = ? AND id = ?",
    )
    .bind(archived)
    .bind(&now)
    .bind(&now)
    .bind(owner.logbook_id())
    .bind(id)
    .execute(&mut **tx)
    .await?;
    if updated.rows_affected() == 0 {
        return Err(StoreError::NotFound("Station"));
    }
    get(tx, owner, id).await
}

pub async fn get(conn: &mut SqliteConnection, access: &LogbookAccess, id: &str) -> Result<Station, StoreError> {
    access.revalidate(conn).await?;
    let row = sqlx::query(
        "SELECT id, wavelog_id, name, callsign, fields, archived_at, created_at, updated_at \
         FROM stations WHERE logbook_id = ? AND id = ?",
    )
    .bind(access.logbook_id())
    .bind(id)
    .fetch_optional(conn)
    .await?
    .ok_or(StoreError::NotFound("Station"))?;
    Station::from_row(&row)
}

pub async fn list(conn: &mut SqliteConnection, access: &LogbookAccess) -> Result<Vec<Station>, StoreError> {
    access.revalidate(conn).await?;
    sqlx::query(
        "SELECT id, wavelog_id, name, callsign, fields, archived_at, created_at, updated_at \
         FROM stations WHERE logbook_id = ? ORDER BY archived_at IS NOT NULL, name, id",
    )
    .bind(access.logbook_id())
    .fetch_all(conn)
    .await?
    .iter()
    .map(Station::from_row)
    .collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::store::testing::World;

    pub(crate) fn input(name: &str) -> StationInput {
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

    #[tokio::test]
    async fn creates_a_station_with_normalized_values() {
        let world = World::new().await;
        let access = world.access(&world.owner).await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        let station = create(&mut tx, &access.owner().unwrap(), &input(" Home "))
            .await
            .unwrap();
        assert_eq!((station.name.as_str(), station.callsign.as_str()), ("Home", "JL1HIS/1"));
        assert_eq!(
            station.fields,
            BTreeMap::from([
                ("MY_CITY".to_owned(), "Minato".to_owned()),
                ("MY_CNTY".to_owned(), "100103".to_owned()),
            ])
        );
        assert_eq!(station.archived_at, None);
    }

    #[tokio::test]
    async fn refuses_identity_fields_as_defaults() {
        let world = World::new().await;
        let access = world.access(&world.owner).await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        for name in ["STATION_CALLSIGN", "call", "My:City"] {
            let mut bad = input("Home");
            bad.fields.insert(name.into(), "x".into());
            let refused = create(&mut tx, &access.owner().unwrap(), &bad).await;
            assert!(
                matches!(refused, Err(StoreError::Invalid(_) | StoreError::Qso(_))),
                "{name}: {refused:?}"
            );
        }
        let mut repeated = input("Home");
        repeated.fields.insert("MY_CITY".into(), "Shiba".into());
        assert!(matches!(
            create(&mut tx, &access.owner().unwrap(), &repeated).await,
            Err(StoreError::Invalid(_))
        ));
    }

    #[tokio::test]
    async fn keeps_wavelog_ids_unique_within_a_logbook() {
        let world = World::new().await;
        let access = world.access(&world.owner).await;
        let owner = access.owner().unwrap();
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        let mut first = input("Home");
        first.wavelog_id = Some(3);
        create(&mut tx, &owner, &first).await.unwrap();
        let mut second = input("Park");
        second.wavelog_id = Some(3);
        assert!(matches!(
            create(&mut tx, &owner, &second).await,
            Err(StoreError::Conflict(_))
        ));
    }

    #[tokio::test]
    async fn updates_and_archives_only_its_own_logbook_stations() {
        let world = World::new().await;
        let access = world.access(&world.owner).await;
        let owner = access.owner().unwrap();
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        let station = create(&mut tx, &owner, &input("Home")).await.unwrap();

        let mut changed = input("Home");
        changed.callsign = "JL1HIS".into();
        let updated = update(&mut tx, &owner, &station.id, &changed).await.unwrap();
        assert_eq!(updated.callsign, "JL1HIS");

        let archived = set_archived(&mut tx, &owner, &station.id, true).await.unwrap();
        assert!(archived.archived_at.is_some());
        let again = set_archived(&mut tx, &owner, &station.id, true).await.unwrap();
        assert_eq!(again.archived_at, archived.archived_at);
        let restored = set_archived(&mut tx, &owner, &station.id, false).await.unwrap();
        assert_eq!(restored.archived_at, None);

        assert!(matches!(
            update(&mut tx, &owner, "no-such-station", &changed).await,
            Err(StoreError::NotFound("Station"))
        ));
    }

    #[tokio::test]
    async fn lists_active_stations_first() {
        let world = World::new().await;
        let access = world.access(&world.owner).await;
        let owner = access.owner().unwrap();
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        let old = create(&mut tx, &owner, &input("Alpha")).await.unwrap();
        create(&mut tx, &owner, &input("Charlie")).await.unwrap();
        create(&mut tx, &owner, &input("Bravo")).await.unwrap();
        set_archived(&mut tx, &owner, &old.id, true).await.unwrap();
        tx.commit().await.unwrap();

        let operator = world.access(&world.operator).await;
        let mut conn = world.db.acquire().await.unwrap();
        let names: Vec<_> = list(&mut conn, &operator)
            .await
            .unwrap()
            .into_iter()
            .map(|s| s.name)
            .collect();
        assert_eq!(names, ["Bravo", "Charlie", "Alpha"]);
    }
}
