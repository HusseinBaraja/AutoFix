#[cfg(not(test))]
mod native;
#[cfg(test)]
mod tests;

use crate::{
    platform,
    settings::{AppConfig, CorrectionEngine, CorrectionMode},
};

pub(crate) struct TrayIcon {
    context: TrayMenuContext,
    native: Option<NativeTray>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TrayMenuContext {
    pub(crate) status: TrayStatus,
    pub(crate) app_name: String,
    pub(crate) correction_mode: CorrectionMode,
    pub(crate) engine: CorrectionEngine,
    pub(crate) visual_state: TrayVisualState,
    pub(crate) can_undo: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TrayStatus {
    Active,
    Paused,
    BlockedInApp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TrayVisualState {
    Idle,
    Active,
    Correcting,
    Blocked,
    Error,
}

impl TrayIcon {
    pub(crate) fn initialize(config: &AppConfig) -> Self {
        let context = TrayMenuContext::from_config(config);
        let _ = (TrayStatus::placeholder_states(), TrayVisualState::all());
        let native = NativeTray::initialize(&context);
        tracing::info!("tray icon initialized");
        Self { context, native }
    }

    pub(crate) fn process_menu_events(&mut self) -> bool {
        let Some(native) = &mut self.native else {
            return false;
        };

        native.process_events(&mut self.context)
    }

    pub(crate) fn shutdown(self) {
        drop(self.native);
        tracing::info!("tray icon shut down");
    }
}

impl TrayMenuContext {
    fn from_config(config: &AppConfig) -> Self {
        Self {
            status: TrayStatus::Active,
            app_name: platform::active_app_name(),
            correction_mode: config.correction.mode.clone(),
            engine: config.correction.engine.clone(),
            visual_state: TrayVisualState::Idle,
            can_undo: false,
        }
    }

    #[cfg(not(test))]
    fn refresh_current_app(&mut self) {
        self.app_name = platform::active_app_name();
    }

    fn labels(&self) -> Vec<String> {
        vec![
            format!("Status: {}", self.status.label()),
            format!("Current app: {}", self.app_name),
            format!("Correction mode: {}", correction_mode_label(&self.correction_mode)),
            format!("Engine: {}", engine_label(&self.engine)),
            "Undo last correction".to_owned(),
            "Open settings".to_owned(),
            "View logs".to_owned(),
            "Exit".to_owned(),
        ]
    }

    fn tooltip(&self) -> String {
        format!(
            "AutoFix - {} - {}",
            self.status.label(),
            self.visual_state.label()
        )
    }
}

impl TrayStatus {
    fn placeholder_states() -> [Self; 2] {
        [Self::Paused, Self::BlockedInApp]
    }

    fn label(self) -> &'static str {
        match self {
            Self::Active => "Active",
            Self::Paused => "Paused",
            Self::BlockedInApp => "Blocked in this app",
        }
    }
}

impl TrayVisualState {
    fn all() -> [Self; 5] {
        [
            Self::Idle,
            Self::Active,
            Self::Correcting,
            Self::Blocked,
            Self::Error,
        ]
    }

    fn label(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Active => "active",
            Self::Correcting => "correcting",
            Self::Blocked => "blocked",
            Self::Error => "error",
        }
    }
}

fn correction_mode_label(mode: &CorrectionMode) -> &'static str {
    match mode {
        CorrectionMode::TyposOnly => "Typos only",
        CorrectionMode::TyposPlusGrammar => "Typos + grammar",
    }
}

fn engine_label(engine: &CorrectionEngine) -> &'static str {
    match engine {
        CorrectionEngine::Local => "Local",
        CorrectionEngine::Api => "API",
    }
}

#[cfg(not(test))]
type NativeTray = native::NativeTray;

#[cfg(test)]
struct NativeTray;

#[cfg(test)]
impl NativeTray {
    fn initialize(_context: &TrayMenuContext) -> Option<Self> {
        None
    }

    fn process_events(&mut self, _context: &mut TrayMenuContext) -> bool {
        false
    }
}
