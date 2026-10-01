use serde::{Deserialize, Serialize};

use crate::correction::{
    ConfidenceBehavior, ConfidenceBehaviorSettings, CorrectionMode, GrammarCategory,
    MixedLanguagePolicy, UncertainLanguagePolicy,
};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub(crate) struct AppConfig {
    #[serde(default)]
    pub(crate) onboarding: OnboardingConfig,
    pub(crate) general: GeneralConfig,
    pub(crate) shortcuts: ShortcutsConfig,
    pub(crate) triggers: TriggersConfig,
    pub(crate) context: ContextConfig,
    pub(crate) correction: CorrectionConfig,
    #[serde(default)]
    pub(crate) replacement: ReplacementConfig,
    #[serde(default)]
    pub(crate) learning: LearningConfig,
    pub(crate) api: ApiConfig,
    pub(crate) feedback: FeedbackConfig,
    pub(crate) logging: LoggingConfig,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct LearningConfig {
    #[serde(default)]
    pub(crate) mode: LearningMode,
    #[serde(default)]
    pub(crate) rule: LearningRule,
    #[serde(default)]
    pub(crate) per_app: bool,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LearningMode {
    #[default]
    Off,
    Ask,
    Automatic,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LearningRule {
    Dictionary,
    #[default]
    Pair,
}

impl AppConfig {
    /// Snapshot confidence settings, honoring the user's suggestion preference.
    pub(crate) fn confidence_behavior(&self) -> ConfidenceBehaviorSettings {
        let medium = self.correction.medium_confidence_behavior;
        ConfidenceBehaviorSettings {
            high: self.correction.high_confidence_behavior,
            medium: if medium == ConfidenceBehavior::Suggestion
                && !self.feedback.show_medium_confidence_suggestions
            {
                ConfidenceBehavior::DoNothing
            } else {
                medium
            },
            low: ConfidenceBehavior::DoNothing,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct OnboardingConfig {
    pub(crate) completed: bool,
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
    #[serde(default)]
    pub(crate) correct_arbitrary_selection: bool,
}

impl Default for ShortcutsConfig {
    fn default() -> Self {
        Self {
            correct: "Ctrl+Alt+Space".to_owned(),
            undo: "Ctrl+Alt+Z".to_owned(),
            correct_arbitrary_selection: false,
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
    #[serde(default = "default_undo_history_size")]
    pub(crate) undo_history_size: u16,
    #[serde(default = "default_pending_queue_size")]
    pub(crate) pending_queue_size: u16,
    #[serde(default)]
    pub(crate) pending_queue_full_behavior: PendingQueueFullBehavior,
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
            undo_history_size: default_undo_history_size(),
            pending_queue_size: 1,
            pending_queue_full_behavior: PendingQueueFullBehavior::SkipNew,
            initial_context_words: 25,
            initial_context_boundary_chars: vec![".".to_owned()],
            forward_movement_word_limit: 5,
            informative_context_max_chars: 2_000,
            informative_context_min_words: 25,
            executable_context_max_words: 80,
        }
    }
}

fn default_undo_history_size() -> u16 {
    10
}

fn default_pending_queue_size() -> u16 {
    1
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PendingQueueFullBehavior {
    #[default]
    SkipNew,
    CancelOldest,
    MergeNewest,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct CorrectionConfig {
    #[serde(default = "default_true")]
    pub(crate) enabled: bool,
    pub(crate) mode: CorrectionMode,
    pub(crate) engine: CorrectionEngine,
    pub(crate) high_confidence_behavior: ConfidenceBehavior,
    pub(crate) medium_confidence_behavior: ConfidenceBehavior,
    pub(crate) low_confidence_behavior: ConfidenceBehavior,
    pub(crate) enabled_grammar_categories: Vec<GrammarCategory>,
    #[serde(default)]
    pub(crate) preferred_language: Option<String>,
    #[serde(default)]
    pub(crate) app_language_overrides: Vec<String>,
    #[serde(default)]
    pub(crate) uncertain_language_policy: UncertainLanguagePolicy,
    #[serde(default)]
    pub(crate) mixed_language_policy: MixedLanguagePolicy,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ReplacementConfig {
    #[serde(default = "default_true")]
    pub(crate) clipboard_enabled: bool,
}

impl Default for ReplacementConfig {
    fn default() -> Self {
        Self {
            clipboard_enabled: true,
        }
    }
}

impl Default for CorrectionConfig {
    /// Starts with local typos, conservative language policies, and low confidence blocked.
    fn default() -> Self {
        Self {
            enabled: true,
            mode: CorrectionMode::TyposOnly,
            engine: CorrectionEngine::Local,
            high_confidence_behavior: ConfidenceBehavior::Silent,
            medium_confidence_behavior: ConfidenceBehavior::Suggestion,
            low_confidence_behavior: ConfidenceBehavior::DoNothing,
            enabled_grammar_categories: Vec::new(),
            preferred_language: None,
            app_language_overrides: Vec::new(),
            uncertain_language_policy: UncertainLanguagePolicy::default(),
            mixed_language_policy: MixedLanguagePolicy::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CorrectionEngine {
    Local,
    Api,
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
            fallback_to_local: false,
            temperature: 0.0,
            streaming: false,
        }
    }
}

impl From<&ApiConfig> for crate::correction::ApiEngineConfig {
    fn from(config: &ApiConfig) -> Self {
        Self {
            provider_preset: config.provider_preset.clone(),
            base_url: config.base_url.clone(),
            model: config.model.clone(),
            timeout_manual_ms: config.timeout_manual_ms,
            timeout_auto_ms: config.timeout_auto_ms,
            retry_count: config.retry_count,
            fallback_to_local: config.fallback_to_local,
            temperature: config.temperature,
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
