use std::collections::BTreeSet;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{AssertSqlSafe, Row, SqliteConnection, sqlite::SqliteRow};

use crate::{
    domain::{
        adif::{self, LengthUnit, Output, UnencodableError},
        pota,
        qso::{InvalidQsoId, Qso},
        split,
    },
    store::{
        Fault, StoreError, WriteTx,
        access::LogbookAccess,
        bulk::{self, Outcome},
        new_id, now,
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
            _ => Err(StoreError::Invalid(format!("unknown export kind {s:?}").into())),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportState {
    Pending,
    Finished,
}

impl ExportState {
    fn pending_batches(self) -> &'static str {
        match self {
            ExportState::Pending => "> 0",
            ExportState::Finished => "= 0",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportBatch {
    pub id: String,
    pub export_id: String,
    pub kind: ExportKind,
    pub created_by: String,
    pub created_at: String,
    pub applied_at: Option<String>,
    pub dismissed_at: Option<String>,
    pub qso_count: i64,
    pub label: Option<String>,
}

impl ExportBatch {
    fn from_row(row: &SqliteRow) -> Result<ExportBatch, StoreError> {
        Ok(ExportBatch {
            id: row.try_get("id")?,
            export_id: row.try_get("export_id")?,
            kind: ExportKind::parse(row.try_get("kind")?)?,
            created_by: row.try_get("created_by")?,
            created_at: row.try_get("created_at")?,
            applied_at: row.try_get("applied_at")?,
            dismissed_at: row.try_get("dismissed_at")?,
            qso_count: row.try_get("qso_count")?,
            label: row.try_get("label")?,
        })
    }

    fn pending(&self) -> Result<(&'static str, &'static str), StoreError> {
        let Some(fields) = self.kind.sent_fields() else {
            return Err(StoreError::Invalid(
                Fault::coded(
                    "batch_nothing_to_apply",
                    format!("{} export batches have nothing to apply", self.kind.as_str()),
                )
                .with("kind", self.kind.as_str()),
            ));
        };
        if self.applied_at.is_some() {
            return Err(StoreError::Conflict(
                Fault::coded(
                    "batch_applied",
                    format!("export batch {} has already been applied", self.id),
                )
                .with("id", self.id.clone()),
            ));
        }
        if self.dismissed_at.is_some() {
            return Err(StoreError::Conflict(
                Fault::coded(
                    "batch_dismissed",
                    format!("export batch {} has been dismissed", self.id),
                )
                .with("id", self.id.clone()),
            ));
        }
        Ok(fields)
    }
}

const SELECT: &str = "SELECT b.id, b.export_id, b.kind, b.created_by, b.created_at, b.applied_at, b.dismissed_at, \
     b.label, (SELECT count(*) FROM export_batch_qsos q WHERE q.batch_id = b.id) AS qso_count \
     FROM export_batches b";

const PENDING_BATCHES: &str = "sum(kind NOT IN ('adif', 'pota') AND applied_at IS NULL AND dismissed_at IS NULL)";

pub fn document(records: &[Qso], unit: LengthUnit) -> String {
    encoded(records, Output::utf8(unit)).expect("UTF-8 represents every character")
}

pub fn encoded(records: &[Qso], output: Output) -> Result<String, UnencodableError> {
    adif::document(env!("CARGO_PKG_VERSION"), Utc::now(), records, output)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    pub records: Vec<Qso>,
    pub targets: Vec<Target>,
    pub owners: Vec<usize>,
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
        selection
            .owners
            .extend(projected.iter().map(|_| selection.targets.len()));
        selection.records.extend(projected);
        selection.targets.push(Target {
            id: row.id,
            revision: row.revision,
        });
    }
    Ok(selection)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    pub label: String,
    pub missing: Vec<String>,
    pub records: Vec<Qso>,
    pub targets: Vec<Target>,
}

pub fn split(selection: &Selection, fields: &[String]) -> Vec<Part> {
    split::group(&selection.records, fields)
        .into_iter()
        .map(|group| {
            let mut targets: Vec<Target> = Vec::new();
            for &index in &group.records {
                let target = selection.targets[selection.owners[index]];
                if targets.last() != Some(&target) {
                    targets.push(target);
                }
            }
            Part {
                label: group.label,
                missing: group.missing,
                records: group.records.iter().map(|&i| selection.records[i].clone()).collect(),
                targets,
            }
        })
        .collect()
}

pub async fn create(
    tx: &mut WriteTx,
    access: &LogbookAccess,
    kind: ExportKind,
    qsos: &[Target],
    label: Option<&str>,
    export_id: &str,
) -> Result<ExportBatch, StoreError> {
    let access = access.revalidate(tx).await?;
    if qsos.is_empty() {
        return Err(StoreError::Invalid(Fault::coded(
            "batch_empty",
            "an export batch needs a QSO",
        )));
    }
    let mut seen = BTreeSet::new();
    for target in qsos {
        if !seen.insert(target.id) {
            return Err(StoreError::Invalid(
                format!("QSO {} is given more than once", target.id).into(),
            ));
        }
        qso::fetch(tx, &access, &target.id)
            .await?
            .require_revision(target.revision)?;
    }

    let id = new_id();
    sqlx::query(
        "INSERT INTO export_batches (id, export_id, logbook_id, kind, created_by, created_at, label) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(export_id)
    .bind(access.logbook_id())
    .bind(kind.as_str())
    .bind(access.user_id())
    .bind(now())
    .bind(label)
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportPage {
    pub batches: Vec<ExportBatch>,
    pub next: Option<String>,
    pub total: i64,
}

pub async fn page(
    conn: &mut SqliteConnection,
    access: &LogbookAccess,
    state: ExportState,
    after: Option<&str>,
    limit: u32,
) -> Result<ExportPage, StoreError> {
    let access = access.revalidate(conn).await?;
    let matching = format!(
        "GROUP BY export_id HAVING {PENDING_BATCHES} {}",
        state.pending_batches()
    );
    let total = sqlx::query_scalar(AssertSqlSafe(format!(
        "SELECT count(*) FROM (SELECT 1 FROM export_batches WHERE logbook_id = ? {matching})"
    )))
    .bind(access.logbook_id())
    .fetch_one(&mut *conn)
    .await?;

    let older = if after.is_some() { "AND export_id < ?" } else { "" };
    let sql = format!(
        "SELECT export_id FROM export_batches WHERE logbook_id = ? {older} {matching} ORDER BY export_id DESC LIMIT ?"
    );
    let mut ids = sqlx::query_scalar::<_, String>(AssertSqlSafe(sql)).bind(access.logbook_id());
    if let Some(after) = after {
        ids = ids.bind(after);
    }
    let mut ids = ids.bind(i64::from(limit) + 1).fetch_all(&mut *conn).await?;
    let more = ids.len() > limit as usize;
    ids.truncate(limit as usize);

    let marks = vec!["?"; ids.len()].join(", ");
    let sql =
        format!("{SELECT} WHERE b.logbook_id = ? AND b.export_id IN ({marks}) ORDER BY b.export_id DESC, b.id DESC");
    let mut batches = sqlx::query(AssertSqlSafe(sql)).bind(access.logbook_id());
    for id in &ids {
        batches = batches.bind(id);
    }
    let batches = batches
        .fetch_all(conn)
        .await?
        .iter()
        .map(ExportBatch::from_row)
        .collect::<Result<_, _>>()?;
    Ok(ExportPage {
        batches,
        next: more.then(|| ids.pop()).flatten(),
        total,
    })
}

pub async fn apply(tx: &mut WriteTx, access: &LogbookAccess, id: &str) -> Result<Outcome, StoreError> {
    let access = access.revalidate(tx).await?;
    let batch = fetch(tx, &access, id).await?;
    apply_batch(tx, &access, &batch).await
}

pub async fn dismiss(tx: &mut WriteTx, access: &LogbookAccess, id: &str) -> Result<ExportBatch, StoreError> {
    let access = access.revalidate(tx).await?;
    let batch = fetch(tx, &access, id).await?;
    batch.pending()?;
    dismiss_batch(tx, &batch).await?;
    fetch(tx, &access, id).await
}

pub async fn undismiss(tx: &mut WriteTx, access: &LogbookAccess, id: &str) -> Result<ExportBatch, StoreError> {
    let access = access.revalidate(tx).await?;
    fetch(tx, &access, id).await?;
    sqlx::query("UPDATE export_batches SET dismissed_at = NULL WHERE id = ?")
        .bind(id)
        .execute(&mut **tx)
        .await?;
    fetch(tx, &access, id).await
}

#[derive(Debug, Default)]
pub struct ExportOutcome {
    pub batches: usize,
    pub outcome: Outcome,
}

pub async fn apply_export(
    tx: &mut WriteTx,
    access: &LogbookAccess,
    export_id: &str,
) -> Result<ExportOutcome, StoreError> {
    let access = access.revalidate(tx).await?;
    let mut result = ExportOutcome::default();
    for batch in pending_in(tx, &access, export_id).await? {
        let outcome = apply_batch(tx, &access, &batch).await?;
        result.outcome.updated.extend(outcome.updated);
        result.outcome.skipped.extend(outcome.skipped);
        result.batches += 1;
    }
    Ok(result)
}

pub async fn dismiss_export(tx: &mut WriteTx, access: &LogbookAccess, export_id: &str) -> Result<usize, StoreError> {
    let access = access.revalidate(tx).await?;
    let batches = pending_in(tx, &access, export_id).await?;
    for batch in &batches {
        dismiss_batch(tx, batch).await?;
    }
    Ok(batches.len())
}

async fn apply_batch(tx: &mut WriteTx, access: &LogbookAccess, batch: &ExportBatch) -> Result<Outcome, StoreError> {
    let (sent, date) = batch.pending()?;
    let targets = targets(tx, &batch.id).await?;
    let changes = vec![(sent, "Y".to_owned()), (date, today())];
    let outcome = bulk::apply(tx, access, &targets, &changes).await?;
    sqlx::query("UPDATE export_batches SET applied_at = ? WHERE id = ?")
        .bind(now())
        .bind(&batch.id)
        .execute(&mut **tx)
        .await?;
    Ok(outcome)
}

async fn dismiss_batch(tx: &mut WriteTx, batch: &ExportBatch) -> Result<(), StoreError> {
    sqlx::query("UPDATE export_batches SET dismissed_at = ? WHERE id = ?")
        .bind(now())
        .bind(&batch.id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn pending_in(
    conn: &mut SqliteConnection,
    access: &LogbookAccess,
    export_id: &str,
) -> Result<Vec<ExportBatch>, StoreError> {
    let batches: Vec<ExportBatch> = sqlx::query(AssertSqlSafe(format!(
        "{SELECT} WHERE b.logbook_id = ? AND b.export_id = ? ORDER BY b.created_at, b.id"
    )))
    .bind(access.logbook_id())
    .bind(export_id)
    .fetch_all(conn)
    .await?
    .iter()
    .map(ExportBatch::from_row)
    .collect::<Result<_, _>>()?;
    if batches.is_empty() {
        return Err(StoreError::NotFound("export"));
    }
    Ok(batches
        .into_iter()
        .filter(|b| b.applied_at.is_none() && b.dismissed_at.is_none() && b.kind.sent_fields().is_some())
        .collect())
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
                .map_err(|e: InvalidQsoId| StoreError::Invalid(format!("stored QSO ID {e}").into()))?;
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
            bulk::{
                Change, SkipReason,
                tests::{target, three_qsos},
            },
            testing::World,
        },
    };

    fn row(fields: &[(&str, &str)]) -> StoredQso {
        StoredQso {
            id: QsoId::generate(),
            station_id: "station".into(),
            instrument_id: None,
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

    #[test]
    fn splits_a_selection_keeping_each_target_once_per_part() {
        let rows = [
            row(&[
                ("CALL", "JA1AAA"),
                ("MY_POTA_REF", "JP-0001,JP-0002"),
                ("STATION_CALLSIGN", "JL1HIS"),
            ]),
            row(&[
                ("CALL", "JA1BBB"),
                ("MY_POTA_REF", "JP-0001"),
                ("STATION_CALLSIGN", "JL1HIS/P"),
            ]),
        ];
        let selection = project(&rows, ExportKind::Pota, None).unwrap();
        assert_eq!(selection.owners, [0, 0, 1]);

        let by_call = split(&selection, &["STATION_CALLSIGN".into()]);
        assert_eq!(by_call.len(), 2);
        assert_eq!((by_call[0].label.as_str(), by_call[0].records.len()), ("JL1HIS", 2));
        assert_eq!(
            by_call[0].targets,
            [Target {
                id: rows[0].id,
                revision: 2
            }]
        );
        assert_eq!(by_call[1].label, "JL1HIS/P");

        let by_park = split(&selection, &["MY_SIG_INFO".into()]);
        let parks: Vec<_> = by_park
            .iter()
            .map(|p| (p.label.as_str(), p.records.len(), p.targets.len()))
            .collect();
        assert_eq!(parks, [("JP-0001", 2, 2), ("JP-0002", 1, 1)]);
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
        let batch = create(&mut tx, &access, ExportKind::QslCard, &targets, None, "export")
            .await
            .unwrap();
        assert_eq!(
            (
                batch.kind,
                batch.qso_count,
                batch.applied_at.as_deref(),
                batch.label.as_deref()
            ),
            (ExportKind::QslCard, 3, None, None)
        );

        let mut sorted = targets.clone();
        sorted.sort_by_key(|t| t.id.to_string());
        assert_eq!(get(&mut tx, &access, &batch.id).await.unwrap(), (batch.clone(), sorted));

        let edited = Change {
            fields: BTreeMap::from([("QSL_SENT_VIA".to_owned(), "D".to_owned())]),
            ..Change::default()
        };
        bulk::update(&mut tx, &access, &[target(&rows[1])], &edited)
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
            create(&mut tx, &access, ExportKind::Eqsl, &[stale], None, "export").await,
            Err(StoreError::Conflict(_))
        ));
        assert!(matches!(
            create(
                &mut tx,
                &access,
                ExportKind::Eqsl,
                &[target(&rows[0]), target(&rows[0])],
                None,
                "export"
            )
            .await,
            Err(StoreError::Invalid(_))
        ));
        assert!(matches!(
            create(&mut tx, &access, ExportKind::Eqsl, &[], None, "export").await,
            Err(StoreError::Invalid(_))
        ));
        let pota = create(&mut tx, &access, ExportKind::Pota, &[target(&rows[0])], None, "export")
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
        let batch = create(
            &mut tx,
            &access,
            ExportKind::Lotw,
            &[target(&rows[0])],
            Some("JL1HIS"),
            "export",
        )
        .await
        .unwrap();
        assert_eq!(batch.label.as_deref(), Some("JL1HIS"));
        assert!(matches!(
            get(&mut tx, &other_access, &batch.id).await,
            Err(StoreError::NotFound(_))
        ));
        assert!(matches!(
            create(
                &mut tx,
                &other_access,
                ExportKind::Lotw,
                &[target(&rows[0])],
                None,
                "export"
            )
            .await,
            Err(StoreError::NotFound(_))
        ));
    }
}
