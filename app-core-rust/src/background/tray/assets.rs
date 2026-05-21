use std::path::{Path, PathBuf};

const BRAND_ICON_PATH: &str = "../assets/brand/autofix-logo.ico";

pub(crate) fn brand_icon_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(BRAND_ICON_PATH)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brand_icon_asset_is_available_to_background_crate() {
        let path = brand_icon_path();
        let metadata = std::fs::metadata(&path).expect("brand icon asset should exist");

        assert!(metadata.is_file());
        assert!(metadata.len() > 0);
    }
}
