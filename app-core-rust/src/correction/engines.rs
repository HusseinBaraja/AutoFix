use super::api::ApiCorrectionEngine;
use super::local_rule;
use super::{CorrectionInput, CorrectionOutput, EngineFailure, EngineFailureKind, EngineKind};

/// Common interface for local and API-backed correction engines.
///
/// Engine selection is explicit for every task. Implementations must accept
/// both correction modes; callers must not route by perceived task difficulty.
pub trait CorrectionEngine: Send + Sync {
    fn kind(&self) -> EngineKind;
    fn correct(&self, input: &CorrectionInput) -> CorrectionOutput;
}

#[derive(Debug, Default)]
pub struct LocalRuleEngine;

#[derive(Debug, Default)]
pub struct LocalMlEngine;

pub struct OpenAiCompatibleApiEngine(ApiCorrectionEngine);
pub struct CustomApiEngine(ApiCorrectionEngine);

impl Default for OpenAiCompatibleApiEngine {
    fn default() -> Self {
        Self(ApiCorrectionEngine::unconfigured(
            EngineKind::OpenAiCompatibleApi,
        ))
    }
}

impl Default for CustomApiEngine {
    fn default() -> Self {
        Self(ApiCorrectionEngine::unconfigured(EngineKind::CustomApi))
    }
}

impl OpenAiCompatibleApiEngine {
    pub fn new(config: super::ApiEngineConfig) -> Self {
        Self(ApiCorrectionEngine::new(
            EngineKind::OpenAiCompatibleApi,
            config,
        ))
    }
}

impl CustomApiEngine {
    pub fn new(config: super::ApiEngineConfig) -> Self {
        Self(ApiCorrectionEngine::new(EngineKind::CustomApi, config))
    }
}

impl CorrectionEngine for OpenAiCompatibleApiEngine {
    fn kind(&self) -> EngineKind {
        self.0.kind()
    }
    fn correct(&self, input: &CorrectionInput) -> CorrectionOutput {
        self.0.correct(input)
    }
}

impl CorrectionEngine for CustomApiEngine {
    fn kind(&self) -> EngineKind {
        self.0.kind()
    }
    fn correct(&self, input: &CorrectionInput) -> CorrectionOutput {
        self.0.correct(input)
    }
}

impl CorrectionEngine for LocalRuleEngine {
    fn kind(&self) -> EngineKind {
        EngineKind::LocalRule
    }

    fn correct(&self, input: &CorrectionInput) -> CorrectionOutput {
        local_rule::correct(input)
    }
}

macro_rules! placeholder_engine {
    ($engine:ty, $kind:expr) => {
        impl CorrectionEngine for $engine {
            fn kind(&self) -> EngineKind {
                $kind
            }

            fn correct(&self, input: &CorrectionInput) -> CorrectionOutput {
                CorrectionOutput::failed(
                    input.executable_context.clone(),
                    EngineFailure {
                        kind: EngineFailureKind::NotImplemented,
                        message: format!("{:?} is not implemented", self.kind()),
                        retryable: false,
                    },
                    0,
                )
            }
        }
    };
}

placeholder_engine!(LocalMlEngine, EngineKind::LocalMl);

/// Fixed engine registry. The caller supplies the engine for every task.
pub struct CorrectionEngines {
    local_rule: LocalRuleEngine,
    local_ml: LocalMlEngine,
    open_ai_compatible_api: OpenAiCompatibleApiEngine,
    custom_api: CustomApiEngine,
}

impl Default for CorrectionEngines {
    fn default() -> Self {
        Self {
            local_rule: LocalRuleEngine,
            local_ml: LocalMlEngine,
            open_ai_compatible_api: OpenAiCompatibleApiEngine::default(),
            custom_api: CustomApiEngine::default(),
        }
    }
}

impl CorrectionEngines {
    pub fn with_api_config(config: super::api::ApiEngineConfig) -> Self {
        Self {
            open_ai_compatible_api: OpenAiCompatibleApiEngine::new(config.clone()),
            custom_api: CustomApiEngine::new(config),
            ..Self::default()
        }
    }
    pub fn engine(&self, kind: EngineKind) -> &dyn CorrectionEngine {
        match kind {
            EngineKind::LocalRule => &self.local_rule,
            EngineKind::LocalMl => &self.local_ml,
            EngineKind::OpenAiCompatibleApi => &self.open_ai_compatible_api,
            EngineKind::CustomApi => &self.custom_api,
        }
    }

    pub fn correct_with(&self, kind: EngineKind, input: &CorrectionInput) -> CorrectionOutput {
        self.engine(kind).correct(input)
    }
}
