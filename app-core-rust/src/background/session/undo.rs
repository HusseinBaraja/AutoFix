//! Session-owned correction history. Text and metadata never leave app memory.

use std::{ops::Range, time::SystemTime};

use super::{ContextConfig, Session};
use crate::{
    background::{
        replacement::{ReplacedRange, ReplacementConfirmation, ReplacementMethod},
        security::TriggerKind,
    },
    correction::ConfidenceTier,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CorrectionUndo {
    session_id: u64,
    executable_range: Range<usize>, // Unicode scalars in the original executable context.
    text_range: Option<ReplacedRange>, // Backwards from the caret at correction time.
    original: String,
    replacement: String,
    timestamp: SystemTime,
    trigger: Option<TriggerKind>,
    confidence: Option<ConfidenceTier>,
    method: Option<ReplacementMethod>,
    language: Option<String>,
    informative_start: Option<usize>, // Current byte offset; None means proof was lost.
    caret_anchor: u64,
}

/// Only a recorded, fully retained correction may make informative text editable.
pub(in crate::background) struct CorrectionUndoTarget {
    pub(in crate::background) language: Option<String>,
    pub(in crate::background) corrected: String,
    pub(in crate::background) original: String,
    pub(in crate::background) following: String,
}

impl Session {
    /// Record the exact executable text only after its native mutation succeeded.
    pub(super) fn record_undo(
        &mut self,
        original: String,
        replacement: String,
        executable_range: Range<usize>,
        limits: &ContextConfig,
    ) {
        if original == replacement {
            return;
        }
        let informative_start = self
            .informative_context
            .len()
            .checked_sub(replacement.len())
            .filter(|start| self.informative_context.get(*start..) == Some(replacement.as_str()));
        self.correction_undo_history.push(CorrectionUndo {
            session_id: self.id,
            executable_range,
            text_range: None,
            original,
            replacement,
            timestamp: SystemTime::now(),
            trigger: None,
            confidence: None,
            method: None,
            language: None,
            informative_start,
            caret_anchor: self.versions.caret_anchor,
        });
        self.limit_undo_history(limits);
    }

    /// Attach the actual engine decision and verified replacement receipt.
    pub(in crate::background) fn record_undo_metadata(
        &mut self,
        language: Option<String>,
        trigger: TriggerKind,
        confidence: ConfidenceTier,
        confirmation: ReplacementConfirmation,
    ) {
        if let Some(last) = self.correction_undo_history.last_mut() {
            last.language = language;
            last.trigger = Some(trigger);
            last.confidence = Some(confidence);
            last.method = confirmation.method;
            last.text_range = confirmation.range;
        }
    }

    /// Evict oldest records after commits or capacity reductions, keeping the newest rejections.
    pub(super) fn limit_undo_history(&mut self, limits: &ContextConfig) {
        let excess = self
            .correction_undo_history
            .len()
            .saturating_sub(usize::from(limits.undo_history_size.clamp(1, 1000)));
        self.correction_undo_history.drain(..excess);
    }

    /// Shrinking context preserves full history text but removes unsafe range proofs.
    pub(super) fn trim_undo_anchors(&mut self, removed_bytes: usize) {
        for entry in &mut self.correction_undo_history {
            entry.informative_start = entry
                .informative_start
                .and_then(|start| start.checked_sub(removed_bytes));
        }
    }

    /// Keep history text but revoke every native range proof after movement or re-anchoring.
    pub(super) fn invalidate_undo_anchors(&mut self) {
        for entry in &mut self.correction_undo_history {
            entry.informative_start = None;
        }
    }

    /// Expose only the newest fully retained span with the same session and caret anchor.
    /// Following session text is carried separately so native undo can preserve it.
    pub(in crate::background) fn undo_target(&self) -> Option<CorrectionUndoTarget> {
        let last = self.correction_undo_history.last()?;
        let start = last.informative_start?;
        let end = start.checked_add(last.replacement.len())?;
        if self.position_uncertain()
            || last.session_id != self.id
            || last.caret_anchor != self.versions.caret_anchor
            || self.informative_context.get(start..end)? != last.replacement
        {
            return None;
        }
        Some(CorrectionUndoTarget {
            language: last.language.clone(),
            corrected: last.replacement.clone(),
            original: last.original.clone(),
            following: format!(
                "{}{}",
                self.informative_context.get(end..)?,
                self.known_before_caret()
                    .strip_prefix(&self.informative_context)?
            ),
        })
    }

    /// Called after verified native undo; originals remain read-only and new typing stays editable.
    pub(crate) fn undo_last_correction(&mut self, limits: &ContextConfig) -> bool {
        if self.undo_target().is_none() {
            return false;
        }
        let last = self.correction_undo_history.pop().unwrap();
        let start = last.informative_start.unwrap();
        self.restore_pending();
        self.informative_context
            .replace_range(start..start + last.replacement.len(), &last.original);
        self.shrink_informative(limits);
        self.pending_corrections.clear();
        self.versions.context = self.versions.context.wrapping_add(1);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::background::{
        target::SessionKey,
        typing::{MovementSignal, TypedInput},
    };

    /// Create a session with no imported document context.
    fn session() -> Session {
        Session::new(1, SessionKey::WindowHandle(1))
    }

    /// Simulate a verified app correction through the same session commit path.
    fn correct(session: &mut Session, original: &str, replacement: &str, limits: &ContextConfig) {
        session.input(TypedInput::Text(original.into()), limits);
        assert!(session.queue_correction(original.into(), replacement.into()));
        assert!(session.apply_next_correction(limits));
    }

    /// Repeated undo stops at the configured capacity and preserves evicted corrections.
    #[test]
    fn oldest_entries_are_evicted_at_default_and_custom_capacity() {
        for capacity in [1, 3, 10] {
            let limits = ContextConfig {
                undo_history_size: capacity,
                ..Default::default()
            };
            let mut session = session();
            for _ in 0..12 {
                correct(&mut session, "teh ", "the ", &limits);
                assert!(session.correction_undo_history.len() <= usize::from(capacity));
            }
            for _ in 0..capacity {
                assert!(session.undo_last_correction(&limits));
            }
            assert!(!session.undo_last_correction(&limits));
            assert_eq!(
                session.informative_context(),
                format!(
                    "{}{}",
                    "the ".repeat(12 - usize::from(capacity)),
                    "teh ".repeat(usize::from(capacity))
                )
            );
            assert_eq!(session.executable_context(), "");
        }
    }

    /// Unicode undo restores exact originals while newer typing remains executable.
    #[test]
    fn length_changing_unicode_undo_preserves_new_typing_and_originals_are_informative() {
        let limits = ContextConfig::default();
        let mut session = session();
        correct(&mut session, "é😃 teh", "é the", &limits);
        correct(&mut session, " alot", " a lot", &limits);
        session.input(TypedInput::Text("尾".into()), &limits);
        assert_eq!(session.undo_target().unwrap().original, " alot");
        assert!(session.undo_last_correction(&limits));
        let older = session.undo_target().unwrap();
        assert_eq!(older.corrected, "é the");
        assert_eq!(older.original, "é😃 teh");
        assert_eq!(older.following, " alot尾");
        assert!(session.undo_last_correction(&limits));
        assert_eq!(session.informative_context(), "é😃 teh alot");
        assert_eq!(session.editable_context(), "尾");
    }

    /// Trimming a Unicode prefix preserves complete undo spans and their byte anchors.
    #[test]
    fn shrinking_prefix_preserves_complete_older_spans_and_adjusts_unicode_offsets() {
        let limits = ContextConfig {
            informative_context_max_chars: 9,
            ..Default::default()
        };
        let mut session = session();
        session.set_informative_context("é😃. ".into(), &limits);
        correct(&mut session, "teh ", "the ", &limits);
        correct(&mut session, "wierd", "weird", &limits);
        assert_eq!(session.informative_context(), "the weird");
        assert_eq!(session.correction_undo_history.len(), 2);
        assert!(session.undo_last_correction(&limits));
        assert!(session.undo_last_correction(&limits));
        assert_eq!(session.informative_context(), "teh wierd");
    }

    /// Partial retained spans keep full history text but cannot authorize native undo.
    #[test]
    fn trimming_retains_full_history_text_but_refuses_partial_native_undo() {
        let limits = ContextConfig {
            informative_context_max_chars: 2,
            ..Default::default()
        };
        let mut session = session();
        correct(&mut session, "teh 😃", "the é", &limits);
        let entry = &session.correction_undo_history[0];
        assert_eq!(entry.original, "teh 😃");
        assert_eq!(entry.replacement, "the é");
        assert_eq!(entry.executable_range, 0..5);
        assert!(entry.informative_start.is_none());
        assert!(session.undo_target().is_none());
        assert!(!session.undo_last_correction(&limits));
        assert_eq!(session.informative_context(), " é");
        assert_eq!(session.correction_undo_history.len(), 1);
    }

    /// Undo records retain the engine decision and verified native range receipt.
    #[test]
    fn receipt_records_session_timestamp_trigger_confidence_and_native_method() {
        let limits = ContextConfig::default();
        let mut session = session();
        let before = SystemTime::now();
        correct(&mut session, "teh", "the", &limits);
        let range = ReplacedRange {
            start_back: 8,
            end_back: 5,
        };
        session.record_undo_metadata(
            Some("en".into()),
            TriggerKind::Character,
            ConfidenceTier::Medium,
            ReplacementConfirmation {
                success: true,
                method: Some(ReplacementMethod::SendInput),
                range: Some(range),
            },
        );
        let entry = &session.correction_undo_history[0];
        assert_eq!(entry.session_id, session.id());
        assert!(entry.timestamp >= before && entry.timestamp <= SystemTime::now());
        assert_eq!(entry.trigger, Some(TriggerKind::Character));
        assert_eq!(entry.confidence, Some(ConfidenceTier::Medium));
        assert_eq!(entry.method, Some(ReplacementMethod::SendInput));
        assert_eq!(entry.text_range, Some(range));
        assert_eq!(entry.language.as_deref(), Some("en"));
    }

    /// Caret movement revokes range proof while retaining bounded history in memory.
    #[test]
    fn movement_invalidates_proof_without_discarding_session_history() {
        let limits = ContextConfig::default();
        let mut session = session();
        correct(&mut session, "teh", "the", &limits);
        session.input(TypedInput::Uncertain(MovementSignal::MouseClick), &limits);
        assert!(session.undo_target().is_none());
        assert!(!session.undo_last_correction(&limits));
        assert_eq!(session.correction_undo_history.len(), 1);
        session.set_informative_context("the".into(), &limits);
        assert!(session.undo_target().is_none());
    }

    /// Accepted unchanged text cannot create an app correction to undo.
    #[test]
    fn unchanged_commits_do_not_create_undo_entries() {
        let limits = ContextConfig::default();
        let mut session = session();
        correct(&mut session, "unchanged", "unchanged", &limits);
        assert!(session.correction_undo_history.is_empty());
        assert!(session.undo_target().is_none());
    }
}
