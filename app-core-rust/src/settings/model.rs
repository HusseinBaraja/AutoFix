use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct AppConfig {
    pub(crate) general: GeneralConfig,
    pub(crate) shortcuts: ShortcutsConfig,
    pub(crate) triggers: TriggersConfig,
    pub(crate) context: ContextConfig,
    pub(crate) correction: CorrectionConfig,
    pub(crate) api: ApiConfig,
    pub(crate) feedback: FeedbackConfig,
    pub(crate) logging: LoggingConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            general: GeneralConfig::default(),
            shortcuts: ShortcutsConfig::default(),
            triggers: TriggersConfig::default(),
            context: ContextConfig::default(),
            correction: CorrectionConfig::default(),
            api: ApiConfig::default(),
            feedback: FeedbackConfig::default(),
            logging: LoggingConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct GeneralConfig {
    pub(crate) start_with_windows: bool,
    pub(crate) run_mode: RunMode,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            start_with_windows: false,
            run_mode: RunMode::Blocklist,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RunMode {
    Blocklist,
    Allowlist,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ShortcutsConfig {
    pub(crate) correct: String,
    pub(crate) undo: String,
}

impl Default for ShortcutsConfig {
    fn default() -> Self {
        Self {
            correct: "Ctrl+Alt+Space".to_owned(),
            undo: "Ctrl+Alt+Z".to_owned(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct TriggersConfig {
    pub(crate) word_count_enabled: bool,
    pub(crate) word_count: u16,
    pub(crate) character_trigger_enabled: bool,
    pub(crate) characters: Vec<String>,
}

impl Default for TriggersConfig {
    fn default() -> Self {
        Self {
            word_count_enabled: true,
            word_count: 10,
            character_trigger_enabled: true,
            characters: vec![".".to_owned()],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ContextConfig {
    pub(crate) initial_context_words: u16,
    pub(crate) initial_context_boundary_chars: Vec<String>,
    pub(crate) forward_movement_word_limit: u16,
    pub(crate) informative_context_max_chars: u32,
    pub(crate) informative_context_min_words: u16,
    pub(crate) executable_context_max_words: u16,
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            initial_context_words: 25,
            initial_context_boundary_chars: vec![".".to_owned()],
            forward_movement_word_limit: 5,
            informative_context_max_chars: 2_000,
            informative_context_min_words: 25,
            executable_context_max_words: 80,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct CorrectionConfig {
    pub(crate) mode: CorrectionMode,
    pub(crate) engine: CorrectionEngine,
    pub(crate) high_confidence_behavior: ConfidenceBehavior,
    pub(crate) medium_confidence_behavior: ConfidenceBehavior,
    pub(crate) low_confidence_behavior: ConfidenceBehavior,
    pub(crate) enabled_grammar_categories: Vec<GrammarCategory>,
}

impl Default for CorrectionConfig {
    fn default() -> Self {
        Self {
            mode: CorrectionMode::TyposOnly,
            engine: CorrectionEngine::Local,
            high_confidence_behavior: ConfidenceBehavior::Silent,
            medium_confidence_behavior: ConfidenceBehavior::Suggestion,
            low_confidence_behavior: ConfidenceBehavior::DoNothing,
            enabled_grammar_categories: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CorrectionMode {
    TyposOnly,
    TyposPlusGrammar,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CorrectionEngine {
    Local,
    Api,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ConfidenceBehavior {
    DoNothing,
    Suggestion,
    Silent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum GrammarCategory {
    Agreement,
    Capitalization,
    Clarity,
    Punctuation,
    Tense,
    WordOrder,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct ApiConfig {
    pub(crate) provider_preset: String,
    pub(crate) base_url: Option<String>,
    pub(crate) model: String,
    pub(crate) timeout_manual_ms: u64,
    pub(crate) timeout_auto_ms: u64,
    pub(crate) retry_count: u8,
    pub(crate) fallback_to_local: bool,
    pub(crate) temperature: f32,
    pub(crate) streaming: bool,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            provider_preset: "openai_compatible".to_owned(),
            base_url: None,
            model: "gpt-4.1-mini".to_owned(),
            timeout_manual_ms: 3_000,
            timeout_auto_ms: 700,
            retry_count: 1,
            fallback_to_local: true,
            temperature: 0.0,
            streaming: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct FeedbackConfig {
    pub(crate) tray_state_enabled: bool,
    pub(crate) show_correction_applied_notification: bool,
    pub(crate) show_skipped_reason: bool,
    pub(crate) show_medium_confidence_suggestions: bool,
    pub(crate) show_blocked_app_notice: bool,
    pub(crate) show_timeout_notice: bool,
}

impl Default for FeedbackConfig {
    fn default() -> Self {
        Self {
            tray_state_enabled: true,
            show_correction_applied_notification: false,
            show_skipped_reason: true,
            show_medium_confidence_suggestions: true,
            show_blocked_app_notice: true,
            show_timeout_notice: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct LoggingConfig {
    pub(crate) metadata_only_logs_enabled: bool,
    pub(crate) debug_mode_enabled: bool,
    pub(crate) redacted_debug_mode_enabled: bool,
    pub(crate) full_text_debug_mode_enabled: bool,
    pub(crate) log_retention_days: Option<u16>,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            metadata_only_logs_enabled: true,
            debug_mode_enabled: false,
            redacted_debug_mode_enabled: false,
            full_text_debug_mode_enabled: false,
            log_retention_days: None,
        }
    }
}
