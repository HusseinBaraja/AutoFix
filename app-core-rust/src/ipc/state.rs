use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use crate::{
    settings::{save_config, AppConfig, CorrectionEngine, CorrectionMode, ValidateConfig},
    storage::{AppRule, Database},
};

use super::protocol::{
    AppRuleDeletedResponse, AppRuleRequest, AppRuleUpdatedResponse, AppRulesResponse,
    AppStatusResponse, BackgroundRunningResponse, CorrectionEngineResponse, CorrectionModeResponse,
    IpcCorrectionEngine, IpcCorrectionMode, IpcRequest, IpcResponse, LogsResponse,
    SettingUpdatedResponse, ShutdownAcceptedResponse, TestCorrectionEngineResponse, UndoResponse,
};

pub(crate) struct IpcServerState {
    config_path: PathBuf,
    database_path: PathBuf,
    log_directory: PathBuf,
    config: AppConfig,
    shutdown_requested: Arc<AtomicBool>,
}

impl IpcServerState {
    pub(crate) fn new(
        config_path: PathBuf,
        database_path: PathBuf,
        log_directory: PathBuf,
        config: AppConfig,
        shutdown_requested: Arc<AtomicBool>,
    ) -> Self {
        Self {
            config_path,
            database_path,
            log_directory,
            config,
            shutdown_requested,
        }
    }

    pub(crate) fn handle(&mut self, request: IpcRequest) -> IpcResponse {
        match request {
            IpcRequest::GetAppStatus => IpcResponse::AppStatus(self.status()),
            IpcRequest::GetCorrectionMode => IpcResponse::CorrectionMode(CorrectionModeResponse {
                mode: self.config.correction.mode.clone().into(),
            }),
            IpcRequest::GetCurrentEngine => IpcResponse::CurrentEngine(CorrectionEngineResponse {
                engine: self.config.correction.engine.clone().into(),
            }),
            IpcRequest::ReloadConfig => self.reload_config(),
            IpcRequest::UpdateSetting(update) => self.update_setting(update.path, update.value),
            IpcRequest::ListAppRules => self.list_app_rules(),
            IpcRequest::UpsertAppRule(rule) => self.upsert_app_rule(rule),
            IpcRequest::DeleteAppRule(rule) => self.delete_app_rule(rule),
            IpcRequest::ResetAppRules => self.reset_app_rules(),
            IpcRequest::OpenLogs => IpcResponse::Logs(LogsResponse {
                log_directory: self.log_directory.display().to_string(),
                opened: false,
            }),
            IpcRequest::RequestUndoLastCorrection => IpcResponse::UndoRequested(UndoResponse {
                accepted: true,
                message: "undo requested".to_owned(),
            }),
            IpcRequest::TestCorrectionEngineLater => {
                IpcResponse::TestCorrectionEngineQueued(TestCorrectionEngineResponse {
                    accepted: true,
                    message: "engine test queued".to_owned(),
                })
            }
            IpcRequest::IsBackgroundRunning => {
                IpcResponse::BackgroundRunning(BackgroundRunningResponse { running: true })
            }
            IpcRequest::ShutdownAll => {
                self.shutdown_requested.store(true, Ordering::Relaxed);
                IpcResponse::ShutdownAccepted(ShutdownAcceptedResponse { accepted: true })
            }
        }
    }

    fn list_app_rules(&self) -> IpcResponse {
        self.with_database(|database| {
            database
                .app_rules()
                .list()
                .map(|rules| {
                    IpcResponse::AppRules(AppRulesResponse {
                        rules: rules.into_iter().map(AppRuleRequest::from).collect(),
                    })
                })
                .map_err(|error| error.to_string())
        })
    }

    fn upsert_app_rule(&self, rule: AppRuleRequest) -> IpcResponse {
        if let Err(error) = validate_app_rule_request(&rule) {
            return IpcResponse::error(error);
        }

        let response = AppRuleUpdatedResponse {
            process_name: rule.process_name.clone(),
            window_title_pattern: rule.window_title_pattern.clone(),
        };
        self.with_database(|database| {
            database
                .app_rules()
                .upsert(&rule.into())
                .map(|_| IpcResponse::AppRuleUpdated(response))
                .map_err(|error| error.to_string())
        })
    }

    fn delete_app_rule(&self, rule: super::protocol::DeleteAppRuleRequest) -> IpcResponse {
        self.with_database(|database| {
            database
                .app_rules()
                .delete(&rule.process_name, rule.window_title_pattern.as_deref())
                .map(|deleted| IpcResponse::AppRuleDeleted(AppRuleDeletedResponse { deleted }))
                .map_err(|error| error.to_string())
        })
    }

    fn reset_app_rules(&self) -> IpcResponse {
        self.with_database(|database| {
            database
                .app_rules()
                .reset_to_defaults()
                .and_then(|_| database.app_rules().list())
                .map(|rules| {
                    IpcResponse::AppRulesReset(AppRulesResponse {
                        rules: rules.into_iter().map(AppRuleRequest::from).collect(),
                    })
                })
                .map_err(|error| error.to_string())
        })
    }

    fn with_database(
        &self,
        action: impl FnOnce(&Database) -> Result<IpcResponse, String>,
    ) -> IpcResponse {
        match Database::open(&self.database_path).map_err(|error| error.to_string()) {
            Ok(database) => action(&database).unwrap_or_else(IpcResponse::error),
            Err(error) => IpcResponse::error(error),
        }
    }

    fn status(&self) -> AppStatusResponse {
        AppStatusResponse {
            running: true,
            correction_mode: self.config.correction.mode.clone().into(),
            engine: self.config.correction.engine.clone().into(),
        }
    }

    fn reload_config(&mut self) -> IpcResponse {
        match crate::settings::load_config(&self.config_path) {
            Ok(config) => {
                self.config = config;
                IpcResponse::ConfigReloaded(self.status())
            }
            Err(error) => IpcResponse::error(error.to_string()),
        }
    }

    fn update_setting(&mut self, path: String, value: serde_json::Value) -> IpcResponse {
        let mut next = self.config.clone();
        let result = apply_setting(&mut next, &path, value)
            .and_then(|_| next.validate().map_err(|error| error.to_string()))
            .and_then(|_| save_config(&self.config_path, &next).map_err(|error| error.to_string()));

        match result {
            Ok(()) => {
                self.config = next;
                IpcResponse::SettingUpdated(SettingUpdatedResponse { path })
            }
            Err(error) => IpcResponse::error(error),
        }
    }
}

fn validate_app_rule_request(rule: &AppRuleRequest) -> Result<(), String> {
    if rule.process_name.trim().is_empty() {
        return Err("process_name must not be empty".to_owned());
    }
    if !matches!(rule.list_behavior.as_str(), "allowlist" | "blocklist") {
        return Err("list_behavior must be allowlist or blocklist".to_owned());
    }
    Ok(())
}

impl From<AppRule> for AppRuleRequest {
    fn from(value: AppRule) -> Self {
        Self {
            process_name: value.process_name,
            window_title_pattern: value.window_title_pattern,
            list_behavior: value.list_behavior,
            manual_shortcut_allowed: value.manual_shortcut_allowed,
            word_count_trigger_allowed: value.word_count_trigger_allowed,
            character_trigger_allowed: value.character_trigger_allowed,
            local_engine_allowed: value.local_engine_allowed,
            api_engine_allowed: value.api_engine_allowed,
        }
    }
}

impl From<AppRuleRequest> for AppRule {
    fn from(value: AppRuleRequest) -> Self {
        Self {
            process_name: value.process_name.trim().to_owned(),
            window_title_pattern: value.window_title_pattern.and_then(|pattern| {
                (!pattern.trim().is_empty()).then(|| pattern.trim().to_owned())
            }),
            list_behavior: value.list_behavior,
            manual_shortcut_allowed: value.manual_shortcut_allowed,
            word_count_trigger_allowed: value.word_count_trigger_allowed,
            character_trigger_allowed: value.character_trigger_allowed,
            local_engine_allowed: value.local_engine_allowed,
            api_engine_allowed: value.api_engine_allowed,
        }
    }
}

fn apply_setting(
    config: &mut AppConfig,
    path: &str,
    value: serde_json::Value,
) -> Result<(), String> {
    let mut document = serde_json::to_value(&*config).map_err(|error| error.to_string())?;
    replace_path_value(&mut document, path, value)?;
    *config = serde_json::from_value(document).map_err(|error| error.to_string())?;

    Ok(())
}

fn replace_path_value(
    document: &mut serde_json::Value,
    path: &str,
    value: serde_json::Value,
) -> Result<(), String> {
    let segments = path.split('.').collect::<Vec<_>>();
    if segments.is_empty() || segments.iter().any(|segment| segment.is_empty()) {
        return Err("setting path must not be empty".to_owned());
    }

    let mut current = document;
    for segment in &segments[..segments.len() - 1] {
        current = current
            .get_mut(*segment)
            .ok_or_else(|| format!("unknown setting path: {path}"))?;
    }

    let leaf = segments.last().expect("path has at least one segment");
    let object = current
        .as_object_mut()
        .ok_or_else(|| format!("setting path is not editable: {path}"))?;
    if !object.contains_key(*leaf) {
        return Err(format!("unknown setting path: {path}"));
    }

    object.insert((*leaf).to_owned(), value);
    Ok(())
}

impl From<CorrectionMode> for IpcCorrectionMode {
    fn from(value: CorrectionMode) -> Self {
        match value {
            CorrectionMode::TyposOnly => Self::TyposOnly,
            CorrectionMode::TyposPlusGrammar => Self::TyposPlusGrammar,
        }
    }
}

impl From<CorrectionEngine> for IpcCorrectionEngine {
    fn from(value: CorrectionEngine) -> Self {
        match value {
            CorrectionEngine::Local => Self::Local,
            CorrectionEngine::Api => Self::Api,
        }
    }
}
