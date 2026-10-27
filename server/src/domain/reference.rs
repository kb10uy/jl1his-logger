use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Cursor, Read},
};

use callfind::cty::{CtyDatabase, CtyError};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use zip::{CompressionMethod, ZipArchive};

use crate::domain::qso::{Qso, QsoError};

pub const MAX_ARCHIVE_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_ARCHIVE_ENTRIES: usize = 256;
pub const MAX_ARCHIVE_TEXT_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum ReferenceError {
    #[error(transparent)]
    Cty(#[from] CtyError),
    #[error(transparent)]
    Toml(#[from] toml::de::Error),
    #[error("{0}")]
    Invalid(String),
}

fn invalid(message: impl Into<String>) -> ReferenceError {
    ReferenceError::Invalid(message.into())
}

#[derive(Debug, Clone)]
pub struct Cty {
    pub database: CtyDatabase,
    pub codes: DxccCodes,
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Park {
    pub name_en: Option<String>,
    pub name_ja: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Entity {
    pub name: String,
    pub dxcc: Option<u32>,
    pub continent: &'static str,
    pub cq_zone: u8,
    pub itu_zone: u8,
    pub latitude: f64,
    pub longitude: f64,
}

impl Entity {
    pub fn fill(&self, qso: &mut Qso) -> Result<(), QsoError> {
        if let Some(dxcc) = self.dxcc {
            qso.fill("DXCC", &dxcc.to_string())?;
        }
        qso.fill("CQZ", &self.cq_zone.to_string())?;
        qso.fill("ITUZ", &self.itu_zone.to_string())?;
        qso.fill("CONT", self.continent)
    }
}

pub type DxccCodes = BTreeMap<String, u32>;
pub type Parks = BTreeMap<String, Park>;

pub fn parse_cty_archive(bytes: &[u8]) -> Result<Cty, ReferenceError> {
    if bytes.len() > MAX_ARCHIVE_BYTES {
        return Err(invalid(format!(
            "the archive is larger than {} MiB",
            MAX_ARCHIVE_BYTES / 1024 / 1024
        )));
    }
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(|_| invalid("the file is not a zip archive"))?;
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err(invalid(format!(
            "the archive has more than {MAX_ARCHIVE_ENTRIES} entries"
        )));
    }
    if archive.has_duplicate_names() {
        return Err(invalid("the archive has entries with the same name"));
    }
    archive
        .validate_header_consistency()
        .map_err(|_| invalid("the headers of the archive are inconsistent"))?;
    let dat = archive_text(&mut archive, "cty.dat")?;
    let csv = archive_text(&mut archive, "cty.csv")?;
    let (database, version) = parse_cty(&dat)?;
    let codes = parse_cty_csv(&csv)?;
    let mut declared = BTreeSet::new();
    for entity in database.entities() {
        let prefix = match entity.waedc_only {
            true => format!("*{}", entity.primary_prefix),
            false => entity.primary_prefix.clone(),
        };
        if !codes.contains_key(&prefix) {
            return Err(invalid(format!("cty.csv has no row for {prefix} of cty.dat")));
        }
        if !declared.insert(prefix) {
            return Err(invalid(format!(
                "cty.dat declares {} more than once",
                entity.primary_prefix
            )));
        }
    }
    if let Some(extra) = codes.keys().find(|prefix| !declared.contains(*prefix)) {
        return Err(invalid(format!(
            "cty.csv has a row for {extra}, which cty.dat does not declare"
        )));
    }
    let codes = codes
        .into_iter()
        .filter(|(prefix, _)| !prefix.starts_with('*'))
        .collect();
    Ok(Cty {
        database,
        codes,
        version,
    })
}

fn archive_text(archive: &mut ZipArchive<Cursor<&[u8]>>, name: &str) -> Result<String, ReferenceError> {
    let mut found = None;
    for index in 0..archive.len() {
        let entry = archive
            .by_index_raw(index)
            .map_err(|_| invalid("the archive cannot be read"))?;
        if entry.name_raw() != name.as_bytes() {
            continue;
        }
        if entry.encrypted() {
            return Err(invalid(format!("{name} is encrypted")));
        }
        if !matches!(
            entry.compression(),
            CompressionMethod::Stored | CompressionMethod::Deflated
        ) {
            return Err(invalid(format!("{name} is compressed with an unsupported method")));
        }
        found = Some(index);
    }
    let index = found.ok_or_else(|| invalid(format!("the archive has no {name} at its root")))?;
    let entry = archive
        .by_index(index)
        .map_err(|_| invalid(format!("{name} cannot be read")))?;
    let mut bytes = Vec::new();
    let read = entry.take(MAX_ARCHIVE_TEXT_BYTES + 1).read_to_end(&mut bytes);
    if bytes.len() as u64 > MAX_ARCHIVE_TEXT_BYTES {
        return Err(invalid(format!(
            "{name} is larger than {} MiB",
            MAX_ARCHIVE_TEXT_BYTES / 1024 / 1024
        )));
    }
    read.map_err(|_| invalid(format!("{name} is damaged")))?;
    String::from_utf8(bytes).map_err(|_| invalid(format!("{name} is not ASCII or UTF-8 text")))
}

fn parse_cty(text: &str) -> Result<(CtyDatabase, Option<String>), ReferenceError> {
    let database = CtyDatabase::parse(text)?;
    if database.is_empty() {
        return Err(invalid("cty.dat declares no entity"));
    }
    let version = text.split("=VER").skip(1).find_map(|rest| {
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        (!digits.is_empty()).then_some(digits)
    });
    Ok((database, version))
}

fn parse_cty_csv(text: &str) -> Result<DxccCodes, ReferenceError> {
    let mut codes = DxccCodes::new();
    for (index, line) in text.lines().enumerate().filter(|(_, line)| !line.trim().is_empty()) {
        let number = index + 1;
        let columns: Vec<&str> = line.split(',').collect();
        let [prefix, _, code, _, _, _, _, _, _, _] = columns[..] else {
            return Err(invalid(format!("line {number} of cty.csv does not have 10 columns")));
        };
        let prefix = prefix.trim();
        if prefix.is_empty() {
            return Err(invalid(format!("line {number} of cty.csv has no primary prefix")));
        }
        let code = code
            .trim()
            .parse()
            .map_err(|_| invalid(format!("line {number} of cty.csv has no DXCC entity code")))?;
        if codes.insert(prefix.to_owned(), code).is_some() {
            return Err(invalid(format!("cty.csv has more than one row for {prefix}")));
        }
    }
    if codes.is_empty() {
        return Err(invalid("cty.csv has no rows"));
    }
    Ok(codes)
}

pub fn text(bytes: &[u8]) -> Result<&str, ReferenceError> {
    std::str::from_utf8(bytes).map_err(|_| invalid("the file is not UTF-8 text"))
}

pub fn parse_parks(text: &str) -> Result<Parks, ReferenceError> {
    table(text, "park")
}

fn table<K: Ord + DeserializeOwned, V: DeserializeOwned>(
    text: &str,
    what: &str,
) -> Result<BTreeMap<K, V>, ReferenceError> {
    let table: BTreeMap<K, V> = toml::from_str(text)?;
    match table.is_empty() {
        true => Err(ReferenceError::Invalid(format!("the {what} table is empty"))),
        false => Ok(table),
    }
}

pub fn entity(cty: &Cty, call: &str) -> Option<Entity> {
    let found = cty.database.lookup(call)?;
    Some(Entity {
        name: found.entity.name.clone(),
        dxcc: cty.codes.get(&found.entity.primary_prefix).copied(),
        continent: found.location.continent.abbreviation(),
        cq_zone: found.location.cq_zone,
        itu_zone: found.location.itu_zone,
        latitude: found.location.latitude,
        longitude: found.location.longitude,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use std::io::Write;

    use rstest::rstest;
    use zip::{ZipWriter, write::SimpleFileOptions};

    use super::*;

    pub(crate) const CTY: &str = "\
Japan:                    25:  45:  AS:   36.40:  -138.38:    -9.0:  JA:
    7J,7K,7L,7M,7N,8J,8K,8L,8M,8N,JA,JE,JF,JG,JH,JI,JJ,JK,JL,JM,JN,JO,JP,JQ,JR,JS;
Ogasawara:                27:  45:  AS:   27.05:  -142.20:    -9.0:  JD/o:
    JD1,=JD1YAA,=VER20260915;
";

    pub(crate) const CTY_CSV: &str = "\
JA,Japan,339,AS,25,45,36.40,-138.38,-9.0,7J 7K 7L 7M 7N 8J 8K 8L 8M 8N JA JE JF JG JH JI JJ JK JL JM JN JO JP JQ JR JS;\r
JD/o,Ogasawara,192,AS,27,45,27.05,-142.20,-9.0,JD1 =JD1YAA;\r
";

    const WAE: &str = "\
Italy:                    15:  28:  EU:   42.82:   -12.58:    -1.0:  I:
    I,IT9;
Sicily:                   15:  28:  EU:   37.50:   -14.00:    -1.0:  *IT9:
    IT9;
";

    const WAE_CSV: &str = "\
I,Italy,248,EU,15,28,42.82,-12.58,-1.0,I IT9;
*IT9,Sicily,248,EU,15,28,37.50,-14.00,-1.0,IT9;
";

    pub(crate) fn cty_archive(dat: &str, csv: &str) -> Vec<u8> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for (name, text) in [
            ("readme.txt", "notes"),
            ("cty.dat", dat),
            ("cty.csv", csv),
            ("wl_cty.dat", dat),
        ] {
            writer.start_file(name, deflated).unwrap();
            writer.write_all(text.as_bytes()).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    struct Raw<'a> {
        name: &'a str,
        method: u16,
        flags: u16,
        data: &'a [u8],
    }

    impl<'a> Raw<'a> {
        fn stored(name: &'a str, data: &'a [u8]) -> Raw<'a> {
            Raw {
                name,
                method: 0,
                flags: 0,
                data,
            }
        }
    }

    fn crc32(data: &[u8]) -> u32 {
        let mut crc = !0u32;
        for &byte in data {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                crc = match crc & 1 {
                    1 => (crc >> 1) ^ 0xedb8_8320,
                    _ => crc >> 1,
                };
            }
        }
        !crc
    }

    fn raw_archive(entries: &[Raw<'_>]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        for entry in entries {
            let offset = out.len() as u32;
            let mut common = Vec::new();
            common.extend(entry.flags.to_le_bytes());
            common.extend(entry.method.to_le_bytes());
            common.extend([0; 4]);
            common.extend(crc32(entry.data).to_le_bytes());
            common.extend((entry.data.len() as u32).to_le_bytes());
            common.extend((entry.data.len() as u32).to_le_bytes());
            common.extend((entry.name.len() as u16).to_le_bytes());
            common.extend(0u16.to_le_bytes());
            out.extend(0x0403_4b50u32.to_le_bytes());
            out.extend(20u16.to_le_bytes());
            out.extend(&common);
            out.extend(entry.name.as_bytes());
            out.extend(entry.data);
            central.extend(0x0201_4b50u32.to_le_bytes());
            central.extend(20u16.to_le_bytes());
            central.extend(20u16.to_le_bytes());
            central.extend(&common);
            central.extend([0; 10]);
            central.extend(offset.to_le_bytes());
            central.extend(entry.name.as_bytes());
        }
        let directory = out.len() as u32;
        out.extend(&central);
        out.extend(0x0605_4b50u32.to_le_bytes());
        out.extend([0; 4]);
        out.extend((entries.len() as u16).to_le_bytes());
        out.extend((entries.len() as u16).to_le_bytes());
        out.extend((central.len() as u32).to_le_bytes());
        out.extend(directory.to_le_bytes());
        out.extend([0; 2]);
        out
    }

    fn refusal(bytes: &[u8]) -> String {
        parse_cty_archive(bytes).unwrap_err().to_string()
    }

    #[test]
    fn reads_cty_and_codes_from_an_archive() {
        let cty = parse_cty_archive(&cty_archive(CTY, CTY_CSV)).unwrap();
        assert_eq!(cty.version.as_deref(), Some("20260915"));
        assert_eq!(cty.database.lookup("JL1HIS/P").unwrap().entity.name, "Japan");
        assert_eq!(entity(&cty, "JD1ABC").unwrap().dxcc, Some(192));

        let stored = raw_archive(&[
            Raw::stored("cty.csv", CTY_CSV.as_bytes()),
            Raw::stored("cty.dat", CTY.as_bytes()),
        ]);
        assert_eq!(parse_cty_archive(&stored).unwrap().codes, cty.codes);

        let unversioned = "Japan: 25: 45: AS: 36.40: -138.38: -9.0: JA:\n JA;\n";
        let csv = "JA,Japan,339,AS,25,45,36.40,-138.38,-9.0,JA;\n";
        assert_eq!(parse_cty_archive(&cty_archive(unversioned, csv)).unwrap().version, None);
    }

    #[test]
    fn folds_wae_entities_into_their_dxcc_entity() {
        let cty = parse_cty_archive(&cty_archive(WAE, WAE_CSV)).unwrap();
        assert_eq!(cty.codes, DxccCodes::from([("I".into(), 248)]));
        let sicily = entity(&cty, "IT9ABC").unwrap();
        assert_eq!((sicily.name.as_str(), sicily.dxcc), ("Italy", Some(248)));
    }

    #[test]
    fn describes_the_entity_of_a_call() {
        let cty = parse_cty_archive(&cty_archive(CTY, CTY_CSV)).unwrap();
        assert_eq!(
            entity(&cty, " ja1xyz/p "),
            Some(Entity {
                name: "Japan".into(),
                dxcc: Some(339),
                continent: "AS",
                cq_zone: 25,
                itu_zone: 45,
                latitude: 36.4,
                longitude: 138.38,
            })
        );
        assert_eq!(entity(&cty, "XX0XX"), None);
    }

    #[test]
    fn fills_only_the_entity_fields_a_qso_lacks() {
        let cty = parse_cty_archive(&cty_archive(CTY, CTY_CSV)).unwrap();
        let mut qso = Qso::from_fields([("CALL", "JA1XYZ"), ("CQZ", "26")]).unwrap();
        entity(&cty, "JA1XYZ").unwrap().fill(&mut qso).unwrap();
        assert_eq!(
            ["DXCC", "CQZ", "ITUZ", "CONT"].map(|name| qso.get(name)),
            [Some("339"), Some("26"), Some("45"), Some("AS")]
        );

        let uncoded = Entity {
            dxcc: None,
            ..entity(&cty, "JD1ABC").unwrap()
        };
        let mut qso = Qso::from_fields([("CALL", "JD1ABC")]).unwrap();
        uncoded.fill(&mut qso).unwrap();
        assert_eq!(
            ["DXCC", "CQZ", "ITUZ", "CONT"].map(|name| qso.get(name)),
            [None, Some("27"), Some("45"), Some("AS")]
        );
    }

    #[rstest]
    #[case::missing_row("JA,Japan,339,AS,25,45,36.40,-138.38,-9.0,JA;\n", "no row for JD/o")]
    #[case::extra_row(
        &format!("{CTY_CSV}KH1,Baker Howland,20,OC,31,61,0.00,176.00,12.0,KH1;\n"),
        "row for KH1, which cty.dat does not declare"
    )]
    #[case::renamed_prefix(&CTY_CSV.replace("JD/o,", "JD1,"), "no row for JD/o")]
    #[case::repeated_row(&format!("{CTY_CSV}JA,Japan,339,AS,25,45,36.40,-138.38,-9.0,JA;\n"), "more than one row for JA")]
    #[case::short_row("JA,Japan,339\n", "line 1 of cty.csv does not have 10 columns")]
    #[case::long_row(&CTY_CSV.replace("JD1 =JD1YAA;", "JD1,=JD1YAA;"), "line 2 of cty.csv does not have 10 columns")]
    #[case::text_code(&CTY_CSV.replace(",339,", ",Japan,"), "line 1 of cty.csv has no DXCC entity code")]
    #[case::empty_prefix(&CTY_CSV.replace("JD/o,", ","), "line 2 of cty.csv has no primary prefix")]
    #[case::empty("\n", "cty.csv has no rows")]
    fn refuses_a_cty_csv_that_does_not_match_cty_dat(#[case] csv: &str, #[case] reason: &str) {
        let error = refusal(&cty_archive(CTY, csv));
        assert!(error.contains(reason), "{error}");
    }

    #[rstest]
    #[case::not_cty("Japan\n")]
    #[case::empty("")]
    fn refuses_an_archive_whose_cty_dat_is_malformed(#[case] dat: &str) {
        assert!(parse_cty_archive(&cty_archive(dat, CTY_CSV)).is_err());
    }

    #[rstest]
    #[case::not_a_zip(b"cty.dat".to_vec(), "not a zip archive")]
    #[case::empty_zip(raw_archive(&[]), "no cty.dat at its root")]
    #[case::missing_dat(raw_archive(&[Raw::stored("cty.csv", CTY_CSV.as_bytes())]), "no cty.dat at its root")]
    #[case::missing_csv(raw_archive(&[Raw::stored("cty.dat", CTY.as_bytes())]), "no cty.csv at its root")]
    #[case::nested(
        raw_archive(&[Raw::stored("cty/cty.dat", CTY.as_bytes()), Raw::stored("cty/cty.csv", CTY_CSV.as_bytes())]),
        "no cty.dat at its root"
    )]
    #[case::traversal(
        raw_archive(&[Raw::stored("../cty.dat", CTY.as_bytes()), Raw::stored("cty.csv", CTY_CSV.as_bytes())]),
        "no cty.dat at its root"
    )]
    #[case::other_case(
        raw_archive(&[Raw::stored("CTY.DAT", CTY.as_bytes()), Raw::stored("cty.csv", CTY_CSV.as_bytes())]),
        "no cty.dat at its root"
    )]
    #[case::duplicate(
        raw_archive(&[
            Raw::stored("cty.dat", CTY.as_bytes()),
            Raw::stored("cty.csv", CTY_CSV.as_bytes()),
            Raw::stored("cty.dat", b"Japan\n"),
        ]),
        "entries with the same name"
    )]
    #[case::encrypted(
        raw_archive(&[Raw { flags: 1, ..Raw::stored("cty.dat", CTY.as_bytes()) }, Raw::stored("cty.csv", CTY_CSV.as_bytes())]),
        "cty.dat is encrypted"
    )]
    #[case::bzip2(
        raw_archive(&[Raw { method: 12, ..Raw::stored("cty.dat", CTY.as_bytes()) }, Raw::stored("cty.csv", CTY_CSV.as_bytes())]),
        "cty.dat is compressed with an unsupported method"
    )]
    #[case::not_utf8(
        raw_archive(&[Raw::stored("cty.dat", CTY.as_bytes()), Raw::stored("cty.csv", b"JA,\xff\n")]),
        "cty.csv is not ASCII or UTF-8 text"
    )]
    fn refuses_unexpected_archives(#[case] bytes: Vec<u8>, #[case] reason: &str) {
        let error = refusal(&bytes);
        assert!(error.contains(reason), "{error}");
    }

    #[test]
    fn refuses_archives_with_too_many_entries() {
        let names: Vec<String> = (0..MAX_ARCHIVE_ENTRIES - 1).map(|n| format!("{n}.txt")).collect();
        let mut entries: Vec<Raw<'_>> = names.iter().map(|name| Raw::stored(name, b"")).collect();
        entries.push(Raw::stored("cty.dat", CTY.as_bytes()));
        entries.push(Raw::stored("cty.csv", CTY_CSV.as_bytes()));
        assert!(parse_cty_archive(&raw_archive(&entries[1..])).is_ok());
        let error = refusal(&raw_archive(&entries));
        assert!(error.contains("more than 256 entries"), "{error}");
    }

    #[test]
    fn refuses_oversized_archives() {
        let mut bytes = cty_archive(CTY, CTY_CSV);
        bytes.resize(MAX_ARCHIVE_BYTES + 1, 0);
        let error = refusal(&bytes);
        assert!(error.contains("larger than 4 MiB"), "{error}");
    }

    #[test]
    fn stops_reading_an_entry_at_the_limit() {
        let padding = " ".repeat(MAX_ARCHIVE_TEXT_BYTES as usize);
        let bomb = cty_archive(&format!("{CTY}{padding}"), CTY_CSV);
        assert!(bomb.len() < 64 * 1024, "{}", bomb.len());
        let error = refusal(&bomb);
        assert!(error.contains("cty.dat is larger than 4 MiB"), "{error}");

        let mut lying = bomb.clone();
        let declared = (CTY.len() as u32).to_le_bytes();
        let actual = ((CTY.len() + padding.len()) as u32).to_le_bytes();
        let positions: Vec<usize> = lying
            .windows(4)
            .enumerate()
            .filter(|(_, window)| *window == actual)
            .map(|(at, _)| at)
            .collect();
        assert!(!positions.is_empty());
        for at in positions {
            lying[at..at + 4].copy_from_slice(&declared);
        }
        assert!(parse_cty_archive(&lying).is_err());
    }

    #[test]
    fn reads_the_parks_table_qso_tools_reads() {
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
    #[case::empty_parks(parse_parks("").is_err())]
    #[case::not_utf8(text(b"\xff").is_err())]
    fn refuses_malformed_tables(#[case] refused: bool) {
        assert!(refused);
    }
}
