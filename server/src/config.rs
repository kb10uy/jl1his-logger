use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Deserializer, de::Error as _};
use url::{Host, Url};

use crate::domain::adif::{Encoding, LengthUnit};

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub reference: ReferenceConfig,
    pub adif: AdifConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ServerConfig {
    pub listen: SocketAddr,
    #[serde(deserialize_with = "deserialize_public_origin")]
    pub public_origin: Option<Url>,
    pub web_dir: Option<PathBuf>,
    pub auto_migrate: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen: SocketAddr::from(([127, 0, 0, 1], 8080)),
            public_origin: None,
            web_dir: None,
            auto_migrate: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DatabaseConfig {
    pub path: PathBuf,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            path: "jl1his-logger.db".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ReferenceConfig {
    pub cty: PathBuf,
    pub dxcc: PathBuf,
    pub subdivisions: PathBuf,
    pub parks: PathBuf,
}

impl Default for ReferenceConfig {
    fn default() -> Self {
        Self {
            cty: "reference/cty.dat".into(),
            dxcc: "reference/dxcc.toml".into(),
            subdivisions: "reference/subdivisions.toml".into(),
            parks: "reference/parks.toml".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AdifConfig {
    pub length: LengthUnit,
    pub fallback_encoding: Encoding,
}

impl Default for AdifConfig {
    fn default() -> Self {
        Self {
            length: LengthUnit::default(),
            fallback_encoding: Encoding::ShiftJis,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to read {}: {source}", path.display())]
    Read { path: PathBuf, source: std::io::Error },
    #[error("failed to parse {}: {source}", path.display())]
    Parse { path: PathBuf, source: toml::de::Error },
}

impl Config {
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.to_owned(),
            source,
        })?;
        let config: Config = toml::from_str(&text).map_err(|source| ConfigError::Parse {
            path: path.to_owned(),
            source,
        })?;
        Ok(config.relative_to(path.parent().unwrap_or(Path::new(""))))
    }

    fn relative_to(mut self, base: &Path) -> Self {
        let paths = [
            Some(&mut self.database.path),
            self.server.web_dir.as_mut(),
            Some(&mut self.reference.cty),
            Some(&mut self.reference.dxcc),
            Some(&mut self.reference.subdivisions),
            Some(&mut self.reference.parks),
        ];
        for path in paths.into_iter().flatten() {
            *path = base.join(&*path);
        }
        self
    }
}

fn deserialize_public_origin<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Url>, D::Error> {
    let text = String::deserialize(deserializer)?;
    parse_public_origin(&text).map(Some).map_err(D::Error::custom)
}

fn parse_public_origin(s: &str) -> Result<Url, String> {
    let url = Url::parse(s.trim()).map_err(|e| format!("public_origin {s:?}: {e}"))?;
    if matches!(url.host(), Some(Host::Ipv4(_) | Host::Ipv6(_))) {
        return Err(format!(
            "public_origin {s:?}: passkeys need a DNS hostname; use http://localhost for local development"
        ));
    }
    let local = url.host_str() == Some("localhost");
    match url.scheme() {
        "https" => {}
        "http" if local => {}
        _ => {
            return Err(format!(
                "public_origin {s:?}: passkeys need https (or http://localhost)"
            ));
        }
    }
    if url.path() != "/" || url.query().is_some() || url.fragment().is_some() {
        return Err(format!(
            "public_origin {s:?}: must be an origin such as https://log.example.com (the app is served at the root)"
        ));
    }
    if url.host_str().is_none() || !url.username().is_empty() || url.password().is_some() {
        return Err(format!("public_origin {s:?}: must be an origin"));
    }
    Ok(url)
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[test]
    fn example_matches_defaults_but_the_origin() {
        let mut example: Config = toml::from_str(include_str!("../../assets/config.example.toml")).unwrap();
        let origin = example.server.public_origin.take().unwrap();
        assert_eq!(origin.as_str(), "https://log.example.com/");
        assert_eq!(example, Config::default());
    }

    #[test]
    fn resolves_paths_against_the_directory_of_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[server]\nweb_dir = \"web\"\n").unwrap();
        let config = Config::load(&path).unwrap();
        assert_eq!(config.database.path, dir.path().join("jl1his-logger.db"));
        assert_eq!(config.server.web_dir, Some(dir.path().join("web")));
        assert_eq!(config.reference.cty, dir.path().join("reference").join("cty.dat"));
    }

    #[test]
    fn keeps_paths_relative_to_a_file_in_the_working_directory() {
        let config = Config::default().relative_to(Path::new(""));
        assert_eq!(config, Config::default());
    }

    #[test]
    fn refuses_an_origin_passkeys_cannot_use() {
        let error = toml::from_str::<Config>("[server]\npublic_origin = \"http://127.0.0.1:8080\"\n").unwrap_err();
        assert!(error.to_string().contains("hostname"), "{error}");
    }

    #[test]
    fn missing_sections_take_defaults() {
        let config: Config = toml::from_str("[adif]\nlength = \"bytes\"\n").unwrap();
        assert_eq!(config.adif.length, LengthUnit::Bytes);
        assert_eq!(config.adif.fallback_encoding, Encoding::ShiftJis);
        assert_eq!(config.reference, ReferenceConfig::default());
    }

    #[rstest]
    #[case::unknown_key("[adif]\nlenght = \"bytes\"\n")]
    #[case::unknown_section("[referance]\n")]
    #[case::unknown_unit("[adif]\nlength = \"octets\"\n")]
    #[case::unknown_encoding("[adif]\nfallback_encoding = \"euc-jp\"\n")]
    fn rejects_mistakes(#[case] text: &str) {
        assert!(toml::from_str::<Config>(text).is_err());
    }

    #[rstest]
    #[case("https://log.example.com/", "https://log.example.com")]
    #[case("https://log.example.com", "https://log.example.com")]
    #[case("http://localhost:8080", "http://localhost:8080")]
    #[case("https://localhost:8080", "https://localhost:8080")]
    fn accepts_origins(#[case] input: &str, #[case] origin: &str) {
        let url = parse_public_origin(input).unwrap();
        assert_eq!(url.origin().ascii_serialization(), origin);
        webauthn_rs::WebauthnBuilder::new(url.host_str().unwrap(), &url)
            .unwrap()
            .build()
            .unwrap();
    }

    #[rstest]
    #[case("http://127.0.0.1:8080")]
    #[case("https://127.0.0.1:8080")]
    #[case("https://192.0.2.1")]
    #[case("http://[::1]:8080")]
    #[case("https://[::1]:8080")]
    #[case("https://[2001:db8::1]")]
    fn rejects_ip_origins_with_hostname_guidance(#[case] input: &str) {
        let error = parse_public_origin(input).unwrap_err();
        assert!(error.contains("hostname"), "{error}");
        assert!(error.contains("http://localhost"), "{error}");
    }

    #[rstest]
    #[case("log.example.com")]
    #[case("http://log.example.com")]
    #[case("https://example.com/logger")]
    #[case("https://example.com/?a=b")]
    #[case("https://user@example.com")]
    #[case("ftp://example.com")]
    fn rejects_non_origins(#[case] input: &str) {
        assert!(parse_public_origin(input).is_err());
    }
}
