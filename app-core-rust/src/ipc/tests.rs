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

/// .NET tray clients must receive a complete response even when reading starts after the write.
#[test]
fn dotnet_status_clients_preserve_delayed_responses_in_byte_and_message_modes() {
    use std::{
        os::windows::process::CommandExt,
        process::Command,
        thread,
        time::{Duration, Instant},
    };

    let fixture = IpcFixture::start();
    let pipe_name = fixture.pipe_path.strip_prefix(r"\\.\pipe\").unwrap();
    for read_mode in ["Byte", "Message"] {
        let script = format!(
            r#"
$ErrorActionPreference = 'Stop'
$pipe = [System.IO.Pipes.NamedPipeClientStream]::new('.', '{pipe_name}', [System.IO.Pipes.PipeDirection]::InOut, [System.IO.Pipes.PipeOptions]::Asynchronous)
try {{
    $pipe.Connect(2000)
    $pipe.ReadMode = [System.IO.Pipes.PipeTransmissionMode]::{read_mode}
    $bytes = [System.Text.Encoding]::UTF8.GetBytes('{{"type":"get_app_status"}}')
    $pipe.Write($bytes, 0, $bytes.Length)
    $pipe.Flush()
    Start-Sleep -Milliseconds 100
    $reader = [System.IO.StreamReader]::new($pipe)
    try {{
        $read = $reader.ReadToEndAsync()
        if (!$read.Wait(2000)) {{ throw 'IPC response timed out.' }}
        [Console]::Write($read.GetAwaiter().GetResult())
    }} finally {{ $reader.Dispose() }}
}} finally {{ $pipe.Dispose() }}
"#
        );
        let mut child = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .creation_flags(0x08000000) // CREATE_NO_WINDOW
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let started = Instant::now();
        while child.try_wait().unwrap().is_none() {
            // Allow cold PowerShell/.NET startup on busy CI hosts; the connect and
            // response deadlines inside the client remain two seconds each.
            if started.elapsed() > Duration::from_secs(30) {
                child.kill().unwrap();
                let output = child.wait_with_output().unwrap();
                panic!(
                    ".NET IPC client did not finish in {read_mode} mode: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            thread::sleep(Duration::from_millis(10));
        }
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let response: IpcResponse = serde_json::from_slice(&output.stdout)
            .expect(".NET tray client must receive a complete JSON response");
        assert!(matches!(response, IpcResponse::AppStatus(status) if status.running));
    }
}

/// Local anonymous readers must not connect; the owning account remains able to poll status.
#[test]
fn anonymous_local_clients_are_denied_even_read_only_access() {
    use std::{os::windows::ffi::OsStrExt, ptr::null_mut, thread, time::Duration};
    use windows_sys::Win32::{
        Foundation::{
            CloseHandle, GetLastError, ERROR_ACCESS_DENIED, ERROR_FILE_NOT_FOUND, GENERIC_READ,
            INVALID_HANDLE_VALUE,
        },
        Security::{ImpersonateAnonymousToken, RevertToSelf},
        Storage::FileSystem::{CreateFileW, FILE_ATTRIBUTE_NORMAL, OPEN_EXISTING},
        System::Threading::GetCurrentThread,
    };
    let fixture = IpcFixture::start();
    assert!(matches!(
        send_request(&fixture.pipe_path, &IpcRequest::GetAppStatus).unwrap(),
        IpcResponse::AppStatus(_)
    ));
    let pipe_path = fixture.pipe_path.clone();
    let error = thread::spawn(move || {
        struct Revert;
        impl Drop for Revert {
            fn drop(&mut self) {
                assert_ne!(unsafe { RevertToSelf() }, 0);
            }
        }
        assert_ne!(unsafe { ImpersonateAnonymousToken(GetCurrentThread()) }, 0);
        let _revert = Revert;
        let path: Vec<u16> = std::ffi::OsStr::new(&pipe_path)
            .encode_wide()
            .chain(Some(0))
            .collect();
        for _ in 0..100 {
            let pipe = unsafe {
                CreateFileW(
                    path.as_ptr(),
                    GENERIC_READ,
                    0,
                    null_mut(),
                    OPEN_EXISTING,
                    FILE_ATTRIBUTE_NORMAL,
                    null_mut(),
                )
            };
            if pipe != INVALID_HANDLE_VALUE {
                unsafe {
                    CloseHandle(pipe);
                }
                return 0;
            }
            let error = unsafe { GetLastError() };
            if error != ERROR_FILE_NOT_FOUND {
                return error;
            }
            thread::sleep(Duration::from_millis(10));
        }
        ERROR_FILE_NOT_FOUND
    })
    .join()
    .unwrap();
    assert_eq!(error, ERROR_ACCESS_DENIED);
    assert!(matches!(
        send_request(&fixture.pipe_path, &IpcRequest::GetAppStatus).unwrap(),
        IpcResponse::AppStatus(_)
    ));
}

/// An idle or nonreading local client must not monopolize the status endpoint.
#[test]
fn stalled_clients_release_the_status_endpoint_at_the_request_deadline() {
    use std::{
        os::windows::ffi::OsStrExt,
        ptr::null_mut,
        thread,
        time::{Duration, Instant},
    };
    use windows_sys::Win32::{
        Foundation::{CloseHandle, GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE},
        Storage::FileSystem::{CreateFileW, WriteFile, FILE_ATTRIBUTE_NORMAL, OPEN_EXISTING},
    };
    struct Client(HANDLE);
    impl Drop for Client {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
    for send_request_first in [false, true] {
        let fixture = IpcFixture::start();
        let path: Vec<u16> = std::ffi::OsStr::new(&fixture.pipe_path)
            .encode_wide()
            .chain(Some(0))
            .collect();
        let started = Instant::now();
        let client = loop {
            let pipe = unsafe {
                CreateFileW(
                    path.as_ptr(),
                    GENERIC_READ | GENERIC_WRITE,
                    0,
                    null_mut(),
                    OPEN_EXISTING,
                    FILE_ATTRIBUTE_NORMAL,
                    null_mut(),
                )
            };
            if pipe != INVALID_HANDLE_VALUE {
                break Client(pipe);
            }
            assert!(started.elapsed() < Duration::from_secs(2));
            thread::sleep(Duration::from_millis(10));
        };
        if send_request_first {
            let request = serde_json::to_vec(&IpcRequest::GetAppStatus).unwrap();
            let mut written = 0;
            assert_ne!(
                unsafe {
                    WriteFile(
                        client.0,
                        request.as_ptr().cast(),
                        request.len() as u32,
                        &mut written,
                        null_mut(),
                    )
                },
                0
            );
            assert_eq!(written as usize, request.len());
        }
        thread::sleep(Duration::from_millis(1200));
        let resumed = Instant::now();
        let response = send_request(&fixture.pipe_path, &IpcRequest::GetAppStatus).unwrap();
        assert!(matches!(response, IpcResponse::AppStatus(status) if status.running));
        assert!(resumed.elapsed() < Duration::from_millis(500));
        // Keep the unresponsive client open until after the next client has succeeded.
        drop(client);
    }
}

#[test]
fn reports_basic_app_status() {
    let fixture = IpcFixture::start();

    let response = send_request(&fixture.pipe_path, &IpcRequest::GetAppStatus).unwrap();

    match response {
        IpcResponse::AppStatus(status) => {
            assert!(status.running);
            assert_eq!(status.correction_mode, CorrectionMode::TyposOnly.into());
            assert_eq!(status.engine, CorrectionEngine::Local.into());
            assert!(["idle", "active", "correcting", "blocked", "error"]
                .contains(&status.tray_state.as_str()));
        }
        other => panic!("unexpected response: {other:?}"),
    }
}

#[test]
fn feedback_options_can_be_updated_through_ipc() {
    let fixture = IpcFixture::start();
    for (path, value) in [
        ("feedback.show_near_caret_overlay", true),
        ("feedback.show_correction_applied_notification", true),
        ("feedback.show_skipped_reason", true),
        ("feedback.show_medium_confidence_suggestions", true),
        ("feedback.show_blocked_app_notice", false),
        ("feedback.show_timeout_notice", false),
        ("feedback.tray_state_enabled", false),
    ] {
        let response = send_request(
            &fixture.pipe_path,
            &IpcRequest::UpdateSetting(UpdateSettingRequest {
                path: path.into(),
                value: json!(value),
            }),
        )
        .unwrap();
        assert!(matches!(response, IpcResponse::SettingUpdated(_)));
        let config =
            serde_json::to_value(crate::settings::load_config(&fixture.config_path).unwrap())
                .unwrap();
        assert_eq!(
            config["feedback"][path.strip_prefix("feedback.").unwrap()],
            json!(value)
        );
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
