//! Presentation boundary for a future near-caret suggestion UI.
//! V1 supports only opt-in, read-only notices. Acceptance must go through a
//! separate, freshly validated replacement request; display never permits edits.

use crate::settings::FeedbackConfig;

pub(crate) trait SuggestionUi {
    /// Advertise actual display support, separately from confidence preferences.
    fn is_available(&self) -> bool;
    /// Display a bounded preview containing only validated executable text.
    fn show_preview(&self, preview: String);
}

/// Adapter for the existing notice; no interactive overlay or acceptance UI.
pub(crate) struct PreviewSuggestionUi<'a> {
    config: &'a FeedbackConfig,
}

impl<'a> PreviewSuggestionUi<'a> {
    pub(crate) fn new(config: &'a FeedbackConfig) -> Self {
        Self { config }
    }
}

impl SuggestionUi for PreviewSuggestionUi<'_> {
    fn is_available(&self) -> bool {
        cfg!(windows) && self.config.show_medium_confidence_suggestions
    }

    fn show_preview(&self, preview: String) {
        if self.is_available() {
            super::notice::show(preview, self.config.show_near_caret_overlay);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn availability_requires_opt_in_and_platform_support() {
        let mut config = FeedbackConfig::default();
        assert!(!PreviewSuggestionUi::new(&config).is_available());
        config.show_near_caret_overlay = true;
        assert!(!PreviewSuggestionUi::new(&config).is_available());
        config.show_medium_confidence_suggestions = true;
        assert_eq!(
            PreviewSuggestionUi::new(&config).is_available(),
            cfg!(windows)
        );
        config.show_near_caret_overlay = false;
        assert_eq!(
            PreviewSuggestionUi::new(&config).is_available(),
            cfg!(windows)
        );
    }
}
