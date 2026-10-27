use std::{
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

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    pub server: Option<Server>,
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
        };
        config.save(&path).unwrap();
        assert_eq!(Config::load(&path).unwrap(), config);
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "[server]\nurl = \"https://log.example.com\"\n"
        );
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
