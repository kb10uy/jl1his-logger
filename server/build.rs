use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

const ADIF_DIR: &str = "../assets/adif";

fn main() {
    println!("cargo:rerun-if-changed=migrations");
    println!("cargo:rerun-if-env-changed=GIT_COMMIT");
    if let Some(commit) = commit() {
        println!("cargo:rustc-env=JL1HIS_COMMIT={commit}");
    }
    if let Err(e) = adif() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn commit() -> Option<String> {
    if let Some(commit) = env::var("GIT_COMMIT").ok().filter(|c| !c.is_empty()) {
        return Some(commit);
    }
    let mut watched = vec![git(&["rev-parse", "--git-path", "HEAD"])?];
    if let Some(head) = git(&["symbolic-ref", "-q", "HEAD"]) {
        watched.extend(git(&["rev-parse", "--git-path", &head]));
    }
    watched.extend(git(&["rev-parse", "--git-path", "packed-refs"]));
    for path in watched.iter().filter(|p| Path::new(p).exists()) {
        println!("cargo:rerun-if-changed={path}");
    }
    git(&["rev-parse", "HEAD"])
}

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    let text = String::from_utf8(output.stdout).ok()?;
    output.status.success().then(|| text.trim().to_owned())
}

struct Row {
    file: String,
    number: usize,
    cells: BTreeMap<String, String>,
}

impl Row {
    fn get(&self, column: &str) -> Result<&str, String> {
        self.cells
            .get(column)
            .map(|v| v.trim())
            .ok_or_else(|| format!("{} has no column {column:?}", self.file))
    }

    fn flag(&self, column: &str) -> Result<bool, String> {
        match self.get(column)? {
            "" => Ok(false),
            v if v == column => Ok(true),
            v => Err(self.error(&format!("{column} is {v:?}, not empty or {column:?}"))),
        }
    }

    fn required(&self, column: &str) -> Result<&str, String> {
        match self.get(column)? {
            "" => Err(self.error(&format!("{column} is empty"))),
            v => Ok(v),
        }
    }

    fn number(&self, column: &str) -> Result<u32, String> {
        let v = self.required(column)?;
        v.parse()
            .map_err(|_| self.error(&format!("{column} {v:?} is not a number")))
    }

    fn error(&self, message: &str) -> String {
        format!("{} row {}: {message}", self.file, self.number)
    }
}

struct Tables {
    dir: PathBuf,
    version: Option<String>,
}

impl Tables {
    fn read(&mut self, name: &str) -> Result<Vec<Row>, String> {
        let path = self.dir.join(format!("{name}.csv"));
        println!("cargo:rerun-if-changed={}", path.display());
        let file = format!("assets/adif/{name}.csv");
        let bytes = fs::read(&path).map_err(|e| format!("{file}: {e}"))?;
        let text = String::from_utf8(bytes).map_err(|_| format!("{file} is not UTF-8"))?;
        let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
        let mut records = parse_csv(text).map_err(|e| format!("{file}: {e}"))?.into_iter();
        let header = records.next().ok_or_else(|| format!("{file} is empty"))?;
        let mut rows = Vec::new();
        for (i, record) in records.enumerate() {
            let number = i + 2;
            if record.len() != header.len() {
                return Err(format!(
                    "{file} row {number} has {} values, not the {} of the header",
                    record.len(),
                    header.len()
                ));
            }
            let row = Row {
                file: file.clone(),
                number,
                cells: header.iter().cloned().zip(record).collect(),
            };
            let version = row.required("ADIF Version")?;
            match &self.version {
                Some(seen) if seen != version => {
                    return Err(row.error(&format!("ADIF Version {version} differs from {seen}")));
                }
                Some(_) => {}
                None => self.version = Some(version.to_owned()),
            }
            rows.push(row);
        }
        if rows.is_empty() {
            return Err(format!("{file} has no rows"));
        }
        Ok(rows)
    }
}

fn parse_csv(text: &str) -> Result<Vec<Vec<String>>, String> {
    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match (quoted, c) {
            (true, '"') if chars.peek() == Some(&'"') => {
                chars.next();
                field.push('"');
            }
            (true, '"') => quoted = false,
            (true, c) => field.push(c),
            (false, '"') if field.is_empty() => quoted = true,
            (false, '"') => return Err(format!("row {} has a quote inside a value", records.len() + 1)),
            (false, ',') => record.push(std::mem::take(&mut field)),
            (false, '\r') if chars.peek() == Some(&'\n') => {}
            (false, '\n') => {
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
            }
            (false, c) => field.push(c),
        }
    }
    if quoted {
        return Err(format!("row {} has an unterminated quoted value", records.len() + 1));
    }
    if !field.is_empty() || !record.is_empty() {
        record.push(field);
        records.push(record);
    }
    Ok(records)
}

fn adif() -> Result<(), String> {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").map_err(|e| e.to_string())?);
    let out = PathBuf::from(env::var("OUT_DIR").map_err(|e| e.to_string())?).join("adif");
    fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    let mut tables = Tables {
        dir: manifest.join(ADIF_DIR),
        version: None,
    };
    let files = [
        ("bands.rs", bands(&mut tables)?),
        ("modes.rs", modes(&mut tables)?),
        ("qsl.rs", qsl(&mut tables)?),
        ("dxcc.rs", dxcc(&mut tables)?),
    ];
    let version = tables.version.ok_or("no ADIF table names its version")?;
    for (name, text) in files.into_iter().chain([("version.rs", format!("{version:?}\n"))]) {
        let path = out.join(name);
        fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(())
}

fn hz(row: &Row, column: &str) -> Result<u64, String> {
    let mhz = row.required(column)?;
    let invalid = || row.error(&format!("{column} {mhz:?} is not a frequency in MHz"));
    let (whole, fraction) = mhz.split_once('.').unwrap_or((mhz, ""));
    let digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    if !digits(whole) || !digits(fraction) || fraction.len() > 6 || (whole.is_empty() && fraction.is_empty()) {
        return Err(invalid());
    }
    let whole: u64 = match whole {
        "" => 0,
        w => w.parse().map_err(|_| invalid())?,
    };
    let micro: u64 = format!("{fraction:0<6}").parse().map_err(|_| invalid())?;
    whole
        .checked_mul(1_000_000)
        .and_then(|hz| hz.checked_add(micro))
        .ok_or_else(invalid)
}

fn bands(tables: &mut Tables) -> Result<String, String> {
    let mut text = String::from("pub const BANDS: &[Band] = &[\n");
    let mut previous: Option<(String, u64)> = None;
    for row in tables.read("enumerations_band")? {
        if row.flag("Import-only")? {
            continue;
        }
        let name = row.required("Band")?;
        let (lower, upper) = (hz(&row, "Lower Freq (MHz)")?, hz(&row, "Upper Freq (MHz)")?);
        if lower > upper {
            return Err(row.error(&format!("{name} ends below its start")));
        }
        if let Some((before, end)) = &previous
            && *end >= lower
        {
            return Err(row.error(&format!("{name} overlaps {before} or comes before it")));
        }
        previous = Some((name.to_owned(), upper));
        text.push_str(&format!("    band({name:?}, {lower}, {upper}),\n"));
    }
    text.push_str("];\n");
    Ok(text)
}

fn modes(tables: &mut Tables) -> Result<String, String> {
    let mut modes: Vec<(String, Vec<String>)> = Vec::new();
    let mut import_only = Vec::new();
    let mode_rows = tables.read("enumerations_mode")?;
    for row in &mode_rows {
        let name = row.required("Mode")?.to_owned();
        if modes.iter().any(|(m, _)| *m == name) || import_only.contains(&name) {
            return Err(row.error(&format!("{name} is listed twice")));
        }
        match row.flag("Import-only")? {
            true => import_only.push(name),
            false => modes.push((name, Vec::new())),
        }
    }
    let mut submodes: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut owners = BTreeMap::new();
    for row in tables.read("enumerations_submode")? {
        if row.flag("Import-only")? {
            continue;
        }
        let (submode, mode) = (row.required("Submode")?, row.required("Mode")?);
        if !modes.iter().any(|(m, _)| m == mode) {
            return Err(row.error(&format!("{submode} belongs to {mode}, which is not a current mode")));
        }
        submodes.entry(mode.to_owned()).or_default().push(submode.to_owned());
        owners.insert(submode.to_owned(), mode.to_owned());
    }
    for row in &mode_rows {
        let name = row.required("Mode")?;
        let Some((_, list)) = modes.iter_mut().find(|(m, _)| m == name) else {
            continue;
        };
        let known = submodes.get(name).cloned().unwrap_or_default();
        for listed in row.get("Submodes")?.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            let parts: Vec<&str> = listed.split_whitespace().collect();
            let missing_commas = !known.iter().any(|k| k == listed)
                && parts.len() > 1
                && parts.iter().all(|p| known.iter().any(|k| k == p));
            let entries = match missing_commas {
                true => parts,
                false => vec![listed],
            };
            for entry in entries {
                if let Some(owner) = owners.get(entry).filter(|owner| *owner != name) {
                    return Err(row.error(&format!("{entry} is a submode of {owner} as well as {name}")));
                }
                if !list.iter().any(|s| s == entry) {
                    list.push(entry.to_owned());
                }
            }
        }
        for submode in known {
            if !list.contains(&submode) {
                list.push(submode);
            }
        }
    }
    let mut seen = BTreeSet::new();
    for submode in modes.iter().flat_map(|(_, s)| s) {
        if !seen.insert(submode.to_ascii_uppercase()) {
            return Err(format!("the submode {submode} belongs to two modes"));
        }
    }
    for name in &import_only {
        if !seen.contains(&name.to_ascii_uppercase()) {
            return Err(format!(
                "the import-only mode {name} is not a submode of any current mode"
            ));
        }
    }
    let mut text = String::from("pub const MODES: &[Mode] = &[\n");
    for (name, submodes) in &modes {
        text.push_str(&format!("    mode({name:?}, &{submodes:?}),\n"));
    }
    text.push_str(&format!(
        "];\n\npub const IMPORT_ONLY_MODES: &[&str] = &{import_only:?};\n"
    ));
    Ok(text)
}

fn qsl(tables: &mut Tables) -> Result<String, String> {
    let mut text = String::new();
    for (name, file, column) in [
        ("SENT", "enumerations_qsl_sent", "Status"),
        ("RCVD", "enumerations_qsl_rcvd", "Status"),
        ("VIA", "enumerations_qsl_via", "Via"),
    ] {
        let rows = tables.read(file)?;
        let values = rows
            .iter()
            .map(|row| row.required(column))
            .collect::<Result<Vec<_>, _>>()?;
        text.push_str(&format!("const {name}: &[&str] = &{values:?};\n"));
    }
    Ok(text)
}

fn dxcc(tables: &mut Tables) -> Result<String, String> {
    let mut entities = BTreeMap::new();
    for row in tables.read("enumerations_dxcc_entity_code")? {
        let code = row.number("Entity Code")?;
        let entity = (row.required("Entity Name")?.to_owned(), row.flag("Deleted")?);
        if entities.insert(code, entity).is_some() {
            return Err(row.error(&format!("entity {code} is listed twice")));
        }
    }
    let mut subdivisions = BTreeMap::new();
    for row in tables.read("enumerations_primary_administrative_subdivision")? {
        if row.flag("Deleted")? || row.flag("Import-only")? {
            continue;
        }
        let dxcc = row.number("DXCC Entity Code")?;
        if !entities.contains_key(&dxcc) {
            return Err(row.error(&format!("{dxcc} is not a DXCC entity code")));
        }
        let code = row.required("Code")?.to_ascii_uppercase();
        let name = row.required("Primary Administrative Subdivision")?.to_owned();
        if subdivisions.insert((dxcc, code.clone()), name).is_some() {
            return Err(row.error(&format!("{code} of entity {dxcc} is listed twice")));
        }
    }
    let mut text = String::from("pub const ENTITIES: &[DxccEntity] = &[\n");
    for (code, (name, deleted)) in &entities {
        text.push_str(&format!("    entity({code}, {name:?}, {deleted}),\n"));
    }
    text.push_str("];\n\npub const SUBDIVISIONS: &[Subdivision] = &[\n");
    for ((dxcc, code), name) in &subdivisions {
        text.push_str(&format!("    subdivision({dxcc}, {code:?}, {name:?}),\n"));
    }
    text.push_str("];\n");
    Ok(text)
}
