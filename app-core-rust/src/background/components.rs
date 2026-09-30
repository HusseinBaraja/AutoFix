use std::{
    fs,
    sync::{atomic::AtomicBool, Arc},
};

use super::triggers::CorrectionRequest;
use crate::{
    background::{paths::RuntimePaths, BackgroundError},
    ipc::IpcServerState,
    settings::AppConfig,
};

pub(crate) struct NamedPipeIpcServer(crate::ipc::NamedPipeIpcServer);
pub(crate) struct ReplacementEngine;

impl NamedPipeIpcServer {
    pub(crate) fn initialize(
        config: &AppConfig,
        paths: &RuntimePaths,
        shutdown_requested: Arc<AtomicBool>,
    ) -> Result<Self, BackgroundError> {
        fs::create_dir_all(paths.log_directory()).map_err(|source| {
            BackgroundError::CreateDirectory {
                path: paths.log_directory().to_path_buf(),
                source,
            }
        })?;

        let state = IpcServerState::new(
            paths.config_path().to_path_buf(),
            paths.database_path().to_path_buf(),
            paths.log_directory().to_path_buf(),
            config.clone(),
            shutdown_requested,
        );
        tracing::info!("named pipe IPC server initialized");
        Ok(Self(crate::ipc::NamedPipeIpcServer::start(state)))
    }

    pub(crate) fn shutdown(self) {
        self.0.shutdown();
        tracing::info!("named pipe IPC server shut down");
    }
}

impl ReplacementEngine {
    /// Native mutation is a separate feature. Refuse success until it can preserve
    /// clipboard, target undo, and the verified executable range atomically.
    pub(crate) fn replace(
        _target: &super::target::FocusedTarget,
        request: &CorrectionRequest,
        _output: &crate::correction::CorrectionOutput,
    ) -> bool {
        tracing::debug!(
            session_id = request.session_id,
            trigger = request.trigger.as_str(),
            "correction validated; native replacement unavailable"
        );
        false
    }
    pub(crate) fn initialize() -> Self {
        tracing::info!("replacement engine placeholder initialized");
        Self
    }

    pub(crate) fn shutdown(self) {
        tracing::info!("replacement engine placeholder shut down");
    }
}
