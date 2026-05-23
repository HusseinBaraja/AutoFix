mod model;
mod shortcuts;
#[cfg(test)]
mod tests;
mod toml_io;
mod validation;

pub(crate) use model::{AppConfig, CorrectionEngine, CorrectionMode, RunMode};
pub(crate) use shortcuts::{Shortcut, ShortcutKey};
#[cfg(test)]
pub(crate) use toml_io::default_config_toml;
pub(crate) use toml_io::{load_config, save_config, ConfigIoError};
pub(crate) use validation::ValidateConfig;
