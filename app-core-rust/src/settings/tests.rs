use std::{fs, time::SystemTime};

use super::{
    default_config_toml, load_config,
    model::{ConfidenceBehavior, CorrectionEngine, CorrectionMode, GrammarCategory, RunMode},
    save_config, AppConfig, ValidateConfig,
};

#[test]
fn default_config_has_requested_values() {
    let config = AppConfig::default();

    assert_eq!(config.general.run_mode, RunMode::Blocklist);
    assert_eq!(config.shortcuts.correct, "Ctrl+Alt+Space");
    assert_eq!(config.shortcuts.undo, "Ctrl+Alt+Z");
    assert_eq!(config.triggers.word_count, 10);
    assert_eq!(config.triggers.characters, vec!["."]);
    assert_eq!(config.context.initial_context_words, 25);
    assert_eq!(config.context.initial_context_boundary_chars, vec!["."]);
    assert_eq!(config.context.forward_movement_word_limit, 5);
    assert_eq!(config.context.informative_context_min_words, 25);
    assert!(!config.onboarding.completed);
    assert!(config.correction.enabled);
    assert_eq!(config.correction.mode, CorrectionMode::TyposOnly);
    assert_eq!(config.correction.engine, CorrectionEngine::Local);
    assert_eq!(
        config.correction.high_confidence_behavior,
        ConfidenceBehavior::Silent
    );
    assert_eq!(
        config.correction.medium_confidence_behavior,
        ConfidenceBehavior::Suggestion
    );
    assert_eq!(
        config.correction.low_confidence_behavior,
        ConfidenceBehavior::DoNothing
    );
    assert_eq!(config.api.timeout_manual_ms, 3_000);
    assert_eq!(config.api.timeout_auto_ms, 700);
    assert_eq!(config.api.temperature, 0.0);
    assert!(!config.api.streaming);
    assert!(!config.logging.debug_mode_enabled);
    assert!(!config.logging.redacted_debug_mode_enabled);
    assert!(!config.logging.full_text_debug_mode_enabled);
    assert_eq!(config.logging.log_retention_days, None);
}

#[test]
fn generated_toml_has_comments_and_no_api_key_field() {
    let output = default_config_toml().unwrap();

    assert!(output.contains("# AutoFix user configuration."));
    assert!(output.contains("[general]"));
    assert!(output.contains("[api]"));
    assert!(!output.to_lowercase().contains("api_key"));
}

#[test]
fn parses_full_user_config() {
    let config = super::toml_io::parse_config(
        r#"
[general]
start_with_windows = true
run_mode = "allowlist"

[shortcuts]
correct = "Ctrl+Alt+Space"
undo = "Ctrl+Alt+Z"

[triggers]
word_count_enabled = true
word_count = 12
character_trigger_enabled = true
characters = [".", "?", "!"]

[context]
initial_context_words = 25
initial_context_boundary_chars = [".", "?", "!"]
forward_movement_word_limit = 5
informative_context_max_chars = 2000
informative_context_min_words = 25
executable_context_max_words = 80

[correction]
enabled = true
mode = "typos_plus_grammar"
engine = "api"
high_confidence_behavior = "silent"
medium_confidence_behavior = "suggestion"
low_confidence_behavior = "do_nothing"
enabled_grammar_categories = ["agreement", "punctuation"]

[api]
provider_preset = "custom"
base_url = "https://example.test/v1"
model = "typo-model"
timeout_manual_ms = 3000
timeout_auto_ms = 700
retry_count = 2
fallback_to_local = true
temperature = 0.0
streaming = false

[feedback]
tray_state_enabled = true
show_correction_applied_notification = true
show_skipped_reason = true
show_medium_confidence_suggestions = true
show_blocked_app_notice = true
show_timeout_notice = true

[logging]
metadata_only_logs_enabled = true
debug_mode_enabled = false
redacted_debug_mode_enabled = false
full_text_debug_mode_enabled = false
log_retention_days = 30
"#,
    )
    .unwrap();

    assert_eq!(config.general.run_mode, RunMode::Allowlist);
    assert_eq!(config.triggers.word_count, 12);
    assert!(config.correction.enabled);
    assert_eq!(config.correction.mode, CorrectionMode::TyposPlusGrammar);
    assert_eq!(
        config.correction.enabled_grammar_categories,
        vec![GrammarCategory::Agreement, GrammarCategory::Punctuation]
    );
    assert_eq!(config.logging.log_retention_days, Some(30));
}

#[test]
fn rejects_invalid_confidence_behavior() {
    let mut config = AppConfig::default();
    config.correction.low_confidence_behavior = ConfidenceBehavior::Silent;

    let error = config.validate().unwrap_err();

    assert_eq!(error.field(), "correction.low_confidence_behavior");
}

#[test]
fn rejects_invalid_shortcut() {
    let mut config = AppConfig::default();
    config.shortcuts.correct = "Space".to_owned();

    let error = config.validate().unwrap_err();

    assert_eq!(error.field(), "shortcuts.correct");
}

#[test]
fn rejects_conflicting_shortcuts() {
    let mut config = AppConfig::default();
    config.shortcuts.undo = config.shortcuts.correct.clone();

    let error = config.validate().unwrap_err();

    assert_eq!(error.field(), "shortcuts.undo");
}

#[test]
fn rejects_streaming_correction() {
    let mut config = AppConfig::default();
    config.api.streaming = true;

    let error = config.validate().unwrap_err();

    assert_eq!(error.field(), "api.streaming");
}

#[test]
fn rejects_zero_log_retention_days() {
    let mut config = AppConfig::default();
    config.logging.log_retention_days = Some(0);

    let error = config.validate().unwrap_err();

    assert_eq!(error.field(), "logging.log_retention_days");
}

#[test]
fn saves_and_loads_config() {
    let path = std::env::temp_dir().join(format!(
        "autofix-config-{}.toml",
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut config = AppConfig::default();
    config.general.start_with_windows = true;

    save_config(&path, &config).unwrap();
    let loaded = load_config(&path).unwrap();
    fs::remove_file(path).unwrap();

    assert_eq!(
        loaded.general.start_with_windows,
        config.general.start_with_windows
    );
}
