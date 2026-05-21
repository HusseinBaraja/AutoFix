use std::path::{Path, PathBuf};

const FILE_NAME: &str = "AutoFix.SettingsUi.exe";
const SETTINGS_UI_BUILD: &str = "SETTINGS_UI_BUILD";
const SETTINGS_UI_TARGET: &str = "SETTINGS_UI_TARGET";

pub(super) fn settings_app_path() -> PathBuf {
    let current_exe_dir = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf));

    settings_app_path_from(
        Path::new(env!("CARGO_MANIFEST_DIR")),
        current_exe_dir.as_deref(),
        std::env::var(SETTINGS_UI_BUILD).ok().as_deref(),
        std::env::var(SETTINGS_UI_TARGET).ok().as_deref(),
    )
}

fn settings_app_path_from(
    manifest_dir: &Path,
    current_exe_dir: Option<&Path>,
    build_override: Option<&str>,
    target_override: Option<&str>,
) -> PathBuf {
    if let Some(directory) = current_exe_dir {
        let bundled = directory.join(FILE_NAME);
        if bundled.exists() {
            return bundled;
        }
    }

    let candidates = settings_app_candidates(manifest_dir, build_override, target_override);
    candidates
        .iter()
        .find(|path| path.exists())
        .cloned()
        .unwrap_or_else(|| candidates[0].clone())
}

fn settings_app_candidates(
    manifest_dir: &Path,
    build_override: Option<&str>,
    target_override: Option<&str>,
) -> Vec<PathBuf> {
    let repo_root = manifest_dir.parent().unwrap_or(manifest_dir);
    let settings_bin = repo_root.join("ui").join("settings-ui").join("bin");
    let builds = candidate_values(build_override, &["Debug", "Release"]);
    let targets = candidate_values(target_override, &["net8.0-windows", "net9.0-windows"]);

    builds
        .iter()
        .flat_map(|build| {
            let settings_bin = settings_bin.clone();
            targets
                .iter()
                .map(move |target| settings_bin.join(build).join(target).join(FILE_NAME))
        })
        .collect()
}

fn candidate_values(override_value: Option<&str>, defaults: &[&str]) -> Vec<String> {
    let mut values = Vec::new();
    if let Some(value) = override_value {
        push_unique(&mut values, value);
    }
    for value in defaults {
        push_unique(&mut values, value);
    }
    values
}

fn push_unique(values: &mut Vec<String>, value: &str) {
    if !value.is_empty() && !values.iter().any(|existing| existing == value) {
        values.push(value.to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_settings_app_remains_primary() {
        let root = unique_temp_root("bundled");
        let exe_dir = root.join("app");
        std::fs::create_dir_all(&exe_dir).unwrap();
        let bundled = exe_dir.join(FILE_NAME);
        std::fs::write(&bundled, "").unwrap();

        let path = settings_app_path_from(&root.join("app-core-rust"), Some(&exe_dir), None, None);

        assert_eq!(path, bundled);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn settings_app_path_prefers_configured_build_and_target() {
        let root = unique_temp_root("configured");
        let manifest_dir = root.join("app-core-rust");
        let configured = root
            .join("ui")
            .join("settings-ui")
            .join("bin")
            .join("Release")
            .join("net8.0-windows10.0.19041.0")
            .join(FILE_NAME);
        std::fs::create_dir_all(configured.parent().unwrap()).unwrap();
        std::fs::write(&configured, "").unwrap();

        let path = settings_app_path_from(
            &manifest_dir,
            None,
            Some("Release"),
            Some("net8.0-windows10.0.19041.0"),
        );

        assert_eq!(path, configured);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn settings_app_path_searches_release_when_debug_missing() {
        let root = unique_temp_root("release");
        let manifest_dir = root.join("app-core-rust");
        let release = root
            .join("ui")
            .join("settings-ui")
            .join("bin")
            .join("Release")
            .join("net8.0-windows")
            .join(FILE_NAME);
        std::fs::create_dir_all(release.parent().unwrap()).unwrap();
        std::fs::write(&release, "").unwrap();

        let path = settings_app_path_from(&manifest_dir, None, None, None);

        assert_eq!(path, release);
        std::fs::remove_dir_all(root).unwrap();
    }

    fn unique_temp_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "autofix-settings-app-path-{name}-{}",
            std::process::id()
        ))
    }
}
