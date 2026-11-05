use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Cursor, Write},
};

use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

use crate::domain::qso::{Qso, QsoError, field_name};

pub const NONE: &str = "none";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub label: String,
    pub missing: Vec<String>,
    pub records: Vec<usize>,
}

pub fn fields(names: &[String]) -> Result<Vec<String>, QsoError> {
    let mut fields = Vec::new();
    for name in names {
        let name = field_name(name)?;
        if !fields.contains(&name) {
            fields.push(name);
        }
    }
    Ok(fields)
}

pub fn group(records: &[Qso], fields: &[String]) -> Vec<Group> {
    let mut groups: BTreeMap<Vec<String>, Group> = BTreeMap::new();
    for (index, record) in records.iter().enumerate() {
        let values: Vec<&str> = fields
            .iter()
            .map(|f| record.get(f).map(str::trim).filter(|v| !v.is_empty()).unwrap_or(NONE))
            .collect();
        let key = values.iter().map(|v| v.to_ascii_uppercase()).collect();
        groups
            .entry(key)
            .or_insert_with(|| Group {
                label: values.join(" "),
                missing: fields
                    .iter()
                    .zip(&values)
                    .filter(|(_, v)| **v == NONE)
                    .map(|(f, _)| f.clone())
                    .collect(),
                records: Vec::new(),
            })
            .records
            .push(index);
    }
    groups.into_values().collect()
}

pub fn file_names(labels: &[String], extension: &str) -> Vec<String> {
    let mut taken = BTreeSet::new();
    labels
        .iter()
        .map(|label| {
            let mut stem = String::new();
            for c in label.chars() {
                match c.is_ascii_alphanumeric() || c == '-' {
                    true => stem.push(c),
                    false if stem.ends_with('_') => {}
                    false => stem.push('_'),
                }
            }
            let stem = stem.trim_matches('_');
            let stem = if stem.is_empty() { NONE } else { stem };
            let mut name = format!("{stem}.{extension}");
            let mut n = 1;
            while !taken.insert(name.to_ascii_lowercase()) {
                n += 1;
                name = format!("{stem}-{n}.{extension}");
            }
            name
        })
        .collect()
}

pub fn archive(files: &[(String, Vec<u8>)]) -> Vec<u8> {
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in files {
        writer.start_file(name, options).expect("writing to memory");
        writer.write_all(bytes).expect("writing to memory");
    }
    writer.finish().expect("writing to memory").into_inner()
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use zip::ZipArchive;

    use super::*;

    fn qso(fields: &[(&str, &str)]) -> Qso {
        Qso::from_fields(fields.iter().copied()).unwrap()
    }

    #[test]
    fn checks_the_field_names() {
        assert_eq!(
            fields(&["station_callsign".into(), "MY_GRIDSQUARE".into()]).unwrap(),
            ["STATION_CALLSIGN", "MY_GRIDSQUARE"]
        );
        assert!(fields(&["1X".into()]).is_err());
        assert_eq!(fields(&["CALL".into(), "call".into()]).unwrap(), ["CALL"]);
    }

    #[test]
    fn groups_records_by_their_values_ignoring_case() {
        let records = [
            qso(&[("CALL", "JA1AAA"), ("STATION_CALLSIGN", "JL1HIS/P")]),
            qso(&[("CALL", "JA1BBB"), ("STATION_CALLSIGN", "JL1HIS")]),
            qso(&[
                ("CALL", "JA1CCC"),
                ("STATION_CALLSIGN", "jl1his/p"),
                ("MY_GRIDSQUARE", " PM95 "),
            ]),
            qso(&[
                ("CALL", "JA1DDD"),
                ("STATION_CALLSIGN", "JL1HIS"),
                ("MY_GRIDSQUARE", ""),
            ]),
        ];
        let by_call = group(&records, &["STATION_CALLSIGN".into()]);
        assert_eq!(
            by_call,
            [
                Group {
                    label: "JL1HIS".into(),
                    missing: vec![],
                    records: vec![1, 3]
                },
                Group {
                    label: "JL1HIS/P".into(),
                    missing: vec![],
                    records: vec![0, 2]
                },
            ]
        );
        let by_both = group(&records, &["STATION_CALLSIGN".into(), "MY_GRIDSQUARE".into()]);
        let labels: Vec<_> = by_both.iter().map(|g| g.label.as_str()).collect();
        assert_eq!(labels, ["JL1HIS none", "JL1HIS/P none", "jl1his/p PM95"]);
        assert_eq!(by_both[0].records, [1, 3]);
        assert_eq!(by_both[0].missing, ["MY_GRIDSQUARE"]);
        assert_eq!(by_both[2].missing, Vec::<String>::new());
    }

    #[test]
    fn names_files_safely_and_uniquely() {
        let labels: Vec<String> = ["JL1HIS/P", "JL1HIS_P", "../jl1his_p", "東京", "JP-0001 PM95"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(
            file_names(&labels, "adi"),
            [
                "JL1HIS_P.adi",
                "JL1HIS_P-2.adi",
                "jl1his_p-3.adi",
                "none.adi",
                "JP-0001_PM95.adi"
            ]
        );
    }

    #[test]
    fn archives_the_files() {
        let bytes = archive(&[("a.adi".into(), b"<EOH>\n".to_vec()), ("b.adi".into(), vec![])]);
        let mut zip = ZipArchive::new(Cursor::new(bytes)).unwrap();
        assert_eq!(zip.len(), 2);
        let mut text = String::new();
        zip.by_name("a.adi").unwrap().read_to_string(&mut text).unwrap();
        assert_eq!(text, "<EOH>\n");
        assert_eq!(zip.by_index(1).unwrap().name().unwrap(), "b.adi");
    }
}
