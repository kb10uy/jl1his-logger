use std::collections::BTreeSet;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{AssertSqlSafe, Row, SqliteConnection, sqlite::SqliteRow};

use crate::{
    domain::{
        adif::{self, LengthUnit},
        pota,
        qso::{InvalidQsoId, Qso},
    },
    store::{
        StoreError, WriteTx,
        access::LogbookAccess,
        new_id, now,
        qsl::{self, Outcome},
        qso::{self, Filter, StoredQso, Target},
        today,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportKind {
    Adif,
    Pota,
    Eqsl,
    Lotw,
    QslCard,
}

impl ExportKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ExportKind::Adif => "adif",
            ExportKind::Pota => "pota",
            ExportKind::Eqsl => "eqsl",
            ExportKind::Lotw => "lotw",
            ExportKind::QslCard => "qsl_card",
        }
    }

    fn parse(s: &str) -> Result<ExportKind, StoreError> {
        match s {
            "adif" => Ok(ExportKind::Adif),
            "pota" => Ok(ExportKind::Pota),
            "eqsl" => Ok(ExportKind::Eqsl),
            "lotw" => Ok(ExportKind::Lotw),
            "qsl_card" => Ok(ExportKind::QslCard),
            _ => Err(StoreError::Invalid(format!("unknown export kind {s:?}"))),
        }
    }

    fn sent_fields(self) -> Option<(&'static str, &'static str)> {
        match self {
            ExportKind::Adif | ExportKind::Pota => None,
            ExportKind::Eqsl => Some(("EQSL_QSL_SENT", "EQSL_QSLSDATE")),
            ExportKind::Lotw => Some(("LOTW_QSL_SENT", "LOTW_QSLSDATE")),
            ExportKind::QslCard => Some(("QSL_SENT", "QSLSDATE")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportBatch {
    pub id: String,
    pub kind: ExportKind,
    pub created_by: String,
    pub created_at: String,
    pub applied_at: Option<String>,
    pub qso_count: i64,
}

impl ExportBatch {
    fn from_row(row: &SqliteRow) -> Result<ExportBatch, StoreError> {
        Ok(ExportBatch {
            id: row.try_get("id")?,
            kind: ExportKind::parse(row.try_get("kind")?)?,
            created_by: row.try_get("created_by")?,
            created_at: row.try_get("created_at")?,
            applied_at: row.try_get("applied_at")?,
            qso_count: row.try_get("qso_count")?,
        })
    }
}

const SELECT: &str = "SELECT b.id, b.kind, b.created_by, b.created_at, b.applied_at, \
     (SELECT count(*) FROM export_batch_qsos q WHERE q.batch_id = b.id) AS qso_count \
     FROM export_batches b";

pub fn document(records: &[Qso], unit: LengthUnit) -> String {
    adif::document(env!("CARGO_PKG_VERSION"), Utc::now(), records, unit)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    pub records: Vec<Qso>,
    pub targets: Vec<Target>,
}

pub async fn select(
    conn: &mut SqliteConnection,
    access: &LogbookAccess,
    kind: ExportKind,
    filter: &Filter,
    park: Option<&str>,
) -> Result<Selection, StoreError> {
    if park.is_some() && kind != ExportKind::Pota {
        return Err(StoreError::Invalid("a park is only for a pota export".into()));
    }
    let mut rows = qso::search_all(conn, access, filter).await?;
    rows.reverse();
    project(&rows, kind, park)
}

fn project(rows: &[StoredQso], kind: ExportKind, park: Option<&str>) -> Result<Selection, StoreError> {
    let mut selection = Selection::default();
    for row in rows {
        let projected = match kind {
            ExportKind::Pota => pota::project(&row.qso)?
                .into_iter()
                .filter(|r| park.is_none_or(|p| r.get("MY_POTA_REF").is_some_and(|r| r.eq_ignore_ascii_case(p.trim()))))
                .collect(),
            _ => vec![row.qso.clone()],
        };
        if projected.is_empty() {
            continue;
        }
        selection.records.extend(projected);
        selection.targets.push(Target {
            id: row.id,
            revision: row.revision,
        });
    }
    Ok(selection)
}

pub async fn create(
    tx: &mut WriteTx,
    access: &LogbookAccess,
    kind: ExportKind,
    qsos: &[Target],
) -> Result<ExportBatch, StoreError> {
    let access = access.revalidate(tx).await?;
    if qsos.is_empty() {
        return Err(StoreError::Invalid("an export batch needs a QSO".into()));
    }
    let mut seen = BTreeSet::new();
    for target in qsos {
        if !seen.insert(target.id) {
            return Err(StoreError::Invalid(format!(
                "QSO {} is given more than once",
                target.id
            )));
        }
        qso::fetch(tx, &access, &target.id)
            .await?
            .require_revision(target.revision)?;
    }

    let id = new_id();
    sqlx::query("INSERT INTO export_batches (id, logbook_id, kind, created_by, created_at) VALUES (?, ?, ?, ?, ?)")
        .bind(&id)
        .bind(access.logbook_id())
        .bind(kind.as_str())
        .bind(access.user_id())
        .bind(now())
        .execute(&mut **tx)
        .await?;
    for target in qsos {
        sqlx::query("INSERT INTO export_batch_qsos (logbook_id, batch_id, qso_id, revision) VALUES (?, ?, ?, ?)")
            .bind(access.logbook_id())
            .bind(&id)
            .bind(target.id.to_string())
            .bind(target.revision)
            .execute(&mut **tx)
            .await?;
    }
    fetch(tx, &access, &id).await
}

pub async fn get(
    conn: &mut SqliteConnection,
    access: &LogbookAccess,
    id: &str,
) -> Result<(ExportBatch, Vec<Target>), StoreError> {
    let access = access.revalidate(conn).await?;
    let batch = fetch(conn, &access, id).await?;
    let targets = targets(conn, id).await?;
    Ok((batch, targets))
}

pub async fn list(conn: &mut SqliteConnection, access: &LogbookAccess) -> Result<Vec<ExportBatch>, StoreError> {
    let access = access.revalidate(conn).await?;
    sqlx::query(AssertSqlSafe(format!(
        "{SELECT} WHERE b.logbook_id = ? ORDER BY b.created_at DESC, b.id DESC"
    )))
    .bind(access.logbook_id())
    .fetch_all(conn)
    .await?
    .iter()
    .map(ExportBatch::from_row)
    .collect()
}

pub async fn apply(tx: &mut WriteTx, access: &LogbookAccess, id: &str) -> Result<Outcome, StoreError> {
    let access = access.revalidate(tx).await?;
    let batch = fetch(tx, &access, id).await?;
    if batch.applied_at.is_some() {
        return Err(StoreError::Conflict(format!(
            "export batch {id} has already been applied"
        )));
    }
    let Some((sent, date)) = batch.kind.sent_fields() else {
        return Err(StoreError::Invalid(format!(
            "{} export batches have nothing to apply",
            batch.kind.as_str()
        )));
    };
    let targets = targets(tx, id).await?;
    let changes = vec![(sent, "Y".to_owned()), (date, today())];
    let outcome = qsl::apply(tx, &access, &targets, &changes).await?;
    sqlx::query("UPDATE export_batches SET applied_at = ? WHERE id = ?")
        .bind(now())
        .bind(id)
        .execute(&mut **tx)
        .await?;
    Ok(outcome)
}

async fn fetch(conn: &mut SqliteConnection, access: &LogbookAccess, id: &str) -> Result<ExportBatch, StoreError> {
    let row = sqlx::query(AssertSqlSafe(format!("{SELECT} WHERE b.logbook_id = ? AND b.id = ?")))
        .bind(access.logbook_id())
        .bind(id)
        .fetch_optional(conn)
        .await?
        .ok_or(StoreError::NotFound("export batch"))?;
    ExportBatch::from_row(&row)
}

async fn targets(conn: &mut SqliteConnection, batch_id: &str) -> Result<Vec<Target>, StoreError> {
    let rows: Vec<(String, i64)> =
        sqlx::query_as("SELECT qso_id, revision FROM export_batch_qsos WHERE batch_id = ? ORDER BY qso_id")
            .bind(batch_id)
            .fetch_all(conn)
            .await?;
    rows.into_iter()
        .map(|(id, revision)| {
            let id = id
                .parse()
                .map_err(|e: InvalidQsoId| StoreError::Invalid(format!("stored QSO ID {e}")))?;
            Ok(Target { id, revision })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::{
        domain::qso::QsoId,
        store::{
            qsl::{
                SkipReason,
                tests::{target, three_qsos},
            },
            testing::World,
        },
    };

    fn row(fields: &[(&str, &str)]) -> StoredQso {
        StoredQso {
            id: QsoId::generate(),
            station_id: "station".into(),
            seq: 1,
            updated_seq: 1,
            revision: 2,
            created_by: "user".into(),
            created_by_callsign: "JL1HIS".into(),
            created_at: "2026-10-22T00:00:00Z".into(),
            updated_by: "user".into(),
            updated_by_callsign: "JL1HIS".into(),
            updated_at: "2026-10-22T00:00:00Z".into(),
            deleted_at: None,
            qso: Qso::from_fields(fields.iter().copied()).unwrap(),
        }
    }

    #[test]
    fn projects_parks_and_leaves_out_qsos_without_one() {
        let rows = [
            row(&[("CALL", "JA1AAA"), ("MY_POTA_REF", "JP-0001,JP-0002")]),
            row(&[("CALL", "JA1BBB")]),
            row(&[("CALL", "JA1CCC"), ("MY_SIG", "POTA"), ("MY_SIG_INFO", "JP-0002")]),
        ];
        let selection = project(&rows, ExportKind::Pota, None).unwrap();
        let calls: Vec<_> = selection
            .records
            .iter()
            .map(|r| (r.get("CALL").unwrap(), r.get("MY_SIG_INFO").unwrap()))
            .collect();
        assert_eq!(
            calls,
            [("JA1AAA", "JP-0001"), ("JA1AAA", "JP-0002"), ("JA1CCC", "JP-0002")]
        );
        assert_eq!(selection.targets.len(), 2);
        assert_eq!(selection.targets[0].revision, 2);

        let selection = project(&rows, ExportKind::Pota, Some("jp-0002")).unwrap();
        assert_eq!((selection.records.len(), selection.targets.len()), (2, 2));

        let selection = project(&rows, ExportKind::Adif, None).unwrap();
        assert_eq!((selection.records.len(), selection.targets.len()), (3, 3));
    }

    #[tokio::test]
    async fn selects_matching_qsos_oldest_first() {
        let world = World::new().await;
        let rows = three_qsos(&world).await;
        let access = world.access(&world.operator).await;
        let mut conn = world.db.acquire().await.unwrap();
        let filter = Filter {
            call: Some(qso::CallMatch::Exact("JA1CCC".into())),
            ..Filter::default()
        };
        let all = select(&mut conn, &access, ExportKind::Adif, &Filter::default(), None)
            .await
            .unwrap();
        assert_eq!(all.targets, rows.iter().map(target).collect::<Vec<_>>());
        let one = select(&mut conn, &access, ExportKind::Eqsl, &filter, None)
            .await
            .unwrap();
        assert_eq!(one.targets, [target(&rows[2])]);
        assert!(matches!(
            select(&mut conn, &access, ExportKind::Adif, &filter, Some("JP-0001")).await,
            Err(StoreError::Invalid(_))
        ));
    }

    #[tokio::test]
    async fn records_and_applies_a_batch_once() {
        let world = World::new().await;
        let rows = three_qsos(&world).await;
        let access = world.access(&world.operator).await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        let targets: Vec<_> = rows.iter().map(target).collect();
        let batch = create(&mut tx, &access, ExportKind::QslCard, &targets).await.unwrap();
        assert_eq!(
            (batch.kind, batch.qso_count, batch.applied_at.as_deref()),
            (ExportKind::QslCard, 3, None)
        );

        let mut sorted = targets.clone();
        sorted.sort_by_key(|t| t.id.to_string());
        assert_eq!(get(&mut tx, &access, &batch.id).await.unwrap(), (batch.clone(), sorted));

        let edited = BTreeMap::from([("QSL_SENT_VIA".to_owned(), "D".to_owned())]);
        qsl::update(&mut tx, &access, &[target(&rows[1])], &edited)
            .await
            .unwrap();

        let outcome = apply(&mut tx, &access, &batch.id).await.unwrap();
        let today = today();
        assert_eq!(outcome.updated.len(), 2);
        for row in &outcome.updated {
            assert_eq!(row.qso.get("QSL_SENT"), Some("Y"));
            assert_eq!(row.qso.get("QSLSDATE"), Some(today.as_str()));
            assert_eq!(row.qso.get("QSL_SENT_VIA"), None);
        }
        assert_eq!(outcome.skipped.len(), 1);
        assert_eq!(outcome.skipped[0].id, rows[1].id);
        assert_eq!(outcome.skipped[0].reason, SkipReason::Revision { current: 2 });

        assert!(matches!(
            apply(&mut tx, &access, &batch.id).await,
            Err(StoreError::Conflict(_))
        ));
        assert!(list(&mut tx, &access).await.unwrap()[0].applied_at.is_some());
    }

    #[tokio::test]
    async fn refuses_stale_rows_and_batches_without_results() {
        let world = World::new().await;
        let rows = three_qsos(&world).await;
        let access = world.access(&world.operator).await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        let stale = Target {
            revision: 2,
            ..target(&rows[0])
        };
        assert!(matches!(
            create(&mut tx, &access, ExportKind::Eqsl, &[stale]).await,
            Err(StoreError::Conflict(_))
        ));
        assert!(matches!(
            create(
                &mut tx,
                &access,
                ExportKind::Eqsl,
                &[target(&rows[0]), target(&rows[0])]
            )
            .await,
            Err(StoreError::Invalid(_))
        ));
        assert!(matches!(
            create(&mut tx, &access, ExportKind::Eqsl, &[]).await,
            Err(StoreError::Invalid(_))
        ));
        let pota = create(&mut tx, &access, ExportKind::Pota, &[target(&rows[0])])
            .await
            .unwrap();
        assert!(matches!(
            apply(&mut tx, &access, &pota.id).await,
            Err(StoreError::Invalid(_))
        ));
    }

    #[tokio::test]
    async fn keeps_batches_inside_their_logbook() {
        let world = World::new().await;
        let rows = three_qsos(&world).await;
        let access = world.access(&world.owner).await;
        let (_, other_access) = world.other_logbook().await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        let batch = create(&mut tx, &access, ExportKind::Lotw, &[target(&rows[0])])
            .await
            .unwrap();
        assert!(matches!(
            get(&mut tx, &other_access, &batch.id).await,
            Err(StoreError::NotFound(_))
        ));
        assert!(matches!(
            create(&mut tx, &other_access, ExportKind::Lotw, &[target(&rows[0])]).await,
            Err(StoreError::NotFound(_))
        ));
    }
}
