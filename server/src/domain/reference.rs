use std::collections::BTreeMap;

use callfind::cty::{CtyDatabase, CtyError, Resolution};
use serde::Deserialize;

#[derive(Debug, thiserror::Error)]
pub enum ReferenceError {
    #[error(transparent)]
    Cty(#[from] CtyError),
    #[error(transparent)]
    Toml(#[from] toml::de::Error),
    #[error("{0}")]
    Invalid(String),
}

#[derive(Debug, Clone)]
pub struct Cty {
    pub database: CtyDatabase,
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Park {
    pub name_en: Option<String>,
    pub name_ja: Option<String>,
}

pub type DxccCodes = BTreeMap<String, u32>;
pub type Subdivisions = BTreeMap<u32, BTreeMap<String, String>>;
pub type Parks = BTreeMap<String, Park>;

pub fn parse_cty(text: &str) -> Result<Cty, ReferenceError> {
    let database = CtyDatabase::parse(text)?;
    if database.is_empty() {
        return Err(ReferenceError::Invalid("cty.dat declares no entity".into()));
    }
    let version = text.split("=VER").skip(1).find_map(|rest| {
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        (!digits.is_empty()).then_some(digits)
    });
    Ok(Cty { database, version })
}

pub fn parse_dxcc(text: &str) -> Result<DxccCodes, ReferenceError> {
    let codes: DxccCodes = toml::from_str(text)?;
    if codes.is_empty() {
        return Err(ReferenceError::Invalid("the DXCC table is empty".into()));
    }
    Ok(codes)
}

pub fn parse_subdivisions(text: &str) -> Result<Subdivisions, ReferenceError> {
    let tables: BTreeMap<String, BTreeMap<String, String>> = toml::from_str(text)?;
    let mut subdivisions = Subdivisions::new();
    for (dxcc, names) in tables {
        let code = dxcc
            .trim()
            .parse()
            .map_err(|_| ReferenceError::Invalid(format!("{dxcc:?} is not a DXCC entity code")))?;
        let names = names
            .into_iter()
            .map(|(state, name)| (state.trim().to_ascii_uppercase(), name))
            .collect();
        subdivisions.insert(code, names);
    }
    if subdivisions.is_empty() {
        return Err(ReferenceError::Invalid("the subdivision table is empty".into()));
    }
    Ok(subdivisions)
}

pub fn parse_parks(text: &str) -> Result<Parks, ReferenceError> {
    let parks: Parks = toml::from_str(text)?;
    if parks.is_empty() {
        return Err(ReferenceError::Invalid("the park table is empty".into()));
    }
    Ok(parks)
}

pub fn dxcc_code(codes: &DxccCodes, resolution: &Resolution<'_>) -> Option<u32> {
    codes.get(&resolution.entity.primary_prefix).copied()
}

#[cfg(test)]
pub(crate) mod tests {
    use rstest::rstest;

    use super::*;

    pub(crate) const CTY: &str = "\
Japan:                    25:  45:  AS:   36.40:  -138.38:    -9.0:  JA:
    7J,7K,7L,7M,7N,8J,8K,8L,8M,8N,JA,JE,JF,JG,JH,JI,JJ,JK,JL,JM,JN,JO,JP,JQ,JR,JS;
Ogasawara:                27:  45:  AS:   27.05:  -142.20:    -9.0:  JD/o:
    JD1,=JD1YAA,=VER20260915;
";

    #[test]
    fn reads_cty_and_its_version() {
        let cty = parse_cty(CTY).unwrap();
        assert_eq!(cty.version.as_deref(), Some("20260915"));
        let found = cty.database.lookup("JL1HIS/P").unwrap();
        assert_eq!(found.entity.name, "Japan");
        let codes = parse_dxcc("JA = 339\n\"JD/o\" = 192\n").unwrap();
        assert_eq!(dxcc_code(&codes, &found), Some(339));
        let ogasawara = cty.database.lookup("JD1ABC").unwrap();
        assert_eq!(dxcc_code(&codes, &ogasawara), Some(192));
        assert_eq!(
            parse_cty("Japan: 25: 45: AS: 36.40: -138.38: -9.0: JA:\n JA;\n")
                .unwrap()
                .version,
            None
        );
    }

    #[test]
    fn reads_the_tables_qso_tools_reads() {
        let subdivisions = parse_subdivisions("[339]\n01 = \"Hokkaido\"\nab = \"x\"\n").unwrap();
        assert_eq!(subdivisions[&339]["01"], "Hokkaido");
        assert_eq!(subdivisions[&339]["AB"], "x");
        let parks = parse_parks("JP-0001 = { name_en = \"Rishiri\", name_ja = \"利尻\" }\nJP-0002 = {}\n").unwrap();
        assert_eq!(parks["JP-0001"].name_ja.as_deref(), Some("利尻"));
        assert_eq!(
            parks["JP-0002"],
            Park {
                name_en: None,
                name_ja: None
            }
        );
    }

    #[rstest]
    #[case::not_cty(parse_cty("Japan\n").err())]
    #[case::empty_cty(parse_cty("").err())]
    #[case::empty_dxcc(parse_dxcc("").err())]
    #[case::text_code(parse_dxcc("JA = \"339\"\n").err())]
    #[case::text_entity(parse_subdivisions("[Japan]\n01 = \"Hokkaido\"\n").err())]
    #[case::empty_parks(parse_parks("").err())]
    fn refuses_malformed_tables(#[case] error: Option<ReferenceError>) {
        assert!(error.is_some());
    }
}
