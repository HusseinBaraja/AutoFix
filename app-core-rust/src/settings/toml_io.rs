use std::{
    error::Error,
    fmt, fs, io,
    path::{Path, PathBuf},
};

use super::{validation::ConfigValidationError, AppConfig, ValidateConfig};

#[derive(Debug)]
pub(crate) enum ConfigIoError {
    Read { path: PathBuf, source: io::Error },
    Write { path: PathBuf, source: io::Error },
    Parse(toml::de::Error),
    Serialize(toml::ser::Error),
    Validation(ConfigValidationError),
}

impl fmt::Display for ConfigIoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => {
                write!(formatter, "failed to read {}: {}", path.display(), source)
            }
            Self::Write { path, source } => {
                write!(formatter, "failed to write {}: {}", path.display(), source)
            }
            Self::Parse(source) => write!(formatter, "failed to parse config: {}", source),
            Self::Serialize(source) => write!(formatter, "failed to serialize config: {}", source),
            Self::Validation(source) => write!(formatter, "invalid config: {}", source),
        }
    }
}

impl Error for ConfigIoError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read { source, .. } | Self::Write { source, .. } => Some(source),
            Self::Parse(source) => Some(source),
            Self::Serialize(source) => Some(source),
            Self::Validation(source) => Some(source),
        }
    }
}

/// Normalize previously supported retry counts on read; writes remain strict.
pub(crate) fn parse_config(input: &str) -> Result<AppConfig, ConfigIoError> {
    let mut config = toml::from_str::<AppConfig>(input).map_err(ConfigIoError::Parse)?;
    if config.api.retry_count > 1 {
        tracing::warn!(
            retry_count = config.api.retry_count,
            "legacy API retry count reduced to 1"
        );
        config.api.retry_count = 1;
    }
    config.validate().map_err(ConfigIoError::Validation)?;
    Ok(config)
}

pub(crate) fn config_to_toml(config: &AppConfig) -> Result<String, ConfigIoError> {
    config.validate().map_err(ConfigIoError::Validation)?;
    let body = toml::to_string_pretty(config).map_err(ConfigIoError::Serialize)?;
    Ok(format!("{}{}", generated_comments(), body))
}

pub(crate) fn load_config(path: impl AsRef<Path>) -> Result<AppConfig, ConfigIoError> {
    let path = path.as_ref();
    let input = fs::read_to_string(path).map_err(|source| ConfigIoError::Read {
        path: path.to_path_buf(),
        source,
    })?;

    parse_config(&input)
}

pub(crate) fn save_config(path: impl AsRef<Path>, config: &AppConfig) -> Result<(), ConfigIoError> {
    let path = path.as_ref();
    let output = config_to_toml(config)?;
    fs::write(path, output).map_err(|source| ConfigIoError::Write {
        path: path.to_path_buf(),
        source,
    })
}

/// Describe configuration boundaries and queue defaults without serializing credentials.
fn generated_comments() -> &'static str {
    r#"# AutoFix user configuration.
# Store API keys in Windows Credential Manager, not in this TOML file.
# Shortcut format uses key names joined by '+', for example Ctrl+Alt+Space.
# Correction streaming stays disabled because corrections need bounded latency.
# Pending queue size counts running and waiting corrections per session (1 to 16).
# Full queue: skip_new, cancel_oldest, or merge_newest and wait for the next trigger.

"#
}
