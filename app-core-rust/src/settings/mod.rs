pub(crate) mod import_recovery;
mod model;
mod shortcuts;
#[cfg(test)]
mod tests;
mod toml_io;
mod validation;

pub(crate) use crate::correction::CorrectionMode;
pub(crate) use model::{
    AppConfig, ContextConfig, CorrectionEngine, FeedbackConfig, LearningConfig, LearningMode,
    LearningRule, PendingQueueFullBehavior, RunMode,
};
pub(crate) use shortcuts::{Shortcut, ShortcutKey};
#[cfg(test)]
pub(crate) use tests::save_config;
pub(crate) use toml_io::{edit_config, load_config, load_or_create_config, ConfigIoError};
pub(crate) use validation::ValidateConfig;
