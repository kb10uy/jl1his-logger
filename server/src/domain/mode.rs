use crate::domain::qso::Warning;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mode {
    pub name: &'static str,
    pub submodes: &'static [&'static str],
}

const fn mode(name: &'static str, submodes: &'static [&'static str]) -> Mode {
    Mode { name, submodes }
}

pub const MODES: &[Mode] = &[
    mode("AM", &[]),
    mode("ARDOP", &[]),
    mode("ATV", &[]),
    mode("CHIP", &["CHIP64", "CHIP128"]),
    mode("CLO", &[]),
    mode("CONTESTI", &[]),
    mode("CW", &["PCW"]),
    mode("DIGITALVOICE", &["C4FM", "DMR", "DSTAR", "FREEDV", "M17"]),
    mode(
        "DOMINO",
        &[
            "DOM-M", "DOM4", "DOM5", "DOM8", "DOM11", "DOM16", "DOM22", "DOM44", "DOM88", "DOMINOEX", "DOMINOF",
        ],
    ),
    mode(
        "DYNAMIC",
        &["FREEDATA", "VARA HF", "VARA SATELLITE", "VARA FM 1200", "VARA FM 9600"],
    ),
    mode("FAX", &[]),
    mode("FM", &[]),
    mode("FSK441", &[]),
    mode("FSK", &["SCAMP_FAST", "SCAMP_SLOW", "SCAMP_VSLOW"]),
    mode("FT8", &[]),
    mode(
        "HELL",
        &[
            "FMHELL", "FSKH105", "FSKH245", "FSKHELL", "HELL80", "HELLX5", "HELLX9", "HFSK", "PSKHELL", "SLOWHELL",
        ],
    ),
    mode("ISCAT", &["ISCAT-A", "ISCAT-B"]),
    mode("JT4", &["JT4A", "JT4B", "JT4C", "JT4D", "JT4E", "JT4F", "JT4G"]),
    mode("JT6M", &[]),
    mode(
        "JT9",
        &[
            "JT9-1",
            "JT9-2",
            "JT9-5",
            "JT9-10",
            "JT9-30",
            "JT9A",
            "JT9B",
            "JT9C",
            "JT9D",
            "JT9E",
            "JT9E FAST",
            "JT9F",
            "JT9F FAST",
            "JT9G",
            "JT9G FAST",
            "JT9H",
            "JT9H FAST",
        ],
    ),
    mode("JT44", &[]),
    mode("JT65", &["JT65A", "JT65B", "JT65B2", "JT65C", "JT65C2"]),
    mode(
        "MFSK",
        &[
            "FSQCALL", "FST4", "FST4W", "FT2", "FT4", "JS8", "JTMS", "JTTY", "MFSK4", "MFSK8", "MFSK11", "MFSK16",
            "MFSK22", "MFSK31", "MFSK32", "MFSK64", "MFSK64L", "MFSK128", "MFSK128L", "Q65",
        ],
    ),
    mode("MSK144", &[]),
    mode("MTONE", &["SCAMP_OO", "SCAMP_OO_SLW"]),
    mode("MT63", &[]),
    mode("OFDM", &["RIBBIT_PIX", "RIBBIT_SMS"]),
    mode(
        "OLIVIA",
        &[
            "OLIVIA 4/125",
            "OLIVIA 4/250",
            "OLIVIA 8/250",
            "OLIVIA 8/500",
            "OLIVIA 16/500",
            "OLIVIA 16/1000",
            "OLIVIA 32/1000",
        ],
    ),
    mode("OPERA", &["OPERA-BEACON", "OPERA-QSO"]),
    mode("PAC", &["PAC2", "PAC3", "PAC4"]),
    mode("PAX", &["PAX2"]),
    mode("PKT", &[]),
    mode(
        "PSK",
        &[
            "8PSK125",
            "8PSK125F",
            "8PSK125FL",
            "8PSK250",
            "8PSK250F",
            "8PSK250FL",
            "8PSK500",
            "8PSK500F",
            "8PSK1000",
            "8PSK1000F",
            "8PSK1200F",
            "FSK31",
            "PSK10",
            "PSK31",
            "PSK63",
            "PSK63F",
            "PSK63RC4",
            "PSK63RC5",
            "PSK63RC10",
            "PSK63RC20",
            "PSK63RC32",
            "PSK125",
            "PSK125C12",
            "PSK125R",
            "PSK125RC10",
            "PSK125RC12",
            "PSK125RC16",
            "PSK125RC4",
            "PSK125RC5",
            "PSK250",
            "PSK250C6",
            "PSK250R",
            "PSK250RC2",
            "PSK250RC3",
            "PSK250RC5",
            "PSK250RC6",
            "PSK250RC7",
            "PSK500",
            "PSK500C2",
            "PSK500C4",
            "PSK500R",
            "PSK500RC2",
            "PSK500RC3",
            "PSK500RC4",
            "PSK800C2",
            "PSK800RC2",
            "PSK1000",
            "PSK1000C2",
            "PSK1000R",
            "PSK1000RC2",
            "PSKAM10",
            "PSKAM31",
            "PSKAM50",
            "PSKFEC31",
            "QPSK31",
            "QPSK63",
            "QPSK125",
            "QPSK250",
            "QPSK500",
            "SIM31",
        ],
    ),
    mode("PSK2K", &[]),
    mode("Q15", &[]),
    mode("QRA64", &["QRA64A", "QRA64B", "QRA64C", "QRA64D", "QRA64E"]),
    mode("ROS", &["ROS-EME", "ROS-HF", "ROS-MF"]),
    mode("RTTY", &["ASCI"]),
    mode("RTTYM", &[]),
    mode("SSB", &["LSB", "USB"]),
    mode("SSTV", &[]),
    mode("T10", &[]),
    mode(
        "THOR",
        &[
            "THOR-M", "THOR4", "THOR5", "THOR8", "THOR11", "THOR16", "THOR22", "THOR25X4", "THOR50X1", "THOR50X2",
            "THOR100",
        ],
    ),
    mode(
        "THRB",
        &["THRBX", "THRBX1", "THRBX2", "THRBX4", "THROB1", "THROB2", "THROB4"],
    ),
    mode("TOR", &["AMTORFEC", "GTOR", "NAVTEX", "SITORB"]),
    mode("V4", &[]),
    mode("VOI", &[]),
    mode("WINMOR", &[]),
    mode("WSPR", &[]),
];

pub const IMPORT_ONLY_MODES: &[&str] = &[
    "AMTORFEC", "ASCI", "C4FM", "CHIP64", "CHIP128", "DOMINOF", "DSTAR", "FMHELL", "FSK31", "GTOR", "HELL80", "HFSK",
    "JT4A", "JT4B", "JT4C", "JT4D", "JT4E", "JT4F", "JT4G", "JT65A", "JT65B", "JT65C", "MFSK8", "MFSK16", "PAC2",
    "PAC3", "PAX2", "PCW", "PSK10", "PSK31", "PSK63", "PSK63F", "PSK125", "PSKAM10", "PSKAM31", "PSKAM50", "PSKFEC31",
    "PSKHELL", "QPSK31", "QPSK63", "QPSK125", "THRBX",
];

impl Mode {
    pub fn named(name: &str) -> Option<&'static Mode> {
        MODES.iter().find(|m| m.name.eq_ignore_ascii_case(name.trim()))
    }

    pub fn of_submode(submode: &str) -> Option<&'static Mode> {
        MODES
            .iter()
            .find(|m| m.submodes.iter().any(|s| s.eq_ignore_ascii_case(submode.trim())))
    }

    pub fn of_import_only(name: &str) -> Option<&'static Mode> {
        IMPORT_ONLY_MODES
            .iter()
            .any(|m| m.eq_ignore_ascii_case(name.trim()))
            .then(|| Mode::of_submode(name))
            .flatten()
    }
}

pub fn warning(mode: &str, submode: Option<&str>) -> Option<Warning> {
    let Some(known) = Mode::named(mode).or_else(|| Mode::of_import_only(mode)) else {
        return Some(Warning::UnknownMode { mode: mode.to_owned() });
    };
    let submode = submode?;
    match Mode::of_submode(submode) {
        None => Some(Warning::UnknownSubmode {
            submode: submode.to_owned(),
        }),
        Some(owner) if owner != known => Some(Warning::SubmodeOfAnotherMode {
            mode: mode.to_owned(),
            submode: submode.to_owned(),
            expected: owner.name.to_owned(),
        }),
        Some(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use rstest::rstest;
    use serde::Deserialize;

    use super::*;

    #[derive(Deserialize)]
    struct Fixture {
        modes: Vec<FixtureMode>,
        import_only: Vec<String>,
    }

    #[derive(Deserialize)]
    struct FixtureMode {
        mode: String,
        submodes: Vec<String>,
    }

    #[test]
    fn matches_the_shared_mode_table() {
        let fixture: Fixture = serde_json::from_str(include_str!("../../../fixtures/reference/modes.json")).unwrap();
        let modes: Vec<(&str, Vec<&str>)> = MODES.iter().map(|m| (m.name, m.submodes.to_vec())).collect();
        let expected: Vec<(&str, Vec<&str>)> = fixture
            .modes
            .iter()
            .map(|m| (m.mode.as_str(), m.submodes.iter().map(String::as_str).collect()))
            .collect();
        assert_eq!(modes, expected);
        assert_eq!(IMPORT_ONLY_MODES, fixture.import_only);
    }

    #[test]
    fn gives_every_submode_and_import_only_mode_one_owner() {
        let mut seen = BTreeSet::new();
        for submode in MODES.iter().flat_map(|m| m.submodes) {
            assert!(
                seen.insert(submode.to_ascii_uppercase()),
                "{submode} belongs to two modes"
            );
        }
        for name in IMPORT_ONLY_MODES {
            assert!(Mode::named(name).is_none(), "{name} is also a current mode");
            assert!(Mode::of_import_only(name).is_some(), "{name} has no current mode");
        }
    }

    #[rstest]
    #[case("CW", None, None)]
    #[case(" ssb ", Some("usb"), None)]
    #[case("MFSK", Some("FT4"), None)]
    #[case("DYNAMIC", Some("VARA HF"), None)]
    #[case("C4FM", None, None)]
    #[case("PSK31", Some("PSK31"), None)]
    #[case("FT9", None, Some(Warning::UnknownMode { mode: "FT9".into() }))]
    #[case("FT9", Some("NOPE"), Some(Warning::UnknownMode { mode: "FT9".into() }))]
    #[case("SSB", Some("XSB"), Some(Warning::UnknownSubmode { submode: "XSB".into() }))]
    #[case(
        "FT8",
        Some("FT4"),
        Some(Warning::SubmodeOfAnotherMode { mode: "FT8".into(), submode: "FT4".into(), expected: "MFSK".into() })
    )]
    #[case(
        "DSTAR",
        Some("USB"),
        Some(Warning::SubmodeOfAnotherMode { mode: "DSTAR".into(), submode: "USB".into(), expected: "SSB".into() })
    )]
    fn warns_about_modes_outside_the_table(
        #[case] mode: &str,
        #[case] submode: Option<&str>,
        #[case] expected: Option<Warning>,
    ) {
        assert_eq!(warning(mode, submode), expected);
    }
}
