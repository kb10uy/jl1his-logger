use std::collections::BTreeMap;

use sqlx::{AssertSqlSafe, Row, SqliteConnection, sqlite::SqliteRow};

use crate::{
    domain::qso::{MAX_FIELDS, Qso, QsoError, field_name, field_value},
    store::{
        Fault, StoreError, WriteTx,
        access::{LogbookAccess, OwnerAccess},
        account::user::normalize_callsign,
        name as store_name, new_id, now,
        qso::{self, Holder, StoredQso, TimeRange},
    },
};

const NOT_DEFAULTS: &[&str] = &["CALL", "QSO_DATE", "TIME_ON", "BAND", "MODE", "STATION_CALLSIGN"];

const EQUIPMENT: &[&str] = &["MY_RIG", "MY_ANTENNA", "TX_PWR"];

const SELECT: &str = "SELECT id, name, callsign, fields, archived_at, created_at, updated_at, \
                      EXISTS (SELECT 1 FROM station_favorites f WHERE f.user_id = ? AND f.station_id = stations.id) \
                      AS favorite FROM stations";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StationInput {
    pub name: String,
    pub callsign: String,
    pub fields: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Station {
    pub id: String,
    pub name: String,
    pub callsign: String,
    pub fields: BTreeMap<String, String>,
    pub archived_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub favorite: bool,
}

impl Station {
    fn from_row(row: &SqliteRow) -> Result<Station, StoreError> {
        let fields: String = row.try_get("fields")?;
        Ok(Station {
            id: row.try_get("id")?,
            name: row.try_get("name")?,
            callsign: row.try_get("callsign")?,
            fields: serde_json::from_str(&fields)
                .map_err(|e| StoreError::Invalid(format!("stored Station fields are not a string map: {e}").into()))?,
            archived_at: row.try_get("archived_at")?,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
            favorite: row.try_get("favorite")?,
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
    if let Some(name) = input
        .fields
        .keys()
        .map(|name| name.to_ascii_uppercase())
        .find(|name| EQUIPMENT.contains(&name.as_str()))
    {
        return Err(StoreError::Invalid(
            Fault::coded(
                "instrument_field",
                format!("{name} belongs to an Instrument, not a Station"),
            )
            .with("field", name.as_str()),
        ));
    }
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
            return Err(StoreError::Invalid(
                Fault::coded("field_not_allowed", format!("{name} cannot be {what}")).with("field", name.as_str()),
            ));
        }
        if !value.is_empty() && fields.insert(name.clone(), value.clone()).is_some() {
            return Err(StoreError::Invalid(
                Fault::coded("field_repeated", format!("{name} is given more than once")).with("field", name.as_str()),
            ));
        }
    }
    if fields.len() > MAX_FIELDS {
        return Err(QsoError::TooManyFields(fields.len()).into());
    }
    Ok(fields)
}

pub async fn create(tx: &mut WriteTx, owner: &OwnerAccess<'_>, input: &StationInput) -> Result<Station, StoreError> {
    owner.revalidate(tx).await?;
    let normalized = normalize(input)?;
    let id = new_id();
    let now = now();
    sqlx::query(
        "INSERT INTO stations (id, logbook_id, name, callsign, fields, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(owner.logbook_id())
    .bind(&normalized.name)
    .bind(&normalized.callsign)
    .bind(&normalized.fields)
    .bind(&now)
    .bind(&now)
    .execute(&mut **tx)
    .await?;
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
        "UPDATE stations SET name = ?, callsign = ?, fields = ?, updated_at = ? \
         WHERE logbook_id = ? AND id = ?",
    )
    .bind(&normalized.name)
    .bind(&normalized.callsign)
    .bind(&normalized.fields)
    .bind(now())
    .bind(owner.logbook_id())
    .bind(id)
    .execute(&mut **tx)
    .await?;
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

pub async fn set_favorite(
    tx: &mut WriteTx,
    access: &LogbookAccess,
    id: &str,
    favorite: bool,
) -> Result<Station, StoreError> {
    access.revalidate(tx).await?;
    fetch(tx, access, id).await?;
    let query = match favorite {
        true => sqlx::query(
            "INSERT INTO station_favorites (logbook_id, user_id, station_id, created_at) VALUES (?, ?, ?, ?) \
             ON CONFLICT DO NOTHING",
        )
        .bind(access.logbook_id())
        .bind(access.user_id())
        .bind(id)
        .bind(now()),
        false => sqlx::query("DELETE FROM station_favorites WHERE user_id = ? AND station_id = ?")
            .bind(access.user_id())
            .bind(id),
    };
    query.execute(&mut **tx).await?;
    fetch(tx, access, id).await
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
    qso::rewrite_all(tx, owner, &plan, expected).await
}

async fn rewrite_plan(
    conn: &mut SqliteConnection,
    access: &LogbookAccess,
    id: &str,
    range: TimeRange,
) -> Result<Vec<(StoredQso, Qso)>, StoreError> {
    let station = fetch(conn, access, id).await?;
    let mut plan = vec![];
    for current in qso::held_by(conn, access, Holder::Station(&station.id), range).await? {
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
        .bind(access.user_id())
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
        "{SELECT} WHERE logbook_id = ? ORDER BY archived_at IS NOT NULL, favorite DESC, name, id"
    )))
    .bind(access.user_id())
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
        store::{
            logbook::membership,
            testing::{World, contact, station_input as input},
        },
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
    #[case::rig("my_rig", "IC-705")]
    #[case::antenna("MY_ANTENNA", "")]
    #[case::power("TX_PWR", "5")]
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

    #[tokio::test]
    async fn lists_the_favorites_of_each_member_first() {
        let world = World::new().await;
        let alpha = world.station("Alpha").await;
        let charlie = world.station("Charlie").await;
        world.station("Bravo").await;
        let owner = world.access(&world.owner).await;
        let operator = world.access(&world.operator).await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        let favorite = set_favorite(&mut tx, &operator, &charlie.id, true).await.unwrap();
        assert!(favorite.favorite);
        assert!(
            set_favorite(&mut tx, &operator, &charlie.id, true)
                .await
                .unwrap()
                .favorite
        );
        assert_eq!(favorite.updated_at, charlie.updated_at);
        set_favorite(&mut tx, &operator, &alpha.id, true).await.unwrap();
        set_archived(&mut tx, &owner.owner().unwrap(), &alpha.id, true)
            .await
            .unwrap();
        tx.commit().await.unwrap();

        let mut conn = world.db.acquire().await.unwrap();
        let listed = |stations: Vec<Station>| -> Vec<(String, bool)> {
            stations.into_iter().map(|s| (s.name, s.favorite)).collect()
        };
        assert_eq!(
            listed(list(&mut conn, &operator).await.unwrap()),
            [
                ("Charlie".to_owned(), true),
                ("Bravo".to_owned(), false),
                ("Alpha".to_owned(), true)
            ]
        );
        assert_eq!(
            listed(list(&mut conn, &owner).await.unwrap()),
            [
                ("Bravo".to_owned(), false),
                ("Charlie".to_owned(), false),
                ("Alpha".to_owned(), false)
            ]
        );
        drop(conn);

        let (_, other) = world.other_logbook().await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        let removed = set_favorite(&mut tx, &operator, &charlie.id, false).await.unwrap();
        assert!(!removed.favorite);
        assert!(
            !set_favorite(&mut tx, &operator, &charlie.id, false)
                .await
                .unwrap()
                .favorite
        );
        assert!(matches!(
            set_favorite(&mut tx, &other, &charlie.id, true).await,
            Err(StoreError::NotFound("Station"))
        ));
    }

    #[tokio::test]
    async fn forgets_the_favorites_of_a_removed_member() {
        let world = World::new().await;
        let station = world.station("Home").await;
        let operator = world.access(&world.operator).await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        set_favorite(&mut tx, &operator, &station.id, true).await.unwrap();
        let owner = world.access(&world.owner).await;
        membership::remove(&mut tx, &owner.owner().unwrap(), &world.operator.id)
            .await
            .unwrap();
        let left: i64 = sqlx::query_scalar("SELECT count(*) FROM station_favorites")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(left, 0);
    }
}
