use std::collections::BTreeMap;

use clap::{Args, Subcommand};

use crate::{
    cli::{CliResult, Context, DayRange, LogbookArg, parse_field},
    store::{
        WriteTx,
        access::LogbookAccess,
        logbook::station::{self, Station, StationInput},
    },
};

#[derive(Args)]
pub struct StationFields {
    #[arg(long, help = "Old Wavelog station ID kept for /wl2 clients")]
    wavelog_id: Option<i64>,
    #[arg(
        long = "field",
        value_name = "NAME=VALUE",
        value_parser = parse_field,
        help = "A MY_* default baked into new QSOs; repeat it, and give an empty value to remove one"
    )]
    fields: Vec<(String, String)>,
}

impl StationFields {
    fn apply(&self, input: &mut StationInput) {
        if let Some(id) = self.wavelog_id {
            input.wavelog_id = Some(id);
        }
        for (name, value) in &self.fields {
            let name = name.to_ascii_uppercase();
            match value.is_empty() {
                true => input.fields.remove(&name),
                false => input.fields.insert(name, value.clone()),
            };
        }
    }
}

#[derive(Subcommand)]
pub enum StationCommand {
    #[command(about = "Adds a Station")]
    Add {
        name: String,
        #[arg(long, help = "STATION_CALLSIGN baked into new QSOs")]
        callsign: String,
        #[command(flatten)]
        fields: StationFields,
        #[command(flatten)]
        logbook: LogbookArg,
    },
    #[command(about = "Lists the Stations")]
    List {
        #[command(flatten)]
        logbook: LogbookArg,
    },
    #[command(about = "Changes a Station; past QSOs keep their values until `station rewrite`")]
    Update {
        station: String,
        #[arg(long, help = "New name")]
        name: Option<String>,
        #[arg(long, help = "New STATION_CALLSIGN")]
        callsign: Option<String>,
        #[command(flatten)]
        fields: StationFields,
        #[command(flatten)]
        logbook: LogbookArg,
    },
    #[command(about = "Archives a Station")]
    Archive {
        station: String,
        #[command(flatten)]
        logbook: LogbookArg,
    },
    #[command(about = "Brings an archived Station back")]
    Unarchive {
        station: String,
        #[command(flatten)]
        logbook: LogbookArg,
    },
    #[command(about = "Rewrites the past QSOs of a Station with its current values")]
    Rewrite {
        station: String,
        #[command(flatten)]
        days: DayRange,
        #[arg(long, help = "The number of QSOs the preview reported; without it only previews")]
        expect: Option<usize>,
        #[command(flatten)]
        logbook: LogbookArg,
    },
}

impl StationCommand {
    fn logbook(&self) -> &LogbookArg {
        match self {
            StationCommand::Add { logbook, .. }
            | StationCommand::List { logbook }
            | StationCommand::Update { logbook, .. }
            | StationCommand::Archive { logbook, .. }
            | StationCommand::Unarchive { logbook, .. }
            | StationCommand::Rewrite { logbook, .. } => logbook,
        }
    }
}

fn print_station(station: &Station) {
    let wavelog = station.wavelog_id.map(|id| format!("wavelog {id}")).unwrap_or_default();
    let archived = station.archived_at.as_ref().map(|_| "archived").unwrap_or("");
    println!(
        "{}\t{}\t{}\t{wavelog}\t{archived}",
        station.id, station.name, station.callsign
    );
    for (name, value) in &station.fields {
        println!("\t{name}={value}");
    }
}

pub async fn station(context: &Context<'_>, command: StationCommand) -> CliResult<()> {
    let access = context.logbook_access(command.logbook()).await?;
    match command {
        StationCommand::List { .. } => {
            let mut conn = context.db.acquire().await?;
            station::list(&mut conn, &access).await?.iter().for_each(print_station);
        }
        StationCommand::Add {
            name, callsign, fields, ..
        } => {
            let mut input = StationInput {
                name,
                callsign,
                wavelog_id: None,
                fields: BTreeMap::new(),
            };
            fields.apply(&mut input);
            let mut tx = WriteTx::begin(context.db).await?;
            let created = station::create(&mut tx, &access.owner()?, &input).await?;
            tx.commit().await?;
            print_station(&created);
        }
        StationCommand::Update {
            station: key,
            name,
            callsign,
            fields,
            ..
        } => {
            let current = context.station(&access, Some(&key)).await?;
            let mut input = StationInput {
                name: name.unwrap_or(current.name),
                callsign: callsign.unwrap_or(current.callsign),
                wavelog_id: current.wavelog_id,
                fields: current.fields,
            };
            fields.apply(&mut input);
            let mut tx = WriteTx::begin(context.db).await?;
            let updated = station::update(&mut tx, &access.owner()?, &current.id, &input).await?;
            tx.commit().await?;
            print_station(&updated);
        }
        StationCommand::Archive { station: key, .. } => set_archived(context, &access, &key, true).await?,
        StationCommand::Unarchive { station: key, .. } => set_archived(context, &access, &key, false).await?,
        StationCommand::Rewrite {
            station: key,
            days,
            expect,
            ..
        } => {
            let target = context.station(&access, Some(&key)).await?;
            let owner = access.owner()?;
            match expect {
                None => {
                    let mut conn = context.db.acquire().await?;
                    let count = station::preview_rewrite(&mut conn, &owner, &target.id, days.range()).await?;
                    println!(
                        "{count} QSOs of {} would change; rerun with --expect {count}",
                        target.name
                    );
                }
                Some(expected) => {
                    let mut tx = WriteTx::begin(context.db).await?;
                    let count = station::rewrite_qsos(&mut tx, &owner, &target.id, days.range(), expected).await?;
                    tx.commit().await?;
                    println!("Rewrote {count} QSOs of {}", target.name);
                }
            }
        }
    }
    Ok(())
}

async fn set_archived(context: &Context<'_>, access: &LogbookAccess, key: &str, archived: bool) -> CliResult<()> {
    let target = context.station(access, Some(key)).await?;
    let mut tx = WriteTx::begin(context.db).await?;
    let changed = station::set_archived(&mut tx, &access.owner()?, &target.id, archived).await?;
    tx.commit().await?;
    print_station(&changed);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies_station_fields_onto_an_input() {
        let mut input = StationInput {
            fields: BTreeMap::from([("MY_CITY".to_owned(), "Minato".to_owned())]),
            ..StationInput::default()
        };
        let fields = StationFields {
            wavelog_id: Some(3),
            fields: vec![
                ("my_city".to_owned(), String::new()),
                ("my_rig".to_owned(), "IC-705".to_owned()),
            ],
        };
        fields.apply(&mut input);
        assert_eq!(input.wavelog_id, Some(3));
        assert_eq!(
            input.fields,
            BTreeMap::from([("MY_RIG".to_owned(), "IC-705".to_owned())])
        );
    }
}
