use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CorrectionMode {
    TyposOnly,
    TyposPlusGrammar,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GrammarCategory {
    Agreement,
    Capitalization,
    Clarity,
    Punctuation,
    Tense,
    WordOrder,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConfidenceBehavior {
    DoNothing,
    Suggestion,
    Silent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConfidenceBehaviorSettings {
    pub high: ConfidenceBehavior,
    pub medium: ConfidenceBehavior,
    pub low: ConfidenceBehavior,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LanguageInfo {
    /// User-selected or otherwise preferred BCP 47 language tag, when known.
    pub primary_language: Option<String>,
    /// BCP 47 language tags detected in the executable span.
    pub detected_languages: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MixedLanguagePolicy {
    /// Preserve words that do not belong to the primary language.
    PreserveNonPrimary,
    /// Correct each detected language using the same correction mode.
    CorrectEachDetectedLanguage,
    /// Correct only the primary language and leave other spans unchanged.
    PrimaryLanguageOnly,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TriggerType {
    ManualShortcut,
    WordCount,
    Character,
    FinalFixBeforeReanchor,
}

/// Complete input for one correction task.
///
/// `informative_context` can guide a correction but is never editable. Every
/// returned change range is relative to `executable_context` only.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CorrectionInput {
    pub informative_context: String,
    pub executable_context: String,
    pub mode: CorrectionMode,
    /// Empty means every grammar category is disabled. This must also be empty
    /// in typos-only mode.
    pub enabled_grammar_categories: Vec<GrammarCategory>,
    pub language_info: LanguageInfo,
    pub mixed_language_policy: MixedLanguagePolicy,
    /// Accepted spellings which engines may use as correction candidates.
    pub custom_dictionary: Vec<String>,
    /// Exact terms which engines must not modify.
    pub protected_terms: Vec<String>,
    pub trigger_type: TriggerType,
    pub confidence_behavior: ConfidenceBehaviorSettings,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EngineBackend {
    Local,
    Api,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EngineKind {
    LocalRule,
    LocalMl,
    OpenAiCompatibleApi,
    CustomApi,
}

impl EngineKind {
    pub const fn backend(self) -> EngineBackend {
        match self {
            Self::LocalRule | Self::LocalMl => EngineBackend::Local,
            Self::OpenAiCompatibleApi | Self::CustomApi => EngineBackend::Api,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConfidenceTier {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CorrectionChangeKind {
    Typo,
    Grammar(GrammarCategory),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CorrectionChange {
    /// Unicode scalar-value offset in the executable input.
    pub start_char: usize,
    /// Exclusive Unicode scalar-value offset in the executable input.
    pub end_char: usize,
    pub original_text: String,
    pub replacement_text: String,
    pub kind: CorrectionChangeKind,
    pub explanation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NoChangeReason {
    NoCorrectionNeeded,
    ConfidenceBelowConfiguredBehavior,
    AllCandidatesProtected,
    UnsupportedLanguage,
    EngineUnavailable,
    TimedOut,
    EngineError,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EngineFailureKind {
    NotImplemented,
    InvalidInput,
    Authentication,
    RateLimited,
    Transport,
    InvalidResponse,
    Internal,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EngineFailure {
    pub kind: EngineFailureKind,
    pub message: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EngineStatus {
    Completed,
    TimedOut,
    Error(EngineFailure),
}

/// Complete result for one correction task, including failures and timeouts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CorrectionOutput {
    pub corrected_executable_text: String,
    pub changes_needed: bool,
    pub confidence: ConfidenceTier,
    /// `None` means the engine cannot provide structured change details.
    pub changes: Option<Vec<CorrectionChange>>,
    /// Present whenever `changes_needed` is false.
    pub no_change_reason: Option<NoChangeReason>,
    pub engine_latency_ms: u64,
    pub status: EngineStatus,
}

impl CorrectionOutput {
    pub fn changed(
        corrected_executable_text: String,
        confidence: ConfidenceTier,
        changes: Option<Vec<CorrectionChange>>,
        engine_latency_ms: u64,
    ) -> Self {
        Self {
            corrected_executable_text,
            changes_needed: true,
            confidence,
            changes,
            no_change_reason: None,
            engine_latency_ms,
            status: EngineStatus::Completed,
        }
    }

    pub fn unchanged(
        executable_text: String,
        confidence: ConfidenceTier,
        reason: NoChangeReason,
        engine_latency_ms: u64,
    ) -> Self {
        Self {
            corrected_executable_text: executable_text,
            changes_needed: false,
            confidence,
            changes: Some(Vec::new()),
            no_change_reason: Some(reason),
            engine_latency_ms,
            status: EngineStatus::Completed,
        }
    }

    pub fn timed_out(executable_text: String, engine_latency_ms: u64) -> Self {
        Self {
            corrected_executable_text: executable_text,
            changes_needed: false,
            confidence: ConfidenceTier::Low,
            changes: None,
            no_change_reason: Some(NoChangeReason::TimedOut),
            engine_latency_ms,
            status: EngineStatus::TimedOut,
        }
    }

    pub fn failed(executable_text: String, failure: EngineFailure, engine_latency_ms: u64) -> Self {
        Self {
            corrected_executable_text: executable_text,
            changes_needed: false,
            confidence: ConfidenceTier::Low,
            changes: None,
            no_change_reason: Some(NoChangeReason::EngineError),
            engine_latency_ms,
            status: EngineStatus::Error(failure),
        }
    }
}
