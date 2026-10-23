pub mod admin;
pub mod backup;
pub mod export;
pub mod import;
pub mod reference;

use std::{error::Error, path::Path};

use chrono::{NaiveDate, NaiveTime};
use clap::{Subcommand, ValueEnum};
use url::Url;

use crate::{
    config::Config,
    db::{self, Db},
    domain::{adif::LengthUnit, qso::parse_date},
    store::{
        access::{self, LogbookAccess, Principal},
        logbook::{self, Logbook},
        qso::TimeRange,
        station::{self, Station},
        user,
    },
};

pub type CliResult<T> = Result<T, Box<dyn Error>>;

pub struct Context<'a> {
    pub db: &'a Db,
    pub database: &'a Path,
    pub config: &'a Config,
    pub actor: Option<&'a str>,
}

#[derive(Subcommand)]
pub enum Command {
    #[command(about = "Creates the first admin and prints a link to register a passkey")]
    Setup(admin::SetupArgs),
    #[command(subcommand, about = "Manages users (admin)")]
    User(admin::UserCommand),
    #[command(subcommand, about = "Manages Logbooks")]
    Logbook(admin::LogbookCommand),
    #[command(subcommand, about = "Lists the members of a Logbook, or manages them (owner)")]
    Member(admin::MemberCommand),
    #[command(subcommand, about = "Manages the Stations of a Logbook (owner)")]
    Station(admin::StationCommand),
    #[command(subcommand, about = "Manages your API tokens")]
    Token(admin::TokenCommand),
    #[command(about = "Imports new QSOs or merges QSL confirmations from an ADIF file")]
    Import(import::ImportArgs),
    #[command(subcommand, about = "Exports QSOs as ADIF and applies the results")]
    Export(export::ExportCommand),
    #[command(subcommand, about = "Reads and writes instance settings (admin)")]
    Settings(admin::SettingsCommand),
    #[command(subcommand, about = "Lists and installs reference data")]
    Reference(reference::ReferenceCommand),
    #[command(about = "Writes a copy of the database while the server runs")]
    Backup(backup::BackupArgs),
    #[command(about = "Replaces the database with a backup; the server must be stopped")]
    Restore(backup::RestoreArgs),
}

pub async fn run(database: &Path, config: &Config, actor: Option<&str>, command: Command) -> CliResult<()> {
    if let Command::Restore(args) = &command {
        return backup::restore(database, args).await;
    }
    let db = db::open(database).await?;
    let result = async {
        match &command {
            Command::Setup(_) => {
                let migrated = db::migrate(&db, &db::snapshot_dir(database)).await?;
                if let Some(snapshot) = &migrated.snapshot {
                    println!("Snapshot written to {}", snapshot.display());
                }
            }
            _ => {
                let status = db::migration_status(&db).await?;
                if !status.pending.is_empty() {
                    return Err(format!(
                        "migrations {:?} are pending; run the migrate subcommand first",
                        status.pending
                    )
                    .into());
                }
            }
        }
        let context = Context {
            db: &db,
            database,
            config,
            actor,
        };
        match command {
            Command::Setup(args) => admin::setup(&context, &args).await,
            Command::User(command) => admin::user(&context, command).await,
            Command::Logbook(command) => admin::logbook(&context, command).await,
            Command::Member(command) => admin::member(&context, command).await,
            Command::Station(command) => admin::station(&context, command).await,
            Command::Token(command) => admin::token(&context, command).await,
            Command::Import(args) => import::run(&context, &args).await,
            Command::Export(command) => export::run(&context, command).await,
            Command::Settings(command) => admin::settings(&context, command).await,
            Command::Reference(command) => reference::run(&context, command).await,
            Command::Backup(args) => backup::backup(&context, &args).await,
            Command::Restore(_) => unreachable!("restore runs without an open database"),
        }
    }
    .await;
    db.close().await;
    result
}

impl Context<'_> {
    pub async fn principal(&self) -> CliResult<Principal> {
        let callsign = self.actor.ok_or("name the user to act as with --as or CLI_CALLSIGN")?;
        let mut conn = self.db.acquire().await?;
        let user = user::find_by_callsign(&mut conn, callsign)
            .await?
            .ok_or_else(|| format!("no user has the callsign {callsign}"))?;
        Ok(Principal::user(&user.id))
    }

    pub async fn logbook_access(&self, logbook: Option<&str>) -> CliResult<LogbookAccess> {
        let principal = self.principal().await?;
        let mut conn = self.db.acquire().await?;
        let user = access::user(&mut conn, &principal).await?;
        let logbooks: Vec<Logbook> = logbook::list(&mut conn, &user)
            .await?
            .into_iter()
            .map(|(logbook, _)| logbook)
            .collect();
        let found = pick(&logbooks, logbook, "Logbook", |l| &l.id, |l| &l.name)?;
        Ok(access::logbook(&mut conn, &principal, &found.id).await?)
    }

    pub async fn station(&self, access: &LogbookAccess, key: Option<&str>) -> CliResult<Station> {
        let mut conn = self.db.acquire().await?;
        let stations = station::list(&mut conn, access).await?;
        Ok(pick(&stations, key, "Station", |s| &s.id, |s| &s.name)?.clone())
    }

    pub fn length(&self, length: Option<LengthArg>) -> LengthUnit {
        length.map(LengthUnit::from).unwrap_or(self.config.adif.length)
    }
}

pub fn pick<'a, T>(
    items: &'a [T],
    key: Option<&str>,
    what: &str,
    id: impl Fn(&T) -> &str,
    name: impl Fn(&T) -> &str,
) -> CliResult<&'a T> {
    let found: Vec<&T> = match key {
        None => items.iter().collect(),
        Some(key) => match items.iter().find(|item| id(item) == key) {
            Some(item) => vec![item],
            None => items
                .iter()
                .filter(|item| name(item).eq_ignore_ascii_case(key.trim()))
                .collect(),
        },
    };
    match (found.as_slice(), key) {
        ([item], _) => Ok(item),
        ([], Some(key)) => Err(format!("no {what} {key:?}").into()),
        ([], None) => Err(format!("there is no {what}").into()),
        (_, Some(key)) => Err(format!("{key:?} names several {what}s; give its ID").into()),
        (_, None) => Err(format!("there are several {what}s; choose one").into()),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum LengthArg {
    Codepoints,
    Bytes,
}

impl From<LengthArg> for LengthUnit {
    fn from(arg: LengthArg) -> LengthUnit {
        match arg {
            LengthArg::Codepoints => LengthUnit::Codepoints,
            LengthArg::Bytes => LengthUnit::Bytes,
        }
    }
}

pub fn parse_day(s: &str) -> Result<NaiveDate, String> {
    parse_date(&s.trim().replace('-', "")).ok_or_else(|| format!("{s:?} is not a date such as 2026-10-22"))
}

pub fn days(from: Option<NaiveDate>, until: Option<NaiveDate>) -> TimeRange {
    let start = |day: NaiveDate| day.and_time(NaiveTime::MIN).and_utc().timestamp();
    TimeRange {
        from: from.map(start),
        until: until.and_then(|day| day.succ_opt()).map(start),
    }
}

pub fn registration_url(origin: &Url, token: &str) -> Url {
    let mut url = origin.join("/register").expect("an origin takes a path");
    url.set_fragment(Some(token));
    url
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    struct Item(&'static str, &'static str);

    const ITEMS: &[Item] = &[
        Item("1", "Home"),
        Item("2", "Club"),
        Item("3", "club"),
        Item("Home", "Park"),
    ];

    fn find(key: Option<&str>) -> Result<&'static str, String> {
        pick(ITEMS, key, "Station", |i| i.0, |i| i.1)
            .map(|i| i.0)
            .map_err(|e| e.to_string())
    }

    #[rstest]
    #[case(Some("2"), Ok("2"))]
    #[case(Some("Home"), Ok("Home"))]
    #[case(Some("park"), Ok("Home"))]
    #[case(Some("Club"), Err("\"Club\" names several Stations; give its ID".into()))]
    #[case(Some("Nowhere"), Err("no Station \"Nowhere\"".into()))]
    #[case(None, Err("there are several Stations; choose one".into()))]
    fn picks_an_item_by_id_or_name(#[case] key: Option<&str>, #[case] expected: Result<&str, String>) {
        assert_eq!(find(key), expected);
    }

    #[test]
    fn picks_the_only_item_without_a_key() {
        let items = [Item("1", "Home")];
        assert_eq!(pick(&items, None, "Station", |i| i.0, |i| i.1).unwrap().0, "1");
    }

    #[test]
    fn turns_days_into_a_time_range() {
        let range = days(
            Some(parse_day("2026-10-01").unwrap()),
            Some(parse_day("20261031").unwrap()),
        );
        assert_eq!(range.from, Some(1_790_812_800));
        assert_eq!(range.until, Some(1_790_812_800 + 31 * 86_400));
        assert!(parse_day("2026-13-01").is_err());
    }

    #[test]
    fn keeps_the_registration_token_out_of_the_request() {
        let origin = Url::parse("https://log.example.com").unwrap();
        let url = registration_url(&origin, "abc");
        assert_eq!(url.as_str(), "https://log.example.com/register#abc");
    }
}
