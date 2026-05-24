use std::{
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::json;

use crate::{
    ipc::{
        send_request, AppRuleRequest, DeleteAppRuleRequest, IpcClientError, IpcRequest,
        IpcResponse, IpcServerState, NamedPipeIpcServer, UpdateSettingRequest, PIPE_NAME,
    },
    settings::{save_config, AppConfig, CorrectionEngine, CorrectionMode},
};

use super::pipe_path_for_process;

static UNIQUE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[test]
fn reports_basic_app_status() {
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
fn reads_update_setting_requests_larger_than_initial_buffer() {
    let fixture = IpcFixture::start();
    let model = "m".repeat(70 * 1024);
    let response = send_request(
        &fixture.pipe_path,
        &IpcRequest::UpdateSetting(UpdateSettingRequest {
            path: "api.model".to_owned(),
            value: json!(model),
        }),
    )
    .unwrap();

    assert!(matches!(response, IpcResponse::SettingUpdated(_)));
    let config = crate::settings::load_config(&fixture.config_path).unwrap();
    assert_eq!(config.api.model.len(), 70 * 1024);
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
fn lists_upserts_and_deletes_app_rules() {
    let fixture = IpcFixture::start();
    let rule = AppRuleRequest {
        process_name: "word.exe".to_owned(),
        window_title_pattern: Some("*admin*".to_owned()),
        list_behavior: "blocklist".to_owned(),
        manual_shortcut_allowed: false,
        word_count_trigger_allowed: false,
        character_trigger_allowed: false,
        local_engine_allowed: false,
        api_engine_allowed: false,
    };

    let upserted =
        send_request(&fixture.pipe_path, &IpcRequest::UpsertAppRule(rule.clone())).unwrap();
    assert!(matches!(upserted, IpcResponse::AppRuleUpdated(_)));

    let listed = send_request(&fixture.pipe_path, &IpcRequest::ListAppRules).unwrap();
    match listed {
        IpcResponse::AppRules(response) => assert!(response.rules.contains(&rule)),
        other => panic!("unexpected response: {other:?}"),
    }

    let deleted = send_request(
        &fixture.pipe_path,
        &IpcRequest::DeleteAppRule(DeleteAppRuleRequest {
            process_name: "word.exe".to_owned(),
            window_title_pattern: Some("*admin*".to_owned()),
        }),
    )
    .unwrap();
    assert_eq!(
        deleted,
        IpcResponse::AppRuleDeleted(crate::ipc::AppRuleDeletedResponse { deleted: true })
    );
}

#[test]
fn resets_app_rules_to_seed_defaults() {
    let fixture = IpcFixture::start();

    let response = send_request(&fixture.pipe_path, &IpcRequest::ResetAppRules).unwrap();

    match response {
        IpcResponse::AppRulesReset(response) => {
            assert!(response
                .rules
                .iter()
                .any(|rule| rule.process_name == "cmd.exe"));
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

#[test]
fn shutdown_all_sets_shared_shutdown_flag() {
    let fixture = IpcFixture::start();

    let response = send_request(&fixture.pipe_path, &IpcRequest::ShutdownAll).unwrap();

    assert!(matches!(response, IpcResponse::ShutdownAccepted(_)));
    assert!(fixture.shutdown_requested.load(Ordering::Relaxed));
}

struct IpcFixture {
    root: PathBuf,
    config_path: PathBuf,
    pipe_path: String,
    shutdown_requested: Arc<AtomicBool>,
    server: Option<NamedPipeIpcServer>,
}

impl IpcFixture {
    fn start() -> Self {
        let root = unique_temp_dir();
        fs::create_dir_all(&root).unwrap();
        let config_path = root.join("settings.toml");
        let database_path = root.join("autofix.sqlite");
        save_config(&config_path, &AppConfig::default()).unwrap();
        let pipe_path = format!("{}-{}", pipe_path_for_process(PIPE_NAME), unique_suffix());
        let shutdown_requested = Arc::new(AtomicBool::new(false));
        let state = IpcServerState::new(
            config_path.clone(),
            database_path.clone(),
            root.join("logs"),
            AppConfig::default(),
            Arc::clone(&shutdown_requested),
        );
        let server = NamedPipeIpcServer::start_for_path(pipe_path.clone(), state);

        Self {
            root,
            config_path,
            pipe_path,
            shutdown_requested,
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
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let counter = UNIQUE_COUNTER.fetch_add(1, Ordering::Relaxed) as u128;

    (nanos << 16) | counter
}
