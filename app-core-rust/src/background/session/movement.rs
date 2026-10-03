//! Caret movement retains typed ranges separately from read-only skipped text.
use super::{ContextConfig, MovementResolution, Session};
use crate::background::{security::TriggerKind, triggers, typing::TypedInput};
use crate::settings::AppConfig;

impl Session {
    /// Movement can leave several disjoint typed spans, but only the configured
    /// number of requests may run. Retained ownership is bounded independently.
    pub(crate) fn reserve_movement_tail(&mut self, trigger: Option<TriggerKind>) -> Option<u64> {
        let original = self.editable_context();
        if self.position_uncertain()
            || original.trim().is_empty()
            || self.frozen_segments.len() >= 16
        {
            return None;
        }
        let id = self.next_segment_id;
        self.next_segment_id = self
            .next_segment_id
            .checked_add(1)
            .expect("segment ID exhausted");
        self.correction_floor += original.chars().count();
        self.frozen_segments.push_back(super::FrozenSegment {
            id,
            original,
            caret_anchor: self.versions.caret_anchor,
            gap: String::new(),
            movement: true,
            final_fix: false,
            submitted: false,
            queued_trigger: trigger,
            retire: false,
        });
        Some(id)
    }
    /// Resolve a backward move into retained typing without crossing a read-only
    /// gap or retaining an unproved suffix from a later span.
    pub(super) fn continue_retained_backward(&mut self, preceding: &str, inserted: &str) -> bool {
        let mut prefix = self.informative_context.clone();
        let mut typed_chars = 0;
        let mut found = None;
        for (index, segment) in self.frozen_segments.iter().enumerate() {
            for offset in segment
                .original
                .char_indices()
                .map(|(offset, _)| offset)
                .chain(std::iter::once(segment.original.len()))
            {
                let candidate = format!("{}{}", prefix, &segment.original[..offset]);
                let anchored = if prefix.is_empty() {
                    preceding == candidate
                } else {
                    super::matching_anchor(
                        &candidate,
                        segment.original[..offset].chars().count(),
                        prefix.chars().count(),
                        preceding.chars().count(),
                    )
                    .is_some_and(|anchor| preceding.ends_with(anchor))
                };
                if anchored {
                    if found.is_some() {
                        return false;
                    }
                    found = Some((
                        index,
                        typed_chars + segment.original[..offset].chars().count(),
                    ));
                }
            }
            prefix.push_str(&segment.original);
            prefix.push_str(&segment.gap);
            typed_chars += segment.original.chars().count();
        }
        let Some((index, caret)) = found else {
            return false;
        };
        let typed: String = self
            .frozen_segments
            .iter()
            .take(index + 1)
            .map(|segment| segment.original.as_str())
            .collect();
        self.frozen_segments.truncate(index);
        self.correction_floor = self
            .frozen_segments
            .iter()
            .map(|segment| segment.original.chars().count())
            .sum();
        self.executable.clear_executable();
        self.executable.input(TypedInput::Text(typed));
        self.executable.set_caret(caret);
        self.executable.input(TypedInput::Text(inserted.to_owned()));
        self.refresh_movement_anchors();
        self.versions.context = self.versions.context.wrapping_add(1);
        self.versions.executable = self.versions.executable.wrapping_add(1);
        true
    }
    pub(crate) fn has_movement_context(&self) -> bool {
        self.frozen_segments.iter().any(|segment| segment.movement)
    }

    pub(crate) fn retire_final_work(&mut self, limits: &ContextConfig) {
        while let Some(segment) = self.frozen_segments.front() {
            if !segment.final_fix || self.position_uncertain() {
                break;
            }
            let id = segment.id();
            if !self.retire_final_pending(id, limits) {
                break;
            }
        }
    }
    /// Count retained, unchecked typing across small forward gaps in the same context.
    pub(crate) fn trigger_context(&self) -> String {
        let mut text = String::new();
        for segment in &self.frozen_segments {
            if segment.movement && !segment.final_fix && !segment.submitted {
                text.push_str(&segment.original);
                if !segment.gap.is_empty() {
                    text.push(' ');
                }
            }
        }
        text.push_str(&self.editable_context());
        text
    }

    pub(crate) fn resumed_typing(&self) -> Option<String> {
        self.pending_movement
            .as_ref()
            .map(|pending| pending.typed_after.clone())
    }
    /// Reconstruct the physical pre-caret anchor without making gaps executable.
    pub(crate) fn known_before_caret(&self) -> String {
        let mut text = self.correction_informative_context();
        text.push_str(&self.editable_context());
        text
    }

    pub(super) fn refresh_movement_anchors(&mut self) {
        for segment in &mut self.frozen_segments {
            segment.caret_anchor = self.versions.caret_anchor;
        }
    }

    /// Keep the same typed context across a small gap. A long move reserves one
    /// final check on the old span and starts a fresh active span at the new caret.
    pub(super) fn continue_forward(
        &mut self,
        old: &str,
        gap: &str,
        typed_after: String,
        final_fix: bool,
        limits: &ContextConfig,
    ) -> MovementResolution {
        // Forget the previously tracked post-caret suffix; captured gap text is
        // read-only even if some of it was typed earlier in this session.
        let retained = self.executable_context();
        self.executable.clear_executable();
        self.executable.input(TypedInput::Text(retained));
        self.refresh_movement_anchors();
        while let Some(segment) = self.frozen_segments.front() {
            if !segment.final_fix || !segment.submitted {
                break;
            }
            if !self.retire_final_pending(segment.id(), limits) {
                break;
            }
        }
        if !gap.is_empty() || final_fix {
            if let Some(id) = self.reserve_movement_tail(None) {
                let segment = self
                    .frozen_segments
                    .iter_mut()
                    .find(|segment| segment.id() == id)
                    .unwrap();
                segment.gap = gap.to_owned();
                segment.movement = true;
                segment.final_fix = final_fix;
                segment.submitted = false;
            } else {
                // Capacity refusal cannot merge skipped text into typed text.
                let preceding = format!("{}{gap}", self.known_before_caret());
                return self.reanchor_after_movement(Some(&preceding), typed_after, None, limits);
            }
        }
        if final_fix {
            for segment in &mut self.frozen_segments {
                if !segment.final_fix {
                    segment.final_fix = true;
                    segment.submitted = false;
                }
            }
        }
        self.executable.input(TypedInput::Text(typed_after));
        self.versions.context = self.versions.context.wrapping_add(1);
        self.versions.executable = self.versions.executable.wrapping_add(1);
        if final_fix {
            MovementResolution::Reanchor {
                final_fix: Some(old.to_owned()),
            }
        } else {
            MovementResolution::Continued
        }
    }

    /// A retained span is submitted once per trigger; final work is submitted
    /// immediately after position proof, before work on the new active span.
    pub(crate) fn movement_requests(
        &mut self,
        trigger: Option<TriggerKind>,
        config: &AppConfig,
    ) -> Vec<triggers::CorrectionRequest> {
        if self.position_uncertain() {
            return Vec::new();
        }
        self.retire_rejected_final_work(&config.context);
        let mut requests = Vec::new();
        let mut informative = self.informative_context.clone();
        let mut slots = usize::from(config.context.pending_queue_size.clamp(1, 16)).saturating_sub(
            self.frozen_segments
                .iter()
                .filter(|segment| segment.submitted)
                .count(),
        );
        for segment in &mut self.frozen_segments {
            if segment.movement && !segment.submitted {
                if let Some(trigger) = trigger {
                    segment.queued_trigger = Some(trigger);
                }
            }
            if slots > 0
                && segment.movement
                && !segment.submitted
                && (segment.final_fix || segment.queued_trigger.is_some())
            {
                let kind = if segment.final_fix {
                    TriggerKind::FinalFixBeforeReanchor
                } else {
                    segment.queued_trigger.unwrap()
                };
                if let Some(mut request) = triggers::retained(
                    self.id,
                    kind,
                    &informative,
                    &segment.original,
                    self.versions,
                    config,
                ) {
                    request.pending_segment_id = Some(segment.id());
                    segment.submitted = true;
                    slots -= 1;
                    requests.push(request);
                }
            }
            informative.push_str(&segment.original);
            informative.push_str(&segment.gap);
        }
        requests
    }

    /// Release failed final work without retrying or changing the document.
    pub(crate) fn retire_final_pending(&mut self, id: u64, limits: &ContextConfig) -> bool {
        if let Some(segment) = self
            .frozen_segments
            .iter_mut()
            .find(|segment| segment.id() == id && segment.final_fix)
        {
            segment.retire = true;
        }
        self.retire_rejected_final_work(limits);
        !self
            .frozen_segments
            .iter()
            .any(|segment| segment.id() == id)
    }

    fn retire_rejected_final_work(&mut self, limits: &ContextConfig) {
        while let Some(segment) = self.frozen_segments.front() {
            if !segment.retire || self.position_uncertain() {
                break;
            }
            let id = segment.id();
            let original = segment.original.clone();
            if !self.complete_pending(id, &original, limits) {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::background::{session::SessionManager, typing::MovementSignal};

    fn manager() -> SessionManager {
        let mut manager = SessionManager::new(ContextConfig::default());
        manager.focus(&super::super::tests::target(1, 10, None));
        manager
    }

    #[test]
    fn uncertain_position_keeps_context_until_nonempty_typing_resumes() {
        let mut manager = manager();
        manager.input(TypedInput::Text("typed".into()));
        manager.input(TypedInput::Uncertain(MovementSignal::MouseClick));
        let versions = manager.active().unwrap().versions();
        manager.resolve_movement(Some("elsewhere"));
        manager.input(TypedInput::Text(String::new()));
        manager.resolve_movement(None);
        let session = manager.active().unwrap();
        assert!(session.position_uncertain());
        assert_eq!(session.versions(), versions);
        assert_eq!(session.executable_context(), "typed");
    }

    #[test]
    fn repeated_small_hops_and_backward_typing_preserve_only_owned_text() {
        let mut manager = manager();
        manager.input(TypedInput::Text("abcd".into()));
        manager.input(TypedInput::Uncertain(MovementSignal::MouseClick));
        manager.input(TypedInput::Text("é".into()));
        manager.resolve_movement(Some("abcd gap é"));
        manager.input(TypedInput::Uncertain(MovementSignal::MouseClick));
        manager.input(TypedInput::Text("Z".into()));
        manager.resolve_movement(Some("abcd gap é gap Z"));
        assert_eq!(manager.active().unwrap().executable_context(), "abcdéZ");
        assert_eq!(
            manager.active().unwrap().known_before_caret(),
            "abcd gap é gap Z"
        );
        manager.input(TypedInput::Uncertain(MovementSignal::MouseClick));
        manager.input(TypedInput::Text("X".into()));
        assert_eq!(
            manager.resolve_movement(Some("abX")),
            MovementResolution::Continued
        );
        assert_eq!(manager.active().unwrap().executable_context(), "abX");
        manager.input(TypedInput::Right);
        manager.input(TypedInput::Text("Y".into()));
        manager.resolve_movement(None);
        assert_eq!(manager.active().unwrap().executable_context(), "abXcY");
    }

    #[test]
    fn typing_at_two_unresolved_positions_is_never_joined() {
        let mut manager = manager();
        manager.input(TypedInput::Text("old".into()));
        manager.input(TypedInput::Uncertain(MovementSignal::MouseClick));
        manager.input(TypedInput::Text("first".into()));
        manager.input(TypedInput::Uncertain(MovementSignal::MouseClick));
        manager.input(TypedInput::Text("second".into()));
        manager.resolve_movement(Some("other second"));
        assert_eq!(manager.active().unwrap().executable_context(), "second");
        assert_eq!(manager.active().unwrap().informative_context(), "other ");
    }
}
