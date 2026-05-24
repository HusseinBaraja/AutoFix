use super::{
    assets, opens_settings_for_tray_event, TrayCommandTargets, TrayMenuContext, TrayVisualState,
};
use std::{error::Error, path::Path, process::Command};
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    Icon, TrayIcon as NativeIcon, TrayIconBuilder, TrayIconEvent,
};

const ID_UNDO: &str = "autofix.undo_last_correction";
const ID_SETTINGS: &str = "autofix.open_settings";
const ID_LOGS: &str = "autofix.view_logs";
const ID_EXIT: &str = "autofix.exit";
const TRAY_LOGO_ALPHA: &[u8] =
    include_bytes!("../../../../assets/brand/autofix-tray-mask-16.alpha");

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

        while let Ok(event) = TrayIconEvent::receiver().try_recv() {
            if opens_settings_for_tray_event(&event) {
                self.open_settings();
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
        open_path(&self.targets.settings_app_path);
    }

    fn view_logs(&self) {
        open_path(&self.targets.logs_path);
    }
}

fn open_path(path: &Path) {
    let mut command = command_for_path(path);

    if let Err(error) = command.spawn() {
        tracing::warn!("failed to open {}: {}", path.display(), error);
    }
}

fn command_for_path(path: &Path) -> Command {
    if path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
    {
        let mut command = Command::new(path);
        if let Some(directory) = path.parent() {
            command.current_dir(directory);
        }
        return command;
    }

    let mut command = Command::new("explorer.exe");
    command.arg(path);
    command
}

fn icon_for_state(state: TrayVisualState) -> Result<Icon, tray_icon::BadIcon> {
    if let Some(icon) = brand_icon() {
        return Ok(icon);
    }

    let rgba = match state {
        TrayVisualState::Idle => [91, 99, 112, 255],
        TrayVisualState::Active => [38, 166, 91, 255],
        TrayVisualState::Correcting => [52, 152, 219, 255],
        TrayVisualState::Blocked => [150, 154, 160, 255],
        TrayVisualState::Error => [220, 76, 70, 255],
    };
    Icon::from_rgba(tinted_logo_icon(rgba), 16, 16)
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use std::path::Path;

    use super::command_for_path;

    #[test]
    fn executable_paths_launch_directly_from_their_directory() {
        let path = Path::new(r"C:\AutoFix\AutoFix.SettingsUi.exe");
        let command = command_for_path(path);

        assert_eq!(command.get_program(), path.as_os_str());
        assert_eq!(command.get_current_dir(), Some(Path::new(r"C:\AutoFix")));
        assert_eq!(command.get_args().count(), 0);
    }

    #[test]
    fn non_executable_paths_open_with_explorer() {
        let path = Path::new(r"C:\AutoFix\logs");
        let command = command_for_path(path);
        let args = command.get_args().collect::<Vec<_>>();

        assert_eq!(command.get_program(), OsStr::new("explorer.exe"));
        assert_eq!(args, [path.as_os_str()]);
    }
}

fn brand_icon() -> Option<Icon> {
    match Icon::from_path(assets::brand_icon_path(), Some((16, 16))) {
        Ok(icon) => Some(icon),
        Err(error) => {
            tracing::warn!("failed to load brand tray icon asset: {}", error);
            None
        }
    }
}

fn tinted_logo_icon(rgba: [u8; 4]) -> Vec<u8> {
    assert_eq!(
        TRAY_LOGO_ALPHA.len(),
        16 * 16,
        "tray alpha asset autofix-tray-mask-16.alpha must contain exactly 256 bytes"
    );

    let mut pixels = Vec::with_capacity(16 * 16 * 4);
    for alpha in TRAY_LOGO_ALPHA {
        pixels.extend_from_slice(&[
            rgba[0],
            rgba[1],
            rgba[2],
            alpha.saturating_mul(rgba[3]) / 255,
        ]);
    }
    pixels
}
