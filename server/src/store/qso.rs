use std::{collections::BTreeMap, sync::LazyLock};

use chrono::{NaiveDate, NaiveTime};
use sqlx::{
    AssertSqlSafe, Connection, Row, Sqlite, SqliteConnection,
    query::Query,
    sqlite::{SqliteArguments, SqliteRow},
};

use crate::{
    domain::{
        callsign,
        duplicate::DuplicateKey,
        qso::{Checked, Columns, InvalidQsoId, Qso, QsoId, Warning},
    },
    store::{
        StoreError, WriteTx,
        access::LogbookAccess,
        logbook::{self, next_insert, next_update},
        now, station,
    },
};

const PAGE: u32 = 500;
const MAX_LIST: usize = 100;

const BOOKKEEPING: &[&str] = &[
    "id",
    "station_id",
    "seq",
    "updated_seq",
    "revision",
    "created_by",
    "created_at",
    "updated_by",
    "updated_at",
    "deleted_at",
];

static ADIF_COLUMNS: LazyLock<Vec<String>> =
    LazyLock::new(|| Columns::NAMES.iter().map(|name| name.to_ascii_lowercase()).collect());

static SELECT: LazyLock<String> = LazyLock::new(|| {
    let columns: Vec<&str> = BOOKKEEPING
        .iter()
        .copied()
        .chain(ADIF_COLUMNS.iter().map(String::as_str))
        .chain([
            "fields",
            "(SELECT callsign FROM users WHERE users.id = qsos.created_by) AS created_by_callsign",
            "(SELECT callsign FROM users WHERE users.id = qsos.updated_by) AS updated_by_callsign",
        ])
        .collect();
    format!("SELECT {} FROM qsos", columns.join(", "))
});

static INSERT: LazyLock<String> = LazyLock::new(|| {
    let columns: Vec<&str> = [
        "id",
        "logbook_id",
        "station_id",
        "seq",
        "updated_seq",
        "created_by",
        "created_at",
        "updated_by",
        "updated_at",
    ]
    .into_iter()
    .chain(ADIF_COLUMNS.iter().map(String::as_str))
    .chain(["fields", "call_base", "qso_time", "freq_hz"])
    .collect();
    let placeholders = vec!["?"; columns.len()].join(", ");
    format!("INSERT INTO qsos ({}) VALUES ({placeholders})", columns.join(", "))
});

static UPDATE: LazyLock<String> = LazyLock::new(|| {
    let assignments: Vec<String> = ["station_id", "updated_seq", "updated_by", "updated_at"]
        .into_iter()
        .chain(ADIF_COLUMNS.iter().map(String::as_str))
        .chain(["fields", "call_base", "qso_time", "freq_hz"])
        .map(|column| format!("{column} = ?"))
        .collect();
    format!(
        "UPDATE qsos SET {}, revision = revision + 1 \
         WHERE logbook_id = ? AND id = ? AND revision = ? AND deleted_at IS NULL",
        assignments.join(", ")
    )
});

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredQso {
    pub id: QsoId,
    pub station_id: String,
    pub seq: i64,
    pub updated_seq: i64,
    pub revision: i64,
    pub created_by: String,
    pub created_by_callsign: String,
    pub created_at: String,
    pub updated_by: String,
    pub updated_by_callsign: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
    pub qso: Qso,
}

impl StoredQso {
    fn from_row(row: &SqliteRow) -> Result<StoredQso, StoreError> {
        let mut values = vec![];
        for (name, column) in Columns::NAMES.iter().zip(ADIF_COLUMNS.iter()) {
            if let Some(value) = row.try_get::<Option<String>, _>(column.as_str())? {
                values.push((name.to_string(), value));
            }
        }
        let fields: String = row.try_get("fields")?;
        let fields: BTreeMap<String, String> = serde_json::from_str(&fields)
            .map_err(|e| StoreError::Invalid(format!("stored QSO fields are not a string map: {e}")))?;
        let id: String = row.try_get("id")?;
        Ok(StoredQso {
            id: id
                .parse()
                .map_err(|e: InvalidQsoId| StoreError::Invalid(format!("stored QSO ID {e}")))?,
            station_id: row.try_get("station_id")?,
            seq: row.try_get("seq")?,
            updated_seq: row.try_get("updated_seq")?,
            revision: row.try_get("revision")?,
            created_by: row.try_get("created_by")?,
            created_by_callsign: row.try_get("created_by_callsign")?,
            created_at: row.try_get("created_at")?,
            updated_by: row.try_get("updated_by")?,
            updated_by_callsign: row.try_get("updated_by_callsign")?,
            updated_at: row.try_get("updated_at")?,
            deleted_at: row.try_get("deleted_at")?,
            qso: Qso::from_fields(values.into_iter().chain(fields))?,
        })
    }

    pub fn require_revision(&self, base: i64) -> Result<(), StoreError> {
        if self.deleted_at.is_some() {
            return Err(StoreError::Conflict(format!("QSO {} has been deleted", self.id)));
        }
        if self.revision != base {
            return Err(StoreError::Conflict(format!(
                "QSO {} is at revision {}, not {base}",
                self.id, self.revision
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TimeRange {
    pub from: Option<i64>,
    pub until: Option<i64>,
}

impl TimeRange {
    pub fn days(from: Option<NaiveDate>, until: Option<NaiveDate>) -> TimeRange {
        let start = |day: NaiveDate| day.and_time(NaiveTime::MIN).and_utc().timestamp();
        TimeRange {
            from: from.map(start),
            until: until.and_then(|day| day.succ_opt()).map(start),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub id: QsoId,
    pub revision: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallMatch {
    Exact(String),
    Base(String),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filter {
    pub time: TimeRange,
    pub station_ids: Option<Vec<String>>,
    pub band: Option<String>,
    pub mode: Option<String>,
    pub call: Option<CallMatch>,
    pub qsl_sent: Option<Vec<String>>,
    pub contest_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confirmation {
    Card,
    Lotw,
    Eqsl,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InsertionQuery {
    pub filter: Filter,
    pub mode_or_submode: Option<String>,
    pub confirmed_by: Vec<Confirmation>,
    pub after_seq: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InsertionPage {
    pub rows: Vec<StoredQso>,
    pub total: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cursor {
    pub qso_time: i64,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub rows: Vec<StoredQso>,
    pub next: Option<Cursor>,
}

const PREVIOUS_FIELDS: [&str; 7] = ["NAME", "GRIDSQUARE", "QTH", "STATE", "CNTY", "IOTA", "QSL_VIA"];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Previous {
    pub count: u64,
    pub last_qso_time: Option<i64>,
    pub fields: BTreeMap<&'static str, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QsoInput {
    pub station_id: String,
    pub qso: Qso,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Created {
    New { stored: StoredQso, warnings: Vec<Warning> },
    Existing(StoredQso),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeSet {
    pub cursor_generation: String,
    pub last_updated_seq: i64,
    pub rows: Vec<StoredQso>,
    pub has_more: bool,
}

enum Prepared {
    Existing(StoredQso),
    New {
        station_id: String,
        qso: Qso,
        checked: Checked,
    },
}

async fn prepare(
    conn: &mut SqliteConnection,
    access: &LogbookAccess,
    id: &QsoId,
    input: QsoInput,
) -> Result<Prepared, StoreError> {
    let owner: Option<String> = sqlx::query_scalar("SELECT logbook_id FROM qsos WHERE id = ?")
        .bind(id.to_string())
        .fetch_optional(&mut *conn)
        .await?;
    match owner {
        Some(logbook_id) if logbook_id == access.logbook_id() => {
            return Ok(Prepared::Existing(fetch(conn, access, id).await?));
        }
        Some(_) => return Err(StoreError::Conflict(format!("the ID {id} belongs to another QSO"))),
        None => {}
    }

    let station = station::fetch(conn, access, &input.station_id).await?;
    let mut qso = input.qso;
    qso.bake_station(&station.callsign, &station.fields)?;
    qso.fill("OPERATOR", access.callsign())?;
    let checked = check(conn, access, &qso, None).await?;
    Ok(Prepared::New {
        station_id: station.id,
        qso,
        checked,
    })
}

pub async fn preview(
    conn: &mut SqliteConnection,
    access: &LogbookAccess,
    id: &QsoId,
    input: QsoInput,
) -> Result<Option<(Qso, Vec<Warning>)>, StoreError> {
    let access = &access.revalidate(conn).await?;
    Ok(match prepare(conn, access, id, input).await? {
        Prepared::Existing(_) => None,
        Prepared::New { qso, checked, .. } => Some((qso, checked.warnings)),
    })
}

pub async fn create(
    tx: &mut WriteTx,
    access: &LogbookAccess,
    id: &QsoId,
    input: QsoInput,
) -> Result<Created, StoreError> {
    let access = &access.revalidate(tx).await?;
    let (station_id, qso, checked) = match prepare(tx, access, id, input).await? {
        Prepared::Existing(stored) => return Ok(Created::Existing(stored)),
        Prepared::New {
            station_id,
            qso,
            checked,
        } => (station_id, qso, checked),
    };

    let sequence = next_insert(tx, access.logbook_id()).await?;
    let now = now();
    let mut query = sqlx::query(AssertSqlSafe(INSERT.as_str()))
        .bind(id.to_string())
        .bind(access.logbook_id())
        .bind(&station_id)
        .bind(sequence.seq)
        .bind(sequence.updated_seq)
        .bind(access.user_id())
        .bind(&now)
        .bind(access.user_id())
        .bind(&now);
    for name in Columns::NAMES {
        query = query.bind(qso.columns.get(name));
    }
    query
        .bind(fields_json(&qso))
        .bind(&checked.derived.call_base)
        .bind(checked.derived.qso_time)
        .bind(checked.derived.freq_hz)
        .execute(&mut **tx)
        .await?;

    Ok(Created::New {
        stored: fetch(tx, access, id).await?,
        warnings: checked.warnings,
    })
}

pub async fn update(
    tx: &mut WriteTx,
    access: &LogbookAccess,
    id: &QsoId,
    base_revision: i64,
    input: QsoInput,
) -> Result<(StoredQso, Vec<Warning>), StoreError> {
    access.revalidate(tx).await?;
    let current = fetch(tx, access, id).await?;
    current.require_revision(base_revision)?;
    let station = station::fetch(tx, access, &input.station_id).await?;
    let checked = check(tx, access, &input.qso, Some(id)).await?;
    let stored = write_update(tx, access, &current, &station.id, &input.qso, &checked).await?;
    Ok((stored, checked.warnings))
}

pub(crate) async fn rewrite(
    tx: &mut WriteTx,
    access: &LogbookAccess,
    current: &StoredQso,
    qso: &Qso,
) -> Result<StoredQso, StoreError> {
    let checked = qso.check()?;
    write_update(tx, access, current, &current.station_id, qso, &checked).await
}

async fn write_update(
    tx: &mut WriteTx,
    access: &LogbookAccess,
    current: &StoredQso,
    station_id: &str,
    qso: &Qso,
    checked: &Checked,
) -> Result<StoredQso, StoreError> {
    let updated_seq = next_update(tx, access.logbook_id()).await?;
    let mut query = sqlx::query(AssertSqlSafe(UPDATE.as_str()))
        .bind(station_id)
        .bind(updated_seq)
        .bind(access.user_id())
        .bind(now());
    for name in Columns::NAMES {
        query = query.bind(qso.columns.get(name));
    }
    let updated = query
        .bind(fields_json(qso))
        .bind(&checked.derived.call_base)
        .bind(checked.derived.qso_time)
        .bind(checked.derived.freq_hz)
        .bind(access.logbook_id())
        .bind(current.id.to_string())
        .bind(current.revision)
        .execute(&mut **tx)
        .await?;
    if updated.rows_affected() == 0 {
        return Err(StoreError::Conflict(format!(
            "QSO {} changed while it was being updated",
            current.id
        )));
    }
    fetch(tx, access, &current.id).await
}

pub async fn delete(
    tx: &mut WriteTx,
    access: &LogbookAccess,
    id: &QsoId,
    base_revision: i64,
) -> Result<StoredQso, StoreError> {
    access.revalidate(tx).await?;
    let current = fetch(tx, access, id).await?;
    if current.deleted_at.is_some() {
        return Ok(current);
    }
    current.require_revision(base_revision)?;
    let updated_seq = next_update(tx, access.logbook_id()).await?;
    let now = now();
    sqlx::query(
        "UPDATE qsos SET deleted_at = ?, updated_seq = ?, revision = revision + 1, updated_by = ?, updated_at = ? \
         WHERE logbook_id = ? AND id = ?",
    )
    .bind(&now)
    .bind(updated_seq)
    .bind(access.user_id())
    .bind(&now)
    .bind(access.logbook_id())
    .bind(id.to_string())
    .execute(&mut **tx)
    .await?;
    fetch(tx, access, id).await
}

pub async fn get(conn: &mut SqliteConnection, access: &LogbookAccess, id: &QsoId) -> Result<StoredQso, StoreError> {
    access.revalidate(conn).await?;
    fetch(conn, access, id).await
}

pub async fn changes(
    conn: &mut SqliteConnection,
    access: &LogbookAccess,
    after: i64,
    limit: u32,
) -> Result<ChangeSet, StoreError> {
    let mut tx = conn.begin().await?;
    let logbook = logbook::get(&mut tx, access).await?;
    let sql = format!(
        "{} WHERE logbook_id = ? AND updated_seq > ? AND updated_seq <= ? ORDER BY updated_seq LIMIT ?",
        *SELECT
    );
    let mut rows = sqlx::query(AssertSqlSafe(sql))
        .bind(access.logbook_id())
        .bind(after)
        .bind(logbook.last_updated_seq)
        .bind(i64::from(limit) + 1)
        .fetch_all(&mut *tx)
        .await?
        .iter()
        .map(StoredQso::from_row)
        .collect::<Result<Vec<_>, _>>()?;
    let has_more = rows.len() > limit as usize;
    rows.truncate(limit as usize);
    Ok(ChangeSet {
        cursor_generation: logbook.cursor_generation,
        last_updated_seq: logbook.last_updated_seq,
        rows,
        has_more,
    })
}

pub async fn search(
    conn: &mut SqliteConnection,
    access: &LogbookAccess,
    filter: &Filter,
    after: Option<&Cursor>,
    limit: u32,
) -> Result<Page, StoreError> {
    access.revalidate(conn).await?;
    if limit == 0 {
        return Err(StoreError::Invalid("a page needs at least one row".into()));
    }
    page(conn, access, filter, after, limit).await
}

pub async fn search_all(
    conn: &mut SqliteConnection,
    access: &LogbookAccess,
    filter: &Filter,
) -> Result<Vec<StoredQso>, StoreError> {
    access.revalidate(conn).await?;
    let mut tx = conn.begin().await?;
    let mut rows = vec![];
    let mut after = None;
    loop {
        let page = page(&mut tx, access, filter, after.as_ref(), PAGE).await?;
        rows.extend(page.rows);
        match page.next {
            Some(next) => after = Some(next),
            None => return Ok(rows),
        }
    }
}

async fn page(
    conn: &mut SqliteConnection,
    access: &LogbookAccess,
    filter: &Filter,
    after: Option<&Cursor>,
    limit: u32,
) -> Result<Page, StoreError> {
    let mut conditions = Conditions::of(access, filter)?;
    if let Some(after) = after {
        conditions.push(
            "(qso_time, id) < (?, ?)",
            [Bind::Int(after.qso_time), Bind::Text(after.id.clone())],
        );
    }

    let sql = format!(
        "{} WHERE {} ORDER BY qso_time DESC, id DESC LIMIT ?",
        *SELECT,
        conditions.sql()
    );
    let mut rows = conditions
        .bind(sqlx::query(AssertSqlSafe(sql)))
        .bind(i64::from(limit) + 1)
        .fetch_all(conn)
        .await?
        .iter()
        .map(StoredQso::from_row)
        .collect::<Result<Vec<_>, _>>()?;
    let next = match rows.len() > limit as usize {
        true => {
            rows.truncate(limit as usize);
            let last = rows.last().expect("a full page has a last row");
            Some(Cursor {
                qso_time: last.qso.qso_time()?,
                id: last.id.to_string(),
            })
        }
        false => None,
    };
    Ok(Page { rows, next })
}

pub async fn previous(
    conn: &mut SqliteConnection,
    access: &LogbookAccess,
    call: &str,
    station_ids: Option<Vec<String>>,
) -> Result<Previous, StoreError> {
    access.revalidate(conn).await?;
    let filter = Filter {
        call: Some(CallMatch::Base(call.to_owned())),
        station_ids,
        ..Filter::default()
    };
    let conditions = Conditions::of(access, &filter)?;
    let sql = format!("SELECT count(*), max(qso_time) FROM qsos WHERE {}", conditions.sql());
    let row = conditions
        .bind(sqlx::query(AssertSqlSafe(sql)))
        .fetch_one(&mut *conn)
        .await?;
    let mut previous = Previous {
        count: row.try_get::<i64, _>(0)? as u64,
        last_qso_time: row.try_get(1)?,
        fields: BTreeMap::new(),
    };

    let mut after = None;
    while previous.count > 0 {
        let page = page(conn, access, &filter, after.as_ref(), PAGE).await?;
        for row in page.rows {
            for name in PREVIOUS_FIELDS {
                if !previous.fields.contains_key(name)
                    && let Some(value) = row.qso.get(name).map(str::trim).filter(|v| !v.is_empty())
                {
                    previous.fields.insert(name, value.to_owned());
                }
            }
        }
        after = page.next;
        if after.is_none() || previous.fields.len() == PREVIOUS_FIELDS.len() {
            break;
        }
    }
    Ok(previous)
}

pub async fn by_insertion(
    conn: &mut SqliteConnection,
    access: &LogbookAccess,
    query: &InsertionQuery,
    offset: u64,
    limit: u32,
) -> Result<InsertionPage, StoreError> {
    access.revalidate(conn).await?;
    if limit == 0 {
        return Err(StoreError::Invalid("a page needs at least one row".into()));
    }
    let mut conditions = Conditions::of(access, &query.filter)?;
    conditions.push("seq > ?", [Bind::Int(query.after_seq)]);
    if let Some(mode) = &query.mode_or_submode {
        let mode = mode.trim().to_owned();
        conditions.push(
            "(trim(mode) = ? COLLATE NOCASE OR trim(coalesce(submode, '')) = ? COLLATE NOCASE)",
            [Bind::Text(mode.clone()), Bind::Text(mode)],
        );
    }
    if !query.confirmed_by.is_empty() {
        let any: Vec<&str> = query
            .confirmed_by
            .iter()
            .map(|confirmation| match confirmation {
                Confirmation::Card => "upper(trim(coalesce(qsl_rcvd, ''))) IN ('Y', 'V')",
                Confirmation::Lotw => "upper(trim(coalesce(lotw_qsl_rcvd, ''))) IN ('Y', 'V')",
                Confirmation::Eqsl => "upper(trim(coalesce(eqsl_qsl_rcvd, ''))) IN ('Y', 'V')",
            })
            .collect();
        conditions.push(format!("({})", any.join(" OR ")), []);
    }
    let offset = i64::try_from(offset).map_err(|_| StoreError::Invalid("the page is out of range".into()))?;

    let mut tx = conn.begin().await?;
    let count_sql = format!("SELECT count(*) FROM qsos WHERE {}", conditions.sql());
    let total: i64 = conditions
        .bind(sqlx::query(AssertSqlSafe(count_sql)))
        .fetch_one(&mut *tx)
        .await?
        .try_get(0)?;
    let sql = format!("{} WHERE {} ORDER BY seq LIMIT ? OFFSET ?", *SELECT, conditions.sql());
    let rows = conditions
        .bind(sqlx::query(AssertSqlSafe(sql)))
        .bind(i64::from(limit))
        .bind(offset)
        .fetch_all(&mut *tx)
        .await?
        .iter()
        .map(StoredQso::from_row)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(InsertionPage {
        rows,
        total: total.try_into().unwrap_or_default(),
    })
}

pub async fn contest_ids(conn: &mut SqliteConnection, access: &LogbookAccess) -> Result<Vec<String>, StoreError> {
    access.revalidate(conn).await?;
    Ok(sqlx::query_scalar(
        "SELECT min(trim(contest_id)) FROM qsos \
         WHERE logbook_id = ? AND deleted_at IS NULL AND trim(coalesce(contest_id, '')) <> '' \
         GROUP BY upper(trim(contest_id)) ORDER BY upper(trim(contest_id))",
    )
    .bind(access.logbook_id())
    .fetch_all(conn)
    .await?)
}

enum Bind {
    Text(String),
    Int(i64),
}

struct Conditions {
    clauses: Vec<String>,
    binds: Vec<Bind>,
}

impl Conditions {
    fn of(access: &LogbookAccess, filter: &Filter) -> Result<Conditions, StoreError> {
        let mut conditions = Conditions {
            clauses: vec![],
            binds: vec![],
        };
        conditions.push("logbook_id = ?", [Bind::Text(access.logbook_id().to_owned())]);
        conditions.push("deleted_at IS NULL", []);
        let text = |value: &str| [Bind::Text(value.trim().to_owned())];
        if let Some(station_ids) = &filter.station_ids {
            conditions.any_of("station_id", station_ids.iter().cloned())?;
        }
        if let Some(band) = &filter.band {
            conditions.push("trim(band) = ? COLLATE NOCASE", text(band));
        }
        if let Some(mode) = &filter.mode {
            conditions.push("trim(mode) = ? COLLATE NOCASE", text(mode));
        }
        match &filter.call {
            Some(CallMatch::Exact(call)) => conditions.push("trim(call) = ? COLLATE NOCASE", text(call)),
            Some(CallMatch::Base(call)) => conditions.push("call_base = ?", [Bind::Text(callsign::base(call))]),
            None => {}
        }
        if let Some(contest_id) = &filter.contest_id {
            conditions.push("trim(contest_id) = ? COLLATE NOCASE", text(contest_id));
        }
        if let Some(values) = &filter.qsl_sent {
            conditions.any_of(
                "upper(trim(coalesce(qsl_sent, '')))",
                values.iter().map(|v| v.trim().to_ascii_uppercase()),
            )?;
        }
        if let Some(from) = filter.time.from {
            conditions.push("qso_time >= ?", [Bind::Int(from)]);
        }
        if let Some(until) = filter.time.until {
            conditions.push("qso_time < ?", [Bind::Int(until)]);
        }
        Ok(conditions)
    }

    fn push(&mut self, clause: impl Into<String>, binds: impl IntoIterator<Item = Bind>) {
        self.clauses.push(clause.into());
        self.binds.extend(binds);
    }

    fn any_of(&mut self, expression: &str, values: impl IntoIterator<Item = String>) -> Result<(), StoreError> {
        let values: Vec<Bind> = values.into_iter().map(Bind::Text).collect();
        let clause = match values.len() {
            0 => "0".to_owned(),
            n if n > MAX_LIST => {
                return Err(StoreError::Invalid(format!(
                    "a filter lists {n} values; at most {MAX_LIST} are taken"
                )));
            }
            n => format!("{expression} IN ({})", vec!["?"; n].join(", ")),
        };
        self.push(clause, values);
        Ok(())
    }

    fn sql(&self) -> String {
        self.clauses.join(" AND ")
    }

    fn bind<'q>(&self, mut query: Query<'q, Sqlite, SqliteArguments>) -> Query<'q, Sqlite, SqliteArguments> {
        for bind in &self.binds {
            query = match bind {
                Bind::Text(value) => query.bind(value.clone()),
                Bind::Int(value) => query.bind(*value),
            };
        }
        query
    }
}

pub async fn possible_duplicates(
    conn: &mut SqliteConnection,
    access: &LogbookAccess,
    qso: &Qso,
    except: Option<&QsoId>,
) -> Result<Vec<StoredQso>, StoreError> {
    access.revalidate(conn).await?;
    duplicates_of(conn, access, qso, except).await
}

async fn duplicates_of(
    conn: &mut SqliteConnection,
    access: &LogbookAccess,
    qso: &Qso,
    except: Option<&QsoId>,
) -> Result<Vec<StoredQso>, StoreError> {
    let key = DuplicateKey::of(qso)?;
    let sql = format!(
        "{} WHERE logbook_id = ? AND call_base = ? AND qso_time BETWEEN ? AND ? AND deleted_at IS NULL \
         ORDER BY qso_time, id",
        *SELECT
    );
    let rows = sqlx::query(AssertSqlSafe(sql))
        .bind(access.logbook_id())
        .bind(callsign::base(&key.call))
        .bind(key.time_range().start())
        .bind(key.time_range().end())
        .fetch_all(conn)
        .await?
        .iter()
        .map(StoredQso::from_row)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows
        .into_iter()
        .filter(|row| Some(&row.id) != except && DuplicateKey::of(&row.qso).is_ok_and(|other| key.matches(&other)))
        .collect())
}

async fn check(
    conn: &mut SqliteConnection,
    access: &LogbookAccess,
    qso: &Qso,
    except: Option<&QsoId>,
) -> Result<Checked, StoreError> {
    let mut checked = qso.check()?;
    let ids: Vec<String> = duplicates_of(conn, access, qso, except)
        .await?
        .into_iter()
        .map(|row| row.id.to_string())
        .collect();
    if !ids.is_empty() {
        checked.warnings.push(Warning::PossibleDuplicate { ids });
    }
    Ok(checked)
}

pub(crate) async fn of_station(
    conn: &mut SqliteConnection,
    access: &LogbookAccess,
    station_id: &str,
    range: TimeRange,
) -> Result<Vec<StoredQso>, StoreError> {
    let sql = format!(
        "{} WHERE logbook_id = ? AND station_id = ? AND deleted_at IS NULL \
         AND qso_time >= ? AND qso_time < ? ORDER BY qso_time, id",
        *SELECT
    );
    sqlx::query(AssertSqlSafe(sql))
        .bind(access.logbook_id())
        .bind(station_id)
        .bind(range.from.unwrap_or(i64::MIN))
        .bind(range.until.unwrap_or(i64::MAX))
        .fetch_all(conn)
        .await?
        .iter()
        .map(StoredQso::from_row)
        .collect()
}

pub(crate) async fn fetch(
    conn: &mut SqliteConnection,
    access: &LogbookAccess,
    id: &QsoId,
) -> Result<StoredQso, StoreError> {
    let sql = format!("{} WHERE logbook_id = ? AND id = ?", *SELECT);
    let row = sqlx::query(AssertSqlSafe(sql))
        .bind(access.logbook_id())
        .bind(id.to_string())
        .fetch_optional(conn)
        .await?
        .ok_or(StoreError::NotFound("QSO"))?;
    StoredQso::from_row(&row)
}

fn fields_json(qso: &Qso) -> String {
    serde_json::to_string(&qso.fields).expect("a string map serializes")
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::{
        domain::qso::QsoError,
        store::{
            membership,
            station::Station,
            testing::{World, contact, station_input},
        },
    };

    fn entry(station: &Station, call: &str, time_on: &str) -> QsoInput {
        let mut qso = contact(call, time_on);
        for (name, value) in [("FREQ", "7.041"), ("NAME", "太郎"), ("APP_HISLOG_NOTE", "x")] {
            qso.set(name, Some(value.to_owned())).unwrap();
        }
        QsoInput {
            station_id: station.id.clone(),
            qso,
        }
    }

    async fn create_one(world: &World, access: &LogbookAccess, input: QsoInput) -> (QsoId, Created) {
        let id = QsoId::generate();
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        let created = create(&mut tx, access, &id, input).await.unwrap();
        tx.commit().await.unwrap();
        (id, created)
    }

    fn new(created: Created) -> (StoredQso, Vec<Warning>) {
        match created {
            Created::New { stored, warnings } => (stored, warnings),
            Created::Existing(_) => panic!("expected a new QSO"),
        }
    }

    #[tokio::test]
    async fn creates_a_qso_with_station_values_baked_in() {
        let world = World::new().await;
        let station = world.station("Home").await;
        let access = world.access(&world.operator).await;
        let (id, created) = create_one(&world, &access, entry(&station, "JA1XYZ", "040506")).await;
        let (stored, warnings) = new(created);

        assert_eq!(stored.id, id);
        assert_eq!((stored.seq, stored.updated_seq, stored.revision), (1, 1, 1));
        assert_eq!(
            (stored.created_by.as_str(), stored.station_id.as_str()),
            (world.operator.id.as_str(), station.id.as_str())
        );
        assert!(warnings.is_empty());
        assert_eq!(stored.qso.get("STATION_CALLSIGN"), Some("JL1HIS/1"));
        assert_eq!(stored.qso.get("MY_CITY"), Some("Minato"));
        assert_eq!(stored.qso.get("OPERATOR"), Some("JA1OPR"));
        assert_eq!(stored.qso.get("NAME"), Some("太郎"));
        assert_eq!(stored.qso.fields.get("APP_HISLOG_NOTE").map(String::as_str), Some("x"));

        let (call_base, qso_time, freq_hz): (String, i64, Option<i64>) =
            sqlx::query_as("SELECT call_base, qso_time, freq_hz FROM qsos WHERE id = ?")
                .bind(stored.id.to_string())
                .fetch_one(&world.db)
                .await
                .unwrap();
        assert_eq!(
            (call_base.as_str(), qso_time, freq_hz),
            ("JA1XYZ", 1_792_641_906, Some(7_041_000))
        );
    }

    #[tokio::test]
    async fn returns_the_existing_row_when_an_id_is_resent() {
        let world = World::new().await;
        let station = world.station("Home").await;
        let access = world.access(&world.operator).await;
        let (id, created) = create_one(&world, &access, entry(&station, "JA1XYZ", "040506")).await;
        let (first, _) = new(created);

        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        let again = create(&mut tx, &access, &id, entry(&station, "JA1ZZZ", "050000"))
            .await
            .unwrap();
        tx.commit().await.unwrap();
        assert_eq!(again, Created::Existing(first));

        let mut conn = world.db.acquire().await.unwrap();
        let changes = changes(&mut conn, &access, 0, 10).await.unwrap();
        assert_eq!(changes.last_updated_seq, 1);
    }

    #[tokio::test]
    async fn refuses_an_id_used_by_another_logbook() {
        let world = World::new().await;
        let station = world.station("Home").await;
        let access = world.access(&world.operator).await;
        let (id, _) = create_one(&world, &access, entry(&station, "JA1XYZ", "040506")).await;

        let (_, other_access) = world.other_logbook().await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        let other_station = station::create(&mut tx, &other_access.owner().unwrap(), &station_input("Away"))
            .await
            .unwrap();
        let refused = create(&mut tx, &other_access, &id, entry(&other_station, "JA1XYZ", "040506")).await;
        assert!(matches!(refused, Err(StoreError::Conflict(_))), "{refused:?}");
        assert!(matches!(
            get(&mut tx, &other_access, &id).await,
            Err(StoreError::NotFound("QSO"))
        ));
    }

    #[tokio::test]
    async fn refuses_a_station_of_another_logbook_and_invalid_records() {
        let world = World::new().await;
        let station = world.station("Home").await;
        let access = world.access(&world.operator).await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();

        let mut elsewhere = entry(&station, "JA1XYZ", "040506");
        elsewhere.station_id = "no-such-station".into();
        assert!(matches!(
            create(&mut tx, &access, &QsoId::generate(), elsewhere).await,
            Err(StoreError::NotFound("Station"))
        ));

        let mut invalid = entry(&station, "JA1XYZ", "040506");
        invalid.qso.set("BAND", Some("41m".into())).unwrap();
        assert!(matches!(
            create(&mut tx, &access, &QsoId::generate(), invalid).await,
            Err(StoreError::Qso(QsoError::Invalid { name, .. })) if name == "BAND"
        ));

        let mut conn = world.db.acquire().await.unwrap();
        let logbook = logbook::get(&mut conn, &access).await.unwrap();
        assert_eq!((logbook.last_seq, logbook.last_updated_seq), (0, 0));
    }

    #[tokio::test]
    async fn warns_about_possible_duplicates_without_refusing() {
        let world = World::new().await;
        let station = world.station("Home").await;
        let access = world.access(&world.operator).await;
        let (_, first) = create_one(&world, &access, entry(&station, "JA1XYZ", "040506")).await;
        let (first, _) = new(first);

        let (_, second) = create_one(&world, &access, entry(&station, "ja1xyz", "041000")).await;
        let (second, warnings) = new(second);
        assert_eq!(
            warnings,
            [Warning::PossibleDuplicate {
                ids: vec![first.id.to_string()]
            }]
        );

        let (_, far) = create_one(&world, &access, entry(&station, "JA1XYZ", "060000")).await;
        assert_eq!(new(far).1, []);

        let mut conn = world.db.acquire().await.unwrap();
        let candidate = entry(&station, "JA1XYZ", "0415").qso;
        assert_eq!(
            possible_duplicates(&mut conn, &access, &candidate, None).await.unwrap(),
            [first.clone(), second]
        );
        assert_eq!(
            possible_duplicates(&mut conn, &access, &candidate, Some(&first.id))
                .await
                .unwrap()
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn updates_against_the_revision_it_was_based_on() {
        let world = World::new().await;
        let station = world.station("Home").await;
        let operator = world.access(&world.operator).await;
        let (id, created) = create_one(&world, &operator, entry(&station, "JA1XYZ", "040506")).await;
        let (stored, _) = new(created);

        let owner = world.access(&world.owner).await;
        let mut edited = stored.qso.clone();
        edited.set("NAME", Some("花子".into())).unwrap();
        edited.set("MY_RIG", Some("IC-705".into())).unwrap();
        let input = QsoInput {
            station_id: station.id.clone(),
            qso: edited,
        };
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        let (updated, warnings) = update(&mut tx, &owner, &id, 1, input.clone()).await.unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!((updated.revision, updated.seq, updated.updated_seq), (2, 1, 2));
        assert_eq!(updated.updated_by, world.owner.id);
        assert_eq!(updated.qso.get("NAME"), Some("花子"));
        assert_eq!(updated.qso.get("MY_RIG"), Some("IC-705"));

        let stale = update(&mut tx, &operator, &id, 1, input).await;
        assert!(matches!(stale, Err(StoreError::Conflict(_))), "{stale:?}");
    }

    #[tokio::test]
    async fn deletes_with_a_tombstone_once() {
        let world = World::new().await;
        let station = world.station("Home").await;
        let access = world.access(&world.operator).await;
        let (id, _) = create_one(&world, &access, entry(&station, "JA1XYZ", "040506")).await;

        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        assert!(matches!(
            delete(&mut tx, &access, &id, 7).await,
            Err(StoreError::Conflict(_))
        ));
        let deleted = delete(&mut tx, &access, &id, 1).await.unwrap();
        assert!(deleted.deleted_at.is_some());
        assert_eq!((deleted.revision, deleted.updated_seq), (2, 2));
        assert_eq!(deleted.qso.get("CALL"), Some("JA1XYZ"));
        assert_eq!(delete(&mut tx, &access, &id, 1).await.unwrap(), deleted);

        let input = entry(&station, "JA1XYZ", "040506");
        assert!(matches!(
            update(&mut tx, &access, &id, 2, input.clone()).await,
            Err(StoreError::Conflict(_))
        ));
        let later = create(&mut tx, &access, &QsoId::generate(), input).await.unwrap();
        assert!(matches!(later, Created::New { warnings, .. } if warnings.is_empty()));
    }

    async fn four_qsos(world: &World) -> (LogbookAccess, Station, Vec<QsoId>) {
        let station = world.station("Home").await;
        let access = world.access(&world.operator).await;
        let mut ids = vec![];
        for (call, time_on, mode, qsl_sent) in [
            ("JA1AAA", "010000", "CW", Some("R")),
            ("ja1aaa/p", "020000", "cw", None),
            ("JA1BBB", "030000", "SSB", Some("Y")),
            ("JA1AAA", "040000", "CW", Some("q")),
        ] {
            let mut input = entry(&station, call, time_on);
            input.qso.set("MODE", Some(mode.into())).unwrap();
            input.qso.set("QSL_SENT", qsl_sent.map(str::to_owned)).unwrap();
            let (id, _) = create_one(world, &access, input).await;
            ids.push(id);
        }
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        delete(&mut tx, &access, &ids[3], 1).await.unwrap();
        tx.commit().await.unwrap();
        (access, station, ids)
    }

    fn found(page: &Page) -> Vec<QsoId> {
        page.rows.iter().map(|r| r.id).collect()
    }

    #[tokio::test]
    async fn searches_newest_first_with_keyset_pages() {
        let world = World::new().await;
        let (access, _, ids) = four_qsos(&world).await;
        let mut conn = world.db.acquire().await.unwrap();
        let all = Filter::default();
        let first = search(&mut conn, &access, &all, None, 2).await.unwrap();
        assert_eq!(found(&first), [ids[2], ids[1]]);
        let second = search(&mut conn, &access, &all, first.next.as_ref(), 2).await.unwrap();
        assert_eq!((found(&second), second.next), (vec![ids[0]], None));
        assert!(matches!(
            search(&mut conn, &access, &all, None, 0).await,
            Err(StoreError::Invalid(_))
        ));

        let everything = search_all(&mut conn, &access, &all).await.unwrap();
        assert_eq!(
            everything.iter().map(|r| r.id).collect::<Vec<_>>(),
            [ids[2], ids[1], ids[0]]
        );
    }

    #[rstest]
    #[case::base_call_and_mode(
        |_: &Station| Filter { call: Some(CallMatch::Base("JA1AAA/1".into())), mode: Some(" Cw".into()), ..Filter::default() },
        &[1, 0]
    )]
    #[case::exact_call(|_: &Station| Filter { call: Some(CallMatch::Exact("ja1aaa".into())), ..Filter::default() }, &[0])]
    #[case::qsl_sent(|_: &Station| Filter { qsl_sent: Some(vec!["r".into(), "".into()]), ..Filter::default() }, &[1, 0])]
    #[case::no_qsl_sent(|_: &Station| Filter { qsl_sent: Some(vec![]), ..Filter::default() }, &[])]
    #[case::time_station_and_band(
        |station: &Station| Filter {
            time: TimeRange { from: Some(1792634400), until: Some(1792638000) },
            station_ids: Some(vec![station.id.clone()]),
            band: Some("40M".into()),
            ..Filter::default()
        },
        &[1]
    )]
    #[case::other_station(|_: &Station| Filter { station_ids: Some(vec!["other".into()]), ..Filter::default() }, &[])]
    #[tokio::test]
    async fn filters_searches(#[case] filter: fn(&Station) -> Filter, #[case] expected: &[usize]) {
        let world = World::new().await;
        let (access, station, ids) = four_qsos(&world).await;
        let mut conn = world.db.acquire().await.unwrap();
        let page = search(&mut conn, &access, &filter(&station), None, 10).await.unwrap();
        let expected: Vec<QsoId> = expected.iter().map(|i| ids[*i]).collect();
        assert_eq!(found(&page), expected);
    }

    #[tokio::test]
    async fn lists_changes_after_a_cursor_including_tombstones() {
        let world = World::new().await;
        let station = world.station("Home").await;
        let access = world.access(&world.operator).await;
        let mut ids = vec![];
        for minute in ["0400", "0420", "0440"] {
            ids.push(create_one(&world, &access, entry(&station, "JA1XYZ", minute)).await.0);
        }
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        delete(&mut tx, &access, &ids[0], 1).await.unwrap();
        tx.commit().await.unwrap();

        let mut conn = world.db.acquire().await.unwrap();
        let page = changes(&mut conn, &access, 0, 2).await.unwrap();
        assert_eq!(page.last_updated_seq, 4);
        assert!(page.has_more);
        let seqs: Vec<_> = page.rows.iter().map(|r| r.updated_seq).collect();
        assert_eq!(seqs, [2, 3]);

        let rest = changes(&mut conn, &access, 3, 2).await.unwrap();
        assert!(!rest.has_more);
        assert_eq!(rest.rows.len(), 1);
        assert_eq!(rest.rows[0].id, ids[0]);
        assert!(rest.rows[0].deleted_at.is_some());
        assert_eq!(rest.cursor_generation, world.logbook.cursor_generation);
    }

    #[rstest]
    #[case("CALL")]
    #[case("call")]
    #[case("QSL_SENT")]
    #[case("APP:NOTE")]
    #[case("app_note")]
    #[tokio::test]
    async fn rejects_invalid_field_layout_before_writing(#[case] name: &str) {
        let world = World::new().await;
        let station = world.station("Home").await;
        let access = world.access(&world.operator).await;
        let (id, created) = create_one(&world, &access, entry(&station, "JA1XYZ", "040506")).await;
        let (original, _) = new(created);
        let mut invalid = QsoInput {
            station_id: station.id.clone(),
            qso: original.qso.clone(),
        };
        invalid.qso.fields.insert(name.into(), "x".into());

        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        assert!(matches!(
            create(&mut tx, &access, &QsoId::generate(), invalid.clone()).await,
            Err(StoreError::Qso(_))
        ));
        assert!(matches!(
            update(&mut tx, &access, &id, original.revision, invalid).await,
            Err(StoreError::Qso(_))
        ));
        tx.commit().await.unwrap();

        let mut conn = world.db.acquire().await.unwrap();
        assert_eq!(get(&mut conn, &access, &id).await.unwrap(), original);
        let book = logbook::get(&mut conn, &access).await.unwrap();
        assert_eq!((book.last_seq, book.last_updated_seq), (1, 1));
        let page = changes(&mut conn, &access, 0, 10).await.unwrap();
        assert_eq!(page.rows, [original]);
        drop(conn);

        let (_, created) = create_one(&world, &access, entry(&station, "JA1NEW", "050000")).await;
        let (stored, _) = new(created);
        assert_eq!((stored.seq, stored.updated_seq), (2, 2));
    }

    #[rstest]
    #[case::removed(false)]
    #[case::disabled(true)]
    #[tokio::test]
    async fn rejects_stale_access_for_qso_operations(#[case] disable: bool) {
        let world = World::new().await;
        let station = world.station("Home").await;
        let access = world.access(&world.operator).await;
        let (id, created) = create_one(&world, &access, entry(&station, "JA1XYZ", "040506")).await;
        let (stored, _) = new(created);
        let input = QsoInput {
            station_id: station.id,
            qso: stored.qso.clone(),
        };

        let owner = world.access(&world.owner).await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        if disable {
            sqlx::query("UPDATE users SET disabled_at = ? WHERE id = ?")
                .bind(now())
                .bind(&world.operator.id)
                .execute(&mut *tx)
                .await
                .unwrap();
        } else {
            membership::remove(&mut tx, &owner.owner().unwrap(), &world.operator.id)
                .await
                .unwrap();
        }
        tx.commit().await.unwrap();

        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        assert!(matches!(get(&mut tx, &access, &id).await, Err(StoreError::NotFound(_))));
        assert!(matches!(
            changes(&mut tx, &access, 0, 10).await,
            Err(StoreError::NotFound(_))
        ));
        assert!(matches!(
            possible_duplicates(&mut tx, &access, &input.qso, None).await,
            Err(StoreError::NotFound(_))
        ));
        assert!(matches!(
            search_all(&mut tx, &access, &Filter::default()).await,
            Err(StoreError::NotFound(_))
        ));
        for create_id in [id, QsoId::generate()] {
            assert!(matches!(
                create(&mut tx, &access, &create_id, input.clone()).await,
                Err(StoreError::NotFound(_))
            ));
        }
        assert!(matches!(
            update(&mut tx, &access, &id, 1, input).await,
            Err(StoreError::NotFound(_))
        ));
        assert!(matches!(
            delete(&mut tx, &access, &id, 1).await,
            Err(StoreError::NotFound(_))
        ));
        tx.commit().await.unwrap();

        let mut conn = world.db.acquire().await.unwrap();
        assert_eq!(get(&mut conn, &owner, &id).await.unwrap(), stored);
        let book = logbook::get(&mut conn, &owner).await.unwrap();
        assert_eq!((book.last_seq, book.last_updated_seq), (1, 1));
    }

    #[test]
    fn turns_days_into_a_time_range() {
        let day = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
        let range = TimeRange::days(Some(day("2026-10-01")), Some(day("2026-10-31")));
        assert_eq!(range.from, Some(1_790_812_800));
        assert_eq!(range.until, Some(1_790_812_800 + 31 * 86_400));
        assert_eq!(TimeRange::days(None, None), TimeRange::default());
    }
}
