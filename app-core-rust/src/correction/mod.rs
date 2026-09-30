//! Stable contract between correction request producers and correction engines.
//!
//! Informative context is read-only. Engines may return edits only for
//! `CorrectionInput::executable_context`.

mod api;
mod confidence;
mod contract;
mod engines;
pub(crate) mod language;
mod local_rule;
mod mixed_language;

pub(crate) use api::valid_base_url;
pub use api::{ApiCorrectionEngine, ApiEngineConfig, ApiNotice};
pub use contract::{
    ConfidenceBehavior, ConfidenceBehaviorSettings, ConfidenceTier, CorrectionChange,
    CorrectionChangeKind, CorrectionInput, CorrectionMode, CorrectionOutput, EngineBackend,
    EngineFailure, EngineFailureKind, EngineKind, EngineStatus, GrammarCategory, LanguageInfo,
    MixedLanguagePolicy, NoChangeReason, TriggerType, UncertainLanguagePolicy,
};
pub use engines::{
    CorrectionEngine, CorrectionEngines, CustomApiEngine, LocalMlEngine, LocalRuleEngine,
    OpenAiCompatibleApiEngine,
};

#[cfg(test)]
mod tests;
