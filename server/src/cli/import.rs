use std::{
    collections::{BTreeMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use chrono::Utc;
use clap::{Args, ValueEnum};

use crate::{
    cli::{CliResult, Context, LengthArg},
    db::Db,
    domain::{
        adif::{self, LengthUnit},
        confirmation::{self, MatchKey, Source},
        import::RecordIds,
        qso::{Qso, QsoError, QsoId, Warning},
    },
    store::{
        StoreError, WriteTx,
        access::LogbookAccess,
        qsl::{self, SkipReason},
        qso::{self, CallMatch, Created, Filter, QsoInput, Target, TimeRange},
        station::Station,
    },
};

pub const BATCH: usize = 500;
const CANDIDATES: u32 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SourceArg {
    New,
    Lotw,
    Eqsl,
    Card,
}

#[derive(Args)]
pub struct ImportArgs {
    #[arg(help = "ADIF file: .adi, or .adx for ADX")]
    file: PathBuf,
    #[arg(
        long,
        value_enum,
        help = "New QSOs, or the QSL confirmations to merge into existing ones"
    )]
    source: SourceArg,
    #[arg(long, help = "Logbook ID or name; may be left out if you have only one")]
    logbook: Option<String>,
    #[arg(long, help = "Station of the new QSOs; for a merge, matches only its QSOs")]
    station: Option<String>,
    #[arg(
        long,
        value_enum,
        help = "How the .adi file counts lengths [default: from the configuration]"
    )]
    length: Option<LengthArg>,
    #[arg(long, help = "Write the changes; without it the import only previews")]
    apply: bool,
    #[arg(
        long,
        value_name = "FILE",
        help = "Write the records that were not imported to this ADIF file"
    )]
    rejects: Option<PathBuf>,
}

pub async fn run(context: &Context<'_>, args: &ImportArgs) -> CliResult<()> {
    let unit = context.length(args.length);
    let records = read(&args.file, unit)?;
    let access = context.logbook_access(args.logbook.as_deref()).await?;
    let rejects = match args.source {
        SourceArg::New => {
            let station = context.station(&access, args.station.as_deref()).await?;
            let report = import_new(context.db, &access, &station, records, args.apply).await?;
            report.print(args.apply);
            report.rejects
        }
        source => {
            let source = match source {
                SourceArg::Lotw => Source::Lotw,
                SourceArg::Eqsl => Source::Eqsl,
                _ => Source::Card,
            };
            let station = match &args.station {
                Some(key) => Some(context.station(&access, Some(key)).await?),
                None => None,
            };
            let today = Utc::now().format("%Y%m%d").to_string();
            let report = merge(
                context.db,
                &access,
                source,
                station.as_ref(),
                records,
                args.apply,
                &today,
            )
            .await?;
            report.print(args.apply);
            report.rejects
        }
    };
    if let Some(path) = &args.rejects {
        write_adif(path, &rejects, unit)?;
        println!("Wrote {} rejected records to {}", rejects.len(), path.display());
    }
    if !args.apply {
        println!("Nothing was written; rerun with --apply to import.");
    }
    Ok(())
}

pub fn read(path: &Path, unit: LengthUnit) -> CliResult<Vec<Result<Qso, QsoError>>> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let adx = path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("adx"));
    let records = match adx {
        true => adif::read_adx(&text),
        false => adif::read_adi(&text, unit),
    };
    Ok(records.map_err(|e| format!("{}: {e}", path.display()))?)
}

pub fn write_adif(path: &Path, records: &[Qso], unit: LengthUnit) -> CliResult<()> {
    let mut text = adif::header(env!("CARGO_PKG_VERSION"), Utc::now(), unit);
    for record in records {
        text.push_str(&adif::record(record, unit));
    }
    fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(())
}

#[derive(Debug, Default)]
pub struct NewReport {
    pub read: usize,
    pub created: usize,
    pub existing: usize,
    pub duplicates: Vec<(usize, Vec<String>)>,
    pub outside_band: Vec<(usize, String, String)>,
    pub invalid: Vec<(usize, String)>,
    pub rejects: Vec<Qso>,
}

impl NewReport {
    fn reject(&mut self, index: usize, error: impl ToString, record: Option<Qso>) {
        self.invalid.push((index, error.to_string()));
        self.rejects.extend(record);
    }

    fn print(&self, applied: bool) {
        let verb = if applied { "Created" } else { "Would create" };
        println!("Read {} records.", self.read);
        println!("{verb} {} QSOs; {} were already imported.", self.created, self.existing);
        for (index, ids) in &self.duplicates {
            println!("  record {index}: possible duplicate of {}", ids.join(", "));
        }
        for (index, freq, band) in &self.outside_band {
            println!("  record {index}: FREQ {freq} is outside {band}");
        }
        println!("Skipped {} invalid records.", self.invalid.len());
        for (index, error) in &self.invalid {
            println!("  record {index}: {error}");
        }
    }
}

pub async fn import_new(
    db: &Db,
    access: &LogbookAccess,
    station: &Station,
    records: Vec<Result<Qso, QsoError>>,
    apply: bool,
) -> CliResult<NewReport> {
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
    for batch in planned.chunks(BATCH) {
        let mut tx = WriteTx::begin(db).await?;
        for (index, id, qso) in batch {
            let input = QsoInput {
                station_id: station.id.clone(),
                qso: qso.clone(),
            };
            match qso::create(&mut tx, access, id, input).await {
                Ok(Created::New { warnings, .. }) => {
                    report.created += 1;
                    for warning in warnings {
                        match warning {
                            Warning::PossibleDuplicate { ids } => report.duplicates.push((*index, ids)),
                            Warning::FrequencyOutsideBand { freq, band } => {
                                report.outside_band.push((*index, freq, band))
                            }
                        }
                    }
                }
                Ok(Created::Existing(_)) => report.existing += 1,
                Err(StoreError::Qso(e)) => report.reject(*index, e, Some(qso.clone())),
                Err(e) => return Err(e.into()),
            }
        }
        if apply {
            tx.commit().await?;
        }
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
    pub skipped: Vec<(String, SkipReason)>,
    pub differences: BTreeMap<&'static str, usize>,
    pub rejects: Vec<Qso>,
}

impl MergeReport {
    fn print(&self, applied: bool) {
        let verb = if applied { "Merged" } else { "Would merge" };
        println!(
            "Read {} records, {} of them not confirmations.",
            self.read, self.not_confirmations
        );
        println!(
            "{verb} {} confirmations; {} were already received.",
            self.merged, self.already
        );
        if !self.differences.is_empty() {
            let counts: Vec<String> = self.differences.iter().map(|(f, n)| format!("{f} {n}")).collect();
            println!(
                "Matched QSOs differing from the records (not applied): {}",
                counts.join(", ")
            );
        }
        let unmatched: Vec<String> = self.unmatched.iter().map(usize::to_string).collect();
        println!("Unmatched records: {}", self.unmatched.len());
        if !unmatched.is_empty() {
            println!("  records {}", unmatched.join(", "));
        }
        println!("Ambiguous records: {}", self.ambiguous.len());
        for (index, ids) in &self.ambiguous {
            println!("  record {index}: {}", ids.join(", "));
        }
        println!("Invalid records: {}", self.invalid.len());
        for (index, error) in &self.invalid {
            println!("  record {index}: {error}");
        }
        for (id, reason) in &self.skipped {
            println!("  skipped {id}: {reason:?}");
        }
    }
}

pub async fn merge(
    db: &Db,
    access: &LogbookAccess,
    source: Source,
    station: Option<&Station>,
    records: Vec<Result<Qso, QsoError>>,
    apply: bool,
    today: &str,
) -> CliResult<MergeReport> {
    let mut report = MergeReport {
        read: records.len(),
        ..MergeReport::default()
    };
    let mut planned: Vec<(Target, confirmation::Changes)> = vec![];
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
        let key = match MatchKey::of(&record) {
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
            station_id: station.map(|s| s.id.clone()),
            call: Some(CallMatch::Exact(key.call.clone())),
            ..Filter::default()
        };
        let page = qso::search(&mut conn, access, &filter, None, CANDIDATES).await?;
        let candidates: Vec<_> = page.rows.iter().filter(|row| key.matches(&row.qso)).collect();
        match candidates.as_slice() {
            [] => {
                report.unmatched.push(index);
                report.rejects.push(record);
            }
            [row] => {
                for field in confirmation::differences(&record, &row.qso) {
                    *report.differences.entry(field).or_default() += 1;
                }
                if source.received(&row.qso) || !targeted.insert(row.id.clone()) {
                    report.already += 1;
                } else {
                    let target = Target {
                        id: row.id.parse()?,
                        revision: row.revision,
                    };
                    planned.push((target, changes));
                }
            }
            rows if rows.iter().all(|row| source.received(&row.qso)) => report.already += 1,
            rows => {
                report
                    .ambiguous
                    .push((index, rows.iter().map(|row| row.id.clone()).collect()));
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
        let mut groups: BTreeMap<&confirmation::Changes, Vec<Target>> = BTreeMap::new();
        for (target, changes) in batch {
            groups.entry(changes).or_default().push(target.clone());
        }
        let mut tx = WriteTx::begin(db).await?;
        for (changes, targets) in groups {
            let changes: BTreeMap<String, String> = changes.iter().map(|(f, v)| ((*f).to_owned(), v.clone())).collect();
            let outcome = qsl::update(&mut tx, access, &targets, &changes).await?;
            report.merged += outcome.updated.len();
            report
                .skipped
                .extend(outcome.skipped.into_iter().map(|s| (s.id, s.reason)));
        }
        tx.commit().await?;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{
        qso::StoredQso,
        station::{self, tests::input as station_input},
        testing::World,
    };

    fn record(fields: &[(&str, &str)]) -> Qso {
        Qso::from_fields(fields.iter().copied()).unwrap()
    }

    fn contact(call: &str, time_on: &str) -> Qso {
        record(&[
            ("CALL", call),
            ("QSO_DATE", "20261022"),
            ("TIME_ON", time_on),
            ("BAND", "40m"),
            ("MODE", "CW"),
        ])
    }

    async fn setup() -> (World, LogbookAccess, Station) {
        let world = World::new().await;
        let access = world.access(&world.owner).await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        let station = station::create(&mut tx, &access.owner().unwrap(), &station_input("Home"))
            .await
            .unwrap();
        tx.commit().await.unwrap();
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

        let preview = import_new(&world.db, &access, &station, records(), false)
            .await
            .unwrap();
        assert_eq!((preview.read, preview.created, preview.existing), (5, 3, 0));
        assert_eq!(preview.invalid.iter().map(|(i, _)| *i).collect::<Vec<_>>(), [3, 4]);
        assert_eq!(preview.duplicates.len(), 1);
        assert!(all(&world, &access).await.is_empty());

        let first = import_new(&world.db, &access, &station, records(), true).await.unwrap();
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
        let again = import_new(&world.db, &access, &station, more, true).await.unwrap();
        assert_eq!((again.created, again.existing), (1, 3));
        assert_eq!(all(&world, &access).await.len(), 4);
    }

    #[tokio::test]
    async fn keeps_a_deleted_import_deleted() {
        let (world, access, station) = setup().await;
        let records = || vec![Ok(contact("JA1AAA", "0101"))];
        import_new(&world.db, &access, &station, records(), true).await.unwrap();
        let row = all(&world, &access).await.remove(0);
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        qso::delete(&mut tx, &access, &row.id.parse().unwrap(), row.revision)
            .await
            .unwrap();
        tx.commit().await.unwrap();

        let again = import_new(&world.db, &access, &station, records(), true).await.unwrap();
        assert_eq!((again.created, again.existing), (0, 1));
        assert!(all(&world, &access).await[0].deleted_at.is_some());
    }

    #[tokio::test]
    async fn merges_confirmations_once_and_reports_the_rest() {
        let (world, access, station) = setup().await;
        let stored = vec![
            Ok(contact("JA1AAA", "0101")),
            Ok(contact("JA1BBB", "0202")),
            Ok(contact("JA1BBB", "0210")),
            Ok(record(&[
                ("CALL", "JA1CCC"),
                ("QSO_DATE", "20261022"),
                ("TIME_ON", "0303"),
                ("BAND", "40m"),
                ("MODE", "CW"),
                ("LOTW_QSL_RCVD", "Y"),
            ])),
        ];
        import_new(&world.db, &access, &station, stored, true).await.unwrap();

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
        import_new(&world.db, &access, &station, vec![Ok(sent)], true)
            .await
            .unwrap();

        let card = record(&[
            ("CALL", "JA1AAA"),
            ("QSO_DATE", "20261022"),
            ("TIME_ON", "0101"),
            ("BAND", "40m"),
            ("MODE", "CW"),
            ("QSL_RCVD", "Y"),
            ("QSL_RCVD_VIA", "B"),
        ]);
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
