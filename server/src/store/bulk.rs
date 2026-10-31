use std::collections::BTreeMap;

use crate::{
    domain::{
        confirmation::Changes,
        qsl,
        qso::{QsoId, field_name},
    },
    store::{
        Fault, StoreError, WriteTx,
        access::LogbookAccess,
        logbook::{instrument, station},
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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Change {
    pub station_id: Option<String>,
    pub instrument_id: Option<Option<String>>,
    pub fields: BTreeMap<String, String>,
}

pub async fn update(
    tx: &mut WriteTx,
    access: &LogbookAccess,
    targets: &[Target],
    change: &Change,
) -> Result<Outcome, StoreError> {
    let access = access.revalidate(tx).await?;
    if change.station_id.is_none() && change.instrument_id.is_none() && change.fields.is_empty() {
        return Err(StoreError::Invalid(
            "a bulk change needs a field, a Station or an Instrument".into(),
        ));
    }
    let mut fields: Vec<(String, String)> = vec![];
    for (name, value) in &change.fields {
        let name = field_name(name)?;
        if fields.iter().any(|(n, _)| *n == name) {
            return Err(StoreError::Invalid(
                Fault::coded("field_repeated", format!("{name} is given more than once")).with("field", name),
            ));
        }
        if let Some(field) = qsl::field(&name) {
            qsl::check(field, value)?;
        }
        fields.push((name, value.clone()));
    }
    if let Some(id) = &change.station_id {
        station::fetch(tx, &access, id).await?;
    }
    if let Some(Some(id)) = &change.instrument_id {
        instrument::fetch(tx, &access, id).await?;
    }

    let mut outcome = Outcome::default();
    for target in targets {
        let current = match live(tx, &access, target).await? {
            Ok(current) => current,
            Err(reason) => {
                outcome.skipped.push(Skipped { id: target.id, reason });
                continue;
            }
        };
        let mut changed = current.qso.clone();
        for (name, value) in &fields {
            changed.set(name, Some(value.clone()))?;
        }
        let station_id = change.station_id.as_deref().unwrap_or(&current.station_id);
        let instrument_id = match &change.instrument_id {
            Some(id) => id.as_deref(),
            None => current.instrument_id.as_deref(),
        };
        outcome
            .updated
            .push(qso::relocate(tx, &access, &current, (station_id, instrument_id), changed).await?);
    }
    Ok(outcome)
}

pub(crate) async fn apply(
    tx: &mut WriteTx,
    access: &LogbookAccess,
    targets: &[Target],
    changes: &Changes,
) -> Result<Outcome, StoreError> {
    let mut outcome = Outcome::default();
    for target in targets {
        let current = match live(tx, access, target).await? {
            Ok(current) => current,
            Err(reason) => {
                outcome.skipped.push(Skipped { id: target.id, reason });
                continue;
            }
        };
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

async fn live(
    tx: &mut WriteTx,
    access: &LogbookAccess,
    target: &Target,
) -> Result<Result<StoredQso, SkipReason>, StoreError> {
    let current = match qso::fetch(tx, access, &target.id).await {
        Ok(current) => current,
        Err(StoreError::NotFound(_)) => return Ok(Err(SkipReason::Missing)),
        Err(e) => return Err(e),
    };
    if current.deleted_at.is_some() {
        return Ok(Err(SkipReason::Deleted));
    }
    if current.revision != target.revision {
        return Ok(Err(SkipReason::Revision {
            current: current.revision,
        }));
    }
    Ok(Ok(current))
}

#[cfg(test)]
pub(crate) mod tests {
    use rstest::rstest;

    use super::*;
    use crate::store::{
        qso::{Created, QsoInput},
        testing::{World, contact, station_input},
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
                instrument_id: None,
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

    fn fields(changes: &[(&str, &str)]) -> Change {
        Change {
            fields: changes
                .iter()
                .map(|(n, v)| ((*n).to_owned(), (*v).to_owned()))
                .collect(),
            ..Change::default()
        }
    }

    #[tokio::test]
    async fn changes_fields_of_current_rows_and_skips_the_rest() {
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
        let change = fields(&[("qsl_sent", "y"), ("QSLSDATE", "20261023"), ("name", "Taro")]);
        let outcome = update(
            &mut tx,
            &access,
            &[target(&rows[0]), stale, target(&rows[2]), missing.clone()],
            &change,
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
            ["QSL_SENT", "QSLSDATE", "NAME", "CALL"].map(|name| updated.qso.get(name)),
            [Some("y"), Some("20261023"), Some("Taro"), Some("JA1AAA")]
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
    #[case::invalid_qsl_value(&[("QSL_SENT", "V")])]
    #[case::invalid_qsl_date(&[("QSLRDATE", "2026")])]
    #[case::invalid_name(&[("MY RIG", "IC-705")])]
    #[case::repeated(&[("QSL_SENT", "Y"), ("qsl_sent", "N")])]
    #[case::required_cleared(&[("CALL", "")])]
    #[case::invalid_band(&[("BAND", "41m")])]
    #[case::empty(&[])]
    #[tokio::test]
    async fn refuses_invalid_changes(#[case] changes: &[(&str, &str)]) {
        let world = World::new().await;
        let rows = three_qsos(&world).await;
        let access = world.access(&world.operator).await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        let refused = update(&mut tx, &access, &[target(&rows[0])], &fields(changes)).await;
        assert!(
            matches!(refused, Err(StoreError::Invalid(_) | StoreError::Qso(_))),
            "{refused:?}"
        );
    }

    #[tokio::test]
    async fn clears_a_field_with_an_empty_value() {
        let world = World::new().await;
        let rows = three_qsos(&world).await;
        let access = world.access(&world.operator).await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        let cleared = fields(&[("QSL_SENT", ""), ("MY_CITY", "")]);
        let outcome = update(&mut tx, &access, &[target(&rows[0])], &cleared).await.unwrap();
        let qso = &outcome.updated[0].qso;
        assert_eq!((qso.get("QSL_SENT"), qso.get("MY_CITY")), (None, None));
    }

    #[tokio::test]
    async fn moves_qsos_to_another_station_and_instrument_baking_their_values() {
        let world = World::new().await;
        let rows = three_qsos(&world).await;
        let owner = world.access(&world.owner).await;
        let access = world.access(&world.operator).await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        let mut away = station_input("Away");
        away.callsign = "JL1HIS/2".into();
        away.fields.insert("my_city".into(), "Chiyoda".into());
        away.fields.remove("MY_CNTY");
        let away = station::create(&mut tx, &owner.owner().unwrap(), &away).await.unwrap();
        tx.commit().await.unwrap();
        let rig = world.instrument("IC-705").await;

        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        let change = Change {
            station_id: Some(away.id.clone()),
            instrument_id: Some(Some(rig.id.clone())),
            fields: BTreeMap::from([("MY_CITY".to_owned(), "Shibuya".to_owned())]),
        };
        let moved = update(&mut tx, &access, &[target(&rows[0]), target(&rows[1])], &change)
            .await
            .unwrap();
        let names = ["STATION_CALLSIGN", "MY_CITY", "MY_CNTY", "MY_RIG", "TX_PWR"];
        for row in &moved.updated {
            assert_eq!(
                (row.station_id.as_str(), row.instrument_id.as_deref()),
                (away.id.as_str(), Some(rig.id.as_str()))
            );
            assert_eq!(
                names.map(|name| row.qso.get(name)),
                [
                    Some(away.callsign.as_str()),
                    Some("Shibuya"),
                    None,
                    Some("IC-705"),
                    Some("0.5")
                ]
            );
        }

        let unequipped = Change {
            instrument_id: Some(None),
            ..Change::default()
        };
        let back = update(&mut tx, &access, &[target(&moved.updated[0])], &unequipped)
            .await
            .unwrap();
        let row = &back.updated[0];
        assert_eq!(
            (row.station_id.as_str(), row.instrument_id.as_deref()),
            (away.id.as_str(), None)
        );
        assert_eq!(
            names.map(|name| row.qso.get(name)),
            [Some(away.callsign.as_str()), Some("Shibuya"), None, None, None]
        );
    }

    #[tokio::test]
    async fn refuses_a_station_or_instrument_outside_the_logbook() {
        let world = World::new().await;
        let rows = three_qsos(&world).await;
        let access = world.access(&world.operator).await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        for change in [
            Change {
                station_id: Some("nowhere".into()),
                ..Change::default()
            },
            Change {
                instrument_id: Some(Some("nothing".into())),
                ..Change::default()
            },
        ] {
            let refused = update(&mut tx, &access, &[target(&rows[0])], &change).await;
            assert!(matches!(refused, Err(StoreError::NotFound(_))), "{refused:?}");
        }
    }
}
