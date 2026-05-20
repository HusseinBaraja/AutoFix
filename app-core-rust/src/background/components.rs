use crate::{background::paths::RuntimePaths, ipc::IpcServerState, settings::AppConfig};

pub(crate) struct NamedPipeIpcServer(crate::ipc::NamedPipeIpcServer);
pub(crate) struct GlobalShortcutListener;
pub(crate) struct SessionManager;
pub(crate) struct CorrectionEngineRouter;
pub(crate) struct ReplacementEngine;

impl NamedPipeIpcServer {
    pub(crate) fn initialize(config: &AppConfig, paths: &RuntimePaths) -> Self {
        let state = IpcServerState::new(
            paths.config_path().to_path_buf(),
            paths.log_directory().to_path_buf(),
            config.clone(),
        );
        tracing::info!("named pipe IPC server initialized");
        Self(crate::ipc::NamedPipeIpcServer::start(state))
    }

    pub(crate) fn shutdown(self) {
        self.0.shutdown();
        tracing::info!("named pipe IPC server shut down");
    }
}

impl GlobalShortcutListener {
    pub(crate) fn initialize(_config: &AppConfig) -> Self {
        tracing::info!("global shortcut placeholder initialized");
        Self
    }

    pub(crate) fn shutdown(self) {
        tracing::info!("global shortcut placeholder shut down");
    }
}

impl SessionManager {
    pub(crate) fn initialize() -> Self {
        tracing::info!("session manager placeholder initialized");
        Self
    }

    pub(crate) fn shutdown(self) {
        tracing::info!("session manager placeholder shut down");
    }
}

impl CorrectionEngineRouter {
    pub(crate) fn initialize(_config: &AppConfig) -> Self {
        tracing::info!("correction engine router placeholder initialized");
        Self
    }

    pub(crate) fn shutdown(self) {
        tracing::info!("correction engine router placeholder shut down");
    }
}

impl ReplacementEngine {
    pub(crate) fn initialize() -> Self {
        tracing::info!("replacement engine placeholder initialized");
        Self
    }

    pub(crate) fn shutdown(self) {
        tracing::info!("replacement engine placeholder shut down");
    }
}
