use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::json;

use crate::{
    ipc::{
        send_request, IpcClientError, IpcRequest, IpcResponse, IpcServerState, NamedPipeIpcServer,
        UpdateSettingRequest, PIPE_NAME,
    },
    settings::{save_config, AppConfig, CorrectionEngine, CorrectionMode},
};

use super::pipe_path_for_process;

#[test]
fn reports_status_without_exposing_typed_text() {
    let fixture = IpcFixture::start();

    let response = send_request(&fixture.pipe_path, &IpcRequest::GetAppStatus).unwrap();

    match response {
        IpcResponse::AppStatus(status) => {
            assert!(status.running);
            assert_eq!(status.correction_mode, CorrectionMode::TyposOnly.into());
            assert_eq!(status.engine, CorrectionEngine::Local.into());
        }
        other => panic!("unexpected response: {other:?}"),
    }
}

#[test]
fn updates_correction_engine_setting_and_persists_config() {
    let fixture = IpcFixture::start();
    let response = send_request(
        &fixture.pipe_path,
        &IpcRequest::UpdateSetting(UpdateSettingRequest {
            path: "correction.engine".to_owned(),
            value: json!("api"),
        }),
    )
    .unwrap();

    assert!(matches!(response, IpcResponse::SettingUpdated(_)));
    let config = crate::settings::load_config(&fixture.config_path).unwrap();
    assert_eq!(config.correction.engine, CorrectionEngine::Api);
}

#[test]
fn updates_any_valid_config_setting_by_path() {
    let fixture = IpcFixture::start();
    let response = send_request(
        &fixture.pipe_path,
        &IpcRequest::UpdateSetting(UpdateSettingRequest {
            path: "shortcuts.correct".to_owned(),
            value: json!("Ctrl+Shift+Space"),
        }),
    )
    .unwrap();

    assert!(matches!(response, IpcResponse::SettingUpdated(_)));
    let config = crate::settings::load_config(&fixture.config_path).unwrap();
    assert_eq!(config.shortcuts.correct, "Ctrl+Shift+Space");
}

#[test]
fn reloads_config_from_disk() {
    let fixture = IpcFixture::start();
    let mut config = AppConfig::default();
    config.correction.mode = CorrectionMode::TyposPlusGrammar;
    save_config(&fixture.config_path, &config).unwrap();

    let response = send_request(&fixture.pipe_path, &IpcRequest::ReloadConfig).unwrap();

    match response {
        IpcResponse::ConfigReloaded(status) => {
            assert_eq!(
                status.correction_mode,
                CorrectionMode::TyposPlusGrammar.into()
            );
        }
        other => panic!("unexpected response: {other:?}"),
    }
}

#[test]
fn unavailable_pipe_returns_background_unavailable() {
    let missing_pipe = format!("{}-missing", pipe_path_for_process(PIPE_NAME));

    let error = send_request(&missing_pipe, &IpcRequest::IsBackgroundRunning).unwrap_err();

    assert!(matches!(error, IpcClientError::Unavailable));
}

struct IpcFixture {
    root: PathBuf,
    config_path: PathBuf,
    pipe_path: String,
    server: Option<NamedPipeIpcServer>,
}

impl IpcFixture {
    fn start() -> Self {
        let root = unique_temp_dir();
        fs::create_dir_all(&root).unwrap();
        let config_path = root.join("config.toml");
        save_config(&config_path, &AppConfig::default()).unwrap();
        let pipe_path = format!("{}-{}", pipe_path_for_process(PIPE_NAME), unique_suffix());
        let state =
            IpcServerState::new(config_path.clone(), root.join("logs"), AppConfig::default());
        let server = NamedPipeIpcServer::start_for_path(pipe_path.clone(), state);

        Self {
            root,
            config_path,
            pipe_path,
            server: Some(server),
        }
    }
}

impl Drop for IpcFixture {
    fn drop(&mut self) {
        if let Some(server) = self.server.take() {
            server.shutdown();
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn unique_temp_dir() -> PathBuf {
    std::env::temp_dir().join(format!("autofix-ipc-{}", unique_suffix()))
}

fn unique_suffix() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos()
}
