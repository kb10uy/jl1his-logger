use std::collections::BTreeMap;

use crate::{
    domain::qsl,
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
    pub id: String,
    pub reason: SkipReason,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Outcome {
    pub updated: Vec<StoredQso>,
    pub skipped: Vec<Skipped>,
}

pub(crate) type Changes = Vec<(&'static str, String)>;

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
    let targets: Vec<_> = targets.iter().map(|t| (t.id.to_string(), t.revision)).collect();
    apply(tx, &access, &targets, &checked).await
}

pub(crate) async fn apply(
    tx: &mut WriteTx,
    access: &LogbookAccess,
    targets: &[(String, i64)],
    changes: &Changes,
) -> Result<Outcome, StoreError> {
    let mut outcome = Outcome::default();
    for (id, revision) in targets {
        let skip = |reason| Skipped { id: id.clone(), reason };
        let current = match qso::fetch(tx, access, id).await {
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
        if current.revision != *revision {
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
    use super::*;
    use crate::{
        domain::qso::{Qso, QsoId},
        store::{
            qso::{Created, QsoInput},
            station::{self, tests::input as station_input},
            testing::World,
        },
    };

    pub(crate) async fn three_qsos(world: &World) -> Vec<StoredQso> {
        let access = world.access(&world.owner).await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        let station = station::create(&mut tx, &access.owner().unwrap(), &station_input("Home"))
            .await
            .unwrap();
        let mut stored = vec![];
        for (call, time_on) in [("JA1AAA", "0101"), ("JA1BBB", "0202"), ("JA1CCC", "0303")] {
            let input = QsoInput {
                station_id: station.id.clone(),
                qso: Qso::from_fields([
                    ("CALL", call),
                    ("QSO_DATE", "20261022"),
                    ("TIME_ON", time_on),
                    ("BAND", "40m"),
                    ("MODE", "CW"),
                    ("QSL_SENT", "R"),
                ])
                .unwrap(),
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
            id: row.id.parse().unwrap(),
            revision: row.revision,
        }
    }

    #[tokio::test]
    async fn changes_qsl_fields_of_current_rows_and_skips_the_rest() {
        let world = World::new().await;
        let rows = three_qsos(&world).await;
        let access = world.access(&world.operator).await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        qso::delete(&mut tx, &access, &rows[2].id.parse().unwrap(), rows[2].revision)
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
                    id: rows[1].id.clone(),
                    reason: SkipReason::Revision { current: 1 }
                },
                Skipped {
                    id: rows[2].id.clone(),
                    reason: SkipReason::Deleted
                },
                Skipped {
                    id: missing.id.to_string(),
                    reason: SkipReason::Missing
                },
            ]
        );
    }

    #[tokio::test]
    async fn refuses_other_fields_and_invalid_values() {
        let world = World::new().await;
        let rows = three_qsos(&world).await;
        let access = world.access(&world.operator).await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        for (name, value) in [("CALL", "JA1ZZZ"), ("QSL_SENT", "V"), ("QSLRDATE", "2026")] {
            let changes = BTreeMap::from([(name.to_owned(), value.to_owned())]);
            let refused = update(&mut tx, &access, &[target(&rows[0])], &changes).await;
            assert!(
                matches!(refused, Err(StoreError::Invalid(_) | StoreError::Qso(_))),
                "{name}: {refused:?}"
            );
        }
        let repeated = BTreeMap::from([
            ("QSL_SENT".to_owned(), "Y".to_owned()),
            ("qsl_sent".to_owned(), "N".to_owned()),
        ]);
        assert!(matches!(
            update(&mut tx, &access, &[target(&rows[0])], &repeated).await,
            Err(StoreError::Invalid(_))
        ));
        let cleared = BTreeMap::from([("QSL_SENT".to_owned(), String::new())]);
        let outcome = update(&mut tx, &access, &[target(&rows[0])], &cleared).await.unwrap();
        assert_eq!(outcome.updated[0].qso.get("QSL_SENT"), None);
    }
}
