use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const FILE_NAME: &str = if cfg!(debug_assertions) {
    "config.dev.toml"
} else {
    "config.toml"
};

pub struct Paths {
    pub config: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default = "default_close_to_tray")]
    pub close_to_tray: bool,
    pub server: Option<Server>,
    #[serde(default)]
    pub rig: Rig,
    #[serde(default = "default_listeners")]
    pub listeners: Vec<Listener>,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            language: None,
            close_to_tray: true,
            server: None,
            rig: Rig::default(),
            listeners: default_listeners(),
        }
    }
}

fn default_close_to_tray() -> bool {
    true
}

fn socket_address(host: &str, port: u16) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Rig {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub poll_ms: u64,
    pub modes: BTreeMap<String, String>,
}

impl Default for Rig {
    fn default() -> Rig {
        Rig {
            enabled: false,
            host: "127.0.0.1".to_owned(),
            port: 4532,
            poll_ms: 1000,
            modes: BTreeMap::new(),
        }
    }
}

impl Rig {
    pub fn address(&self) -> String {
        socket_address(&self.host, self.port)
    }

    pub fn same_connection(&self, other: &Rig) -> bool {
        self.enabled == other.enabled
            && self.host == other.host
            && self.port == other.port
            && self.poll_ms == other.poll_ms
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Listener {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
}

impl Default for Listener {
    fn default() -> Listener {
        Listener {
            enabled: false,
            host: "127.0.0.1".to_owned(),
            port: 2237,
        }
    }
}

impl Listener {
    pub fn address(&self) -> String {
        socket_address(&self.host, self.port)
    }
}

fn default_listeners() -> Vec<Listener> {
    [2237, 2333]
        .map(|port| Listener {
            port,
            ..Listener::default()
        })
        .to_vec()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Server {
    pub url: String,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("could not read {path}: {source}")]
    Read { path: PathBuf, source: io::Error },
    #[error("{path} is not a valid settings file: {source}")]
    Parse { path: PathBuf, source: toml::de::Error },
    #[error("could not write {path}: {source}")]
    Write { path: PathBuf, source: io::Error },
}

impl Config {
    pub fn load(path: &Path) -> Result<Config, ConfigError> {
        match fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).map_err(|source| ConfigError::Parse {
                path: path.to_owned(),
                source,
            }),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Config::default()),
            Err(source) => Err(ConfigError::Read {
                path: path.to_owned(),
                source,
            }),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        let write = |source| ConfigError::Write {
            path: path.to_owned(),
            source,
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(write)?;
        }
        let text = toml::to_string(self).expect("the settings always serialize");
        let staged = path.with_extension("toml.new");
        fs::write(&staged, text).map_err(write)?;
        fs::rename(&staged, path).map_err(write)
    }
}

pub fn path_argument(mut args: impl Iterator<Item = String>) -> Option<PathBuf> {
    while let Some(arg) = args.next() {
        if arg == "--config" {
            return args.next().map(PathBuf::from);
        }
        if let Some(path) = arg.strip_prefix("--config=") {
            return Some(PathBuf::from(path));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn a_missing_file_is_the_default() {
        let dir = tempdir().unwrap();
        assert_eq!(
            Config::load(&dir.path().join("config.toml")).unwrap(),
            Config::default()
        );
    }

    #[test]
    fn saves_and_loads_the_server() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nested").join("config.toml");
        let config = Config {
            server: Some(Server {
                url: "https://log.example.com".to_owned(),
            }),
            ..Config::default()
        };
        config.save(&path).unwrap();
        assert_eq!(Config::load(&path).unwrap(), config);
        assert!(
            fs::read_to_string(&path)
                .unwrap()
                .starts_with("close_to_tray = true\n\n[server]\nurl = \"https://log.example.com\"\n")
        );
    }

    #[test]
    fn saves_and_loads_the_language_and_the_rig() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let config = Config {
            language: Some("ja".to_owned()),
            close_to_tray: false,
            rig: Rig {
                enabled: true,
                modes: BTreeMap::from([("PKTUSB".to_owned(), "MFSK|FT4".to_owned())]),
                ..Rig::default()
            },
            ..Config::default()
        };
        config.save(&path).unwrap();
        assert_eq!(Config::load(&path).unwrap(), config);
    }

    #[test]
    fn fills_what_the_rig_table_leaves_out() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "[rig]\nenabled = true\nport = 4533\n").unwrap();
        let rig = Config::load(&path).unwrap().rig;
        assert!(rig.enabled);
        assert_eq!(rig.address(), "127.0.0.1:4533");
        assert_eq!(rig.poll_ms, 1000);
    }

    #[test]
    fn saves_and_loads_the_listeners() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let config = Config {
            listeners: vec![Listener {
                enabled: true,
                host: "::1".to_owned(),
                port: 2238,
            }],
            ..Config::default()
        };
        config.save(&path).unwrap();
        assert_eq!(Config::load(&path).unwrap(), config);
        assert_eq!(config.listeners[0].address(), "[::1]:2238");
    }

    #[test]
    fn listens_on_the_usual_ports_unless_told_otherwise() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(
            &path,
            "[rig]
enabled = true
",
        )
        .unwrap();
        let config = Config::load(&path).unwrap();
        assert!(config.close_to_tray);
        let listeners = config.listeners;
        let addresses: Vec<_> = listeners.iter().map(|l| (l.enabled, l.address())).collect();
        assert_eq!(
            addresses,
            [
                (false, "127.0.0.1:2237".to_owned()),
                (false, "127.0.0.1:2333".to_owned())
            ]
        );
    }

    #[rstest]
    #[case("localhost", "localhost:4532")]
    #[case("::1", "[::1]:4532")]
    #[case("[::1]", "[::1]:4532")]
    fn addresses_rigctld(#[case] host: &str, #[case] expected: &str) {
        let rig = Rig {
            host: host.to_owned(),
            ..Rig::default()
        };
        assert_eq!(rig.address(), expected);
    }

    #[test]
    fn refuses_a_broken_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "[server\n").unwrap();
        assert!(matches!(Config::load(&path), Err(ConfigError::Parse { .. })));
    }

    #[rstest]
    #[case(&["app"], None)]
    #[case(&["app", "--config", "a.toml"], Some("a.toml"))]
    #[case(&["app", "--config=b.toml"], Some("b.toml"))]
    #[case(&["app", "--config"], None)]
    fn reads_the_config_argument(#[case] args: &[&str], #[case] expected: Option<&str>) {
        let args = args.iter().map(|a| a.to_string()).skip(1);
        assert_eq!(path_argument(args), expected.map(PathBuf::from));
    }
}
