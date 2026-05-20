#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AppRule {
    pub(crate) process_name: String,
    pub(crate) window_title_pattern: Option<String>,
    pub(crate) list_behavior: String,
    pub(crate) manual_shortcut_allowed: bool,
    pub(crate) word_count_trigger_allowed: bool,
    pub(crate) character_trigger_allowed: bool,
    pub(crate) local_engine_allowed: bool,
    pub(crate) api_engine_allowed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CustomDictionaryEntry {
    pub(crate) language_code: String,
    pub(crate) app_process_name: Option<String>,
    pub(crate) entry: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LearnedCorrectionRule {
    pub(crate) learning_enabled: bool,
    pub(crate) original_text: String,
    pub(crate) rejected_correction: Option<String>,
    pub(crate) rule_type: String,
    pub(crate) language_code: Option<String>,
    pub(crate) app_process_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LanguageOverride {
    pub(crate) app_process_name: String,
    pub(crate) language_code: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CorrectionMetadata {
    pub(crate) session_id: String,
    pub(crate) app_process_name: String,
    pub(crate) trigger_type: String,
    pub(crate) confidence_tier: String,
    pub(crate) engine_used: String,
    pub(crate) replacement_method: String,
    pub(crate) result_reason: String,
    pub(crate) latency_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DebugEvent {
    pub(crate) session_id: Option<String>,
    pub(crate) event_type: String,
    pub(crate) severity: String,
    pub(crate) message: String,
    pub(crate) mode: DebugLogMode,
    pub(crate) payload: DebugPayload,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DebugLogMode {
    Off,
    Redacted,
    FullText,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DebugPayload {
    None,
    Redacted { label: String },
    FullText { typed_text: String },
}
