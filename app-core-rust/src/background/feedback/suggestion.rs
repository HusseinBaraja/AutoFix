//! Presentation boundary for a future near-caret suggestion UI.
//! V1 supports only opt-in, read-only notices. Acceptance must go through a
//! separate, freshly validated replacement request; display never permits edits.

use crate::background::{pipeline::InputStamp, target::FocusedTarget};
use crate::settings::FeedbackConfig;
use std::sync::{atomic::AtomicBool, Arc};

/// A preview retains the exact origin that passed completion validation.
pub(in crate::background) struct Preview {
    pub(in crate::background) text: String,
    pub(in crate::background) stamp: InputStamp,
    pub(in crate::background) target: FocusedTarget,
    pub(in crate::background) cancelled: Arc<AtomicBool>,
}

pub(in crate::background) trait SuggestionUi {
    /// Advertise actual display support, separately from confidence preferences.
    fn is_available(&self) -> bool;
    /// Display a bounded preview containing only validated executable text.
    fn show_preview(&self, preview: Preview, policy_cancelled: Arc<AtomicBool>);
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

    fn show_preview(&self, preview: Preview, policy_cancelled: Arc<AtomicBool>) {
        if self.is_available() {
            let origin = super::notice::Origin::validated(
                preview.stamp,
                preview.target,
                preview.cancelled,
                policy_cancelled,
            );
            super::notice::show(preview.text, self.config.show_near_caret_overlay, origin);
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
