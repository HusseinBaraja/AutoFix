mod admin;
mod components;
mod message_loop;
mod paths;
mod shortcuts;
#[cfg(test)]
mod tests;
mod tray;

use std::{
    error::Error,
    fmt, fs,
    path::{Path, PathBuf},
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
    shortcuts::{GlobalShortcutListener, ShortcutAction},
    tray::TrayIcon,
};

pub(crate) struct BackgroundRuntime {
    database: Database,
    components: RuntimeComponents,
}

struct RuntimeComponents {
    config_path: PathBuf,
    config_modified_at: Option<SystemTime>,
    tray_icon: TrayIcon,
    ipc_server: NamedPipeIpcServer,
    global_shortcut: GlobalShortcutListener,
    session_manager: SessionManager,
    correction_engine_router: CorrectionEngineRouter,
    replacement_engine: ReplacementEngine,
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

impl BackgroundRuntime {
    fn start(paths: RuntimePaths) -> Result<Self, BackgroundError> {
        reject_elevated_process()?;
        initialize_logging();
        ensure_parent_directory(paths.config_path())?;
        ensure_parent_directory(paths.database_path())?;

        let config = load_or_create_config(paths.config_path())?;
        let database = Database::open(paths.database_path()).map_err(BackgroundError::Database)?;
        let components = RuntimeComponents::start(&config, &paths)?;

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
        self.components.run_until_exit();
    }
}

impl RuntimeComponents {
    fn start(config: &AppConfig, paths: &RuntimePaths) -> Result<Self, BackgroundError> {
        Ok(Self {
            tray_icon: TrayIcon::initialize(config, paths),
            config_path: paths.config_path().to_path_buf(),
            config_modified_at: modified_at(paths.config_path()),
            ipc_server: NamedPipeIpcServer::initialize(config, paths)?,
            global_shortcut: GlobalShortcutListener::initialize(config),
            session_manager: SessionManager::initialize(),
            correction_engine_router: CorrectionEngineRouter::initialize(config),
            replacement_engine: ReplacementEngine::initialize(),
        })
    }

    fn shutdown(self) {
        self.replacement_engine.shutdown();
        self.correction_engine_router.shutdown();
        self.session_manager.shutdown();
        self.global_shortcut.shutdown();
        self.ipc_server.shutdown();
        self.tray_icon.shutdown();
    }

    fn run_until_exit(&mut self) {
        message_loop::run_until_exit(|event| {
            match event {
                message_loop::MessageLoopEvent::Hotkey(id) => self.process_shortcut(id),
                message_loop::MessageLoopEvent::Poll => {}
                message_loop::MessageLoopEvent::Tick => self.reload_shortcuts_if_config_changed(),
            }

            self.tray_icon.process_menu_events()
        });
    }

    fn process_shortcut(&mut self, id: usize) {
        match GlobalShortcutListener::action_for_id(id) {
            Some(ShortcutAction::Correct) => {
                if self.correction_shortcut_enabled() {
                    tracing::info!("correction pipeline placeholder triggered by shortcut");
                } else {
                    tracing::info!("correction shortcut ignored because context is blocked");
                }
            }
            Some(ShortcutAction::Undo) => {
                tracing::info!("undo pipeline placeholder triggered by shortcut");
            }
            None => {}
        }
    }

    fn correction_shortcut_enabled(&self) -> bool {
        !self.is_app_blocked_by_rules()
            && !self.is_secure_or_password_field_focused()
            && !self.is_focused_context_unsupported()
    }

    fn is_app_blocked_by_rules(&self) -> bool {
        false
    }

    fn is_secure_or_password_field_focused(&self) -> bool {
        false
    }

    fn is_focused_context_unsupported(&self) -> bool {
        false
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
