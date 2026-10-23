use std::{
    fs,
    path::{Path, PathBuf},
};

use chrono::Utc;
use clap::{Args, ValueEnum};

use crate::{
    cli::{CliResult, Context, LengthArg},
    domain::{
        adif::{self, LengthUnit},
        confirmation::Source,
        qso::{Qso, QsoError, Warning},
    },
    store::import::{MergeReport, NewReport, import_new, merge},
};

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
    let text = adif::document(env!("CARGO_PKG_VERSION"), Utc::now(), records, unit);
    fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(())
}

impl NewReport {
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
        for (index, warning) in &self.modes {
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
        println!("Skipped {} invalid records.", self.invalid.len());
        for (index, error) in &self.invalid {
            println!("  record {index}: {error}");
        }
    }
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
