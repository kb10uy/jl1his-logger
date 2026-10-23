use std::collections::{BTreeMap, HashSet};

use sqlx::Connection;

use crate::{
    db::Db,
    domain::{
        confirmation::{self, Changes, Source},
        duplicate::DuplicateKey,
        import::RecordIds,
        qso::{Qso, QsoError, QsoId, Warning},
    },
    store::{
        StoreError, WriteTx,
        access::LogbookAccess,
        instrument::Instrument,
        qsl::{self, Skipped},
        qso::{self, CallMatch, Created, Filter, QsoInput, Target, TimeRange},
        station::Station,
    },
};

const BATCH: usize = 500;

#[derive(Debug, Default)]
pub struct NewReport {
    pub read: usize,
    pub created: usize,
    pub existing: usize,
    pub duplicates: Vec<(usize, Vec<String>)>,
    pub outside_band: Vec<(usize, String, String)>,
    pub modes: Vec<(usize, Warning)>,
    pub invalid: Vec<(usize, String)>,
    pub rejects: Vec<Qso>,
}

impl NewReport {
    fn reject(&mut self, index: usize, error: impl ToString, record: Option<Qso>) {
        self.invalid.push((index, error.to_string()));
        self.rejects.extend(record);
    }

    fn note(&mut self, index: usize, warnings: Vec<Warning>) -> Vec<String> {
        let mut duplicates = vec![];
        for warning in warnings {
            match warning {
                Warning::PossibleDuplicate { ids } => duplicates.extend(ids),
                Warning::FrequencyOutsideBand { freq, band } => self.outside_band.push((index, freq, band)),
                mode => self.modes.push((index, mode)),
            }
        }
        duplicates
    }

    fn duplicates_of(&mut self, index: usize, mut duplicates: Vec<String>) {
        duplicates.sort();
        duplicates.dedup();
        if !duplicates.is_empty() {
            self.duplicates.push((index, duplicates));
        }
    }
}

pub async fn import_new(
    db: &Db,
    access: &LogbookAccess,
    station: &Station,
    instrument: Option<&Instrument>,
    records: Vec<Result<Qso, QsoError>>,
    apply: bool,
) -> Result<NewReport, StoreError> {
    let mut report = NewReport {
        read: records.len(),
        ..NewReport::default()
    };
    let mut ids = RecordIds::new(access.logbook_id());
    let mut planned: Vec<(usize, QsoId, Qso)> = vec![];
    for (index, record) in (1..).zip(records) {
        match record {
            Ok(qso) => match ids.next(&qso) {
                Ok(id) => planned.push((index, id, qso)),
                Err(e) => report.reject(index, e, Some(qso)),
            },
            Err(e) => report.reject(index, e, None),
        }
    }
    let input = |qso: &Qso| QsoInput {
        station_id: station.id.clone(),
        instrument_id: instrument.map(|i| i.id.clone()),
        qso: qso.clone(),
    };
    if !apply {
        let mut previewed: BTreeMap<i64, Vec<(String, DuplicateKey)>> = BTreeMap::new();
        let mut conn = db.acquire().await?;
        let mut tx = conn.begin().await?;
        for (index, id, qso) in &planned {
            match qso::preview(&mut tx, access, id, input(qso)).await {
                Ok(Some((baked, warnings))) => {
                    report.created += 1;
                    let mut duplicates = report.note(*index, warnings);
                    let key = DuplicateKey::of(&baked)?;
                    duplicates.extend(
                        previewed
                            .range(key.time_range())
                            .flat_map(|(_, rows)| rows)
                            .filter(|(_, previous)| key.matches(previous))
                            .map(|(id, _)| id.clone()),
                    );
                    previewed.entry(key.qso_time).or_default().push((id.to_string(), key));
                    report.duplicates_of(*index, duplicates);
                }
                Ok(None) => report.existing += 1,
                Err(StoreError::Qso(e)) => report.reject(*index, e, Some(qso.clone())),
                Err(e) => return Err(e),
            }
        }
        return Ok(report);
    }
    for batch in planned.chunks(BATCH) {
        let mut tx = WriteTx::begin(db).await?;
        for (index, id, qso) in batch {
            match qso::create(&mut tx, access, id, input(qso)).await {
                Ok(Created::New { warnings, .. }) => {
                    report.created += 1;
                    let duplicates = report.note(*index, warnings);
                    report.duplicates_of(*index, duplicates);
                }
                Ok(Created::Existing(_)) => report.existing += 1,
                Err(StoreError::Qso(e)) => report.reject(*index, e, Some(qso.clone())),
                Err(e) => return Err(e),
            }
        }
        tx.commit().await?;
    }
    Ok(report)
}

#[derive(Debug, Default)]
pub struct MergeReport {
    pub read: usize,
    pub not_confirmations: usize,
    pub merged: usize,
    pub already: usize,
    pub unmatched: Vec<usize>,
    pub ambiguous: Vec<(usize, Vec<String>)>,
    pub invalid: Vec<(usize, String)>,
    pub skipped: Vec<Skipped>,
    pub differences: BTreeMap<&'static str, usize>,
    pub rejects: Vec<Qso>,
}

pub async fn merge(
    db: &Db,
    access: &LogbookAccess,
    source: Source,
    station: Option<&Station>,
    records: Vec<Result<Qso, QsoError>>,
    apply: bool,
    today: &str,
) -> Result<MergeReport, StoreError> {
    let mut report = MergeReport {
        read: records.len(),
        ..MergeReport::default()
    };
    let mut planned: Vec<(Target, Changes)> = vec![];
    let mut targeted = HashSet::new();
    let mut conn = db.acquire().await?;
    for (index, record) in (1..).zip(records) {
        let reject = |report: &mut MergeReport, error: String, record: Option<Qso>| {
            report.invalid.push((index, error));
            report.rejects.extend(record);
        };
        let record = match record {
            Ok(record) => record,
            Err(e) => {
                reject(&mut report, e.to_string(), None);
                continue;
            }
        };
        let changes = match source.confirmation(&record, today) {
            Ok(Some(changes)) => changes,
            Ok(None) => {
                report.not_confirmations += 1;
                continue;
            }
            Err(e) => {
                reject(&mut report, e.to_string(), Some(record));
                continue;
            }
        };
        let key = match DuplicateKey::of(&record) {
            Ok(key) => key,
            Err(e) => {
                reject(&mut report, e.to_string(), Some(record));
                continue;
            }
        };
        let filter = Filter {
            time: TimeRange {
                from: Some(*key.time_range().start()),
                until: Some(key.time_range().end() + 1),
            },
            station_ids: station.map(|s| vec![s.id.clone()]),
            call: Some(CallMatch::Exact(key.call.clone())),
            ..Filter::default()
        };
        let candidates: Vec<_> = qso::search_all(&mut conn, access, &filter)
            .await?
            .into_iter()
            .filter(|row| DuplicateKey::of(&row.qso).is_ok_and(|other| key.matches(&other) && key.same_station(&other)))
            .collect();
        match candidates.as_slice() {
            [] => {
                report.unmatched.push(index);
                report.rejects.push(record);
            }
            [row] => {
                for field in confirmation::differences(&record, &row.qso) {
                    *report.differences.entry(field).or_default() += 1;
                }
                if source.received(&row.qso) || !targeted.insert(row.id) {
                    report.already += 1;
                } else {
                    let target = Target {
                        id: row.id,
                        revision: row.revision,
                    };
                    planned.push((target, changes));
                }
            }
            rows if rows.iter().all(|row| source.received(&row.qso)) => report.already += 1,
            rows => {
                report
                    .ambiguous
                    .push((index, rows.iter().map(|row| row.id.to_string()).collect()));
                report.rejects.push(record);
            }
        }
    }
    drop(conn);

    if !apply {
        report.merged = planned.len();
        return Ok(report);
    }
    for batch in planned.chunks(BATCH) {
        let mut groups: BTreeMap<&Changes, Vec<Target>> = BTreeMap::new();
        for (target, changes) in batch {
            groups.entry(changes).or_default().push(target.clone());
        }
        let mut tx = WriteTx::begin(db).await?;
        let access = access.revalidate(&mut tx).await?;
        for (changes, targets) in groups {
            let outcome = qsl::apply(&mut tx, &access, &targets, changes).await?;
            report.merged += outcome.updated.len();
            report.skipped.extend(outcome.skipped);
        }
        tx.commit().await?;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::store::{
        qso::StoredQso,
        testing::{World, contact},
    };

    fn record(fields: &[(&str, &str)]) -> Qso {
        Qso::from_fields(fields.iter().copied()).unwrap()
    }

    async fn setup() -> (World, LogbookAccess, Station) {
        let world = World::new().await;
        let access = world.access(&world.owner).await;
        let station = world.station("Home").await;
        (world, access, station)
    }

    async fn all(world: &World, access: &LogbookAccess) -> Vec<StoredQso> {
        let mut conn = world.db.acquire().await.unwrap();
        qso::changes(&mut conn, access, 0, 10_000).await.unwrap().rows
    }

    #[tokio::test]
    async fn previews_without_writing_and_reruns_without_duplicating() {
        let (world, access, station) = setup().await;
        let records = || {
            vec![
                Ok(contact("JA1AAA", "0101")),
                Ok(contact("JA1AAA", "0101")),
                Ok(record(&[("CALL", "JA1BBB")])),
                Err(QsoError::FieldName("NAMÉ".into())),
                Ok(contact("JA1CCC", "0303")),
            ]
        };

        let preview = import_new(&world.db, &access, &station, None, records(), false)
            .await
            .unwrap();
        assert_eq!((preview.read, preview.created, preview.existing), (5, 3, 0));
        assert_eq!(preview.invalid.iter().map(|(i, _)| *i).collect::<Vec<_>>(), [3, 4]);
        assert_eq!(preview.duplicates.len(), 1);
        assert!(all(&world, &access).await.is_empty());

        let first = import_new(&world.db, &access, &station, None, records(), true)
            .await
            .unwrap();
        assert_eq!((first.created, first.existing), (3, 0));
        assert_eq!(first.rejects.len(), 1);
        let stored = all(&world, &access).await;
        assert_eq!(stored.len(), 3);
        assert!(
            stored
                .iter()
                .all(|row| row.qso.get("STATION_CALLSIGN") == Some("JL1HIS/1"))
        );
        assert!(stored.iter().all(|row| row.qso.get("OPERATOR") == Some("JA1OWN")));

        let mut more = records();
        more.push(Ok(contact("JA1DDD", "0404")));
        let again = import_new(&world.db, &access, &station, None, more, true)
            .await
            .unwrap();
        assert_eq!((again.created, again.existing), (1, 3));
        assert_eq!(all(&world, &access).await.len(), 4);
    }

    #[tokio::test]
    async fn bakes_the_instrument_where_records_have_no_values() {
        let (world, access, station) = setup().await;
        let rig = world.instrument("IC-705").await;
        let other = world.instrument("FT-817").await;
        let records = || {
            let mut powered = contact("JA1BBB", "0202");
            powered.set("TX_PWR", Some("50".into())).unwrap();
            vec![Ok(contact("JA1AAA", "0101")), Ok(powered)]
        };

        let first = import_new(&world.db, &access, &station, Some(&rig), records(), true)
            .await
            .unwrap();
        assert_eq!(first.created, 2);
        let rerun = import_new(&world.db, &access, &station, Some(&other), records(), true)
            .await
            .unwrap();
        assert_eq!((rerun.created, rerun.existing), (0, 2));

        let stored = all(&world, &access).await;
        let baked: Vec<_> = stored
            .iter()
            .map(|row| {
                (
                    row.instrument_id.as_deref(),
                    row.qso.get("MY_RIG"),
                    row.qso.get("TX_PWR"),
                )
            })
            .collect();
        assert_eq!(
            baked,
            [
                (Some(rig.id.as_str()), Some("IC-705"), Some("0.5")),
                (Some(rig.id.as_str()), Some("IC-705"), Some("50")),
            ]
        );
    }

    #[rstest]
    #[case::empty(false)]
    #[case::existing_duplicate(true)]
    #[tokio::test]
    async fn previews_duplicates_across_batches(#[case] existing: bool) {
        let (world, access, station) = setup().await;
        if existing {
            import_new(
                &world.db,
                &access,
                &station,
                None,
                vec![Ok(contact("JA1AAA", "0059"))],
                true,
            )
            .await
            .unwrap();
        }
        let mut records = vec![Ok(contact("JA1AAA", "0101"))];
        records.extend((1..BATCH).map(|i| Ok(contact(&format!("JA1X{i}"), "0101"))));
        records.extend([Ok(contact("JA1AAA", "0101")), Ok(contact("JA1AAA", "0101"))]);

        let preview = import_new(&world.db, &access, &station, None, records.clone(), false)
            .await
            .unwrap();
        assert_eq!(preview.created, BATCH + 2);
        assert_eq!(preview.duplicates.len(), if existing { 3 } else { 2 });
        assert_eq!(all(&world, &access).await.len(), usize::from(existing));
        let applied = import_new(&world.db, &access, &station, None, records.clone(), true)
            .await
            .unwrap();
        assert_eq!(preview.duplicates, applied.duplicates);

        let rerun = import_new(&world.db, &access, &station, None, records, false)
            .await
            .unwrap();
        assert_eq!((rerun.created, rerun.existing), (0, BATCH + 2));
        assert!(rerun.duplicates.is_empty());
    }

    #[tokio::test]
    async fn previews_while_another_write_is_open() {
        let (world, access, station) = setup().await;
        let held = WriteTx::begin(&world.db).await.unwrap();
        let records = vec![Ok(contact("JA1AAA", "0101")), Ok(contact("JA1AAA", "0101"))];
        let preview = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            import_new(&world.db, &access, &station, None, records, false),
        )
        .await
        .expect("a preview takes no write lock")
        .unwrap();
        assert_eq!((preview.created, preview.duplicates.len()), (2, 1));
        drop(held);
    }

    #[rstest]
    #[case::ambiguous("CW", false, 0, 0, 2)]
    #[case::match_on_later_page("SSB", false, 1, 0, 0)]
    #[case::all_received("CW", true, 0, 1, 0)]
    #[tokio::test]
    async fn matches_confirmations_across_all_pages(
        #[case] newest_mode: &str,
        #[case] received: bool,
        #[case] merged: usize,
        #[case] already: usize,
        #[case] ambiguous: usize,
    ) {
        let (world, access, station) = setup().await;
        let mut oldest = contact("JA1AAA", "0100");
        let mut newest = contact("JA1AAA", "0102");
        newest.set("MODE", Some(newest_mode.into())).unwrap();
        if received {
            oldest.set("LOTW_QSL_RCVD", Some("Y".into())).unwrap();
            newest.set("LOTW_QSL_RCVD", Some("Y".into())).unwrap();
        }
        let mut records = vec![Ok(oldest)];
        records.extend((1..100).map(|i| {
            let mut qso = contact("JA1AAA", "0101");
            qso.set("BAND", Some("20m".into())).unwrap();
            qso.set("COMMENT", Some(i.to_string())).unwrap();
            Ok(qso)
        }));
        records.push(Ok(newest));
        import_new(&world.db, &access, &station, None, records, true)
            .await
            .unwrap();

        let mut confirmation = contact("JA1AAA", "0101");
        confirmation.set("QSL_RCVD", Some("Y".into())).unwrap();
        let report = merge(
            &world.db,
            &access,
            Source::Lotw,
            None,
            vec![Ok(confirmation)],
            true,
            "20261122",
        )
        .await
        .unwrap();
        assert_eq!((report.merged, report.already), (merged, already));
        assert!(report.unmatched.is_empty());
        assert_eq!(
            report.ambiguous.iter().map(|(_, ids)| ids.len()).sum::<usize>(),
            ambiguous
        );
        let changed = all(&world, &access)
            .await
            .into_iter()
            .filter(|row| row.qso.get("LOTW_QSLRDATE").is_some())
            .collect::<Vec<_>>();
        assert_eq!(changed.len(), merged);
        if merged == 1 {
            assert_eq!(changed[0].qso.get("TIME_ON"), Some("0100"));
        }
    }

    #[tokio::test]
    async fn keeps_a_deleted_import_deleted() {
        let (world, access, station) = setup().await;
        let records = || vec![Ok(contact("JA1AAA", "0101"))];
        import_new(&world.db, &access, &station, None, records(), true)
            .await
            .unwrap();
        let row = all(&world, &access).await.remove(0);
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        qso::delete(&mut tx, &access, &row.id, row.revision).await.unwrap();
        tx.commit().await.unwrap();

        let again = import_new(&world.db, &access, &station, None, records(), true)
            .await
            .unwrap();
        assert_eq!((again.created, again.existing), (0, 1));
        assert!(all(&world, &access).await[0].deleted_at.is_some());
    }

    #[tokio::test]
    async fn merges_confirmations_once_and_reports_the_rest() {
        let (world, access, station) = setup().await;
        let mut received = contact("JA1CCC", "0303");
        received.set("LOTW_QSL_RCVD", Some("Y".into())).unwrap();
        let stored = vec![
            Ok(contact("JA1AAA", "0101")),
            Ok(contact("JA1BBB", "0202")),
            Ok(contact("JA1BBB", "0210")),
            Ok(received),
        ];
        import_new(&world.db, &access, &station, None, stored, true)
            .await
            .unwrap();

        let confirmed = |call: &str, time_on: &str, extra: &[(&str, &str)]| {
            let mut qso = contact(call, time_on);
            qso.set("QSL_RCVD", Some("Y".into())).unwrap();
            qso.set("QSLRDATE", Some("20261101".into())).unwrap();
            for (name, value) in extra {
                qso.set(name, Some((*value).to_owned())).unwrap();
            }
            Ok(qso)
        };
        let report_records = || {
            vec![
                confirmed(
                    "ja1aaa",
                    "0105",
                    &[("GRIDSQUARE", "PM95"), ("STATION_CALLSIGN", "JL1HIS/1")],
                ),
                confirmed("JA1AAA", "0101", &[]),
                confirmed("JA1BBB", "0205", &[]),
                confirmed("JA1CCC", "0303", &[]),
                confirmed("JA1ZZZ", "0101", &[]),
                Ok(contact("JA1AAA", "0101")),
                confirmed("JA1AAA", "0101", &[("STATION_CALLSIGN", "JL1HIS/P")]),
            ]
        };

        let preview = merge(
            &world.db,
            &access,
            Source::Lotw,
            None,
            report_records(),
            false,
            "20261122",
        )
        .await
        .unwrap();
        assert_eq!((preview.merged, preview.already, preview.not_confirmations), (1, 2, 1));
        assert_eq!(preview.unmatched, [5, 7]);
        assert_eq!(preview.ambiguous.len(), 1);
        assert_eq!(preview.differences, BTreeMap::from([("GRIDSQUARE", 1)]));
        assert_eq!(preview.rejects.len(), 3);
        assert!(
            all(&world, &access)
                .await
                .iter()
                .all(|r| r.qso.get("LOTW_QSLRDATE").is_none())
        );

        let applied = merge(
            &world.db,
            &access,
            Source::Lotw,
            None,
            report_records(),
            true,
            "20261122",
        )
        .await
        .unwrap();
        assert_eq!((applied.merged, applied.skipped.len()), (1, 0));
        let rows = all(&world, &access).await;
        let aaa = rows.iter().find(|r| r.qso.get("CALL") == Some("JA1AAA")).unwrap();
        assert_eq!(
            (
                aaa.qso.get("LOTW_QSL_RCVD"),
                aaa.qso.get("LOTW_QSLRDATE"),
                aaa.qso.get("QSL_RCVD")
            ),
            (Some("Y"), Some("20261101"), None)
        );
        assert_eq!(aaa.qso.get("GRIDSQUARE"), None);

        let rerun = merge(
            &world.db,
            &access,
            Source::Lotw,
            None,
            report_records(),
            true,
            "20261122",
        )
        .await
        .unwrap();
        assert_eq!((rerun.merged, rerun.already), (0, 3));
    }

    #[tokio::test]
    async fn merges_a_card_dated_today_without_touching_other_fields() {
        let (world, access, station) = setup().await;
        let mut sent = contact("JA1AAA", "0101");
        sent.set("QSL_SENT", Some("Y".into())).unwrap();
        import_new(&world.db, &access, &station, None, vec![Ok(sent)], true)
            .await
            .unwrap();

        let mut card = contact("JA1AAA", "0101");
        card.set("QSL_RCVD", Some("Y".into())).unwrap();
        card.set("QSL_RCVD_VIA", Some("B".into())).unwrap();
        let report = merge(
            &world.db,
            &access,
            Source::Card,
            Some(&station),
            vec![Ok(card)],
            true,
            "20261122",
        )
        .await
        .unwrap();
        assert_eq!(report.merged, 1);
        let merged = all(&world, &access).await.remove(0);
        let fields: Vec<_> = ["QSL_RCVD", "QSLRDATE", "QSL_RCVD_VIA", "QSL_SENT"]
            .iter()
            .map(|name| merged.qso.get(name))
            .collect();
        assert_eq!(fields, [Some("Y"), Some("20261122"), Some("B"), Some("Y")]);
    }
}
