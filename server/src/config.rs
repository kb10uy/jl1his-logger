use std::path::{Path, PathBuf};

use serde::Deserialize;
use url::{Host, Url};

use crate::domain::adif::LengthUnit;

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub reference: ReferenceConfig,
    pub adif: AdifConfig,
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
            cty: "data/reference/cty.dat".into(),
            dxcc: "data/reference/dxcc.toml".into(),
            subdivisions: "data/reference/subdivisions.toml".into(),
            parks: "data/reference/parks.toml".into(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AdifConfig {
    pub length: LengthUnit,
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
        toml::from_str(&text).map_err(|source| ConfigError::Parse {
            path: path.to_owned(),
            source,
        })
    }
}

pub fn parse_public_origin(s: &str) -> Result<Url, String> {
    let url = Url::parse(s.trim()).map_err(|e| format!("PUBLIC_ORIGIN {s:?}: {e}"))?;
    if matches!(url.host(), Some(Host::Ipv4(_) | Host::Ipv6(_))) {
        return Err(format!(
            "PUBLIC_ORIGIN {s:?}: passkeys need a DNS hostname; use http://localhost for local development"
        ));
    }
    let local = url.host_str() == Some("localhost");
    match url.scheme() {
        "https" => {}
        "http" if local => {}
        _ => {
            return Err(format!(
                "PUBLIC_ORIGIN {s:?}: passkeys need https (or http://localhost)"
            ));
        }
    }
    if url.path() != "/" || url.query().is_some() || url.fragment().is_some() {
        return Err(format!(
            "PUBLIC_ORIGIN {s:?}: must be an origin such as https://log.example.com (the app is served at the root)"
        ));
    }
    if url.host_str().is_none() || !url.username().is_empty() || url.password().is_some() {
        return Err(format!("PUBLIC_ORIGIN {s:?}: must be an origin"));
    }
    Ok(url)
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[test]
    fn example_matches_defaults() {
        let example: Config = toml::from_str(include_str!("../../assets/config.example.toml")).unwrap();
        assert_eq!(example, Config::default());
    }

    #[test]
    fn missing_sections_take_defaults() {
        let config: Config = toml::from_str("[adif]\nlength = \"bytes\"\n").unwrap();
        assert_eq!(config.adif.length, LengthUnit::Bytes);
        assert_eq!(config.reference, ReferenceConfig::default());
    }

    #[rstest]
    #[case::unknown_key("[adif]\nlenght = \"bytes\"\n")]
    #[case::unknown_section("[referance]\n")]
    #[case::unknown_unit("[adif]\nlength = \"octets\"\n")]
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
