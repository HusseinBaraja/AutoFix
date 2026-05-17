use std::{error::Error, fmt};

use super::{
    model::{ConfidenceBehavior, CorrectionEngine, CorrectionMode, GrammarCategory, RunMode},
    AppConfig,
};

pub(crate) trait ValidateConfig {
    fn validate(&self) -> Result<(), ConfigValidationError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConfigValidationError {
    field: &'static str,
    message: &'static str,
}

impl ConfigValidationError {
    fn new(field: &'static str, message: &'static str) -> Self {
        Self { field, message }
    }

    pub(crate) fn field(&self) -> &'static str {
        self.field
    }
}

impl fmt::Display for ConfigValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.field, self.message)
    }
}

impl Error for ConfigValidationError {}

impl ValidateConfig for AppConfig {
    fn validate(&self) -> Result<(), ConfigValidationError> {
        validate_shortcut("shortcuts.correct", &self.shortcuts.correct)?;
        validate_shortcut("shortcuts.undo", &self.shortcuts.undo)?;
        validate_positive("triggers.word_count", self.triggers.word_count)?;
        validate_non_empty_list("triggers.characters", &self.triggers.characters)?;
        validate_non_empty_list(
            "context.initial_context_boundary_chars",
            &self.context.initial_context_boundary_chars,
        )?;
        validate_positive(
            "context.initial_context_words",
            self.context.initial_context_words,
        )?;
        validate_positive(
            "context.forward_movement_word_limit",
            self.context.forward_movement_word_limit,
        )?;
        validate_positive_u32(
            "context.informative_context_max_chars",
            self.context.informative_context_max_chars,
        )?;
        validate_positive(
            "context.informative_context_min_words",
            self.context.informative_context_min_words,
        )?;
        validate_positive(
            "context.executable_context_max_words",
            self.context.executable_context_max_words,
        )?;
        validate_correction(self)?;
        validate_api(self)?;
        validate_logging(self)?;

        Ok(())
    }
}

fn validate_shortcut(field: &'static str, value: &str) -> Result<(), ConfigValidationError> {
    if value.trim().is_empty() {
        return Err(ConfigValidationError::new(field, "must not be empty"));
    }

    Ok(())
}

fn validate_positive(field: &'static str, value: u16) -> Result<(), ConfigValidationError> {
    if value == 0 {
        return Err(ConfigValidationError::new(
            field,
            "must be greater than zero",
        ));
    }

    Ok(())
}

fn validate_positive_u32(field: &'static str, value: u32) -> Result<(), ConfigValidationError> {
    if value == 0 {
        return Err(ConfigValidationError::new(
            field,
            "must be greater than zero",
        ));
    }

    Ok(())
}

fn validate_non_empty_list(
    field: &'static str,
    values: &[String],
) -> Result<(), ConfigValidationError> {
    if values.is_empty() || values.iter().any(|value| value.is_empty()) {
        return Err(ConfigValidationError::new(
            field,
            "must contain non-empty strings",
        ));
    }

    Ok(())
}

fn validate_correction(config: &AppConfig) -> Result<(), ConfigValidationError> {
    match config.general.run_mode {
        RunMode::Blocklist | RunMode::Allowlist => {}
    }
    match config.correction.engine {
        CorrectionEngine::Local | CorrectionEngine::Api => {}
    }
    match config.correction.mode {
        CorrectionMode::TyposOnly if !config.correction.enabled_grammar_categories.is_empty() => {
            return Err(ConfigValidationError::new(
                "correction.enabled_grammar_categories",
                "must be empty unless grammar mode is enabled",
            ));
        }
        CorrectionMode::TyposOnly | CorrectionMode::TyposPlusGrammar => {}
    }
    if config.correction.low_confidence_behavior != ConfidenceBehavior::DoNothing {
        return Err(ConfigValidationError::new(
            "correction.low_confidence_behavior",
            "must be do_nothing",
        ));
    }
    for category in &config.correction.enabled_grammar_categories {
        match category {
            GrammarCategory::Agreement
            | GrammarCategory::Capitalization
            | GrammarCategory::Clarity
            | GrammarCategory::Punctuation
            | GrammarCategory::Tense
            | GrammarCategory::WordOrder => {}
        }
    }

    Ok(())
}

fn validate_api(config: &AppConfig) -> Result<(), ConfigValidationError> {
    if config.api.model.trim().is_empty() {
        return Err(ConfigValidationError::new("api.model", "must not be empty"));
    }
    if config.api.provider_preset.trim().is_empty() {
        return Err(ConfigValidationError::new(
            "api.provider_preset",
            "must not be empty",
        ));
    }
    if config.api.timeout_auto_ms == 0 {
        return Err(ConfigValidationError::new(
            "api.timeout_auto_ms",
            "must be greater than zero",
        ));
    }
    if config.api.timeout_manual_ms == 0 {
        return Err(ConfigValidationError::new(
            "api.timeout_manual_ms",
            "must be greater than zero",
        ));
    }
    if !(0.0..=2.0).contains(&config.api.temperature) {
        return Err(ConfigValidationError::new(
            "api.temperature",
            "must be between 0 and 2",
        ));
    }
    if config.api.streaming {
        return Err(ConfigValidationError::new(
            "api.streaming",
            "must remain disabled for correction",
        ));
    }

    Ok(())
}

fn validate_logging(config: &AppConfig) -> Result<(), ConfigValidationError> {
    if config.logging.full_text_debug_mode_enabled && !config.logging.debug_mode_enabled {
        return Err(ConfigValidationError::new(
            "logging.full_text_debug_mode_enabled",
            "requires debug_mode_enabled",
        ));
    }
    if config.logging.redacted_debug_mode_enabled && !config.logging.debug_mode_enabled {
        return Err(ConfigValidationError::new(
            "logging.redacted_debug_mode_enabled",
            "requires debug_mode_enabled",
        ));
    }

    Ok(())
}
