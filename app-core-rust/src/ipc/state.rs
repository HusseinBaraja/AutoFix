use std::path::PathBuf;

use crate::settings::{save_config, AppConfig, CorrectionEngine, CorrectionMode, ValidateConfig};

use super::protocol::{
    AppStatusResponse, BackgroundRunningResponse, CorrectionEngineResponse, CorrectionModeResponse,
    IpcCorrectionEngine, IpcCorrectionMode, IpcRequest, IpcResponse, LogsResponse,
    SettingUpdatedResponse, TestCorrectionEngineResponse, UndoResponse,
};

pub(crate) struct IpcServerState {
    config_path: PathBuf,
    log_directory: PathBuf,
    config: AppConfig,
}

impl IpcServerState {
    pub(crate) fn new(config_path: PathBuf, log_directory: PathBuf, config: AppConfig) -> Self {
        Self {
            config_path,
            log_directory,
            config,
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

fn apply_setting(
    config: &mut AppConfig,
    path: &str,
    value: serde_json::Value,
) -> Result<(), String> {
    match path {
        "correction.mode" => {
            config.correction.mode = serde_json::from_value(value).map_err(|e| e.to_string())?
        }
        "correction.engine" => {
            config.correction.engine = serde_json::from_value(value).map_err(|e| e.to_string())?
        }
        _ => return Err(format!("unsupported setting path: {path}")),
    }

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
