use sqlx::{AssertSqlSafe, SqliteConnection};

use super::{SELECT, StoredQso};
use crate::{
    domain::{
        callsign,
        duplicate::DuplicateKey,
        qso::{Checked, Qso, QsoId, Warning},
    },
    store::{StoreError, access::LogbookAccess},
};

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

pub(super) async fn check(
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

#[cfg(test)]
mod tests {
    use super::{
        super::tests::{create_one, entry, new},
        *,
    };
    use crate::store::testing::World;

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
}
