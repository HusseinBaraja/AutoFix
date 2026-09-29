//! Stable contract between correction request producers and correction engines.
//!
//! Informative context is read-only. Engines may return edits only for
//! `CorrectionInput::executable_context`.

mod api;
mod contract;
mod engines;
mod local_rule;

pub(crate) use api::valid_base_url;
pub use api::{ApiCorrectionEngine, ApiEngineConfig, ApiNotice};
pub use contract::{
    ConfidenceBehavior, ConfidenceBehaviorSettings, ConfidenceTier, CorrectionChange,
    CorrectionChangeKind, CorrectionInput, CorrectionMode, CorrectionOutput, EngineBackend,
    EngineFailure, EngineFailureKind, EngineKind, EngineStatus, GrammarCategory, LanguageInfo,
    MixedLanguagePolicy, NoChangeReason, TriggerType,
};
pub use engines::{
    CorrectionEngine, CorrectionEngines, CustomApiEngine, LocalMlEngine, LocalRuleEngine,
    OpenAiCompatibleApiEngine,
};

#[cfg(test)]
mod tests;
