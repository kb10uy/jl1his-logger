use jl1his_query::{Condition, Range, Term};
use sqlx::{Sqlite, query::Query, sqlite::SqliteArguments};

use super::{ADIF_COLUMNS, CallMatch, Filter};
use crate::{
    domain::callsign,
    store::{Fault, StoreError, access::LogbookAccess},
};

const MAX_LIST: usize = 100;

pub(super) enum Bind {
    Text(String),
    Int(i64),
    Real(f64),
}

pub(super) struct Conditions {
    clauses: Vec<String>,
    binds: Vec<Bind>,
}

impl Conditions {
    pub(super) fn of(access: &LogbookAccess, filter: &Filter) -> Result<Conditions, StoreError> {
        let mut conditions = Conditions {
            clauses: vec![],
            binds: vec![],
        };
        conditions.push("logbook_id = ?", [Bind::Text(access.logbook_id().to_owned())]);
        conditions.push("deleted_at IS NULL", []);
        let text = |value: &str| [Bind::Text(value.trim().to_owned())];
        if let Some(station_ids) = &filter.station_ids {
            conditions.any_of("station_id", station_ids.iter().cloned())?;
        }
        if let Some(band) = &filter.band {
            conditions.push("trim(band) = ? COLLATE NOCASE", text(band));
        }
        if let Some(mode) = &filter.mode {
            conditions.push("trim(mode) = ? COLLATE NOCASE", text(mode));
        }
        match &filter.call {
            Some(CallMatch::Exact(call)) => conditions.push("trim(call) = ? COLLATE NOCASE", text(call)),
            Some(CallMatch::Base(call)) => conditions.push("call_base = ?", [Bind::Text(callsign::base(call))]),
            None => {}
        }
        if let Some(contest_id) = &filter.contest_id {
            conditions.push("trim(contest_id) = ? COLLATE NOCASE", text(contest_id));
        }
        if let Some(values) = &filter.qsl_sent {
            conditions.any_of(
                "upper(trim(coalesce(qsl_sent, '')))",
                values.iter().map(|v| v.trim().to_ascii_uppercase()),
            )?;
        }
        if let Some(from) = filter.time.from {
            conditions.push("qso_time >= ?", [Bind::Int(from)]);
        }
        if let Some(until) = filter.time.until {
            conditions.push("qso_time < ?", [Bind::Int(until)]);
        }
        for term in filter.query.terms() {
            conditions.term(access, term);
        }
        Ok(conditions)
    }

    fn term(&mut self, access: &LogbookAccess, term: &Term) {
        let mut binds = vec![];
        let any = |clauses: Vec<String>| format!("({})", clauses.join(" OR "));
        let clause = match &term.condition {
            Condition::Call(patterns) => any(
                patterns
                    .iter()
                    .map(|pattern| {
                        binds.push(Bind::Text(pattern.replace('[', "[[]")));
                        "upper(trim(coalesce(call, ''))) GLOB ?".to_owned()
                    })
                    .collect(),
            ),
            Condition::Base(bases) => {
                binds.extend(bases.iter().cloned().map(Bind::Text));
                format!("call_base IN ({})", vec!["?"; bases.len()].join(", "))
            }
            Condition::Field { name, values } => {
                let field = field_expression(name, &mut binds);
                binds.extend(values.iter().cloned().map(Bind::Text));
                format!(
                    "upper(trim(coalesce({field}, ''))) IN ({})",
                    vec!["?"; values.len()].join(", ")
                )
            }
            Condition::Station(needles) => any(
                needles
                    .iter()
                    .map(|needle| {
                        binds.extend([
                            Bind::Text(access.logbook_id().to_owned()),
                            Bind::Text(needle.clone()),
                            Bind::Text(needle.clone()),
                        ]);
                        "(station_id IN (SELECT id FROM stations WHERE logbook_id = ? AND instr(lower(name), ?) > 0)                          OR instr(lower(coalesce(station_callsign, '')), ?) > 0)"
                            .to_owned()
                    })
                    .collect(),
            ),
            Condition::Instrument(needles) => any(
                needles
                    .iter()
                    .map(|needle| {
                        binds.extend([
                            Bind::Text(access.logbook_id().to_owned()),
                            Bind::Text(needle.clone()),
                            Bind::Text("$.MY_RIG".to_owned()),
                            Bind::Text(needle.clone()),
                        ]);
                        "(coalesce(instrument_id, '') IN (SELECT id FROM instruments WHERE logbook_id = ? AND instr(lower(name), ?) > 0) \
                         OR instr(lower(coalesce(json_extract(fields, ?), '')), ?) > 0)"
                            .to_owned()
                    })
                    .collect(),
            ),
            Condition::Park(parks) => any(
                parks
                    .iter()
                    .map(|park| {
                        binds.push(Bind::Text(format!(",{park},")));
                        "instr(',' || upper(replace(coalesce(my_pota_ref, ''), ' ', '')) || ',', ?) > 0".to_owned()
                    })
                    .collect(),
            ),
            Condition::Date { from, until } => {
                let mut clauses = vec![];
                if let Some(from) = from {
                    clauses.push("qso_time >= ?");
                    binds.push(Bind::Int(from.unix_seconds()));
                }
                if let Some(until) = until {
                    clauses.push("qso_time < ?");
                    binds.push(Bind::Int(until.unix_seconds()));
                }
                format!("({})", clauses.join(" AND "))
            }
            Condition::Freq(range) => {
                let mut clauses = vec!["freq_hz IS NOT NULL".to_owned()];
                for (op, value) in comparisons(range) {
                    clauses.push(format!("freq_hz {op} ?"));
                    binds.push(Bind::Int(value.try_into().unwrap_or(i64::MAX)));
                }
                format!("({})", clauses.join(" AND "))
            }
            Condition::Power(range) => {
                let power = "trim(coalesce(tx_pwr, ''))";
                let mut clauses = vec![format!(
                    "{power} <> '' AND {power} <> '.' AND {power} NOT GLOB '*[^0-9.]*' AND {power} NOT GLOB '*.*.*'"
                )];
                for (op, value) in comparisons(range) {
                    clauses.push(format!("CAST({power} AS REAL) {op} ?"));
                    binds.push(Bind::Real(value as f64 / 1_000_000.0));
                }
                format!("({})", clauses.join(" AND "))
            }
            Condition::Present(name) => {
                let field = field_expression(name, &mut binds);
                format!("trim(coalesce({field}, '')) <> ''")
            }
            Condition::Word(word) => {
                binds.extend((0..4).map(|_| Bind::Text(word.clone())));
                "(instr(lower(coalesce(call, '')), ?) > 0 OR instr(lower(coalesce(name, '')), ?) > 0                  OR instr(lower(coalesce(qth, '')), ?) > 0 OR instr(lower(coalesce(comment, '')), ?) > 0)"
                    .to_owned()
            }
        };
        match term.negated {
            true => self.push(format!("NOT ({clause})"), binds),
            false => self.push(clause, binds),
        }
    }

    pub(super) fn push(&mut self, clause: impl Into<String>, binds: impl IntoIterator<Item = Bind>) {
        self.clauses.push(clause.into());
        self.binds.extend(binds);
    }

    fn any_of(&mut self, expression: &str, values: impl IntoIterator<Item = String>) -> Result<(), StoreError> {
        let values: Vec<Bind> = values.into_iter().map(Bind::Text).collect();
        let clause = match values.len() {
            0 => "0".to_owned(),
            n if n > MAX_LIST => {
                return Err(StoreError::Invalid(
                    Fault::coded(
                        "too_many_values",
                        format!("a filter lists {n} values; at most {MAX_LIST} are taken"),
                    )
                    .with("count", n)
                    .with("max", MAX_LIST),
                ));
            }
            n => format!("{expression} IN ({})", vec!["?"; n].join(", ")),
        };
        self.push(clause, values);
        Ok(())
    }

    pub(super) fn sql(&self) -> String {
        self.clauses.join(" AND ")
    }

    pub(super) fn bind<'q>(&self, mut query: Query<'q, Sqlite, SqliteArguments>) -> Query<'q, Sqlite, SqliteArguments> {
        for bind in &self.binds {
            query = match bind {
                Bind::Text(value) => query.bind(value.clone()),
                Bind::Int(value) => query.bind(*value),
                Bind::Real(value) => query.bind(*value),
            };
        }
        query
    }
}

fn comparisons(range: &Range) -> impl Iterator<Item = (&'static str, u64)> {
    let low = range.low.map(|b| (if b.inclusive { ">=" } else { ">" }, b.value));
    let high = range.high.map(|b| (if b.inclusive { "<=" } else { "<" }, b.value));
    low.into_iter().chain(high)
}

fn field_expression(name: &str, binds: &mut Vec<Bind>) -> String {
    let column = name.to_ascii_lowercase();
    match ADIF_COLUMNS.contains(&column) {
        true => column,
        false => {
            binds.push(Bind::Text(format!("$.\"{name}\"")));
            "json_extract(fields, ?)".to_owned()
        }
    }
}

#[cfg(test)]
mod tests {
    use jl1his_query::{self as text};
    use rstest::rstest;

    use super::super::{
        CallMatch, Filter, StoredQso, TimeRange, count, delete, get, search, search_all, targets,
        tests::{create_one, entry, found, four_qsos},
    };
    use crate::{
        domain::qso::QsoId,
        store::{
            WriteTx,
            logbook::{
                instrument::{self, InstrumentInput},
                station::Station,
            },
            testing::World,
        },
    };

    #[rstest]
    #[case::base_call_and_mode(
        |_: &Station| Filter { call: Some(CallMatch::Base("JA1AAA/1".into())), mode: Some(" Cw".into()), ..Filter::default() },
        &[1, 0]
    )]
    #[case::exact_call(|_: &Station| Filter { call: Some(CallMatch::Exact("ja1aaa".into())), ..Filter::default() }, &[0])]
    #[case::qsl_sent(|_: &Station| Filter { qsl_sent: Some(vec!["r".into(), "".into()]), ..Filter::default() }, &[1, 0])]
    #[case::no_qsl_sent(|_: &Station| Filter { qsl_sent: Some(vec![]), ..Filter::default() }, &[])]
    #[case::time_station_and_band(
        |station: &Station| Filter {
            time: TimeRange { from: Some(1792634400), until: Some(1792638000) },
            station_ids: Some(vec![station.id.clone()]),
            band: Some("40M".into()),
            ..Filter::default()
        },
        &[1]
    )]
    #[case::other_station(|_: &Station| Filter { station_ids: Some(vec!["other".into()]), ..Filter::default() }, &[])]
    #[tokio::test]
    async fn filters_searches(#[case] filter: fn(&Station) -> Filter, #[case] expected: &[usize]) {
        let world = World::new().await;
        let (access, station, ids) = four_qsos(&world).await;
        let mut conn = world.db.acquire().await.unwrap();
        let page = search(&mut conn, &access, &filter(&station), None, 10).await.unwrap();
        let expected: Vec<QsoId> = expected.iter().map(|i| ids[*i]).collect();
        assert_eq!(found(&page), expected);
    }

    struct Row<'a>(&'a StoredQso, &'a str, Option<&'a str>);

    impl text::Record for Row<'_> {
        fn field(&self, name: &str) -> Option<&str> {
            self.0.qso.get(name)
        }

        fn station_name(&self) -> Option<&str> {
            Some(self.1)
        }

        fn instrument_name(&self) -> Option<&str> {
            self.2
        }
    }

    #[tokio::test]
    async fn searches_text_queries_as_the_matcher_does() {
        let world = World::new().await;
        let home = world.station("Home").await;
        let park = world.station("Mt. Takao").await;
        let access = world.access(&world.operator).await;
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        let portable = InstrumentInput {
            name: "Portable QRP".into(),
            my_rig: Some("KX2".into()),
            ..InstrumentInput::default()
        };
        let rig = instrument::create(&mut tx, &access, &portable).await.unwrap();
        tx.commit().await.unwrap();
        let rows: &[(&Station, &[(&str, &str)])] = &[
            (
                &home,
                &[
                    ("MY_RIG", "IC-705"),
                    ("CALL", "JA1AAA"),
                    ("QSO_DATE", "20261017"),
                    ("QSL_SENT", "r"),
                    ("NAME", "Taro"),
                    ("LOTW_QSL_SENT", "Y"),
                    ("TX_PWR", "0.5"),
                ],
            ),
            (
                &home,
                &[
                    ("CALL", "ja1aaa/p"),
                    ("QSO_DATE", "20261018"),
                    ("BAND", "20m"),
                    ("FREQ", "14.020"),
                    ("EQSL_QSL_RCVD", "v"),
                    ("TX_PWR", "100"),
                ],
            ),
            (
                &park,
                &[
                    ("CALL", "JH1XYZ"),
                    ("QSO_DATE", "20261019"),
                    ("MY_POTA_REF", "JP-0012, JP-0345"),
                    ("LOTW_QSL_RCVD", "Y"),
                    ("TX_PWR", "5.0"),
                ],
            ),
            (
                &park,
                &[
                    ("CALL", "7K1XYZ"),
                    ("QSO_DATE", "20261019"),
                    ("MODE", "SSB"),
                    ("QTH", "Yokohama"),
                    ("EQSL_QSL_SENT", "Y"),
                    ("TX_PWR", "QRP"),
                ],
            ),
            (
                &home,
                &[
                    ("CALL", "W6ABC"),
                    ("QSO_DATE", "20261020"),
                    ("TX_PWR", "10"),
                    ("COMMENT", "big [pile]"),
                ],
            ),
            (
                &home,
                &[
                    ("CALL", "KH6/W1AW"),
                    ("QSO_DATE", "20261021"),
                    ("STATION_CALLSIGN", "JL1HIS"),
                    ("QSL_RCVD", "V"),
                    ("FREQ", "7.0745"),
                    ("TX_PWR", "5"),
                ],
            ),
            (
                &home,
                &[
                    ("CALL", "JA1ZZZ"),
                    ("QSO_DATE", "20261021"),
                    ("CONTEST_ID", "ALL-JA1"),
                    ("DXCC", "339"),
                    ("FREQ", "7.041"),
                    ("TX_PWR", "1.2.3"),
                ],
            ),
        ];
        let mut stored_rows = vec![];
        for (station, fields) in rows {
            let mut input = entry(station, "JA1AAA", "0100");
            if station.id == park.id {
                input.instrument_id = Some(rig.id.clone());
            }
            input.qso.set("FREQ", None).unwrap();
            for (name, value) in *fields {
                input.qso.set(name, Some((*value).to_owned())).unwrap();
            }
            let (id, _) = create_one(&world, &access, input).await;
            let mut conn = world.db.acquire().await.unwrap();
            stored_rows.push(get(&mut conn, &access, &id).await.unwrap());
        }
        let mut tx = WriteTx::begin(&world.db).await.unwrap();
        delete(&mut tx, &access, &stored_rows.pop().unwrap().id, 1)
            .await
            .unwrap();
        tx.commit().await.unwrap();

        let name_of = |row: &StoredQso| {
            if row.station_id == park.id {
                park.name.as_str()
            } else {
                home.name.as_str()
            }
        };
        let instrument_of = |row: &StoredQso| match row.instrument_id == Some(rig.id.clone()) {
            true => Some(rig.name.as_str()),
            false => None,
        };
        let mut conn = world.db.acquire().await.unwrap();
        for q in [
            "call:JA1*",
            "call:ja1aaa",
            "call:*XYZ",
            "call:J?1*",
            "call:[*",
            "-call:JA1*",
            "base:JA1AAA",
            "base:w1aw",
            "band:20m,15m",
            "-band:40m",
            "mode:ssb",
            "sent:R,Q",
            "-sent:R",
            "rcvd:Y,V",
            "contest:all-ja1",
            "dxcc:339",
            "station:takao",
            "station:home,jl1his/1",
            "station:\"jl1his\"",
            "-station:takao",
            "instrument:qrp",
            "instrument:kx2",
            "instrument:ic-705,portable",
            "-instrument:qrp",
            "park:jp-0345",
            "park:JP-0012,JP-9999",
            "-park:JP-0012",
            "date:2026-10-19",
            "date:2026-10-18..2026-10-20",
            "date:..2026-10-18",
            "date:>2026-10-19",
            "date:<=2026-10-19",
            "lotw-sent:Y",
            "-lotw-sent:Y",
            "lotw-rcvd:y",
            "eqsl-sent:Y",
            "eqsl-rcvd:Y,V",
            "no:eqsl-rcvd",
            "has:lotw-sent",
            "freq:7..7.3",
            "freq:14.02",
            "freq:>7.041",
            "freq:<=7.041",
            "freq:7.0745",
            "-freq:14..14.35",
            "pwr:5",
            "pwr:<=5",
            "pwr:>5",
            "pwr:<5",
            "pwr:0.5..10",
            "-pwr:>=10",
            "has:pwr",
            "has:freq",
            "no:freq",
            "no:park",
            "has:name",
            "-has:qth",
            "adif.TX_PWR:10",
            "adif.tx_pwr:5",
            "-adif.TX_PWR:10",
            "adif.MY_CITY:minato",
            "has:app_hislog_note",
            "taro",
            "yoko",
            "-xyz",
            "\"[pile]\"",
            "call:JA1* band:40m -sent:R",
        ] {
            let filter = Filter {
                query: text::Query::parse(q).unwrap(),
                ..Filter::default()
            };
            let mut found: Vec<QsoId> = search_all(&mut conn, &access, &filter)
                .await
                .unwrap()
                .iter()
                .map(|r| r.id)
                .collect();
            let mut expected: Vec<QsoId> = stored_rows
                .iter()
                .filter(|row| filter.query.matches(&Row(row, name_of(row), instrument_of(row))))
                .map(|row| row.id)
                .collect();
            found.sort();
            expected.sort();
            assert_eq!(found, expected, "{q}");
            assert_eq!(
                count(&mut conn, &access, &filter).await.unwrap(),
                expected.len() as u64,
                "{q}"
            );
            let targets: Vec<QsoId> = targets(&mut conn, &access, &filter)
                .await
                .unwrap()
                .iter()
                .map(|t| t.id)
                .collect();
            assert_eq!(targets.len(), expected.len(), "{q}");
        }
    }
}
