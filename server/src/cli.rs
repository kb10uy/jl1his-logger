mod backup;
mod export;
mod import;
mod instrument;
mod logbook;
mod reference;
mod settings;
mod setup;
mod station;
mod token;
mod user;

use std::{error::Error, path::Path};

use chrono::NaiveDate;
use clap::{Args, Subcommand, ValueEnum};
use sqlx::SqliteConnection;
use url::Url;

use crate::{
    config::Config,
    db::{self, Db, Migrated},
    domain::{adif::LengthUnit, qso::parse_date},
    store::{
        WriteTx,
        access::{self, AdminAccess, LogbookAccess, Principal},
        account::user::{self as users, User},
        logbook::{
            self as logbooks, Logbook,
            instrument::Instrument,
            station::{self as stations, Station},
        },
        qso::TimeRange,
    },
};

pub type CliResult<T> = Result<T, Box<dyn Error>>;

struct Context<'a> {
    db: &'a Db,
    database: &'a Path,
    config: &'a Config,
    actor: Option<&'a str>,
}

#[derive(Subcommand)]
pub enum Command {
    #[command(about = "Applies pending migrations after snapshotting the database")]
    Migrate,
    #[command(about = "Creates the first admin and prints a link to register a passkey")]
    Setup(setup::SetupArgs),
    #[command(subcommand, about = "Manages users (admin)")]
    User(user::UserCommand),
    #[command(subcommand, about = "Manages Logbooks")]
    Logbook(logbook::LogbookCommand),
    #[command(subcommand, about = "Lists the members of a Logbook, or manages them (owner)")]
    Member(logbook::MemberCommand),
    #[command(subcommand, about = "Manages the Stations of a Logbook (owner)")]
    Station(station::StationCommand),
    #[command(subcommand, about = "Manages the Instruments of a Logbook")]
    Instrument(instrument::InstrumentCommand),
    #[command(subcommand, about = "Manages your API tokens")]
    Token(token::TokenCommand),
    #[command(about = "Imports new QSOs or merges QSL confirmations from an ADIF file")]
    Import(import::ImportArgs),
    #[command(subcommand, about = "Exports QSOs as ADIF and applies the results")]
    Export(export::ExportCommand),
    #[command(subcommand, about = "Reads and writes instance settings (admin)")]
    Settings(settings::SettingsCommand),
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
        if !matches!(command, Command::Migrate | Command::Setup(_)) {
            db::require_migrated(&db).await?;
        }
        let context = Context {
            db: &db,
            database,
            config,
            actor,
        };
        match command {
            Command::Migrate => {
                let migrated = context.migrate().await?;
                match migrated.applied.is_empty() {
                    true => println!("The database is up to date."),
                    false => println!("Applied migrations {:?}", migrated.applied),
                }
                Ok(())
            }
            Command::Setup(args) => {
                context.migrate().await?;
                setup::setup(&context, &args).await
            }
            Command::User(command) => user::user(&context, command).await,
            Command::Logbook(command) => logbook::logbook(&context, command).await,
            Command::Member(command) => logbook::member(&context, command).await,
            Command::Station(command) => station::station(&context, command).await,
            Command::Instrument(command) => instrument::run(&context, command).await,
            Command::Token(command) => token::token(&context, command).await,
            Command::Import(args) => import::run(&context, &args).await,
            Command::Export(command) => export::run(&context, command).await,
            Command::Settings(command) => settings::settings(&context, command).await,
            Command::Reference(command) => reference::run(&context, command).await,
            Command::Backup(args) => backup::backup(&context, &args).await,
            Command::Restore(_) => unreachable!("restore runs without an open database"),
        }
    }
    .await;
    db.close().await;
    result
}

#[derive(Args)]
pub struct LogbookArg {
    #[arg(long, help = "Logbook ID or name; may be left out if you have only one")]
    logbook: Option<String>,
}

#[derive(Args)]
pub struct DayRange {
    #[arg(long, value_parser = parse_day, help = "First QSO day (UTC), such as 2026-10-01")]
    from: Option<NaiveDate>,
    #[arg(long, value_parser = parse_day, help = "Last QSO day (UTC), inclusive")]
    until: Option<NaiveDate>,
}

impl DayRange {
    fn range(&self) -> TimeRange {
        TimeRange::days(self.from, self.until)
    }
}

impl Context<'_> {
    async fn migrate(&self) -> CliResult<Migrated> {
        let migrated = db::migrate(self.db, &db::snapshot_dir(self.database)).await?;
        if let Some(snapshot) = &migrated.snapshot {
            println!("Snapshot written to {}", snapshot.display());
        }
        Ok(migrated)
    }

    fn public_origin(&self) -> CliResult<&Url> {
        Ok(self
            .config
            .server
            .public_origin
            .as_ref()
            .ok_or("set server.public_origin in the configuration; the registration link points there")?)
    }

    async fn principal(&self) -> CliResult<Principal> {
        let callsign = self.actor.ok_or("name the user to act as with --as")?;
        let mut conn = self.db.acquire().await?;
        Ok(Principal::user(&find_user(&mut conn, callsign).await?.id))
    }

    async fn logbooks(&self) -> CliResult<(Principal, Vec<Logbook>)> {
        let principal = self.principal().await?;
        let mut conn = self.db.acquire().await?;
        let user = access::user(&mut conn, &principal).await?;
        let logbooks = logbooks::list(&mut conn, &user)
            .await?
            .into_iter()
            .map(|(logbook, _)| logbook)
            .collect();
        Ok((principal, logbooks))
    }

    async fn logbook_access(&self, arg: &LogbookArg) -> CliResult<LogbookAccess> {
        let (principal, logbooks) = self.logbooks().await?;
        let found = pick_logbook(&logbooks, arg.logbook.as_deref())?;
        let mut conn = self.db.acquire().await?;
        Ok(access::logbook(&mut conn, &principal, &found.id).await?)
    }

    async fn station(&self, access: &LogbookAccess, key: Option<&str>) -> CliResult<Station> {
        let mut conn = self.db.acquire().await?;
        let stations = stations::list(&mut conn, access).await?;
        Ok(pick(&stations, key, "Station", |s| &s.id, |s| &s.name)?.clone())
    }

    async fn instrument(&self, access: &LogbookAccess, key: &str) -> CliResult<Instrument> {
        let mut conn = self.db.acquire().await?;
        let instruments = crate::store::logbook::instrument::list(&mut conn, access).await?;
        Ok(pick(&instruments, Some(key), "Instrument", |i| &i.id, |i| &i.name)?.clone())
    }

    fn length(&self, length: Option<LengthArg>) -> LengthUnit {
        length.map(LengthUnit::from).unwrap_or(self.config.adif.length)
    }
}

async fn find_user(conn: &mut SqliteConnection, callsign: &str) -> CliResult<User> {
    Ok(users::find_by_callsign(conn, callsign)
        .await?
        .ok_or_else(|| format!("no user has the callsign {callsign}"))?)
}

async fn admin_tx(context: &Context<'_>, principal: &Principal) -> CliResult<(WriteTx, AdminAccess)> {
    let mut tx = WriteTx::begin(context.db).await?;
    let admin = access::admin(&mut tx, principal).await?;
    Ok((tx, admin))
}

fn pick_logbook<'a>(logbooks: &'a [Logbook], key: Option<&str>) -> CliResult<&'a Logbook> {
    pick(logbooks, key, "Logbook", |l| &l.id, |l| &l.name)
}

fn pick<'a, T>(
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
enum LengthArg {
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

fn parse_day(s: &str) -> Result<NaiveDate, String> {
    parse_date(&s.trim().replace('-', "")).ok_or_else(|| format!("{s:?} is not a date such as 2026-10-22"))
}

fn parse_field(s: &str) -> Result<(String, String), String> {
    s.split_once('=')
        .map(|(name, value)| (name.trim().to_owned(), value.to_owned()))
        .filter(|(name, _)| !name.is_empty())
        .ok_or_else(|| format!("{s:?} is not NAME=VALUE"))
}

fn registration_url(origin: &Url, token: &str) -> Url {
    let mut url = origin.join("/register").expect("an origin takes a path");
    url.set_fragment(Some(token));
    url
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::store::testing::World;

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

    #[rstest]
    #[case("2026-10-01", Some((2026, 10, 1)))]
    #[case("20261031", Some((2026, 10, 31)))]
    #[case("2026-13-01", None)]
    fn parses_days_with_or_without_hyphens(#[case] text: &str, #[case] expected: Option<(i32, u32, u32)>) {
        let expected = expected.map(|(y, m, d)| NaiveDate::from_ymd_opt(y, m, d).unwrap());
        assert_eq!(parse_day(text).ok(), expected);
    }

    #[test]
    fn keeps_the_registration_token_out_of_the_request() {
        let origin = Url::parse("https://log.example.com").unwrap();
        let url = registration_url(&origin, "abc");
        assert_eq!(url.as_str(), "https://log.example.com/register#abc");
    }

    #[rstest]
    #[case::member(Some("JA1OPR"), Ok(()))]
    #[case::outsider(Some("JA1OUT"), Err("there is no Logbook".to_owned()))]
    #[case::unknown_user(Some("JA1NOB"), Err("no user has the callsign JA1NOB".to_owned()))]
    #[case::no_actor(None, Err("name the user to act as with --as".to_owned()))]
    #[tokio::test]
    async fn acts_as_the_named_user(#[case] actor: Option<&str>, #[case] expected: Result<(), String>) {
        let world = World::new().await;
        let config = Config::default();
        let context = Context {
            db: &world.db,
            database: Path::new("unused.db"),
            config: &config,
            actor,
        };
        let access = context.logbook_access(&LogbookArg { logbook: None }).await;
        assert_eq!(
            access.as_ref().map(|a| a.logbook_id()).map_err(|e| e.to_string()),
            expected.map(|()| world.logbook.id.as_str())
        );
    }

    #[rstest]
    #[case("MY_CITY=Minato", Ok(("MY_CITY", "Minato")))]
    #[case(" my_rig =IC-705=x", Ok(("my_rig", "IC-705=x")))]
    #[case("MY_RIG=", Ok(("MY_RIG", "")))]
    #[case("=Minato", Err(()))]
    #[case("MY_CITY", Err(()))]
    fn reads_name_value_fields(#[case] text: &str, #[case] expected: Result<(&str, &str), ()>) {
        let read = parse_field(text);
        assert_eq!(
            read.as_ref().map(|(n, v)| (n.as_str(), v.as_str())).map_err(|_| ()),
            expected
        );
    }
}
