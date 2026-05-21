use super::*;

#[test]
fn default_context_matches_config_without_pause_logic() {
    let context = TrayMenuContext::from_config(&AppConfig::default());

    assert_eq!(context.status, TrayStatus::Active);
    assert_eq!(context.correction_mode, CorrectionMode::TyposOnly);
    assert_eq!(context.engine, CorrectionEngine::Local);
    assert_eq!(context.visual_state, TrayVisualState::Idle);
    assert!(!context.can_undo);
}

#[test]
fn labels_are_context_aware_and_compact() {
    let mut config = AppConfig::default();
    config.correction.mode = CorrectionMode::TyposPlusGrammar;
    config.correction.engine = CorrectionEngine::Api;

    let mut context = TrayMenuContext::from_config(&config);
    context.status = TrayStatus::BlockedInApp;
    context.app_name = "Notepad".to_owned();

    assert_eq!(
        context.labels(),
        vec![
            "Status: Blocked in this app",
            "Current app: Notepad",
            "Correction mode: Typos + grammar",
            "Engine: API",
            "Undo last correction",
            "Open settings",
            "View logs",
            "Exit",
        ]
    );
}

#[test]
fn all_subtle_tray_states_have_tooltip_labels() {
    for state in [
        TrayVisualState::Idle,
        TrayVisualState::Active,
        TrayVisualState::Correcting,
        TrayVisualState::Blocked,
        TrayVisualState::Error,
    ] {
        let mut context = TrayMenuContext::from_config(&AppConfig::default());
        context.visual_state = state;
        assert!(context.tooltip().contains(state.label()));
    }
}

#[test]
fn pause_status_exists_as_placeholder_only() {
    assert_eq!(TrayStatus::Paused.label(), "Paused");
}

#[test]
fn command_targets_open_settings_app_and_runtime_folder() {
    let root = std::env::temp_dir().join("autofix-tray-targets");
    let paths = RuntimePaths::new(
        root.join("config.toml"),
        root.join("autofix.sqlite"),
        root.join("logs"),
    );

    let targets = TrayCommandTargets::from_paths(&paths);

    assert_eq!(
        targets.settings_app_path.file_name(),
        Some(std::ffi::OsStr::new("AutoFix.SettingsUi.exe"))
    );
    assert_eq!(targets.logs_path, root.join("logs"));
}
