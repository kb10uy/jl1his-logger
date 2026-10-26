use std::{
    fs,
    path::{Path, PathBuf},
};

use clap::{Args, ValueEnum};

use crate::{
    cli::{CliResult, Context, EncodingArg, LengthArg, LogbookArg},
    domain::{
        adif::{self, Encoding, LengthUnit, Lengths},
        confirmation::Source,
        qso::{Qso, Warning},
    },
    store::{
        export,
        import::{MergeReport, NewReport, import_new, merge},
        today,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SourceArg {
    New,
    Lotw,
    Eqsl,
    Card,
}

impl SourceArg {
    fn confirmation(self) -> Option<Source> {
        match self {
            SourceArg::New => None,
            SourceArg::Lotw => Some(Source::Lotw),
            SourceArg::Eqsl => Some(Source::Eqsl),
            SourceArg::Card => Some(Source::Card),
        }
    }
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
    #[command(flatten)]
    logbook: LogbookArg,
    #[arg(long, help = "Station of the new QSOs; for a merge, matches only its QSOs")]
    station: Option<String>,
    #[arg(long, help = "Instrument of the new QSOs, baked in where a record has no value")]
    instrument: Option<String>,
    #[arg(
        long,
        value_enum,
        help = "How the .adi file counts lengths [default: detected for each value]"
    )]
    length: Option<LengthArg>,
    #[arg(
        long,
        value_enum,
        help = "Encoding of the file [default: UTF-8 if it is valid, else the configured fallback]"
    )]
    encoding: Option<EncodingArg>,
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
    let lengths = match args.length {
        Some(length) => Lengths::Fixed(length.into()),
        None => Lengths::Detect {
            default: context.config.adif.length,
        },
    };
    let fallback = context.config.adif.fallback_encoding;
    let (read, encoding) = read(&args.file, args.encoding.map(Encoding::from), lengths, fallback)?;
    print_reading(encoding, &read.lengths);
    let records = read.records;
    let access = context.logbook_access(&args.logbook).await?;
    let rejects = match args.source.confirmation() {
        None => {
            let station = context.station(&access, args.station.as_deref()).await?;
            let instrument = match &args.instrument {
                Some(key) => Some(context.instrument(&access, key).await?),
                None => None,
            };
            let report = import_new(context.db, &access, &station, instrument.as_ref(), records, args.apply).await?;
            print_new(&report, args.apply);
            report.rejects
        }
        Some(_) if args.instrument.is_some() => return Err("--instrument is only for new QSOs".into()),
        Some(source) => {
            let station = match &args.station {
                Some(key) => Some(context.station(&access, Some(key)).await?),
                None => None,
            };
            let report = merge(
                context.db,
                &access,
                source,
                station.as_ref(),
                records,
                args.apply,
                &today(),
            )
            .await?;
            print_merge(&report, args.apply);
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

fn read(
    path: &Path,
    encoding: Option<Encoding>,
    lengths: Lengths,
    fallback: Encoding,
) -> CliResult<(adif::Read, Encoding)> {
    let bytes = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let (text, encoding) = adif::decode(&bytes, encoding, fallback)
        .map_err(|e| format!("{}: the file is not {}", path.display(), e.name()))?;
    let adx = path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("adx"));
    let read = match adx {
        true => adif::read_adx(&text),
        false => adif::read_adi(&text, encoding, lengths),
    };
    Ok((read.map_err(|e| format!("{}: {e}", path.display()))?, encoding))
}

fn print_reading(encoding: Encoding, lengths: &[LengthUnit]) {
    if encoding != Encoding::Utf8 {
        println!("Read the file as {}.", encoding.name());
    }
    if !lengths.is_empty() {
        let names: Vec<_> = lengths.iter().map(|unit| unit.name()).collect();
        println!("Non-ASCII values count their lengths in {}.", names.join(" and "));
    }
}

pub(super) fn write_adif(path: &Path, records: &[Qso], unit: LengthUnit) -> CliResult<()> {
    fs::write(path, export::document(records, unit)).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(())
}

fn print_new(report: &NewReport, applied: bool) {
    let verb = if applied { "Created" } else { "Would create" };
    println!("Read {} records.", report.read);
    println!(
        "{verb} {} QSOs; {} were already imported.",
        report.created, report.existing
    );
    for (index, ids) in &report.duplicates {
        println!("  record {index}: possible duplicate of {}", ids.join(", "));
    }
    for (index, freq, band) in &report.outside_band {
        println!("  record {index}: FREQ {freq} is outside {band}");
    }
    for (index, warning) in &report.modes {
        match warning {
            Warning::UnknownMode { mode } => println!("  record {index}: MODE {mode} is not an ADIF mode"),
            Warning::UnknownSubmode { submode } => {
                println!("  record {index}: SUBMODE {submode} is not an ADIF submode")
            }
            Warning::SubmodeOfAnotherMode {
                mode,
                submode,
                expected,
            } => {
                println!("  record {index}: SUBMODE {submode} belongs to {expected}, not {mode}")
            }
            _ => {}
        }
    }
    println!("Skipped {} invalid records.", report.invalid.len());
    for (index, error) in &report.invalid {
        println!("  record {index}: {error}");
    }
}

fn print_merge(report: &MergeReport, applied: bool) {
    let verb = if applied { "Merged" } else { "Would merge" };
    println!(
        "Read {} records, {} of them not confirmations.",
        report.read, report.not_confirmations
    );
    println!(
        "{verb} {} confirmations; {} were already received.",
        report.merged, report.already
    );
    if !report.differences.is_empty() {
        let counts: Vec<String> = report.differences.iter().map(|(f, n)| format!("{f} {n}")).collect();
        println!(
            "Matched QSOs differing from the records (not applied): {}",
            counts.join(", ")
        );
    }
    let unmatched: Vec<String> = report.unmatched.iter().map(usize::to_string).collect();
    println!("Unmatched records: {}", report.unmatched.len());
    if !unmatched.is_empty() {
        println!("  records {}", unmatched.join(", "));
    }
    println!("Ambiguous records: {}", report.ambiguous.len());
    for (index, ids) in &report.ambiguous {
        println!("  record {index}: {}", ids.join(", "));
    }
    println!("Invalid records: {}", report.invalid.len());
    for (index, error) in &report.invalid {
        println!("  record {index}: {error}");
    }
    for skipped in &report.skipped {
        println!("  skipped {}: {:?}", skipped.id, skipped.reason);
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    const FIXED: Lengths = Lengths::Fixed(LengthUnit::Codepoints);

    #[rstest]
    #[case(SourceArg::New, None)]
    #[case(SourceArg::Lotw, Some(Source::Lotw))]
    #[case(SourceArg::Eqsl, Some(Source::Eqsl))]
    #[case(SourceArg::Card, Some(Source::Card))]
    fn maps_each_source_to_its_confirmation(#[case] arg: SourceArg, #[case] expected: Option<Source>) {
        assert_eq!(arg.confirmation(), expected);
    }

    #[rstest]
    #[case("log.adi", "<CALL:6>JA1XYZ <EOR>")]
    #[case(
        "log.ADX",
        "<?xml version=\"1.0\"?><ADX><HEADER></HEADER><RECORDS><RECORD><CALL>JA1XYZ</CALL></RECORD></RECORDS></ADX>"
    )]
    fn reads_adi_or_adx_by_the_extension(#[case] name: &str, #[case] text: &str) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(name);
        fs::write(&path, text).unwrap();
        let (read, _) = read(&path, None, FIXED, Encoding::ShiftJis).unwrap();
        let records = read.records;
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].as_ref().unwrap().get("CALL"), Some("JA1XYZ"));
    }

    #[test]
    fn writes_records_that_read_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.adi");
        let qso = Qso::from_fields([("CALL", "JA1XYZ"), ("NAME", "太郎")]).unwrap();
        write_adif(&path, std::slice::from_ref(&qso), LengthUnit::Codepoints).unwrap();
        let (read_back, encoding) = read(&path, None, FIXED, Encoding::ShiftJis).unwrap();
        assert_eq!((read_back.records, encoding), (vec![Ok(qso)], Encoding::Utf8));
        assert!(read(&dir.path().join("missing.adi"), None, FIXED, Encoding::ShiftJis).is_err());
    }

    #[test]
    fn reads_a_shift_jis_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hamlog.adi");
        fs::write(&path, Encoding::ShiftJis.encode("<CALL:6>JA1XYZ <NAME:4>太郎 <EOR>")).unwrap();
        let detect = Lengths::Detect {
            default: LengthUnit::Codepoints,
        };
        let (read_back, encoding) = read(&path, None, detect, Encoding::ShiftJis).unwrap();
        assert_eq!(encoding, Encoding::ShiftJis);
        assert_eq!(read_back.lengths, [LengthUnit::Bytes]);
        assert_eq!(read_back.records[0].as_ref().unwrap().get("NAME"), Some("太郎"));
        assert!(read(&path, Some(Encoding::Utf8), detect, Encoding::ShiftJis).is_err());
    }
}
