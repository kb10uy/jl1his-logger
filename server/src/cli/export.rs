use std::{fs, path::PathBuf};

use clap::{Args, Subcommand, ValueEnum};

use crate::{
    cli::{CliResult, Context, DayRange, EncodingArg, LengthArg, LogbookArg, UnencodableArg},
    domain::adif::{Encoding, LengthUnit, Output, Unencodable},
    store::{
        WriteTx,
        export::{self, ExportKind},
        qso::Filter,
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
    #[command(flatten)]
    logbook: LogbookArg,
    #[arg(long, help = "Only the QSOs of this Station")]
    station: Option<String>,
    #[command(flatten)]
    days: DayRange,
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
    #[arg(
        long,
        value_enum,
        help = "How lengths are counted; bytes for an encoding other than UTF-8 [default: from the configuration]"
    )]
    length: Option<LengthArg>,
    #[arg(long, value_enum, help = "Encoding of the file [default: utf-8]")]
    encoding: Option<EncodingArg>,
    #[arg(
        long,
        value_enum,
        help = "What to do with characters the encoding cannot represent [default: error]"
    )]
    unencodable: Option<UnencodableArg>,
}

#[derive(Subcommand)]
pub enum ExportCommand {
    #[command(about = "Writes an ADIF file and records its QSOs as an export batch")]
    Write(WriteArgs),
    #[command(about = "Lists the export batches")]
    List {
        #[command(flatten)]
        logbook: LogbookArg,
    },
    #[command(about = "Marks the QSOs of an eqsl, lotw or qsl-card batch as sent")]
    Apply {
        batch: String,
        #[command(flatten)]
        logbook: LogbookArg,
    },
}

pub async fn run(context: &Context<'_>, command: ExportCommand) -> CliResult<()> {
    match command {
        ExportCommand::Write(args) => write(context, &args).await,
        ExportCommand::List { logbook } => {
            let access = context.logbook_access(&logbook).await?;
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
            let access = context.logbook_access(&logbook).await?;
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
    let access = context.logbook_access(&args.logbook).await?;
    let station_id = match &args.station {
        Some(key) => Some(context.station(&access, Some(key)).await?.id),
        None => None,
    };
    let filter = Filter {
        time: args.days.range(),
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
    let encoding = args.encoding.map(Encoding::from).unwrap_or_default();
    let unit = encoding
        .export_unit(args.length.map(LengthUnit::from), context.config.adif.length)
        .ok_or_else(|| format!("lengths in {} are counted in bytes", encoding.name()))?;
    let output = Output {
        unit,
        encoding,
        unencodable: args.unencodable.map(Unencodable::from).unwrap_or_default(),
    };
    let text = export::encoded(&selection.records, output).map_err(|e| e.to_string())?;
    let mut tx = WriteTx::begin(context.db).await?;
    let batch = export::create(&mut tx, &access, kind, &selection.targets).await?;
    tx.commit().await?;
    fs::write(&args.output, encoding.encode(&text)).map_err(|e| format!("{}: {e}", args.output.display()))?;
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
