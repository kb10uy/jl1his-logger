use std::collections::BTreeMap;

use crate::{
    domain::{confirmation::Changes, qsl, qso::QsoId},
    store::{
        StoreError, WriteTx,
        access::LogbookAccess,
        qso::{self, StoredQso, Target},
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    Missing,
    Deleted,
    Revision { current: i64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skipped {
    pub id: QsoId,
    pub reason: SkipReason,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Outcome {
    pub updated: Vec<StoredQso>,
    pub skipped: Vec<Skipped>,
}

pub async fn update(
    tx: &mut WriteTx,
    access: &LogbookAccess,
    targets: &[Target],
    changes: &BTreeMap<String, String>,
) -> Result<Outcome, StoreError> {
    let access = access.revalidate(tx).await?;
    if changes.is_empty() {
        return Err(StoreError::Invalid("a QSL change needs a field".into()));
    }
    let mut checked: Changes = vec![];
    for (name, value) in changes {
        let field = qsl::field(name).ok_or_else(|| StoreError::Invalid(format!("{name} is not a QSL field")))?;
        if checked.iter().any(|(f, _)| *f == field) {
            return Err(StoreError::Invalid(format!("{field} is given more than once")));
        }
        qsl::check(field, value)?;
        checked.push((field, value.clone()));
    }
    apply(tx, &access, targets, &checked).await
}

pub(crate) async fn apply(
    tx: &mut WriteTx,
    access: &LogbookAccess,
    targets: &[Target],
    changes: &Changes,
) -> Result<Outcome, StoreError> {
    let mut outcome = Outcome::default();
    for target in targets {
        let skip = |reason| Skipped { id: target.id, reason };
        let current = match qso::fetch(tx, access, &target.id).await {
            Ok(current) => current,
            Err(StoreError::NotFound(_)) => {
                outcome.skipped.push(skip(SkipReason::Missing));
                continue;
            }
            Err(e) => return Err(e),
        };
        if current.deleted_at.is_some() {
            outcome.skipped.push(skip(SkipReason::Deleted));
            continue;
        }
        if current.revision != target.revision {
            outcome.skipped.push(skip(SkipReason::Revision {
                current: current.revision,
            }));
            continue;
        }
        let mut changed = current.qso.clone();
        for (field, value) in changes {
            changed.set(field, Some(value.clone()))?;
        }
        outcome
            .updated
            .push(qso::rewrite(tx, access, &current, &changed).await?);
    }
    Ok(outcome)
}

#[cfg(test)]
pub(crate) mod tests {
    use rstest::rstest;

    use super::*;
    use crate::store::{
        qso::{Created, QsoInput},
        testing::{World, contact},
    };

    pub(crate) async fn three_qsos(world: &World) -> Vec<StoredQso> {
        let station = world.station("Home").await;
        let access = world.access(&world.owner).await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        let mut stored = vec![];
        for (call, time_on) in [("JA1AAA", "0101"), ("JA1BBB", "0202"), ("JA1CCC", "0303")] {
            let mut qso = contact(call, time_on);
            qso.set("QSL_SENT", Some("R".into())).unwrap();
            let input = QsoInput {
                station_id: station.id.clone(),
                qso,
            };
            match qso::create(&mut tx, &access, &QsoId::generate(), input).await.unwrap() {
                Created::New { stored: row, .. } => stored.push(row),
                Created::Existing(_) => unreachable!(),
            }
        }
        tx.commit().await.unwrap();
        stored
    }

    pub(crate) fn target(row: &StoredQso) -> Target {
        Target {
            id: row.id,
            revision: row.revision,
        }
    }

    #[tokio::test]
    async fn changes_qsl_fields_of_current_rows_and_skips_the_rest() {
        let world = World::new().await;
        let rows = three_qsos(&world).await;
        let access = world.access(&world.operator).await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        qso::delete(&mut tx, &access, &rows[2].id, rows[2].revision)
            .await
            .unwrap();
        let stale = Target {
            revision: 9,
            ..target(&rows[1])
        };
        let missing = Target {
            id: QsoId::generate(),
            revision: 1,
        };
        let changes = BTreeMap::from([
            ("qsl_sent".to_owned(), "y".to_owned()),
            ("QSLSDATE".to_owned(), "20261023".to_owned()),
        ]);
        let outcome = update(
            &mut tx,
            &access,
            &[target(&rows[0]), stale, target(&rows[2]), missing.clone()],
            &changes,
        )
        .await
        .unwrap();

        assert_eq!(outcome.updated.len(), 1);
        let updated = &outcome.updated[0];
        assert_eq!(
            (updated.revision, updated.updated_by.as_str()),
            (2, world.operator.id.as_str())
        );
        assert_eq!(
            (updated.qso.get("QSL_SENT"), updated.qso.get("QSLSDATE")),
            (Some("y"), Some("20261023"))
        );
        assert_eq!(
            outcome.skipped,
            [
                Skipped {
                    id: rows[1].id,
                    reason: SkipReason::Revision { current: 1 }
                },
                Skipped {
                    id: rows[2].id,
                    reason: SkipReason::Deleted
                },
                Skipped {
                    id: missing.id,
                    reason: SkipReason::Missing
                },
            ]
        );
    }

    #[rstest]
    #[case::not_a_qsl_field(&[("CALL", "JA1ZZZ")])]
    #[case::invalid_value(&[("QSL_SENT", "V")])]
    #[case::invalid_date(&[("QSLRDATE", "2026")])]
    #[case::repeated(&[("QSL_SENT", "Y"), ("qsl_sent", "N")])]
    #[case::empty(&[])]
    #[tokio::test]
    async fn refuses_other_fields_and_invalid_values(#[case] changes: &[(&str, &str)]) {
        let world = World::new().await;
        let rows = three_qsos(&world).await;
        let access = world.access(&world.operator).await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        let changes: BTreeMap<String, String> = changes
            .iter()
            .map(|(n, v)| ((*n).to_owned(), (*v).to_owned()))
            .collect();
        let refused = update(&mut tx, &access, &[target(&rows[0])], &changes).await;
        assert!(
            matches!(refused, Err(StoreError::Invalid(_) | StoreError::Qso(_))),
            "{refused:?}"
        );
    }

    #[tokio::test]
    async fn clears_a_qsl_field_with_an_empty_value() {
        let world = World::new().await;
        let rows = three_qsos(&world).await;
        let access = world.access(&world.operator).await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        let cleared = BTreeMap::from([("QSL_SENT".to_owned(), String::new())]);
        let outcome = update(&mut tx, &access, &[target(&rows[0])], &cleared).await.unwrap();
        assert_eq!(outcome.updated[0].qso.get("QSL_SENT"), None);
    }
}
