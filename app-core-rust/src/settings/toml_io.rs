use std::{
    error::Error,
    fmt, fs, io,
    path::{Path, PathBuf},
};

use super::import_recovery;
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
    let mut config = toml::from_str::<AppConfig>(input.trim_start_matches('\u{feff}'))
        .map_err(ConfigIoError::Parse)?;
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
    let _access = import_recovery::acquire(path).map_err(|source| ConfigIoError::Read {
        path: path.into(),
        source,
    })?;
    import_recovery::recover(path).map_err(|source| ConfigIoError::Read {
        path: path.into(),
        source,
    })?;
    let input = fs::read_to_string(path).map_err(|source| ConfigIoError::Read {
        path: path.to_path_buf(),
        source,
    })?;

    parse_config(&input)
}

/// Initializes missing settings only after recovery, holding the lock between read and creation.
pub(crate) fn load_or_create_config(path: &Path) -> Result<AppConfig, ConfigIoError> {
    let _access = import_recovery::acquire(path).map_err(|source| ConfigIoError::Read {
        path: path.into(),
        source,
    })?;
    import_recovery::recover(path).map_err(|source| ConfigIoError::Read {
        path: path.into(),
        source,
    })?;
    match fs::read_to_string(path) {
        Ok(input) => parse_config(&input),
        Err(source) if source.kind() == io::ErrorKind::NotFound => {
            let config = AppConfig::default();
            let output = config_to_toml(&config)?;
            import_recovery::write_atomic(path, output.as_bytes()).map_err(|source| {
                ConfigIoError::Write {
                    path: path.into(),
                    source,
                }
            })?;
            Ok(config)
        }
        Err(source) => Err(ConfigIoError::Read {
            path: path.into(),
            source,
        }),
    }
}

/// IPC edits start from the latest recovered config and hold the import lock through the durable write.
pub(crate) fn edit_config(
    path: &Path,
    edit: impl FnOnce(&mut AppConfig) -> Result<(), String>,
) -> Result<AppConfig, String> {
    let _access = import_recovery::acquire(path).map_err(|error| error.to_string())?;
    import_recovery::recover(path).map_err(|error| error.to_string())?;
    let input = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let mut config = parse_config(&input).map_err(|error| error.to_string())?;
    edit(&mut config)?;
    let output = config_to_toml(&config).map_err(|error| error.to_string())?;
    import_recovery::write_atomic(path, output.as_bytes()).map_err(|error| error.to_string())?;
    Ok(config)
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
