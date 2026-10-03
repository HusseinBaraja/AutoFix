//! Frozen automatic segments stay separate from the one active editable context.
use super::{ContextConfig, Session};
use crate::settings::PendingQueueFullBehavior;

pub(super) struct FrozenSegment {
    pub(super) id: u64,
    pub(super) original: String,
    pub(super) caret_anchor: u64,
    // Captured text between owned typed spans is informative, never executable.
    pub(super) gap: String,
    pub(super) movement: bool,
    pub(super) final_fix: bool,
    pub(super) submitted: bool,
    pub(super) queued_trigger: Option<crate::background::security::TriggerKind>,
    pub(super) retire: bool,
}

impl FrozenSegment {
    /// Return the memory-only segment identity used to cancel worker requests.
    pub(super) fn id(&self) -> u64 {
        self.id
    }
    /// Expose the frozen typed text for exact range validation.
    pub(super) fn original(&self) -> &str {
        &self.original
    }
}

impl Session {
    /// Include older frozen text as read-only context for newer corrections.
    pub(crate) fn correction_informative_context(&self) -> String {
        let mut context = self.informative_context.clone();
        for segment in &self.frozen_segments {
            context.push_str(&segment.original);
            context.push_str(&segment.gap);
        }
        context
    }

    /// Snapshot a reserved range and its read-only prefix after overflow handling.
    pub(crate) fn pending_context(&self, id: u64) -> Option<(String, String)> {
        let mut informative = self.informative_context.clone();
        for segment in &self.frozen_segments {
            if segment.id == id {
                return Some((informative, segment.original.clone()));
            }
            informative.push_str(&segment.original);
            informative.push_str(&segment.gap);
        }
        None
    }

    /// Reserve capacity at the trigger, before later keys in the same batch.
    /// Return the segment to submit and all dependent worker requests to cancel.
    pub(crate) fn freeze_pending(&mut self, limits: &ContextConfig) -> (Option<u64>, Vec<u64>) {
        if self.position_uncertain() || self.editable_context().trim().is_empty() {
            return (None, Vec::new());
        }
        let mut cancelled = Vec::new();
        if self.frozen_segments.len() >= usize::from(limits.pending_queue_size.clamp(1, 16)) {
            if self.frozen_segments.iter().any(|segment| segment.movement) {
                return (None, Vec::new());
            }
            match limits.pending_queue_full_behavior {
                PendingQueueFullBehavior::SkipNew => return (None, Vec::new()),
                PendingQueueFullBehavior::CancelOldest => {
                    let oldest = self.frozen_segments.front().unwrap().id;
                    // Unchecked text must stay executable. Restore the whole
                    // suffix before freezing it again, preserving document order.
                    cancelled = self.restore_pending_from(oldest);
                }
                PendingQueueFullBehavior::MergeNewest => {
                    let newest = self.frozen_segments.pop_back().unwrap();
                    self.correction_floor -= newest.original.chars().count();
                    return (None, vec![newest.id]);
                }
            }
        }
        let original = self.editable_context();
        let id = self.next_segment_id;
        self.next_segment_id = self
            .next_segment_id
            .checked_add(1)
            .expect("segment ID exhausted");
        self.correction_floor += original.chars().count();
        self.frozen_segments.push_back(FrozenSegment {
            id,
            original,
            caret_anchor: self.versions.caret_anchor,
            gap: String::new(),
            movement: false,
            final_fix: false,
            submitted: true,
            queued_trigger: None,
            retire: false,
        });
        (Some(id), cancelled)
    }

    /// Verify that a frozen range still belongs to the known pre-caret typed prefix.
    pub(crate) fn pending_submitted(&self, id: u64) -> bool {
        self.frozen_segments
            .iter()
            .any(|segment| segment.id == id && segment.submitted)
    }

    /// Verify that a frozen range still belongs to the known pre-caret typed prefix.
    pub(crate) fn pending_matches(&self, id: u64, original: &str) -> bool {
        !self.position_uncertain()
            && self.frozen_segments.iter().any(|segment| {
                segment.id == id
                    && segment.original == original
                    && segment.caret_anchor == self.versions.caret_anchor
            })
            && self.executable_context().starts_with(
                &self
                    .frozen_segments
                    .iter()
                    .map(|segment| segment.original.as_str())
                    .collect::<String>(),
            )
    }

    /// Return known typed text after a frozen segment that replacement must preserve.
    pub(crate) fn pending_following_text(&self, id: u64) -> Option<String> {
        let mut following: Option<String> = None;
        for segment in &self.frozen_segments {
            if let Some(text) = &mut following {
                text.push_str(&segment.original);
                text.push_str(&segment.gap);
            }
            if segment.id == id {
                following = Some(segment.gap.clone());
            }
        }
        following.map(|mut text| {
            text.push_str(&self.editable_context());
            text
        })
    }

    /// Commit only checked text, in document order. The caller confirms native
    /// mutation first for changed text; unchanged text needs accepted engine output.
    pub(crate) fn complete_pending(
        &mut self,
        id: u64,
        corrected: &str,
        limits: &ContextConfig,
    ) -> bool {
        let Some(segment) = self.frozen_segments.front() else {
            return false;
        };
        if segment.id != id || !self.pending_matches(id, &segment.original) {
            return false;
        }
        let segment = self.frozen_segments.pop_front().unwrap();
        if !self.executable.retire_prefix(&segment.original) {
            self.restore_pending();
            return false;
        }
        self.correction_floor -= segment.original.chars().count();
        self.append_informative(corrected, limits);
        self.versions.context = self.versions.context.wrapping_add(1);
        if corrected != segment.original {
            let executable_range = 0..segment.original.chars().count();
            self.record_undo(
                segment.original,
                corrected.to_owned(),
                executable_range,
                limits,
            );
        }
        self.append_informative(&segment.gap, limits);
        true
    }

    /// Manual correction overrides pending work without importing field text.
    pub(crate) fn restore_pending(&mut self) {
        if !self.frozen_segments.is_empty() {
            self.frozen_segments.retain(|segment| segment.movement);
            while self
                .frozen_segments
                .back()
                .is_some_and(|segment| segment.gap.is_empty() && !segment.final_fix)
            {
                self.frozen_segments.pop_back();
            }
            for segment in &mut self.frozen_segments {
                if !segment.final_fix {
                    segment.submitted = false;
                }
            }
            self.correction_floor = self
                .frozen_segments
                .iter()
                .map(|segment| segment.original.chars().count())
                .sum();
            self.versions.executable = self.versions.executable.wrapping_add(1);
        }
    }

    /// Restore failed or skipped work and its newer reservations without disturbing older work.
    /// A suffix can return to editable text while preserving document order.
    pub(crate) fn restore_pending_from(&mut self, id: u64) -> Vec<u64> {
        let Some(index) = self
            .frozen_segments
            .iter()
            .position(|segment| segment.id == id)
        else {
            return Vec::new();
        };
        if self.frozen_segments[index].movement {
            // A moved range cannot be merged across read-only gaps. Final work
            // gets one attempt; rejected work retires its original without mutation.
            let mut cancelled = Vec::new();
            for segment in self.frozen_segments.iter_mut().skip(index) {
                cancelled.push(segment.id);
                if !segment.final_fix {
                    segment.submitted = false;
                    segment.queued_trigger = None;
                }
            }
            return cancelled;
        }
        let restored = self.frozen_segments.split_off(index);
        self.correction_floor -= restored
            .iter()
            .map(|segment| segment.original.chars().count())
            .sum::<usize>();
        self.versions.executable = self.versions.executable.wrapping_add(1);
        restored.into_iter().map(|segment| segment.id).collect()
    }

    /// A sequence-guarded initial capture excludes the entire known typed span,
    /// including frozen segments. It can establish their shared caret anchor.
    pub(crate) fn set_captured_informative_context(
        &mut self,
        context: String,
        limits: &ContextConfig,
    ) {
        self.informative_context = context;
        self.shrink_informative(limits);
        self.invalidate_undo_anchors();
        self.versions.context = self.versions.context.wrapping_add(1);
        self.versions.caret_anchor = self.versions.caret_anchor.wrapping_add(1);
        for segment in &mut self.frozen_segments {
            segment.caret_anchor = self.versions.caret_anchor;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::background::{
        target::SessionKey,
        typing::{MovementSignal, TypedInput},
    };

    /// Build a typed session without importing any target-application text.
    fn session(text: &str) -> Session {
        let mut session = Session::new(1, SessionKey::WindowHandle(1));
        session.input(TypedInput::Text(text.into()), &ContextConfig::default());
        session
    }

    /// Ordered completion preserves Unicode suffixes and records undo only for changed segments.
    #[test]
    fn frozen_results_commit_in_order_and_preserve_unicode_new_typing_and_undo() {
        let limits = ContextConfig {
            pending_queue_size: 2,
            ..ContextConfig::default()
        };
        let mut session = session("teh ");
        let first = session.freeze_pending(&limits).0.unwrap();
        assert_eq!(session.editable_context(), "");
        session.input(TypedInput::Text("é next ".into()), &limits);
        let second = session.freeze_pending(&limits).0.unwrap();
        session.input(TypedInput::Text("尾".into()), &limits);
        assert_eq!(
            session.pending_following_text(first).as_deref(),
            Some("é next 尾")
        );
        assert!(!session.complete_pending(second, "bad", &limits));
        assert!(session.complete_pending(first, "the ", &limits));
        assert_eq!(session.informative_context(), "the ");
        assert!(session.pending_matches(second, "é next "));
        assert!(session.complete_pending(second, "É next ", &limits));
        assert_eq!(session.informative_context(), "the É next ");
        assert_eq!(session.editable_context(), "尾");
        assert!(session.undo_last_correction(&limits));
        assert!(session.undo_last_correction(&limits));
        assert_eq!(session.informative_context(), "teh é next ");
        assert_eq!(session.editable_context(), "尾");
        assert!(!session.complete_pending(first, "duplicate", &limits));
    }

    /// Default overflow keeps the admitted range and leaves newer text editable.
    #[test]
    fn default_full_queue_skips_and_keeps_current_typing() {
        let limits = ContextConfig::default();
        let mut session = session("first.");
        let first = session.freeze_pending(&limits).0.unwrap();
        session.input(TypedInput::Text(" next.".into()), &limits);
        assert_eq!(session.freeze_pending(&limits), (None, vec![]));
        assert_eq!(session.editable_context(), " next.");
        assert!(session.pending_matches(first, "first."));
        assert_eq!(session.correction_informative_context(), "first.");
    }

    /// Cancel-oldest preserves unchecked original text in the newly admitted range.
    #[test]
    fn cancel_oldest_rechecks_original_with_new_segment() {
        let limits = ContextConfig {
            pending_queue_full_behavior: PendingQueueFullBehavior::CancelOldest,
            ..ContextConfig::default()
        };
        let mut session = session("teh");
        let first = session.freeze_pending(&limits).0.unwrap();
        session.input(TypedInput::Text(" new".into()), &limits);
        let (second, cancelled) = session.freeze_pending(&limits);
        assert_eq!(cancelled, vec![first]);
        assert_ne!(second, Some(first));
        assert_eq!(session.informative_context(), "");
        assert!(!session.pending_matches(first, "teh"));
        let second = second.unwrap();
        assert!(session.pending_matches(second, "teh new"));
        assert!(session.complete_pending(second, "teh new", &limits));
        assert_eq!(session.informative_context(), "teh new");
    }

    /// Overflow must cancel every dependent range rather than commit unchecked text.
    #[test]
    fn cancel_oldest_restores_entire_full_queue_before_freezing_again() {
        let limits = ContextConfig {
            pending_queue_size: 2,
            pending_queue_full_behavior: PendingQueueFullBehavior::CancelOldest,
            ..ContextConfig::default()
        };
        let mut session = session("teh");
        let first = session.freeze_pending(&limits).0.unwrap();
        session.input(TypedInput::Text(" é".into()), &limits);
        let second = session.freeze_pending(&limits).0.unwrap();
        session.input(TypedInput::Text(" 尾".into()), &limits);
        let (new, cancelled) = session.freeze_pending(&limits);
        assert_eq!(cancelled, vec![first, second]);
        let new = new.unwrap();
        assert_eq!(
            session.pending_context(new),
            Some((String::new(), "teh é 尾".into()))
        );
        assert!(session.pending_matches(new, "teh é 尾"));
        assert_eq!(session.informative_context(), "");
        assert!(session.undo_target().is_none());
    }

    /// Frozen commit shrinks memory, preserves newer text, and retains safe undo.
    #[test]
    fn frozen_commit_shrinks_context_and_retains_undo_only_for_the_complete_span() {
        for cap in [5, 8] {
            let limits = ContextConfig {
                informative_context_max_chars: cap,
                ..ContextConfig::default()
            };
            let mut session = session("teh é");
            session.set_informative_context("older ".into(), &limits);
            let id = session.freeze_pending(&limits).0.unwrap();
            session.input(TypedInput::Text(" 尾".into()), &limits);
            assert!(session.complete_pending(id, "the é", &limits));
            assert!(session.informative_context().ends_with("the é"));
            assert!(session.informative_context().chars().count() <= cap as usize);
            assert_eq!(session.editable_context(), " 尾");
            let undo = session.undo_target().unwrap();
            assert_eq!(undo.original, "teh é");
            assert_eq!(undo.corrected, "the é");
            assert_eq!(undo.following, " 尾");
        }
        let limits = ContextConfig {
            informative_context_max_chars: 2,
            ..ContextConfig::default()
        };
        let mut session = session("teh é");
        let id = session.freeze_pending(&limits).0.unwrap();
        assert!(session.complete_pending(id, "the é", &limits));
        assert_eq!(session.informative_context(), " é");
        assert!(session.undo_target().is_none());
    }

    /// Merge-newest restores only the latest frozen range and preserves older pending work.
    #[test]
    fn merge_newest_keeps_older_work_and_waits_for_another_trigger() {
        let limits = ContextConfig {
            pending_queue_size: 2,
            pending_queue_full_behavior: PendingQueueFullBehavior::MergeNewest,
            ..ContextConfig::default()
        };
        let mut session = session("first.");
        let first = session.freeze_pending(&limits).0.unwrap();
        session.input(TypedInput::Text(" second.".into()), &limits);
        let second = session.freeze_pending(&limits).0.unwrap();
        session.input(TypedInput::Text(" third.".into()), &limits);
        assert_eq!(session.freeze_pending(&limits), (None, vec![second]));
        assert_eq!(session.editable_context(), " second. third.");
        assert!(session.pending_matches(first, "first."));
        assert!(!session.pending_matches(second, " second."));
        assert!(session.freeze_pending(&limits).0.is_some());
    }

    /// Manual override recovers all known pending text without reading target text.
    #[test]
    fn manual_override_restores_all_pending_text_in_document_order() {
        let limits = ContextConfig {
            pending_queue_size: 2,
            ..ContextConfig::default()
        };
        let mut session = session("first");
        let id = session.freeze_pending(&limits).0.unwrap();
        session.input(TypedInput::Text(" second".into()), &limits);
        session.freeze_pending(&limits);
        session.input(TypedInput::Text(" third".into()), &limits);
        session.restore_pending();
        assert_eq!(session.editable_context(), "first second third");
        assert!(!session.pending_matches(id, "first"));
    }

    /// Backspace invalidates frozen work only when it crosses the active context boundary.
    #[test]
    fn edits_in_new_context_keep_pending_but_crossing_boundary_discards_it() {
        let limits = ContextConfig::default();
        let mut session = session("teh");
        let id = session.freeze_pending(&limits).0.unwrap();
        session.input(TypedInput::Text("é".into()), &limits);
        session.input(TypedInput::Backspace, &limits);
        assert!(session.pending_matches(id, "teh"));
        session.input(TypedInput::Backspace, &limits);
        assert!(!session.pending_matches(id, "teh"));
        assert_eq!(session.editable_context(), "");
    }

    /// Movement, reanchoring, and buffer eviction invalidate the frozen range proof.
    #[test]
    fn movement_reanchor_and_buffer_eviction_invalidate_frozen_ranges() {
        let limits = ContextConfig::default();
        for input in [
            TypedInput::Left,
            TypedInput::Right,
            TypedInput::Delete,
            TypedInput::Uncertain(MovementSignal::MouseClick),
            TypedInput::Text("a".repeat(4096)),
        ] {
            let mut session = session("teh");
            let id = session.freeze_pending(&limits).0.unwrap();
            session.input(input, &limits);
            assert!(!session.pending_matches(id, "teh"));
        }
        let mut session = session("teh");
        let id = session.freeze_pending(&limits).0.unwrap();
        session.set_informative_context("different ".into(), &limits);
        assert!(!session.pending_matches(id, "teh"));
    }

    /// Initial informative capture cannot duplicate typed frozen text or lose its anchor.
    #[test]
    fn initial_capture_excludes_frozen_text_and_retains_its_anchor() {
        let limits = ContextConfig::default();
        let mut session = session("teh.");
        let id = session.freeze_pending(&limits).0.unwrap();
        session.input(TypedInput::Text(" next".into()), &limits);
        session.set_captured_informative_context("earlier ".into(), &limits);
        assert!(session.pending_matches(id, "teh."));
        assert!(session.complete_pending(id, "teh.", &limits));
        assert_eq!(session.informative_context(), "earlier teh.");
        assert_eq!(session.editable_context(), " next");
    }

    /// The executable word limit retires known original text and invalidates pending ranges.
    #[test]
    fn word_limit_commits_originals_and_invalidates_all_pending_work() {
        let limits = ContextConfig {
            executable_context_max_words: 1,
            ..ContextConfig::default()
        };
        let mut session = session("first ");
        let id = session.freeze_pending(&limits).0.unwrap();
        session.input(TypedInput::Text("next two".into()), &limits);
        assert!(!session.pending_matches(id, "first "));
        assert_eq!(session.informative_context(), "first next two");
        assert_eq!(session.editable_context(), "");
    }
}
