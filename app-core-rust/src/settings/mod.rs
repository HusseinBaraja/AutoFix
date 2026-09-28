mod model;
mod shortcuts;
#[cfg(test)]
mod tests;
mod toml_io;
mod validation;

pub(crate) use crate::correction::CorrectionMode;
pub(crate) use model::{AppConfig, ContextConfig, CorrectionEngine, RunMode};
pub(crate) use shortcuts::{Shortcut, ShortcutKey};
pub(crate) use toml_io::{load_config, save_config, ConfigIoError};
pub(crate) use validation::ValidateConfig;
