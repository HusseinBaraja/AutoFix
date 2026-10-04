//! Deterministic transition contracts: no native capture, engines, workers, or UI.

use super::*;
use crate::{
    background::{
        context_capture::captured_context, security::TriggerKind, target::FocusedElementId,
    },
    settings::{AppConfig, PendingQueueFullBehavior},
};

fn manager(limits: &ContextConfig) -> SessionManager {
    let mut manager = SessionManager::new(limits.clone());
    assert!(manager.focus(&super::tests::target(1, 10, Some("editor"))));
    manager
}

fn type_text(manager: &mut SessionManager, text: &str) {
    manager.input(TypedInput::Text(text.into()));
}

fn move_and_type(manager: &mut SessionManager, preceding: Option<&str>) -> MovementResolution {
    manager.input(TypedInput::Uncertain(MovementSignal::MouseClick));
    type_text(manager, "X");
    manager.resolve_movement(preceding)
}

#[test]
fn new_session_is_empty_and_same_focus_preserves_its_identity_and_state() {
    let mut manager = SessionManager::new(ContextConfig::default());
    assert!(manager.active().is_none());
    manager.input(TypedInput::Text("unfocused".into()));
    assert!(manager.sessions.is_empty());

    let target = super::tests::target(1, 10, Some("editor"));
    assert!(manager.focus(&target));
    let session = manager.active().unwrap();
    let id = session.id();
    assert_ne!(id, 0); // IDs are global; their exact values are not deterministic.
    assert_eq!(session.versions(), ContextVersions::default());
    assert!(session.informative_context().is_empty());
    assert!(session.executable_context().is_empty());
    assert!(session.pending_corrections.is_empty());
    assert!(session.frozen_segments.is_empty());
    assert!(session.correction_undo_history.is_empty());
    assert!(session.detected_language().is_none());
    assert!(!session.position_uncertain());

    type_text(&mut manager, "typed");
    manager
        .active_mut()
        .unwrap()
        .set_detected_language(Some("en".into()));
    let versions = manager.active().unwrap().versions();
    assert!(!manager.focus(&target));
    let session = manager.active().unwrap();
    assert_eq!(session.id(), id);
    assert_eq!(session.versions(), versions);
    assert_eq!(session.executable_context(), "typed");
    assert_eq!(session.detected_language(), Some("en"));

    manager.deactivate(MovementSignal::FocusChange);
    assert!(manager.focus(&target));
    let session = manager.active().unwrap();
    assert_ne!(session.id(), id);
    assert!(session.executable_context().is_empty());
    assert!(session.detected_language().is_none());
}

#[test]
fn smart_hybrid_fallback_changes_session_when_stronger_identity_becomes_available() {
    let mut manager = SessionManager::new(ContextConfig::default());
    let mut target = super::tests::target(7, 0, None);
    target.window_title.clear();
    let keys = [
        SessionKey::TemporaryActiveSession,
        SessionKey::ProcessTitle {
            process_id: 7,
            window_title: "Notes".into(),
        },
        SessionKey::WindowHandle(10),
        SessionKey::FocusedElement("automation:editor".into()),
        SessionKey::FocusedElement("runtime:editor".into()),
    ];
    let mut previous_id = None;
    for (stage, key) in keys.into_iter().enumerate() {
        match stage {
            1 => target.window_title = "Notes".into(),
            2 => target.window_handle = 10,
            3 => target.focused_element_id = Some(FocusedElementId::AutomationId("editor".into())),
            4 => target.focused_element_id = Some(FocusedElementId::RuntimeId("editor".into())),
            _ => {}
        }
        assert_eq!(target.session_key(), key);
        assert!(manager.focus(&target));
        assert!(manager.active_matches(&target));
        let session = manager.active().unwrap();
        assert_ne!(Some(session.id()), previous_id);
        previous_id = Some(session.id());
        assert!(session.executable_context().is_empty());
        assert!(session.informative_context().is_empty());
        type_text(&mut manager, "old");
        assert!(!manager.focus(&target));
    }
}

#[test]
fn informative_capture_excludes_typing_and_uses_nearest_boundary_or_word_limit() {
    let limits = ContextConfig {
        initial_context_words: 2,
        initial_context_boundary_chars: vec![".".into(), "؟".into(), "".into()],
        ..ContextConfig::default()
    };
    let cases = [
        (Some("older one two three é"), "two three "),
        (Some("older. one é"), " one "),
        (Some("older. سابق؟ جديد é"), " جديد "),
        (Some("already ends.é"), ""),
        (Some("foreign text"), ""),
        (None, ""),
    ];
    for (preceding, expected) in cases {
        let mut manager = manager(&limits);
        type_text(&mut manager, "é");
        let captured = captured_context(
            preceding,
            &manager.active().unwrap().executable_context(),
            &limits,
        );
        manager.set_captured_informative_context(captured);
        let session = manager.active().unwrap();
        assert_eq!(
            session.informative_context(),
            expected,
            "capture {preceding:?}"
        );
        assert_eq!(session.editable_context(), "é");
        if !expected.is_empty() {
            assert!(!session.can_complete_correction(None, &format!("{expected}é"), "bad"));
        }
    }
}

#[test]
fn typing_and_backspace_update_executable_versions_without_importing_informative_text() {
    let mut manager = manager(&ContextConfig::default());
    manager.set_informative_context("Read only. ".into());
    let initial = manager.active().unwrap().versions();
    for (step, (input, expected)) in [
        (TypedInput::Text("c".into()), "c"),
        (TypedInput::Text("afé".into()), "café"),
        (TypedInput::Backspace, "caf"),
        (TypedInput::Text("世界".into()), "caf世界"),
    ]
    .into_iter()
    .enumerate()
    {
        manager.input(input);
        let session = manager.active().unwrap();
        assert_eq!(session.executable_context(), expected);
        assert_eq!(session.informative_context(), "Read only. ");
        assert_eq!(
            session.versions(),
            ContextVersions {
                context: initial.context + step as u64 + 1,
                executable: initial.executable + step as u64 + 1,
                caret_anchor: initial.caret_anchor,
            }
        );
    }
    let versions = manager.active().unwrap().versions();
    type_text(&mut manager, "");
    assert_eq!(manager.active().unwrap().versions(), versions);
}

#[test]
fn correction_commit_consumes_once_and_undo_restores_only_the_recorded_suffix() {
    let limits = ContextConfig::default();
    let mut manager = manager(&limits);
    manager.set_informative_context("Old. ".into());
    type_text(&mut manager, "typed teh");
    let session = manager.active_mut().unwrap();
    let versions = session.versions();
    assert!(session.queue_correction("teh".into(), "the longer".into()));
    assert_eq!(session.informative_context(), "Old. ");
    assert_eq!(session.editable_context(), "typed teh");
    assert!(session.apply_next_correction(&limits));
    assert_eq!(session.informative_context(), "Old. typed the longer");
    assert!(session.editable_context().is_empty());
    assert_eq!(session.versions().context, versions.context + 1);
    assert_eq!(session.versions().executable, versions.executable + 1);
    assert_eq!(session.versions().caret_anchor, versions.caret_anchor);
    assert_eq!(session.correction_undo_history.len(), 1);
    assert!(!session.apply_next_correction(&limits));

    type_text(&mut manager, " 新");
    let session = manager.active_mut().unwrap();
    let versions = session.versions();
    let undo = session.undo_target().unwrap();
    assert_eq!(undo.original, "teh");
    assert_eq!(undo.corrected, "the longer");
    assert_eq!(undo.following, " 新");
    assert!(session.undo_last_correction(&limits));
    assert_eq!(session.informative_context(), "Old. typed teh");
    assert_eq!(session.editable_context(), " 新");
    assert_eq!(session.versions().context, versions.context + 1);
    assert_eq!(session.versions().executable, versions.executable);
    assert!(session.correction_undo_history.is_empty());
    assert!(!session.undo_last_correction(&limits));
}

#[test]
fn no_change_commit_creates_no_undo_and_empty_repeat_is_a_noop() {
    let limits = ContextConfig::default();
    let mut manager = manager(&limits);
    manager.set_informative_context("prefix ".into());
    type_text(&mut manager, "unchanged");
    let session = manager.active_mut().unwrap();
    session.complete_without_changes(&limits);
    assert_eq!(session.informative_context(), "prefix unchanged");
    assert!(session.executable_context().is_empty());
    assert!(session.correction_undo_history.is_empty());
    let versions = session.versions();
    session.complete_without_changes(&limits);
    assert_eq!(session.versions(), versions);
    assert!(!session.undo_last_correction(&limits));
}

#[test]
fn uncertain_position_cannot_commit_or_queue_corrections() {
    let limits = ContextConfig::default();
    let mut manager = manager(&limits);
    type_text(&mut manager, "teh");
    manager.input(TypedInput::Uncertain(MovementSignal::MouseClick));
    let session = manager.active_mut().unwrap();
    let versions = session.versions();
    session.complete_without_changes(&limits);
    assert!(!session.queue_correction("teh".into(), "the".into()));
    assert_eq!(session.freeze_pending(&limits), (None, vec![]));
    assert_eq!(session.executable_context(), "teh");
    assert!(session.informative_context().is_empty());
    assert_eq!(session.versions(), versions);
}

#[test]
fn shrinking_on_limit_reload_preserves_typing_and_fully_retained_undo() {
    let mut limits = ContextConfig::default();
    let mut manager = manager(&limits);
    manager.set_informative_context("discard. café ".into());
    type_text(&mut manager, "teh");
    let session = manager.active_mut().unwrap();
    assert!(session.queue_correction("teh".into(), "the".into()));
    assert!(session.apply_next_correction(&limits));
    type_text(&mut manager, " 世界");
    limits.informative_context_max_chars = 8;
    limits.informative_context_min_words = 2;
    manager.update_limits(limits.clone());
    let session = manager.active_mut().unwrap();
    assert_eq!(session.informative_context(), "café the");
    assert_eq!(session.editable_context(), " 世界");
    assert!(session.undo_last_correction(&limits));
    assert_eq!(session.informative_context(), "café teh");
    assert_eq!(session.editable_context(), " 世界");
}

#[test]
fn backward_movement_keeps_only_the_typed_prefix_editable_and_preserves_suffix() {
    let mut manager = manager(&ContextConfig::default());
    type_text(&mut manager, "éabc");
    let id = manager.active().unwrap().id();
    assert_eq!(
        move_and_type(&mut manager, Some("éX")),
        MovementResolution::Continued
    );
    let session = manager.active().unwrap();
    assert_eq!(session.id(), id);
    assert_eq!(session.editable_context(), "éX");
    assert!(session.informative_context().is_empty());
    assert!(!session.can_complete_correction(None, "abc", "unsafe"));
    manager.input(TypedInput::Right);
    assert_eq!(
        manager.resolve_movement(Some("éXa")),
        MovementResolution::Continued
    );
    assert_eq!(manager.active().unwrap().editable_context(), "éXa");
}

#[test]
fn forward_movement_at_or_below_limit_keeps_skipped_words_read_only() {
    for gap in ["", " one ", " one\t世界 "] {
        let limits = ContextConfig {
            forward_movement_word_limit: 2,
            ..ContextConfig::default()
        };
        let mut manager = manager(&limits);
        type_text(&mut manager, "typed");
        let preceding = format!("typed{gap}X");
        assert_eq!(
            move_and_type(&mut manager, Some(&preceding)),
            MovementResolution::Continued
        );
        let session = manager.active_mut().unwrap();
        assert_eq!(
            session.editable_context(),
            if gap.is_empty() { "typedX" } else { "X" }
        );
        assert_eq!(session.known_before_caret(), preceding);
        assert!(session.informative_context().is_empty());
        assert!(session
            .frozen_segments
            .iter()
            .all(|segment| !segment.final_fix));
        if !gap.is_empty() {
            assert_eq!(session.trigger_context(), "typed X");
            assert!(!session.can_complete_correction(None, &preceding, "unsafe"));
        }
    }
}

#[test]
fn forward_movement_above_limit_submits_one_final_fix_and_keeps_new_typing() {
    let mut config = AppConfig::default();
    config.context.forward_movement_word_limit = 2;
    let mut manager = manager(&config.context);
    type_text(&mut manager, "teh");
    assert_eq!(
        move_and_type(&mut manager, Some("teh one two 世界 X")),
        MovementResolution::Reanchor {
            final_fix: Some("teh".into())
        }
    );
    let session = manager.active_mut().unwrap();
    let requests = session.movement_requests(None, &config);
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].trigger, TriggerKind::FinalFixBeforeReanchor);
    assert_eq!(requests[0].executable_context, "teh");
    assert_eq!(session.trigger_context(), "X");
    assert!(session.movement_requests(None, &config).is_empty());
    let id = requests[0].pending_segment_id.unwrap();
    assert_eq!(
        session.pending_following_text(id).as_deref(),
        Some(" one two 世界 X")
    );
    assert!(session.complete_pending(id, "the", &config.context));
    assert_eq!(session.informative_context(), "the one two 世界 ");
    assert_eq!(session.editable_context(), "X");
    assert!(!session.complete_pending(id, "duplicate", &config.context));
}

#[test]
fn reanchor_replaces_informative_context_and_revokes_pending_result_ownership() {
    let limits = ContextConfig::default();
    let mut manager = manager(&limits);
    manager.set_informative_context("old ".into());
    type_text(&mut manager, "teh");
    let session = manager.active_mut().unwrap();
    let id = session.freeze_pending(&limits).0.unwrap();
    let versions = session.versions();
    assert_eq!(
        move_and_type(&mut manager, Some("Elsewhere. new area X")),
        MovementResolution::Reanchor { final_fix: None }
    );
    let session = manager.active_mut().unwrap();
    assert_eq!(session.informative_context(), " new area ");
    assert_eq!(session.editable_context(), "X");
    assert!(session.versions().context > versions.context);
    assert!(session.versions().caret_anchor > versions.caret_anchor);
    assert!(!session.pending_matches(id, "teh"));
    assert!(!session.complete_pending(id, "the", &limits));
    assert!(session.correction_undo_history.is_empty());
}

#[test]
fn final_fix_requires_complete_old_span_proved_before_the_new_caret() {
    for (preceding, eligible) in [
        (Some("teh gap X"), true), // Pending work makes even a short hop final.
        (Some("tX"), false),       // Old suffix lies after the caret.
        (Some("other teh gap X"), false), // Repeated text is not an origin anchor.
        (Some("foreign X"), false),
        (None, false),
    ] {
        let config = AppConfig::default();
        let mut manager = manager(&config.context);
        type_text(&mut manager, "teh");
        let old_id = manager
            .active_mut()
            .unwrap()
            .freeze_pending(&config.context)
            .0
            .unwrap();
        assert_eq!(
            move_and_type(&mut manager, preceding),
            MovementResolution::Reanchor {
                final_fix: eligible.then(|| "teh".into())
            }
        );
        let session = manager.active_mut().unwrap();
        assert!(!session.pending_matches(old_id, "teh"));
        let requests = session.movement_requests(None, &config);
        assert_eq!(
            requests.len(),
            usize::from(eligible),
            "capture {preceding:?}"
        );
        assert_eq!(session.editable_context(), "X");
        if eligible {
            let id = requests[0].pending_segment_id.unwrap();
            assert_ne!(id, old_id);
            assert!(session.retire_final_pending(id, &config.context));
            assert!(session.movement_requests(None, &config).is_empty());
            assert_eq!(session.informative_context(), "teh gap ");
            assert!(session.correction_undo_history.is_empty());
        }
    }
}

#[test]
fn pending_queue_counts_running_segments_and_releases_capacity_on_commit() {
    for capacity in [1, 2, 16] {
        let limits = ContextConfig {
            pending_queue_size: capacity,
            ..ContextConfig::default()
        };
        let mut manager = manager(&limits);
        let mut ids = Vec::new();
        for _ in 0..capacity {
            type_text(&mut manager, "é ");
            let session = manager.active_mut().unwrap();
            ids.push(session.freeze_pending(&limits).0.unwrap());
            assert!(session.editable_context().is_empty());
        }
        type_text(&mut manager, "tail ");
        let session = manager.active_mut().unwrap();
        assert_eq!(session.frozen_segments.len(), usize::from(capacity));
        assert!(ids.iter().all(|id| session.pending_submitted(*id)));
        assert_eq!(session.freeze_pending(&limits), (None, vec![]));
        assert_eq!(session.editable_context(), "tail ");
        assert!(session.complete_pending(ids[0], "é ", &limits));
        let new_id = session.freeze_pending(&limits).0.unwrap();
        assert!(!ids.contains(&new_id));
        assert_eq!(session.frozen_segments.len(), usize::from(capacity));
        assert!(session.correction_undo_history.is_empty());
    }
}

#[test]
fn pending_queue_overflow_policies_preserve_unchecked_text_and_cancel_dependents() {
    for behavior in [
        PendingQueueFullBehavior::SkipNew,
        PendingQueueFullBehavior::CancelOldest,
        PendingQueueFullBehavior::MergeNewest,
    ] {
        let limits = ContextConfig {
            pending_queue_size: 2,
            pending_queue_full_behavior: behavior,
            ..ContextConfig::default()
        };
        let mut manager = manager(&limits);
        type_text(&mut manager, "one ");
        let first = manager
            .active_mut()
            .unwrap()
            .freeze_pending(&limits)
            .0
            .unwrap();
        type_text(&mut manager, "二 ");
        let second = manager
            .active_mut()
            .unwrap()
            .freeze_pending(&limits)
            .0
            .unwrap();
        type_text(&mut manager, "tail");
        let session = manager.active_mut().unwrap();
        let (new, cancelled) = session.freeze_pending(&limits);
        match behavior {
            PendingQueueFullBehavior::SkipNew => {
                assert_eq!((new, cancelled), (None, vec![]));
                assert_eq!(session.editable_context(), "tail");
                assert!(session.pending_matches(first, "one "));
                assert!(session.pending_matches(second, "二 "));
            }
            PendingQueueFullBehavior::CancelOldest => {
                assert_eq!(cancelled, vec![first, second]);
                assert!(session.pending_matches(new.unwrap(), "one 二 tail"));
                assert!(session.editable_context().is_empty());
                assert!(!session.pending_matches(first, "one "));
                assert!(!session.pending_matches(second, "二 "));
            }
            PendingQueueFullBehavior::MergeNewest => {
                assert_eq!((new, cancelled), (None, vec![second]));
                assert_eq!(session.editable_context(), "二 tail");
                assert!(session.pending_matches(first, "one "));
                assert!(!session.pending_matches(second, "二 "));
            }
        }
        assert_eq!(session.known_before_caret(), "one 二 tail");
        assert!(session.informative_context().is_empty());
    }
}

#[test]
fn changed_context_version_discards_queued_result_even_when_typing_returns_to_original() {
    let limits = ContextConfig::default();
    let mut manager = manager(&limits);
    type_text(&mut manager, "teh");
    let session = manager.active_mut().unwrap();
    assert!(session.queue_correction("teh".into(), "the".into()));
    let versions = session.versions();
    type_text(&mut manager, "!");
    manager.input(TypedInput::Backspace);
    let session = manager.active_mut().unwrap();
    assert_eq!(session.editable_context(), "teh");
    assert!(session.versions().context > versions.context);
    assert_eq!(session.versions().caret_anchor, versions.caret_anchor);
    assert!(!session.apply_next_correction(&limits));
    assert!(session.informative_context().is_empty());
    assert!(session.correction_undo_history.is_empty());
}
