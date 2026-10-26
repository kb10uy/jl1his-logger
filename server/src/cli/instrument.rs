use clap::{Args, Subcommand};

use crate::{
    cli::{CliResult, Context, DayRange, LogbookArg, parse_field},
    store::{
        WriteTx,
        access::LogbookAccess,
        logbook::instrument::{self, Instrument, InstrumentInput},
    },
};

#[derive(Args)]
pub struct InstrumentFields {
    #[arg(long, help = "MY_RIG baked into new QSOs; an empty value removes it")]
    rig: Option<String>,
    #[arg(long, help = "MY_ANTENNA baked into new QSOs; an empty value removes it")]
    antenna: Option<String>,
    #[arg(long, help = "TX_PWR in watts baked into new QSOs; an empty value removes it")]
    power: Option<String>,
    #[arg(
        long = "field",
        value_name = "NAME=VALUE",
        value_parser = parse_field,
        help = "Another default baked into new QSOs; repeat it, and give an empty value to remove one"
    )]
    fields: Vec<(String, String)>,
}

impl InstrumentFields {
    fn apply(self, input: &mut InstrumentInput) {
        for (given, field) in [
            (self.rig, &mut input.my_rig),
            (self.antenna, &mut input.my_antenna),
            (self.power, &mut input.tx_pwr),
        ] {
            if let Some(value) = given {
                *field = Some(value);
            }
        }
        for (name, value) in self.fields {
            let name = name.to_ascii_uppercase();
            match value.is_empty() {
                true => input.fields.remove(&name),
                false => input.fields.insert(name, value),
            };
        }
    }
}

#[derive(Subcommand)]
pub enum InstrumentCommand {
    #[command(about = "Adds an Instrument")]
    Add {
        name: String,
        #[command(flatten)]
        fields: InstrumentFields,
        #[command(flatten)]
        logbook: LogbookArg,
    },
    #[command(about = "Lists the Instruments")]
    List {
        #[command(flatten)]
        logbook: LogbookArg,
    },
    #[command(about = "Changes an Instrument; past QSOs keep their values until `instrument rewrite`")]
    Update {
        instrument: String,
        #[arg(long, help = "New name")]
        name: Option<String>,
        #[command(flatten)]
        fields: InstrumentFields,
        #[command(flatten)]
        logbook: LogbookArg,
    },
    #[command(about = "Archives an Instrument")]
    Archive {
        instrument: String,
        #[command(flatten)]
        logbook: LogbookArg,
    },
    #[command(about = "Brings an archived Instrument back")]
    Unarchive {
        instrument: String,
        #[command(flatten)]
        logbook: LogbookArg,
    },
    #[command(about = "Rewrites the past QSOs of an Instrument with its current values (owner)")]
    Rewrite {
        instrument: String,
        #[command(flatten)]
        days: DayRange,
        #[arg(long, help = "The number of QSOs the preview reported; without it only previews")]
        expect: Option<usize>,
        #[command(flatten)]
        logbook: LogbookArg,
    },
}

impl InstrumentCommand {
    fn logbook(&self) -> &LogbookArg {
        match self {
            InstrumentCommand::Add { logbook, .. }
            | InstrumentCommand::List { logbook }
            | InstrumentCommand::Update { logbook, .. }
            | InstrumentCommand::Archive { logbook, .. }
            | InstrumentCommand::Unarchive { logbook, .. }
            | InstrumentCommand::Rewrite { logbook, .. } => logbook,
        }
    }
}

fn print_instrument(instrument: &Instrument) {
    let archived = instrument.archived_at.as_ref().map(|_| "archived").unwrap_or("");
    println!("{}\t{}\t{archived}", instrument.id, instrument.name);
    for (name, value) in instrument.defaults() {
        println!("\t{name}={value}");
    }
}

pub async fn run(context: &Context<'_>, command: InstrumentCommand) -> CliResult<()> {
    let access = context.logbook_access(command.logbook()).await?;
    match command {
        InstrumentCommand::List { .. } => {
            let mut conn = context.db.acquire().await?;
            instrument::list(&mut conn, &access)
                .await?
                .iter()
                .for_each(print_instrument);
        }
        InstrumentCommand::Add { name, fields, .. } => {
            let mut input = InstrumentInput {
                name,
                ..InstrumentInput::default()
            };
            fields.apply(&mut input);
            let mut tx = WriteTx::begin(context.db).await?;
            let created = instrument::create(&mut tx, &access, &input).await?;
            tx.commit().await?;
            print_instrument(&created);
        }
        InstrumentCommand::Update {
            instrument: key,
            name,
            fields,
            ..
        } => {
            let current = context.instrument(&access, &key).await?;
            let mut input = InstrumentInput {
                name: name.unwrap_or(current.name),
                my_rig: current.my_rig,
                my_antenna: current.my_antenna,
                tx_pwr: current.tx_pwr,
                fields: current.fields,
            };
            fields.apply(&mut input);
            let mut tx = WriteTx::begin(context.db).await?;
            let updated = instrument::update(&mut tx, &access, &current.id, &input).await?;
            tx.commit().await?;
            print_instrument(&updated);
        }
        InstrumentCommand::Archive { instrument: key, .. } => set_archived(context, &access, &key, true).await?,
        InstrumentCommand::Unarchive { instrument: key, .. } => set_archived(context, &access, &key, false).await?,
        InstrumentCommand::Rewrite {
            instrument: key,
            days,
            expect,
            ..
        } => {
            let target = context.instrument(&access, &key).await?;
            let owner = access.owner()?;
            match expect {
                None => {
                    let mut conn = context.db.acquire().await?;
                    let count = instrument::preview_rewrite(&mut conn, &owner, &target.id, days.range()).await?;
                    println!(
                        "{count} QSOs of {} would change; rerun with --expect {count}",
                        target.name
                    );
                }
                Some(expected) => {
                    let mut tx = WriteTx::begin(context.db).await?;
                    let count = instrument::rewrite_qsos(&mut tx, &owner, &target.id, days.range(), expected).await?;
                    tx.commit().await?;
                    println!("Rewrote {count} QSOs of {}", target.name);
                }
            }
        }
    }
    Ok(())
}

async fn set_archived(context: &Context<'_>, access: &LogbookAccess, key: &str, archived: bool) -> CliResult<()> {
    let target = context.instrument(access, key).await?;
    let mut tx = WriteTx::begin(context.db).await?;
    let changed = instrument::set_archived(&mut tx, access, &target.id, archived).await?;
    tx.commit().await?;
    print_instrument(&changed);
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[test]
    fn applies_instrument_fields_onto_an_input() {
        let mut input = InstrumentInput {
            name: "IC-705".into(),
            my_rig: Some("IC-705".into()),
            my_antenna: Some("EFHW".into()),
            fields: BTreeMap::from([("MY_SIG".to_owned(), "POTA".to_owned())]),
            ..InstrumentInput::default()
        };
        let fields = InstrumentFields {
            rig: None,
            antenna: Some(String::new()),
            power: Some("0.5".into()),
            fields: vec![
                ("my_sig".to_owned(), String::new()),
                ("my_sig_info".to_owned(), "JP-0001".to_owned()),
            ],
        };
        fields.apply(&mut input);
        assert_eq!(
            (
                input.my_rig.as_deref(),
                input.my_antenna.as_deref(),
                input.tx_pwr.as_deref()
            ),
            (Some("IC-705"), Some(""), Some("0.5"))
        );
        assert_eq!(
            input.fields,
            BTreeMap::from([("MY_SIG_INFO".to_owned(), "JP-0001".to_owned())])
        );
    }
}
