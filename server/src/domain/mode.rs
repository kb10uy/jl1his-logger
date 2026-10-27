use crate::domain::qso::Warning;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mode {
    pub name: &'static str,
    pub submodes: &'static [&'static str],
}

const fn mode(name: &'static str, submodes: &'static [&'static str]) -> Mode {
    Mode { name, submodes }
}

include!(concat!(env!("OUT_DIR"), "/adif/modes.rs"));

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

    use super::*;

    #[rstest]
    #[case("AM", &[])]
    #[case("SSB", &["LSB", "USB"])]
    #[case("DYNAMIC", &["FREEDATA", "VARA HF", "VARA SATELLITE", "VARA FM 1200", "VARA FM 9600"])]
    fn follows_the_adif_mode_enumeration(#[case] name: &str, #[case] submodes: &[&str]) {
        assert_eq!(Mode::named(name).map(|m| m.submodes), Some(submodes));
    }

    #[test]
    fn takes_the_union_of_the_mode_and_submode_tables() {
        let mfsk = Mode::named("MFSK").unwrap().submodes;
        assert!(mfsk.ends_with(&["MFSK64L", "MFSK128", "MFSK128L", "Q65"]), "{mfsk:?}");
        let psk = Mode::named("PSK").unwrap().submodes;
        assert!(psk.contains(&"PSK125C12") && psk.contains(&"PSK63RC4"), "{psk:?}");
        assert_eq!(IMPORT_ONLY_MODES.len(), 42);
        assert!(Mode::named("C4FM").is_none());
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
