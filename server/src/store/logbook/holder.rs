use sqlx::{AssertSqlSafe, SqliteConnection};

use crate::{
    domain::qso::Qso,
    store::{
        StoreError, WriteTx,
        access::LogbookAccess,
        now,
        qso::{self, Holder, StoredQso, TimeRange},
    },
};

struct Tables {
    rows: &'static str,
    favorites: &'static str,
    column: &'static str,
    what: &'static str,
}

fn tables(holder: Holder<'_>) -> (Tables, &str) {
    match holder {
        Holder::Station(id) => (
            Tables {
                rows: "stations",
                favorites: "station_favorites",
                column: "station_id",
                what: "Station",
            },
            id,
        ),
        Holder::Instrument(id) => (
            Tables {
                rows: "instruments",
                favorites: "instrument_favorites",
                column: "instrument_id",
                what: "Instrument",
            },
            id,
        ),
    }
}

pub(super) async fn archive(
    tx: &mut WriteTx,
    access: &LogbookAccess,
    holder: Holder<'_>,
    archived: bool,
) -> Result<(), StoreError> {
    let (tables, id) = tables(holder);
    let now = now();
    let sql = format!(
        "UPDATE {} SET archived_at = CASE WHEN ? THEN coalesce(archived_at, ?) END, updated_at = ? \
         WHERE logbook_id = ? AND id = ?",
        tables.rows
    );
    let updated = sqlx::query(AssertSqlSafe(sql))
        .bind(archived)
        .bind(&now)
        .bind(&now)
        .bind(access.logbook_id())
        .bind(id)
        .execute(&mut **tx)
        .await?;
    match updated.rows_affected() {
        0 => Err(StoreError::NotFound(tables.what)),
        _ => Ok(()),
    }
}

pub(super) async fn favorite(
    tx: &mut WriteTx,
    access: &LogbookAccess,
    holder: Holder<'_>,
    favorite: bool,
) -> Result<(), StoreError> {
    let (tables, id) = tables(holder);
    let query = match favorite {
        true => sqlx::query(AssertSqlSafe(format!(
            "INSERT INTO {} (logbook_id, user_id, {}, created_at) VALUES (?, ?, ?, ?) ON CONFLICT DO NOTHING",
            tables.favorites, tables.column
        )))
        .bind(access.logbook_id())
        .bind(access.user_id())
        .bind(id)
        .bind(now()),
        false => sqlx::query(AssertSqlSafe(format!(
            "DELETE FROM {} WHERE user_id = ? AND {} = ?",
            tables.favorites, tables.column
        )))
        .bind(access.user_id())
        .bind(id),
    };
    query.execute(&mut **tx).await?;
    Ok(())
}

pub(super) async fn rewrite_plan(
    conn: &mut SqliteConnection,
    access: &LogbookAccess,
    holder: Holder<'_>,
    range: TimeRange,
    values: &[(&str, &str)],
) -> Result<Vec<(StoredQso, Qso)>, StoreError> {
    let mut plan = vec![];
    for current in qso::held_by(conn, access, holder, range).await? {
        let mut rewritten = current.qso.clone();
        for (name, value) in values {
            rewritten.set(name, Some((*value).to_owned()))?;
        }
        if rewritten != current.qso {
            plan.push((current, rewritten));
        }
    }
    Ok(plan)
}
