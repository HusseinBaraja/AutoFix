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
pub(crate) struct CorrectionEngineRouter;
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

impl CorrectionEngineRouter {
    /// Record request metadata while the correction engine is still a placeholder.
    pub(crate) fn submit(request: CorrectionRequest) {
        tracing::info!(
            trigger = request.trigger.as_str(),
            executable_chars = request.executable_context.chars().count(),
            informative_chars = request.informative_context.chars().count(),
            following_chars = request.following_context.chars().count(),
            selected_text = request.selected_text,
            temporary_selection = request.temporary_selection,
            primary_language = request.language_info.primary_language.as_deref().unwrap_or("unknown"),
            detected_language_count = request.language_info.detected_languages.len(),
            uncertain_language = request.language_info.is_uncertain(),
            ?request.uncertain_language_policy,
            ?request.mixed_language_policy,
            ?request.versions,
            "correction request accepted by placeholder router"
        );
    }
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
