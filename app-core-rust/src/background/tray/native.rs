use super::{TrayCommandTargets, TrayMenuContext, TrayVisualState};
use std::{error::Error, path::Path, process::Command};
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    Icon, TrayIcon as NativeIcon, TrayIconBuilder,
};

const ID_UNDO: &str = "autofix.undo_last_correction";
const ID_SETTINGS: &str = "autofix.open_settings";
const ID_LOGS: &str = "autofix.view_logs";
const ID_EXIT: &str = "autofix.exit";

pub(crate) struct NativeTray {
    icon: NativeIcon,
    status: MenuItem,
    app: MenuItem,
    mode: MenuItem,
    engine: MenuItem,
    undo: MenuItem,
    targets: TrayCommandTargets,
}

impl NativeTray {
    pub(crate) fn initialize(
        context: &TrayMenuContext,
        targets: TrayCommandTargets,
    ) -> Option<Self> {
        match Self::build(context, targets) {
            Ok(tray) => Some(tray),
            Err(error) => {
                tracing::error!("failed to initialize tray icon: {}", error);
                None
            }
        }
    }

    pub(crate) fn process_events(&mut self, context: &mut TrayMenuContext) -> bool {
        context.refresh_current_app();
        self.apply_context(context);

        let mut exit_requested = false;
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            match event.id.as_ref() {
                ID_UNDO => tracing::info!("undo last correction requested from tray"),
                ID_SETTINGS => self.open_settings(),
                ID_LOGS => self.view_logs(),
                ID_EXIT => exit_requested = true,
                _ => {}
            }
        }

        exit_requested
    }

    fn build(
        context: &TrayMenuContext,
        targets: TrayCommandTargets,
    ) -> Result<Self, Box<dyn Error>> {
        let menu = Menu::new();
        let status = MenuItem::with_id("autofix.status", "", false, None);
        let app = MenuItem::with_id("autofix.current_app", "", false, None);
        let mode = MenuItem::with_id("autofix.correction_mode", "", false, None);
        let engine = MenuItem::with_id("autofix.engine", "", false, None);
        let undo = MenuItem::with_id(ID_UNDO, "Undo last correction", false, None);
        let settings = MenuItem::with_id(ID_SETTINGS, "Open settings", true, None);
        let logs = MenuItem::with_id(ID_LOGS, "View logs", true, None);
        let exit = MenuItem::with_id(ID_EXIT, "Exit", true, None);

        menu.append_items(&[
            &status,
            &app,
            &mode,
            &engine,
            &PredefinedMenuItem::separator(),
            &undo,
            &settings,
            &logs,
            &PredefinedMenuItem::separator(),
            &exit,
        ])?;

        let icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip(context.tooltip())
            .with_icon(icon_for_state(context.visual_state)?)
            .build()?;

        let tray = Self {
            icon,
            status,
            app,
            mode,
            engine,
            undo,
            targets,
        };
        tray.apply_context(context);
        Ok(tray)
    }

    fn apply_context(&self, context: &TrayMenuContext) {
        let labels = context.labels();
        self.status.set_text(&labels[0]);
        self.app.set_text(&labels[1]);
        self.mode.set_text(&labels[2]);
        self.engine.set_text(&labels[3]);
        self.undo.set_enabled(context.can_undo);

        if let Err(error) = self.icon.set_tooltip(Some(context.tooltip())) {
            tracing::warn!("failed to update tray tooltip: {}", error);
        }
        if let Err(error) = self
            .icon
            .set_icon(icon_for_state(context.visual_state).ok())
        {
            tracing::warn!("failed to update tray icon state: {}", error);
        }
    }

    fn open_settings(&self) {
        open_path(&self.targets.settings_path);
    }

    fn view_logs(&self) {
        open_path(&self.targets.logs_path);
    }
}

fn open_path(path: &Path) {
    if let Err(error) = Command::new("explorer.exe").arg(path).spawn() {
        tracing::warn!("failed to open {}: {}", path.display(), error);
    }
}

fn icon_for_state(state: TrayVisualState) -> Result<Icon, tray_icon::BadIcon> {
    let rgba = match state {
        TrayVisualState::Idle => [91, 99, 112, 255],
        TrayVisualState::Active => [38, 166, 91, 255],
        TrayVisualState::Correcting => [52, 152, 219, 255],
        TrayVisualState::Blocked => [150, 154, 160, 255],
        TrayVisualState::Error => [220, 76, 70, 255],
    };
    Icon::from_rgba(solid_icon(rgba), 16, 16)
}

fn solid_icon(rgba: [u8; 4]) -> Vec<u8> {
    let mut pixels = Vec::with_capacity(16 * 16 * 4);
    for _ in 0..(16 * 16) {
        pixels.extend_from_slice(&rgba);
    }
    pixels
}
