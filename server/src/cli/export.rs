use std::path::PathBuf;

use chrono::NaiveDate;
use clap::{Args, Subcommand, ValueEnum};

use crate::{
    cli::{CliResult, Context, LengthArg, days, import::write_adif, parse_day},
    domain::{pota, qso::Qso},
    store::{
        WriteTx,
        access::LogbookAccess,
        export::{self, ExportKind},
        qso::{self, Filter, StoredQso, Target},
    },
};

const PAGE: u32 = 500;

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
    if args.park.is_some() && args.kind != KindArg::Pota {
        return Err("--park is only for a pota export".into());
    }
    let access = context.logbook_access(args.logbook.as_deref()).await?;
    let station_id = match &args.station {
        Some(key) => Some(context.station(&access, Some(key)).await?.id),
        None => None,
    };
    let filter = Filter {
        time: days(args.from, args.until),
        station_id,
        band: args.band.clone(),
        mode: args.mode.clone(),
        call: None,
        qsl_sent: args.qsl_sent.clone(),
        contest_id: args.contest_id.clone(),
    };
    let rows = fetch_all(context, &access, &filter).await?;
    let (records, targets) = project(&rows, args.kind, args.park.as_deref())?;
    if targets.is_empty() {
        println!("No QSOs to export.");
        return Ok(());
    }

    let kind = ExportKind::from(args.kind);
    let mut tx = WriteTx::begin(context.db).await?;
    let batch = export::create(&mut tx, &access, kind, &targets).await?;
    write_adif(&args.output, &records, context.length(args.length))?;
    tx.commit().await?;
    println!(
        "Wrote {} records for {} QSOs to {}",
        records.len(),
        targets.len(),
        args.output.display()
    );
    println!("Export batch {}", batch.id);
    if matches!(kind, ExportKind::Eqsl | ExportKind::Lotw | ExportKind::QslCard) {
        println!("Once they are sent, mark them with: export apply {}", batch.id);
    }
    Ok(())
}

async fn fetch_all(context: &Context<'_>, access: &LogbookAccess, filter: &Filter) -> CliResult<Vec<StoredQso>> {
    let mut conn = context.db.acquire().await?;
    let mut rows = vec![];
    let mut after = None;
    loop {
        let page = qso::search(&mut conn, access, filter, after.as_ref(), PAGE).await?;
        rows.extend(page.rows);
        match page.next {
            Some(next) => after = Some(next),
            None => break,
        }
    }
    rows.reverse();
    Ok(rows)
}

pub fn project(rows: &[StoredQso], kind: KindArg, park: Option<&str>) -> CliResult<(Vec<Qso>, Vec<Target>)> {
    let mut records = vec![];
    let mut targets = vec![];
    for row in rows {
        let projected = match kind {
            KindArg::Pota => pota::project(&row.qso)?
                .into_iter()
                .filter(|r| park.is_none_or(|p| r.get("MY_POTA_REF").is_some_and(|r| r.eq_ignore_ascii_case(p.trim()))))
                .collect(),
            _ => vec![row.qso.clone()],
        };
        if projected.is_empty() {
            continue;
        }
        records.extend(projected);
        targets.push(Target {
            id: row.id.parse()?,
            revision: row.revision,
        });
    }
    Ok((records, targets))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::qso::QsoId;

    fn row(fields: &[(&str, &str)]) -> StoredQso {
        StoredQso {
            id: QsoId::generate().to_string(),
            station_id: "station".into(),
            seq: 1,
            updated_seq: 1,
            revision: 2,
            created_by: "user".into(),
            created_at: "2026-10-22T00:00:00Z".into(),
            updated_by: "user".into(),
            updated_at: "2026-10-22T00:00:00Z".into(),
            deleted_at: None,
            qso: Qso::from_fields(fields.iter().copied()).unwrap(),
        }
    }

    #[test]
    fn projects_parks_and_leaves_out_qsos_without_one() {
        let rows = [
            row(&[("CALL", "JA1AAA"), ("MY_POTA_REF", "JP-0001,JP-0002")]),
            row(&[("CALL", "JA1BBB")]),
            row(&[("CALL", "JA1CCC"), ("MY_SIG", "POTA"), ("MY_SIG_INFO", "JP-0002")]),
        ];
        let (records, targets) = project(&rows, KindArg::Pota, None).unwrap();
        let calls: Vec<_> = records
            .iter()
            .map(|r| (r.get("CALL").unwrap(), r.get("MY_SIG_INFO").unwrap()))
            .collect();
        assert_eq!(
            calls,
            [("JA1AAA", "JP-0001"), ("JA1AAA", "JP-0002"), ("JA1CCC", "JP-0002")]
        );
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].revision, 2);

        let (records, targets) = project(&rows, KindArg::Pota, Some("jp-0002")).unwrap();
        assert_eq!((records.len(), targets.len()), (2, 2));

        let (records, targets) = project(&rows, KindArg::Adif, None).unwrap();
        assert_eq!((records.len(), targets.len()), (3, 3));
    }
}
