mod admin;
mod components;
mod message_loop;
mod paths;
mod process_group;
mod security;
mod shortcuts;
mod target;
#[cfg(test)]
mod tests;

use std::{
    error::Error,
    fmt, fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::SystemTime,
};

use crate::{
    settings::{save_config, AppConfig},
    storage::Database,
};

use self::{
    admin::reject_elevated_process,
    components::{CorrectionEngineRouter, NamedPipeIpcServer, ReplacementEngine, SessionManager},
    paths::RuntimePaths,
    process_group::{shutdown_trusted_autofix_process_group, SiblingDisappearanceMonitor},
    security::{SecurityDecision, SecurityGate, TriggerKind},
    shortcuts::{GlobalShortcutListener, ShortcutAction},
};

pub(crate) struct BackgroundRuntime {
    database: Database,
    components: RuntimeComponents,
}

struct RuntimeComponents {
    config_path: PathBuf,
    config_modified_at: Option<SystemTime>,
    config: AppConfig,
    ipc_server: NamedPipeIpcServer,
    global_shortcut: GlobalShortcutListener,
    session_manager: SessionManager,
    correction_engine_router: CorrectionEngineRouter,
    replacement_engine: ReplacementEngine,
    process_group_monitor: SiblingDisappearanceMonitor,
    shutdown_requested: Arc<AtomicBool>,
}

#[derive(Debug)]
pub(crate) enum BackgroundError {
    ElevatedProcess,
    CreateDirectory {
        path: std::path::PathBuf,
        source: std::io::Error,
    },
    Config(crate::settings::ConfigIoError),
    Database(rusqlite::Error),
}

impl fmt::Display for BackgroundError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ElevatedProcess => write!(formatter, "AutoFix v1 must run as a normal user"),
            Self::CreateDirectory { path, source } => {
                write!(formatter, "failed to create {}: {}", path.display(), source)
            }
            Self::Config(source) => write!(formatter, "config error: {}", source),
            Self::Database(source) => write!(formatter, "database error: {}", source),
        }
    }
}

impl Error for BackgroundError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::CreateDirectory { source, .. } => Some(source),
            Self::Config(source) => Some(source),
            Self::Database(source) => Some(source),
            Self::ElevatedProcess => None,
        }
    }
}

pub(crate) fn run_background_mode() -> Result<(), BackgroundError> {
    let mut runtime = BackgroundRuntime::start(RuntimePaths::for_current_user()?)?;
    runtime.run_until_exit();
    runtime.shutdown();
    Ok(())
}

pub(crate) fn shutdown_process_group_mode() {
    initialize_logging();
    shutdown_trusted_autofix_process_group();
}

impl BackgroundRuntime {
    fn start(paths: RuntimePaths) -> Result<Self, BackgroundError> {
        reject_elevated_process()?;
        initialize_logging();
        ensure_parent_directory(paths.config_path())?;
        ensure_parent_directory(paths.database_path())?;

        let config = load_or_create_config(paths.config_path())?;
        let database = Database::open(paths.database_path()).map_err(BackgroundError::Database)?;
        let shutdown_requested = Arc::new(AtomicBool::new(false));
        let components = RuntimeComponents::start(&config, &paths, shutdown_requested)?;

        tracing::info!("AutoFix background process started");
        Ok(Self {
            database,
            components,
        })
    }

    fn shutdown(self) {
        self.components.shutdown();
        drop(self.database);
        tracing::info!("AutoFix background process exited cleanly");
    }

    fn run_until_exit(&mut self) {
        self.components.run_until_exit(&self.database);
    }
}

impl RuntimeComponents {
    fn start(
        config: &AppConfig,
        paths: &RuntimePaths,
        shutdown_requested: Arc<AtomicBool>,
    ) -> Result<Self, BackgroundError> {
        Ok(Self {
            config_path: paths.config_path().to_path_buf(),
            config_modified_at: modified_at(paths.config_path()),
            config: config.clone(),
            ipc_server: NamedPipeIpcServer::initialize(
                config,
                paths,
                Arc::clone(&shutdown_requested),
            )?,
            global_shortcut: GlobalShortcutListener::initialize(config),
            session_manager: SessionManager::initialize(),
            correction_engine_router: CorrectionEngineRouter::initialize(config),
            replacement_engine: ReplacementEngine::initialize(),
            process_group_monitor: SiblingDisappearanceMonitor::new(),
            shutdown_requested,
        })
    }

    fn shutdown(self) {
        self.replacement_engine.shutdown();
        self.correction_engine_router.shutdown();
        self.session_manager.shutdown();
        self.global_shortcut.shutdown();
        self.ipc_server.shutdown();
    }

    fn run_until_exit(&mut self, database: &Database) {
        message_loop::run_until_exit(|event| {
            match event {
                message_loop::MessageLoopEvent::Hotkey(id) => self.process_shortcut(id, database),
                message_loop::MessageLoopEvent::Poll => {}
                message_loop::MessageLoopEvent::Tick => {
                    self.reload_shortcuts_if_config_changed();
                    if self.process_group_monitor.shutdown_requested() {
                        self.shutdown_requested.store(true, Ordering::Relaxed);
                    }
                }
            }

            self.shutdown_requested.load(Ordering::Relaxed)
        });
    }

    fn process_shortcut(&mut self, id: usize, database: &Database) {
        match GlobalShortcutListener::action_for_id(id) {
            Some(ShortcutAction::Correct) => {
                if self.security_allows(TriggerKind::ManualShortcut, database) {
                    tracing::info!("correction pipeline placeholder triggered by shortcut");
                } else {
                    tracing::info!("correction shortcut ignored because context is blocked");
                }
            }
            Some(ShortcutAction::Undo) => {
                if self.security_allows(TriggerKind::Undo, database) {
                    tracing::info!("undo pipeline placeholder triggered by shortcut");
                } else {
                    tracing::info!("undo shortcut ignored because context is blocked");
                }
            }
            None => {}
        }
    }

    fn security_allows(&self, trigger: TriggerKind, database: &Database) -> bool {
        match SecurityGate::check(trigger, &self.current_config(), database) {
            SecurityDecision::Allowed { target } => {
                let session_key = target.session_key();
                tracing::debug!(
                    process_name = %target.process_name,
                    process_id = target.process_id,
                    trigger = trigger.as_str(),
                    session_key_kind = session_key_kind(&session_key),
                    "security gate allowed target"
                );
                true
            }
            SecurityDecision::Blocked { reason, target } => {
                if let Some(target) = target {
                    let session_key = target.session_key();
                    tracing::info!(
                        process_name = %target.process_name,
                        process_id = target.process_id,
                        trigger = trigger.as_str(),
                        session_key_kind = session_key_kind(&session_key),
                        block_reason = reason.as_str(),
                        "security gate blocked target"
                    );
                } else {
                    tracing::info!(
                        trigger = trigger.as_str(),
                        block_reason = reason.as_str(),
                        "security gate blocked target"
                    );
                }
                false
            }
        }
    }

    fn current_config(&self) -> AppConfig {
        self.config.clone()
    }

    #[allow(dead_code)]
    fn word_count_trigger_allowed(&self, database: &Database) -> bool {
        self.security_allows(TriggerKind::WordCount, database)
    }

    #[allow(dead_code)]
    fn character_trigger_allowed(&self, database: &Database) -> bool {
        self.security_allows(TriggerKind::Character, database)
    }

    #[allow(dead_code)]
    fn final_fix_before_reanchor_allowed(&self, database: &Database) -> bool {
        self.security_allows(TriggerKind::FinalFixBeforeReanchor, database)
    }

    fn reload_shortcuts_if_config_changed(&mut self) {
        let modified_at = modified_at(&self.config_path);
        if modified_at == self.config_modified_at {
            return;
        }

        self.config_modified_at = modified_at;
        match crate::settings::load_config(&self.config_path) {
            Ok(config) => {
                if shortcuts::detect_conflict(&config) {
                    tracing::warn!("shortcut conflict detected while reloading config");
                }
                self.config = config.clone();
                self.global_shortcut.reload(&config);
            }
            Err(error) => tracing::warn!("failed to reload shortcuts from config: {}", error),
        }
    }
}

fn initialize_logging() {
    let _ = tracing_subscriber::fmt().with_target(false).try_init();
}

fn ensure_parent_directory(path: &Path) -> Result<(), BackgroundError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| BackgroundError::CreateDirectory {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    Ok(())
}

fn load_or_create_config(path: &Path) -> Result<AppConfig, BackgroundError> {
    ensure_parent_directory(path)?;

    if !path.exists() {
        save_config(path, &AppConfig::default()).map_err(BackgroundError::Config)?;
    }

    crate::settings::load_config(path).map_err(BackgroundError::Config)
}

fn modified_at(path: &Path) -> Option<SystemTime> {
    path.metadata()
        .and_then(|metadata| metadata.modified())
        .ok()
}

fn session_key_kind(session_key: &target::SessionKey) -> &'static str {
    match session_key {
        target::SessionKey::FocusedElement(_) => "focused_element",
        target::SessionKey::WindowHandle(_) => "window_handle",
        target::SessionKey::ProcessTitle { .. } => "process_title",
        target::SessionKey::TemporaryActiveSession => "temporary_active_session",
    }
}
