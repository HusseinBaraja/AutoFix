mod admin;
mod components;
mod paths;
#[cfg(test)]
mod tests;
mod tray;

use std::{error::Error, fmt, fs, path::Path};

use crate::{
    settings::{save_config, AppConfig},
    storage::Database,
};

use self::{
    admin::reject_elevated_process,
    components::{
        CorrectionEngineRouter, GlobalShortcutListener, NamedPipeIpcServer, ReplacementEngine,
        SessionManager,
    },
    paths::RuntimePaths,
    tray::TrayIcon,
};

pub(crate) struct BackgroundRuntime {
    database: Database,
    components: RuntimeComponents,
}

struct RuntimeComponents {
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
        let components = RuntimeComponents::start(&config, &paths);

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
    fn start(config: &AppConfig, paths: &RuntimePaths) -> Self {
        Self {
            tray_icon: TrayIcon::initialize(config, paths),
            ipc_server: NamedPipeIpcServer::initialize(),
            global_shortcut: GlobalShortcutListener::initialize(config),
            session_manager: SessionManager::initialize(),
            correction_engine_router: CorrectionEngineRouter::initialize(config),
            replacement_engine: ReplacementEngine::initialize(),
        }
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
        message_loop::run_until_exit(|| self.tray_icon.process_menu_events());
    }
}

fn initialize_logging() {
    let _ = tracing_subscriber::fmt().with_target(false).try_init();
}

#[cfg(windows)]
mod message_loop {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, PostQuitMessage, TranslateMessage, MSG,
    };

    pub(super) fn run_until_exit(mut should_exit: impl FnMut() -> bool) {
        unsafe {
            let mut message = std::mem::zeroed::<MSG>();
            loop {
                let result = GetMessageW(&mut message, std::ptr::null_mut(), 0, 0);
                if result <= 0 {
                    break;
                }

                TranslateMessage(&message);
                DispatchMessageW(&message);

                if should_exit() {
                    PostQuitMessage(0);
                }
            }
        }
    }
}

#[cfg(not(windows))]
mod message_loop {
    pub(super) fn run_until_exit(mut should_exit: impl FnMut() -> bool) {
        while !should_exit() {
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
    }
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
