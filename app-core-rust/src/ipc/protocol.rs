use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum IpcCorrectionMode {
    TyposOnly,
    TyposPlusGrammar,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum IpcCorrectionEngine {
    Local,
    Api,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub(crate) enum IpcRequest {
    GetAppStatus,
    GetCorrectionMode,
    GetCurrentEngine,
    ReloadConfig,
    UpdateSetting(UpdateSettingRequest),
    ListAppRules,
    UpsertAppRule(AppRuleRequest),
    DeleteAppRule(DeleteAppRuleRequest),
    ResetAppRules,
    OpenLogs,
    RequestUndoLastCorrection,
    TestCorrectionEngineLater,
    IsBackgroundRunning,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct UpdateSettingRequest {
    pub(crate) path: String,
    pub(crate) value: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct AppRuleRequest {
    pub(crate) process_name: String,
    pub(crate) window_title_pattern: Option<String>,
    pub(crate) list_behavior: String,
    pub(crate) manual_shortcut_allowed: bool,
    pub(crate) word_count_trigger_allowed: bool,
    pub(crate) character_trigger_allowed: bool,
    pub(crate) local_engine_allowed: bool,
    pub(crate) api_engine_allowed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct DeleteAppRuleRequest {
    pub(crate) process_name: String,
    pub(crate) window_title_pattern: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub(crate) enum IpcResponse {
    AppStatus(AppStatusResponse),
    CorrectionMode(CorrectionModeResponse),
    CurrentEngine(CorrectionEngineResponse),
    ConfigReloaded(AppStatusResponse),
    SettingUpdated(SettingUpdatedResponse),
    AppRules(AppRulesResponse),
    AppRuleUpdated(AppRuleUpdatedResponse),
    AppRuleDeleted(AppRuleDeletedResponse),
    AppRulesReset(AppRulesResponse),
    Logs(LogsResponse),
    UndoRequested(UndoResponse),
    TestCorrectionEngineQueued(TestCorrectionEngineResponse),
    BackgroundRunning(BackgroundRunningResponse),
    Error(IpcErrorResponse),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct AppStatusResponse {
    pub(crate) running: bool,
    pub(crate) correction_mode: IpcCorrectionMode,
    pub(crate) engine: IpcCorrectionEngine,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct CorrectionModeResponse {
    pub(crate) mode: IpcCorrectionMode,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct CorrectionEngineResponse {
    pub(crate) engine: IpcCorrectionEngine,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct SettingUpdatedResponse {
    pub(crate) path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct AppRulesResponse {
    pub(crate) rules: Vec<AppRuleRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct AppRuleUpdatedResponse {
    pub(crate) process_name: String,
    pub(crate) window_title_pattern: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct AppRuleDeletedResponse {
    pub(crate) deleted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct LogsResponse {
    pub(crate) log_directory: String,
    pub(crate) opened: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct UndoResponse {
    pub(crate) accepted: bool,
    pub(crate) message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct TestCorrectionEngineResponse {
    pub(crate) accepted: bool,
    pub(crate) message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct BackgroundRunningResponse {
    pub(crate) running: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct IpcErrorResponse {
    pub(crate) message: String,
}

impl IpcResponse {
    pub(crate) fn error(message: impl Into<String>) -> Self {
        Self::Error(IpcErrorResponse {
            message: message.into(),
        })
    }
}
