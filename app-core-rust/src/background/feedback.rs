//! Quiet feedback policy. Tray state is metadata only; opt-in previews stay ephemeral.

use super::security::BlockReason;
mod notice;
pub(crate) mod suggestion;
use crate::settings::FeedbackConfig;
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU8, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use suggestion::{Preview, PreviewSuggestionUi, SuggestionUi};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Event {
    Applied,
    Timeout,
    Error,
    Blocked,
    Skipped(&'static str),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub(super) enum TrayState {
    #[default]
    Idle,
    Active,
    Correcting,
    Blocked,
    Error,
}

static TRAY_STATE: AtomicU8 = AtomicU8::new(TrayState::Idle as u8);

/// The shell polls this bounded, text-free snapshot independently of input processing.
pub(crate) fn tray_state() -> &'static str {
    match TRAY_STATE.load(Ordering::Relaxed) {
        1 => "active",
        2 => "correcting",
        3 => "blocked",
        4 => "error",
        _ => "idle",
    }
}

pub(super) fn notice(event: Event, manual: bool, config: &FeedbackConfig) -> Option<&'static str> {
    match event {
        Event::Applied if config.show_correction_applied_notification => {
            Some("AutoFix: correction applied.")
        }
        Event::Timeout if manual && config.show_timeout_notice => {
            Some("AutoFix: API correction timed out.")
        }
        Event::Error if manual => Some("AutoFix: correction could not be completed."),
        Event::Blocked if manual && config.show_blocked_app_notice => {
            Some("AutoFix: action could not be safely applied.")
        }
        Event::Skipped(reason) if config.show_skipped_reason => Some(reason),
        _ => None,
    }
}

/// Secure fields never receive overlays, including opted-in notices.
pub(super) fn is_app_block(reason: BlockReason) -> bool {
    matches!(
        reason,
        BlockReason::SensitiveAppDefault
            | BlockReason::AppRuleBlocked
            | BlockReason::EngineBlocked
            | BlockReason::AllowlistRequired
            | BlockReason::AppPolicyUnavailable
            | BlockReason::UnsupportedTarget
            | BlockReason::ElevatedTarget
    )
}

#[derive(Default)]
pub(super) struct Feedback {
    blocked: bool,
    error_until: Option<Instant>,
    last_notice: Option<Instant>,
    notice_cancelled: Arc<AtomicBool>,
}

impl Feedback {
    /// Opt-in, read-only preview. Text stays in the short-lived notice worker only.
    pub(super) fn suggestion(&mut self, preview: Preview, config: &FeedbackConfig) {
        let now = Instant::now();
        let ui = PreviewSuggestionUi::new(config);
        if ui.is_available()
            && self
                .last_notice
                .is_none_or(|last| now.duration_since(last) >= Duration::from_millis(2500))
        {
            self.last_notice = Some(now);
            ui.show_preview(preview, Arc::clone(&self.notice_cancelled));
        }
    }
    pub(super) fn reset(&mut self) {
        self.notice_cancelled.store(true, Ordering::Release);
        self.notice_cancelled = Arc::new(AtomicBool::new(false));
        self.blocked = false;
        self.error_until = None;
    }

    pub(super) fn blocked(&mut self, blocked: bool) {
        self.blocked = blocked;
    }

    pub(super) fn event(&mut self, event: Event, manual: bool, config: &FeedbackConfig) {
        let now = Instant::now();
        if matches!(event, Event::Error | Event::Timeout) {
            self.error_until = Some(now + Duration::from_millis(2500));
        }
        if let Some(text) = notice(event, manual, config) {
            if self
                .last_notice
                .is_none_or(|last| now.duration_since(last) >= Duration::from_millis(2500))
            {
                self.last_notice = Some(now);
                notice::show(
                    text,
                    config.show_near_caret_overlay,
                    notice::Origin::current(Arc::clone(&self.notice_cancelled)),
                );
            }
        }
    }

    fn state(&self, enabled: bool, active: bool, correcting: bool, now: Instant) -> TrayState {
        if !enabled {
            TrayState::Idle
        } else if self.blocked {
            TrayState::Blocked
        } else if self.error_until.is_some_and(|until| now < until) {
            TrayState::Error
        } else if correcting {
            TrayState::Correcting
        } else if active {
            TrayState::Active
        } else {
            TrayState::Idle
        }
    }

    pub(super) fn publish(&self, config: &FeedbackConfig, active: bool, correcting: bool) {
        TRAY_STATE.store(
            self.state(
                config.tray_state_enabled,
                active,
                correcting,
                Instant::now(),
            ) as u8,
            Ordering::Relaxed,
        );
    }
}

pub(super) fn suggestion_preview(text: &str) -> String {
    let mut preview: String = text
        .chars()
        .take(90)
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    if text.chars().count() > 90 {
        preview.push('…');
    }
    format!("AutoFix suggestion: {preview}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quiet_defaults_and_independent_options() {
        let mut config = FeedbackConfig::default();
        for event in [
            Event::Applied,
            Event::Timeout,
            Event::Error,
            Event::Blocked,
            Event::Skipped("reason"),
        ] {
            assert_eq!(notice(event, false, &config), None);
        }
        assert!(notice(Event::Error, true, &config).is_some());
        assert!(notice(Event::Blocked, true, &config).is_some());
        assert!(notice(Event::Timeout, true, &config).is_some());
        config.show_timeout_notice = false;
        config.show_blocked_app_notice = false;
        config.show_near_caret_overlay = true;
        assert_eq!(notice(Event::Timeout, true, &config), None);
        assert_eq!(notice(Event::Blocked, true, &config), None);
        assert_eq!(notice(Event::Applied, true, &config), None);
        config.show_correction_applied_notification = true;
        config.show_skipped_reason = true;
        assert!(notice(Event::Applied, false, &config).is_some());
        assert_eq!(
            notice(Event::Skipped("reason"), false, &config),
            Some("reason")
        );
    }

    #[test]
    fn states_expire_and_disabled_state_keeps_idle() {
        let now = Instant::now();
        let mut feedback = Feedback::default();
        assert_eq!(feedback.state(true, false, false, now), TrayState::Idle);
        assert_eq!(feedback.state(true, true, false, now), TrayState::Active);
        assert_eq!(feedback.state(true, true, true, now), TrayState::Correcting);
        feedback.error_until = Some(now + Duration::from_secs(2));
        assert_eq!(feedback.state(true, true, false, now), TrayState::Error);
        assert_eq!(
            feedback.state(true, true, false, now + Duration::from_secs(3)),
            TrayState::Active
        );
        feedback.blocked(true);
        assert_eq!(feedback.state(true, true, true, now), TrayState::Blocked);
        assert_eq!(feedback.state(false, true, true, now), TrayState::Idle);
        feedback.reset();
        assert_eq!(feedback.state(true, false, false, now), TrayState::Idle);
    }

    #[test]
    fn hard_security_blocks_never_show_notices() {
        for reason in [
            BlockReason::PasswordField,
            BlockReason::ProtectedOrHiddenField,
            BlockReason::SecureDesktop,
            BlockReason::CredentialDialog,
            BlockReason::LockScreen,
        ] {
            assert!(!is_app_block(reason));
        }
        assert!(is_app_block(BlockReason::AppRuleBlocked));
    }

    #[test]
    fn previews_are_bounded_unicode_safe_and_have_no_control_characters() {
        assert_eq!(suggestion_preview("a\nb\0c"), "AutoFix suggestion: a b c");
        assert_eq!(
            suggestion_preview(&"語".repeat(100)),
            format!("AutoFix suggestion: {}…", "語".repeat(90))
        );
    }
}
