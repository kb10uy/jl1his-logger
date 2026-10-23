use std::collections::BTreeMap;

use sqlx::{AssertSqlSafe, Row, SqliteConnection, sqlite::SqliteRow};

use crate::{
    domain::qso::{MAX_FIELDS, Qso, QsoError, field_name, field_value},
    store::{
        StoreError, WriteTx,
        access::{LogbookAccess, OwnerAccess},
        is_unique_violation, name as store_name, new_id, now,
        qso::{self, StoredQso, TimeRange},
        user::normalize_callsign,
    },
};

const NOT_DEFAULTS: &[&str] = &["CALL", "QSO_DATE", "TIME_ON", "BAND", "MODE", "STATION_CALLSIGN"];

const SELECT: &str = "SELECT id, wavelog_id, name, callsign, fields, archived_at, created_at, updated_at FROM stations";

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
    let name = store_name("a Station", &input.name)?;
    let fields = defaults(&input.fields, NOT_DEFAULTS, "a Station default")?;
    Ok(Normalized {
        name: name.to_owned(),
        callsign: normalize_callsign(&input.callsign)?,
        fields: serde_json::to_string(&fields).expect("a string map serializes"),
    })
}

pub(super) fn defaults(
    given: &BTreeMap<String, String>,
    refused: &[&str],
    what: &str,
) -> Result<BTreeMap<String, String>, StoreError> {
    let mut fields = BTreeMap::new();
    for (name, value) in given {
        let name = field_name(name)?;
        field_value(&name, value)?;
        if refused.contains(&name.as_str()) {
            return Err(StoreError::Invalid(format!("{name} cannot be {what}")));
        }
        if !value.is_empty() && fields.insert(name.clone(), value.clone()).is_some() {
            return Err(StoreError::Invalid(format!("{name} is given more than once")));
        }
    }
    if fields.len() > MAX_FIELDS {
        return Err(QsoError::TooManyFields(fields.len()).into());
    }
    Ok(fields)
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
    fetch(tx, owner, &id).await
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
    fetch(tx, owner, id).await
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
    fetch(tx, owner, id).await
}

pub async fn preview_rewrite(
    conn: &mut SqliteConnection,
    owner: &OwnerAccess<'_>,
    id: &str,
    range: TimeRange,
) -> Result<usize, StoreError> {
    owner.revalidate(conn).await?;
    Ok(rewrite_plan(conn, owner, id, range).await?.len())
}

pub async fn rewrite_qsos(
    tx: &mut WriteTx,
    owner: &OwnerAccess<'_>,
    id: &str,
    range: TimeRange,
    expected: usize,
) -> Result<usize, StoreError> {
    owner.revalidate(tx).await?;
    let plan = rewrite_plan(tx, owner, id, range).await?;
    if plan.len() != expected {
        return Err(StoreError::Conflict(format!(
            "{} QSOs would change, not {expected}",
            plan.len()
        )));
    }
    for (current, rewritten) in &plan {
        qso::rewrite(tx, owner, current, rewritten).await?;
    }
    Ok(plan.len())
}

async fn rewrite_plan(
    conn: &mut SqliteConnection,
    access: &LogbookAccess,
    id: &str,
    range: TimeRange,
) -> Result<Vec<(StoredQso, Qso)>, StoreError> {
    let station = fetch(conn, access, id).await?;
    let mut plan = vec![];
    for current in qso::of_station(conn, access, &station.id, range).await? {
        let mut rewritten = current.qso.clone();
        rewritten.set("STATION_CALLSIGN", Some(station.callsign.clone()))?;
        for (name, value) in &station.fields {
            rewritten.set(name, Some(value.clone()))?;
        }
        if rewritten != current.qso {
            plan.push((current, rewritten));
        }
    }
    Ok(plan)
}

pub async fn get(conn: &mut SqliteConnection, access: &LogbookAccess, id: &str) -> Result<Station, StoreError> {
    access.revalidate(conn).await?;
    fetch(conn, access, id).await
}

pub(crate) async fn fetch(
    conn: &mut SqliteConnection,
    access: &LogbookAccess,
    id: &str,
) -> Result<Station, StoreError> {
    let row = sqlx::query(AssertSqlSafe(format!("{SELECT} WHERE logbook_id = ? AND id = ?")))
        .bind(access.logbook_id())
        .bind(id)
        .fetch_optional(conn)
        .await?
        .ok_or(StoreError::NotFound("Station"))?;
    Station::from_row(&row)
}

pub async fn list(conn: &mut SqliteConnection, access: &LogbookAccess) -> Result<Vec<Station>, StoreError> {
    access.revalidate(conn).await?;
    sqlx::query(AssertSqlSafe(format!(
        "{SELECT} WHERE logbook_id = ? ORDER BY archived_at IS NOT NULL, name, id"
    )))
    .bind(access.logbook_id())
    .fetch_all(conn)
    .await?
    .iter()
    .map(Station::from_row)
    .collect()
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::{
        domain::qso::QsoId,
        store::testing::{World, contact, station_input as input},
    };

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

    #[rstest]
    #[case::station_callsign("STATION_CALLSIGN", "x")]
    #[case::call("call", "x")]
    #[case::bad_name("My:City", "x")]
    #[case::repeated("MY_CITY", "Shiba")]
    #[tokio::test]
    async fn refuses_identity_fields_as_defaults(#[case] name: &str, #[case] value: &str) {
        let world = World::new().await;
        let access = world.access(&world.owner).await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        let mut bad = input("Home");
        bad.fields.insert(name.into(), value.into());
        let refused = create(&mut tx, &access.owner().unwrap(), &bad).await;
        assert!(
            matches!(refused, Err(StoreError::Invalid(_) | StoreError::Qso(_))),
            "{refused:?}"
        );
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
    async fn rewrites_past_qsos_with_the_stated_count() {
        let world = World::new().await;
        let access = world.access(&world.owner).await;
        let owner = access.owner().unwrap();
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        let station = create(&mut tx, &owner, &input("Home")).await.unwrap();
        let mut ids = vec![];
        for (time_on, my_rig) in [("0101", "IC-705"), ("0202", "IC-705"), ("0303", "")] {
            let mut qso = contact("JA1XYZ", time_on);
            qso.set("MY_RIG", Some(my_rig.into())).unwrap();
            let input = qso::QsoInput {
                station_id: station.id.clone(),
                instrument_id: None,
                qso,
            };
            let id = QsoId::generate();
            qso::create(&mut tx, &access, &id, input).await.unwrap();
            ids.push(id);
        }

        let mut moved = input("Home");
        moved.callsign = "JL1HIS/2".into();
        moved.fields.insert("my_city".into(), "Kawasaki".into());
        update(&mut tx, &owner, &station.id, &moved).await.unwrap();

        let first = contact("JA1XYZ", "0202").qso_time().unwrap();
        let range = TimeRange {
            from: Some(first),
            until: None,
        };
        assert_eq!(preview_rewrite(&mut tx, &owner, &station.id, range).await.unwrap(), 2);
        assert!(matches!(
            rewrite_qsos(&mut tx, &owner, &station.id, range, 3).await,
            Err(StoreError::Conflict(_))
        ));
        assert_eq!(rewrite_qsos(&mut tx, &owner, &station.id, range, 2).await.unwrap(), 2);
        assert_eq!(preview_rewrite(&mut tx, &owner, &station.id, range).await.unwrap(), 0);

        let mut rows = vec![];
        for id in &ids {
            rows.push(qso::get(&mut tx, &access, id).await.unwrap());
        }
        assert_eq!(rows[0].qso.get("STATION_CALLSIGN"), Some("JL1HIS/1"));
        assert_eq!(rows[1].qso.get("STATION_CALLSIGN"), Some("JL1HIS/2"));
        assert_eq!(rows[1].qso.get("MY_CITY"), Some("Kawasaki"));
        assert_eq!(rows[1].qso.get("MY_RIG"), Some("IC-705"));
        assert_eq!((rows[1].revision, rows[2].revision), (2, 2));

        let operator = world.access(&world.operator).await;
        assert!(operator.owner().is_err());
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
