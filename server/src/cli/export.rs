use std::path::PathBuf;

use chrono::NaiveDate;
use clap::{Args, Subcommand, ValueEnum};

use crate::{
    cli::{CliResult, Context, LengthArg, import::write_adif, parse_day},
    store::{
        WriteTx,
        export::{self, ExportKind},
        qso::{Filter, TimeRange},
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum KindArg {
    Adif,
    Pota,
    Eqsl,
    Lotw,
    QslCard,
}

impl From<KindArg> for ExportKind {
    fn from(arg: KindArg) -> ExportKind {
        match arg {
            KindArg::Adif => ExportKind::Adif,
            KindArg::Pota => ExportKind::Pota,
            KindArg::Eqsl => ExportKind::Eqsl,
            KindArg::Lotw => ExportKind::Lotw,
            KindArg::QslCard => ExportKind::QslCard,
        }
    }
}

#[derive(Args)]
pub struct WriteArgs {
    #[arg(value_enum, help = "What the file is for; pota writes one record per park")]
    kind: KindArg,
    #[arg(short, long, help = "ADIF file to write")]
    output: PathBuf,
    #[arg(long, help = "Logbook ID or name; may be left out if you have only one")]
    logbook: Option<String>,
    #[arg(long, help = "Only the QSOs of this Station")]
    station: Option<String>,
    #[arg(long, value_parser = parse_day, help = "First QSO day (UTC), such as 2026-10-01")]
    from: Option<NaiveDate>,
    #[arg(long, value_parser = parse_day, help = "Last QSO day (UTC), inclusive")]
    until: Option<NaiveDate>,
    #[arg(long, help = "Only this band")]
    band: Option<String>,
    #[arg(long, help = "Only this mode")]
    mode: Option<String>,
    #[arg(
        long,
        value_delimiter = ',',
        help = "Only these QSL_SENT values, such as R,Q; an empty value matches none"
    )]
    qsl_sent: Option<Vec<String>>,
    #[arg(long, help = "Only this CONTEST_ID")]
    contest_id: Option<String>,
    #[arg(long, help = "Only this park, for a pota export")]
    park: Option<String>,
    #[arg(long, value_enum, help = "How lengths are counted [default: from the configuration]")]
    length: Option<LengthArg>,
}

#[derive(Subcommand)]
pub enum ExportCommand {
    #[command(about = "Writes an ADIF file and records its QSOs as an export batch")]
    Write(WriteArgs),
    #[command(about = "Lists the export batches")]
    List {
        #[arg(long, help = "Logbook ID or name; may be left out if you have only one")]
        logbook: Option<String>,
    },
    #[command(about = "Marks the QSOs of an eqsl, lotw or qsl-card batch as sent")]
    Apply {
        batch: String,
        #[arg(long, help = "Logbook ID or name; may be left out if you have only one")]
        logbook: Option<String>,
    },
}

pub async fn run(context: &Context<'_>, command: ExportCommand) -> CliResult<()> {
    match command {
        ExportCommand::Write(args) => write(context, &args).await,
        ExportCommand::List { logbook } => {
            let access = context.logbook_access(logbook.as_deref()).await?;
            let mut conn = context.db.acquire().await?;
            for batch in export::list(&mut conn, &access).await? {
                let applied = match &batch.applied_at {
                    Some(at) => format!("applied {at}"),
                    None => "not applied".to_owned(),
                };
                println!(
                    "{}\t{}\t{}\t{} QSOs\t{applied}",
                    batch.id,
                    batch.kind.as_str(),
                    batch.created_at,
                    batch.qso_count
                );
            }
            Ok(())
        }
        ExportCommand::Apply { batch, logbook } => {
            let access = context.logbook_access(logbook.as_deref()).await?;
            let mut tx = WriteTx::begin(context.db).await?;
            let outcome = export::apply(&mut tx, &access, &batch).await?;
            tx.commit().await?;
            println!("Marked {} QSOs as sent.", outcome.updated.len());
            for skipped in &outcome.skipped {
                println!("  skipped {}: {:?}", skipped.id, skipped.reason);
            }
            Ok(())
        }
    }
}

async fn write(context: &Context<'_>, args: &WriteArgs) -> CliResult<()> {
    let access = context.logbook_access(args.logbook.as_deref()).await?;
    let station_id = match &args.station {
        Some(key) => Some(context.station(&access, Some(key)).await?.id),
        None => None,
    };
    let filter = Filter {
        time: TimeRange::days(args.from, args.until),
        station_ids: station_id.map(|id| vec![id]),
        band: args.band.clone(),
        mode: args.mode.clone(),
        qsl_sent: args.qsl_sent.clone(),
        contest_id: args.contest_id.clone(),
        ..Filter::default()
    };
    let kind = ExportKind::from(args.kind);
    let mut conn = context.db.acquire().await?;
    let selection = export::select(&mut conn, &access, kind, &filter, args.park.as_deref()).await?;
    drop(conn);
    if selection.targets.is_empty() {
        println!("No QSOs to export.");
        return Ok(());
    }
    let mut tx = WriteTx::begin(context.db).await?;
    let batch = export::create(&mut tx, &access, kind, &selection.targets).await?;
    tx.commit().await?;
    write_adif(&args.output, &selection.records, context.length(args.length))?;
    println!(
        "Wrote {} records for {} QSOs to {}",
        selection.records.len(),
        selection.targets.len(),
        args.output.display()
    );
    println!("Export batch {}", batch.id);
    if matches!(kind, ExportKind::Eqsl | ExportKind::Lotw | ExportKind::QslCard) {
        println!("Once they are sent, mark them with: export apply {}", batch.id);
    }
    Ok(())
}
