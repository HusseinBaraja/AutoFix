use std::path::{Path, PathBuf};

use crate::background::BackgroundError;

pub(crate) struct RuntimePaths {
    config_path: PathBuf,
    database_path: PathBuf,
}

impl RuntimePaths {
    pub(crate) fn for_current_user() -> Result<Self, BackgroundError> {
        let root = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or(std::env::current_dir().map_err(|source| {
                BackgroundError::CreateDirectory {
                    path: PathBuf::from("."),
                    source,
                }
            })?)
            .join("AutoFix");

        Ok(Self::new(
            root.join("config.toml"),
            root.join("autofix.sqlite"),
        ))
    }

    pub(crate) fn new(config_path: PathBuf, database_path: PathBuf) -> Self {
        Self {
            config_path,
            database_path,
        }
    }

    pub(crate) fn config_path(&self) -> &Path {
        &self.config_path
    }

    pub(crate) fn database_path(&self) -> &Path {
        &self.database_path
    }
}
