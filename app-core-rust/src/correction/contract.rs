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
    Tense,
    WordOrder,
    MissingPunctuation,
    ExtraPunctuation,
    RepeatedWords,
    Articles,
    Prepositions,
    #[serde(alias = "punctuation")]
    Spacing,
    Apostrophes,
    Homophones,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConfidenceBehavior {
    #[default]
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

impl Default for ConfidenceBehaviorSettings {
    fn default() -> Self {
        Self {
            high: ConfidenceBehavior::Silent,
            medium: ConfidenceBehavior::Suggestion,
            low: ConfidenceBehavior::DoNothing,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LanguageInfo {
    /// Resolved BCP 47 language tag (app, global, then session detection).
    pub primary_language: Option<String>,
    /// BCP 47 language tags detected from informative and executable context.
    pub detected_languages: Vec<String>,
}

impl LanguageInfo {
    pub fn is_mixed(&self) -> bool {
        self.detected_languages.len() > 1
            || (self.detected_languages.len() == 1
                && self.detected_languages[0].starts_with("und-")
                && self.primary_language.as_deref().is_some_and(|primary| {
                    let detected = self.detected_languages[0].as_str();
                    let base = primary.split('-').next().unwrap_or("");
                    !primary.eq_ignore_ascii_case(detected)
                        && !matches!(
                            (base, detected),
                            ("ar", "und-Arab")
                                | ("ru", "und-Cyrl")
                                | ("hi", "und-Deva")
                                | ("zh", "und-Hani")
                        )
                }))
    }

    pub fn is_uncertain(&self) -> bool {
        if self.detected_languages.len() != 1 || self.detected_languages[0].starts_with("und-") {
            return true;
        }
        self.primary_language.as_deref().is_some_and(|primary| {
            let primary_base = primary.split('-').next().unwrap_or(primary);
            let detected_base = self.detected_languages[0].split('-').next().unwrap_or("");
            !primary_base.eq_ignore_ascii_case(detected_base)
        })
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UncertainLanguagePolicy {
    /// Apply only high-confidence typo edits to unknown or mixed text.
    #[default]
    HighConfidenceTyposOnly,
    DoNothing,
    CorrectNormally,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MixedLanguagePolicy {
    DisableCorrection,
    /// Correct only tokens in the dominant language.
    #[default]
    #[serde(alias = "preserve_non_primary", alias = "primary_language_only")]
    DominantLanguageOnly,
    /// Correct each token only when the selected engine supports this policy.
    #[serde(alias = "correct_each_detected_language")]
    PerToken,
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
    #[serde(default)]
    pub mixed_language_policy: MixedLanguagePolicy,
    #[serde(default)]
    pub uncertain_language_policy: UncertainLanguagePolicy,
    /// Accepted spellings which engines may use as correction candidates.
    pub custom_dictionary: Vec<String>,
    /// Exact terms which engines must not modify.
    pub protected_terms: Vec<String>,
    pub trigger_type: TriggerType,
    pub confidence_behavior: ConfidenceBehaviorSettings,
    /// True only when the caller can display a suggestion for this request.
    /// V1 has no suggestion UI, so omitted capabilities fail closed.
    #[serde(default)]
    pub suggestion_ui_available: bool,
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
    /// Classifies an engine as local or API backed.
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
    UncertainLanguage,
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
    /// Only `Silent` authorizes automatic replacement. `Suggestion` requires
    /// explicit user acceptance; `DoNothing` never authorizes replacement.
    #[serde(default)]
    pub behavior: ConfidenceBehavior,
    /// `None` means the engine cannot provide structured change details.
    pub changes: Option<Vec<CorrectionChange>>,
    /// Present whenever `changes_needed` is false.
    pub no_change_reason: Option<NoChangeReason>,
    pub engine_latency_ms: u64,
    pub status: EngineStatus,
}

impl CorrectionOutput {
    /// Records a completed candidate and optional structured edits. Engines
    /// must attach the confidence disposition before returning it to callers.
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
            behavior: ConfidenceBehavior::DoNothing,
            changes,
            no_change_reason: None,
            engine_latency_ms,
            status: EngineStatus::Completed,
        }
    }

    /// Records a completed request that left executable text unchanged.
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
            behavior: ConfidenceBehavior::DoNothing,
            changes: Some(Vec::new()),
            no_change_reason: Some(reason),
            engine_latency_ms,
            status: EngineStatus::Completed,
        }
    }

    /// Preserves the original executable text after a timeout.
    pub fn timed_out(executable_text: String, engine_latency_ms: u64) -> Self {
        Self {
            corrected_executable_text: executable_text,
            changes_needed: false,
            confidence: ConfidenceTier::Low,
            behavior: ConfidenceBehavior::DoNothing,
            changes: None,
            no_change_reason: Some(NoChangeReason::TimedOut),
            engine_latency_ms,
            status: EngineStatus::TimedOut,
        }
    }

    /// Preserves the original executable text and reports an engine failure.
    pub fn failed(executable_text: String, failure: EngineFailure, engine_latency_ms: u64) -> Self {
        Self {
            corrected_executable_text: executable_text,
            changes_needed: false,
            confidence: ConfidenceTier::Low,
            behavior: ConfidenceBehavior::DoNothing,
            changes: None,
            no_change_reason: Some(NoChangeReason::EngineError),
            engine_latency_ms,
            status: EngineStatus::Error(failure),
        }
    }
}
