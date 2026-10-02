//! The mutation owner. Strategies may fall through only before any target mutation.
mod clipboard;
mod native;
#[cfg(windows)]
mod send_input;
#[cfg(test)]
mod tests;

use super::{
    pipeline::InputStamp,
    target::{CorrectionEligibility, FocusedTarget},
    triggers::CorrectionRequest,
};
use crate::correction::{ConfidenceBehavior, ConfidenceTier, CorrectionOutput, EngineStatus};
use std::sync::atomic::{AtomicBool, Ordering};

static SHUTTING_DOWN: AtomicBool = AtomicBool::new(false);

/// Stop new native selections and mutations before waiting for input processing.
pub(super) fn begin_shutdown() {
    SHUTTING_DOWN.store(true, Ordering::Release);
}

/// Native boundary checks also cover a processor delayed by a provider call.
fn shutting_down() -> bool {
    SHUTTING_DOWN.load(Ordering::Acquire)
}

/// Join recovery after the input processor has dropped its clipboard transaction.
pub(super) fn finish_shutdown() -> bool {
    #[cfg(windows)]
    {
        clipboard::shutdown()
    }
    #[cfg(not(windows))]
    {
        true
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ReplacementMethod {
    DirectTextApi,
    UiAutomation,
    Clipboard,
    SendInput,
}

/// Exact character offsets backwards from the pre-replacement caret.
/// start_back >= end_back >= 0; the half-open range never extends after that caret.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ReplacedRange {
    pub(super) start_back: usize,
    pub(super) end_back: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct ReplacementResult {
    pub(super) success: bool,
    /// None means rejected before any strategy was attempted.
    pub(super) method: Option<ReplacementMethod>,
    /// Present only after the native range was proved (also on uncertain failures).
    pub(super) range: Option<ReplacedRange>,
    pub(super) reason: Option<String>,
    /// Failed verification or partial input requires dropping the tracked session.
    pub(super) may_have_changed: bool,
}

/// Verified mutation metadata passed to the session's undo owner.
#[derive(Default)]
pub(super) struct ReplacementConfirmation {
    pub(super) success: bool,
    pub(super) method: Option<ReplacementMethod>,
    pub(super) range: Option<ReplacedRange>,
}

impl From<ReplacementResult> for ReplacementConfirmation {
    /// Pass the verified native receipt to session bookkeeping without document text.
    fn from(result: ReplacementResult) -> Self {
        Self {
            success: result.success,
            method: result.method,
            range: result.range,
        }
    }
}

#[cfg(test)]
impl From<bool> for ReplacementConfirmation {
    /// Supply a minimal success receipt for tests that replace through a fake native boundary.
    fn from(success: bool) -> Self {
        Self {
            success,
            ..Self::default()
        }
    }
}

impl ReplacementResult {
    /// Native failures remain visible with default logging. No target text is logged.
    pub(super) fn log_outcome(&self, undo: bool) {
        if self.success {
            tracing::info!(success = true, method = ?self.method, range = ?self.range, undo,
                "native replacement completed");
        } else {
            tracing::warn!(success = false, method = ?self.method, range = ?self.range,
                reason = ?self.reason, may_have_changed = self.may_have_changed, undo,
                "native replacement failed or skipped");
        }
    }
}

struct ReplacementPlan<'a> {
    target: &'a FocusedTarget,
    original: &'a str,
    replacement: &'a str,
    following: &'a str,
    stamp: InputStamp,
}

impl ReplacementPlan<'_> {
    /// Count Unicode scalars backwards from the live caret, excluding newer typed text.
    fn range(&self) -> ReplacedRange {
        let end_back = self.following.chars().count();
        ReplacedRange {
            start_back: end_back + self.original.chars().count(),
            end_back,
        }
    }
}

enum Attempt {
    /// Capability/preparation failed without changing target text or selection.
    Unavailable(String),
    Finished(ReplacementResult),
}

trait ReplacementStrategy {
    /// Identify the strategy so clipboard preferences can skip preparation entirely.
    fn method(&self) -> ReplacementMethod;
    /// Return Unavailable only before target selection or mutation can have changed.
    fn replace(&mut self, plan: &ReplacementPlan<'_>) -> Attempt;
}

/// Reserved layers have an explicit capability refusal, never pretend success.
struct DeferredStrategy(ReplacementMethod);
impl ReplacementStrategy for DeferredStrategy {
    /// Identify the strategy so clipboard preferences can skip preparation entirely.
    fn method(&self) -> ReplacementMethod {
        self.0
    }

    /// Return Unavailable only before target selection or mutation can have changed.
    fn replace(&mut self, _: &ReplacementPlan<'_>) -> Attempt {
        Attempt::Unavailable(format!("{:?} replacement is not implemented", self.0))
    }
}

/// Try methods in safety order and stop on every completed or uncertain attempt.
fn run_strategies(
    plan: &ReplacementPlan<'_>,
    strategies: &mut [&mut dyn ReplacementStrategy],
    clipboard_allowed: bool,
) -> ReplacementResult {
    let mut method = None;
    let mut reasons = Vec::new();
    for strategy in strategies {
        if strategy.method() == ReplacementMethod::Clipboard && !clipboard_allowed {
            continue;
        }
        method = Some(strategy.method());
        match strategy.replace(plan) {
            Attempt::Unavailable(reason) => reasons.push(reason),
            Attempt::Finished(result) => return result,
        }
    }
    ReplacementResult {
        success: false,
        method,
        range: None,
        reason: Some(reasons.join("; ")),
        may_have_changed: false,
    }
}

pub(super) struct ReplacementEngine;
impl ReplacementEngine {
    /// Called only after the pipeline proves session ownership, policy and live text.
    pub(super) fn replace(
        target: &FocusedTarget,
        request: &CorrectionRequest,
        output: &CorrectionOutput,
        stamp: InputStamp,
        clipboard_enabled: bool,
    ) -> ReplacementResult {
        let reason = if target.correction_eligibility() != CorrectionEligibility::Allowed {
            Some("target is protected, unavailable or unsupported")
        } else if request.selected_text {
            // Even opt-in selections must prove the live caret end. V1 cannot do so.
            Some("selected range has no verified pre-caret geometry")
        } else if request.executable_context.is_empty() {
            Some("executable context is empty")
        } else if request.executable_context.contains('\0')
            || output.corrected_executable_text.contains('\0')
            || output.corrected_executable_text.encode_utf16().count() > 65_536
        {
            Some("replacement contains NUL or exceeds the input budget")
        } else if output.status != EngineStatus::Completed
            || !output.changes_needed
            || output.behavior != ConfidenceBehavior::Silent
            || output.confidence == ConfidenceTier::Low
            || output.corrected_executable_text == request.executable_context
        {
            Some("correction does not authorize replacement")
        } else {
            None
        };
        if let Some(reason) = reason {
            return ReplacementResult {
                success: false,
                method: None,
                range: None,
                reason: Some(reason.into()),
                may_have_changed: false,
            };
        }
        let plan = ReplacementPlan {
            target,
            original: &request.executable_context,
            replacement: &output.corrected_executable_text,
            following: &request.replacement_following_text,
            stamp,
        };
        Self::execute(&plan, clipboard_enabled)
    }

    /// Informative text is editable only through a session-issued app correction record.
    pub(super) fn undo(
        target: &FocusedTarget,
        undo: &super::session::CorrectionUndoTarget,
        stamp: InputStamp,
        clipboard_enabled: bool,
    ) -> ReplacementResult {
        Self::execute(
            &ReplacementPlan {
                target,
                original: &undo.corrected,
                replacement: &undo.original,
                following: &undo.following,
                stamp,
            },
            clipboard_enabled,
        )
    }

    /// Apply the same strategy ordering and clipboard preference to correction and undo.
    fn execute(plan: &ReplacementPlan<'_>, clipboard_enabled: bool) -> ReplacementResult {
        run_strategies(
            plan,
            &mut [
                &mut DeferredStrategy(ReplacementMethod::DirectTextApi),
                &mut DeferredStrategy(ReplacementMethod::UiAutomation),
                &mut native::NativeStrategy(ReplacementMethod::Clipboard),
                &mut native::NativeStrategy(ReplacementMethod::SendInput),
            ],
            clipboard_enabled && clipboard::allowed_for(&plan.target.process_name),
        )
    }
}
