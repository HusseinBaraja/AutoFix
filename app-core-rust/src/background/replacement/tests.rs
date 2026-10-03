use super::*;
use crate::{
    background::{
        context_capture::SelectionCapture, session::SessionManager, target::FocusedElementId,
        triggers, typing::TypedInput,
    },
    settings::AppConfig,
};
use std::cell::RefCell;

fn target() -> FocusedTarget {
    FocusedTarget {
        process_id: 1,
        process_name: "editor.exe".into(),
        window_handle: 1,
        window_title: "Test".into(),
        focused_element_id: Some(FocusedElementId::RuntimeId("editor".into())),
        is_elevated: false,
        is_password_or_protected: false,
        is_hidden_or_unavailable: false,
        field_safety_known: true,
        is_secure_desktop: false,
        is_lock_screen: false,
        is_credential_dialog: false,
    }
}
const STAMP: InputStamp = InputStamp {
    position: 1,
    sequence: 2,
};

/// Pre-mutation authorization rejects title, process policy, and protection changes.
#[test]
fn correction_and_undo_refuse_changed_policy_attributes_without_input_movement() {
    let authorized = target();
    assert!(native::same_authorized_target(&authorized, &authorized));
    let mut changed = authorized.clone();
    changed.window_title = "Private document".into();
    assert!(!native::same_authorized_target(&authorized, &changed));
    changed = authorized.clone();
    changed.process_name = "other.exe".into();
    assert!(!native::same_authorized_target(&authorized, &changed));
    changed = authorized.clone();
    changed.is_password_or_protected = true;
    assert!(!native::same_authorized_target(&authorized, &changed));
}

/// Post-edit title markers are accepted without relaxing process, element, or security checks.
#[test]
fn post_mutation_title_change_keeps_identity_and_security_guards() {
    let authorized = target();
    let mut changed = authorized.clone();
    changed.window_title = "*Test".into();
    assert!(native::same_mutated_target(&authorized, &changed));
    for mutate in [
        (|t: &mut FocusedTarget| t.process_id += 1) as fn(&mut FocusedTarget),
        |t| t.window_handle += 1,
        |t| t.process_name = "other.exe".into(),
        |t| t.focused_element_id = None,
        |t| t.is_password_or_protected = true,
        |t| t.is_elevated = true,
        |t| t.is_hidden_or_unavailable = true,
        |t| t.field_safety_known = false,
        |t| t.is_secure_desktop = true,
        |t| t.is_credential_dialog = true,
    ] {
        let mut unsafe_target = changed.clone();
        mutate(&mut unsafe_target);
        assert!(!native::same_mutated_target(&authorized, &unsafe_target));
    }
}

#[test]
fn disabling_clipboard_skips_its_preparation_and_uses_fallback() {
    let target = target();
    let plan = ReplacementPlan {
        target: &target,
        original: "teh",
        replacement: "the",
        following: "",
        selected_text: false,
        stamp: STAMP,
    };
    let calls = RefCell::new(Vec::new());
    let mut clipboard = FakeStrategy {
        method: ReplacementMethod::Clipboard,
        calls: &calls,
        result: Some(ReplacementResult {
            success: true,
            method: Some(ReplacementMethod::Clipboard),
            range: Some(plan.range()),
            reason: None,
            may_have_changed: true,
        }),
    };
    let mut fallback = FakeStrategy {
        method: ReplacementMethod::SendInput,
        calls: &calls,
        result: Some(ReplacementResult {
            success: true,
            method: Some(ReplacementMethod::SendInput),
            range: Some(plan.range()),
            reason: None,
            may_have_changed: true,
        }),
    };
    let result = run_strategies(&plan, &mut [&mut clipboard, &mut fallback], false);
    assert!(result.success);
    assert_eq!(result.method, Some(ReplacementMethod::SendInput));
    assert_eq!(*calls.borrow(), [ReplacementMethod::SendInput]);
}

struct FakeStrategy<'a> {
    method: ReplacementMethod,
    calls: &'a RefCell<Vec<ReplacementMethod>>,
    result: Option<ReplacementResult>,
}
impl ReplacementStrategy for FakeStrategy<'_> {
    fn method(&self) -> ReplacementMethod {
        self.method
    }
    fn replace(&mut self, _: &ReplacementPlan<'_>) -> Attempt {
        self.calls.borrow_mut().push(self.method);
        self.result.take().map_or_else(
            || Attempt::Unavailable("capability unavailable".into()),
            Attempt::Finished,
        )
    }
}

#[test]
fn strategy_order_and_preparation_failures_allow_fallback() {
    let target = target();
    let plan = ReplacementPlan {
        target: &target,
        original: "teh",
        replacement: "the",
        following: " newer",
        selected_text: false,
        stamp: STAMP,
    };
    let calls = RefCell::new(Vec::new());
    let methods = [
        ReplacementMethod::DirectTextApi,
        ReplacementMethod::UiAutomation,
        ReplacementMethod::Clipboard,
        ReplacementMethod::SendInput,
    ];
    let mut strategies: Vec<_> = methods
        .iter()
        .map(|method| FakeStrategy {
            method: *method,
            calls: &calls,
            result: (*method == ReplacementMethod::SendInput).then(|| ReplacementResult {
                success: true,
                method: Some(*method),
                range: Some(plan.range()),
                reason: None,
                may_have_changed: true,
            }),
        })
        .collect();
    let mut refs: Vec<&mut dyn ReplacementStrategy> = strategies
        .iter_mut()
        .map(|s| s as &mut dyn ReplacementStrategy)
        .collect();
    let result = run_strategies(&plan, &mut refs, true);
    assert!(result.success);
    assert_eq!(*calls.borrow(), methods);
    assert_eq!(
        result.range,
        Some(ReplacedRange {
            start_back: 9,
            end_back: 6
        })
    );
}

#[test]
fn successful_safer_method_never_reaches_send_input() {
    for method in [
        ReplacementMethod::DirectTextApi,
        ReplacementMethod::UiAutomation,
        ReplacementMethod::Clipboard,
    ] {
        let target = target();
        let plan = ReplacementPlan {
            target: &target,
            original: "teh",
            replacement: "the",
            following: "",
            selected_text: false,
            stamp: STAMP,
        };
        let calls = RefCell::new(Vec::new());
        let mut safer = FakeStrategy {
            method,
            calls: &calls,
            result: Some(ReplacementResult {
                success: true,
                method: Some(method),
                range: Some(plan.range()),
                reason: None,
                may_have_changed: true,
            }),
        };
        let mut fallback = FakeStrategy {
            method: ReplacementMethod::SendInput,
            calls: &calls,
            result: None,
        };
        let result = run_strategies(&plan, &mut [&mut safer, &mut fallback], true);
        assert!(result.success);
        assert_eq!(result.method, Some(method));
        assert_eq!(*calls.borrow(), [method]);
    }
}

#[test]
fn no_retry_after_paste_partial_input_or_failed_verification() {
    for reason in [
        "clipboard paste timed out",
        "partial SendInput",
        "verification failed",
        "clipboard restore failed",
    ] {
        let target = target();
        let plan = ReplacementPlan {
            target: &target,
            original: "teh",
            replacement: "the",
            following: "",
            selected_text: false,
            stamp: STAMP,
        };
        let calls = RefCell::new(Vec::new());
        let mut clipboard = FakeStrategy {
            method: ReplacementMethod::Clipboard,
            calls: &calls,
            result: Some(ReplacementResult {
                success: false,
                method: Some(ReplacementMethod::Clipboard),
                range: Some(plan.range()),
                reason: Some(reason.into()),
                may_have_changed: true,
            }),
        };
        let mut fallback = FakeStrategy {
            method: ReplacementMethod::SendInput,
            calls: &calls,
            result: None,
        };
        let result = run_strategies(&plan, &mut [&mut clipboard, &mut fallback], true);
        assert!(!result.success);
        assert!(result.may_have_changed);
        assert_eq!(result.reason.as_deref(), Some(reason));
        assert_eq!(*calls.borrow(), [ReplacementMethod::Clipboard]);
    }
}

#[test]
fn unavailable_result_has_method_reason_and_unknown_range() {
    let target = target();
    let plan = ReplacementPlan {
        target: &target,
        original: "teh",
        replacement: "the",
        following: "",
        selected_text: false,
        stamp: STAMP,
    };
    let calls = RefCell::new(Vec::new());
    let mut strategy = FakeStrategy {
        method: ReplacementMethod::SendInput,
        calls: &calls,
        result: None,
    };
    let result = run_strategies(&plan, &mut [&mut strategy], true);
    assert!(!result.success);
    assert_eq!(result.method, Some(ReplacementMethod::SendInput));
    assert_eq!(result.range, None);
    assert_eq!(result.reason.as_deref(), Some("capability unavailable"));
}

#[test]
fn exact_range_counts_unicode_characters_and_preserves_following_text() {
    let target = target();
    let plan = ReplacementPlan {
        target: &target,
        original: "é😃",
        replacement: "new",
        following: " العربية",
        selected_text: false,
        stamp: STAMP,
    };
    assert_eq!(
        plan.range(),
        ReplacedRange {
            start_back: 10,
            end_back: 8
        }
    );
}

#[test]
fn protected_selected_empty_suppressed_and_failed_requests_never_reach_native_strategies() {
    let config = AppConfig::default();
    let mut manager = SessionManager::new(config.context.clone());
    manager.focus(&target());
    manager.input(TypedInput::Text("teh".into()));
    let session = manager.active().unwrap();
    let request = triggers::manual(
        session.id(),
        "informative stays read-only",
        &session.editable_context(),
        session.versions(),
        &SelectionCapture::NoSelection,
        &config,
    )
    .unwrap();
    let mut output = CorrectionOutput::changed("the".into(), ConfidenceTier::High, None, 0);
    output.behavior = ConfidenceBehavior::Silent;
    for case in 0..7 {
        let mut target = target();
        let mut request = request.clone();
        let mut output = output.clone();
        match case {
            0 => target.is_password_or_protected = true,
            1 => {
                request.selected_text = true;
                request.replacement_following_text = "later".into();
            }
            2 => request.executable_context.clear(),
            3 => output.behavior = ConfidenceBehavior::Suggestion,
            4 => output.confidence = ConfidenceTier::Low,
            5 => output.status = EngineStatus::TimedOut,
            6 => output.corrected_executable_text = "bad\0text".into(),
            _ => unreachable!(),
        }
        let result = ReplacementEngine::replace(&target, &request, &output, STAMP, true);
        assert!(!result.success);
        assert_eq!(result.method, None);
        assert_eq!(result.range, None);
        assert!(result.reason.is_some());
        assert!(!result.may_have_changed);
    }
}

#[test]
fn app_undo_targets_only_recorded_span_and_preserves_new_typing() {
    let config = AppConfig::default();
    let mut manager = SessionManager::new(config.context.clone());
    manager.focus(&target());
    manager.input(TypedInput::Text("teh".into()));
    let session = manager.active_mut().unwrap();
    assert!(session.undo_target().is_none());
    assert!(session.queue_correction("teh".into(), "the".into()));
    assert!(session.apply_next_correction(&config.context));
    manager.input(TypedInput::Text(" next".into()));
    let undo = manager.active().unwrap().undo_target().unwrap();
    assert_eq!(undo.corrected, "the");
    assert_eq!(undo.original, "teh");
    assert_eq!(undo.following, " next");
}

#[test]
fn app_undo_refuses_partially_retained_correction() {
    let mut config = AppConfig::default();
    config.context.informative_context_max_chars = 2;
    let mut manager = SessionManager::new(config.context.clone());
    manager.focus(&target());
    manager.input(TypedInput::Text("teh".into()));
    let session = manager.active_mut().unwrap();
    assert!(session.queue_correction("teh".into(), "the".into()));
    assert!(session.apply_next_correction(&config.context));
    assert!(session.undo_target().is_none());
}

/// Opt-in test owns its editor and returns focus/clipboard on exit. Default tests
/// never inject input into the user's desktop or touch the global clipboard.
#[cfg(windows)]
#[test]
#[ignore = "requires an interactive Windows desktop; briefly focuses an isolated test editor"]
fn native_edit_replacement_smoke() {
    const EM_SETSEL: u32 = 0x00b1;
    use crate::background::security::TriggerKind;
    use std::{
        io::{BufRead, BufReader, Read},
        os::windows::process::CommandExt,
        process::{Command, Stdio},
        thread,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(Some(0)).collect()
    }
    let previous = unsafe { GetForegroundWindow() } as isize;
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "native_edit_test_host",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("AUTOFIX_NATIVE_TEST_HOST", "1")
        .creation_flags(0x08000000) // CREATE_NO_WINDOW: the child owns only its test editor.
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let host = stdout
        .by_ref()
        .lines()
        .map(|line| line.unwrap())
        .find(|line| line.contains("AUTOFIX_EDITOR "))
        .unwrap();
    let mut host = host
        .split("AUTOFIX_EDITOR ")
        .nth(1)
        .unwrap()
        .split_whitespace();
    let thread_id: u32 = host.next().unwrap().parse().unwrap();
    let edit: isize = host.next().unwrap().parse().unwrap();
    unsafe {
        let root = GetAncestor(edit as _, GA_ROOT);
        SetForegroundWindow(root);
        PostMessageW(root, WM_APP + 1, 0, 0);
    }
    struct Editor {
        thread_id: u32,
        child: std::process::Child,
        stdout: BufReader<std::process::ChildStdout>,
        previous: isize,
    }
    impl Drop for Editor {
        fn drop(&mut self) {
            unsafe {
                PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0);
                self.stdout.read_to_end(&mut Vec::new()).unwrap();
                self.child.wait().unwrap();
                if self.previous != 0 {
                    SetForegroundWindow(self.previous as _);
                }
            }
        }
    }
    let _editor = Editor {
        thread_id,
        child,
        stdout,
        previous,
    };
    // Foreground activation is asynchronous and may be denied without attaching
    // this test's input queue to its own editor thread. Never accept another app.
    let focus_deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        unsafe {
            use windows_sys::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
            let current_thread = GetCurrentThreadId();
            let mut message: MSG = std::mem::zeroed();
            PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_NOREMOVE);
            let foreground_thread =
                GetWindowThreadProcessId(GetForegroundWindow(), std::ptr::null_mut());
            let foreground_attached = foreground_thread != current_thread
                && AttachThreadInput(current_thread, foreground_thread, 1) != 0;
            let attached = AttachThreadInput(current_thread, thread_id, 1) != 0;
            SetForegroundWindow(GetAncestor(edit as _, GA_ROOT));
            windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus(edit as _);
            PostMessageW(GetAncestor(edit as _, GA_ROOT), WM_APP + 1, 0, 0);
            if attached {
                AttachThreadInput(current_thread, thread_id, 0);
            }
            if foreground_attached {
                AttachThreadInput(current_thread, foreground_thread, 0);
            }
        }
        if matches!(super::super::target::detect_focused_target(),
            super::super::target::TargetDetection::Available(target) if target.process_id == _editor.child.id())
        {
            break;
        }
        assert!(
            std::time::Instant::now() < focus_deadline,
            "test editor could not acquire foreground focus"
        );
        thread::sleep(std::time::Duration::from_millis(25));
    }
    // Both correction and undo use this boundary. A title-only change must
    // refuse every native method while the selection and typed span stay intact.
    let super::super::target::TargetDetection::Available(authorized) =
        super::super::target::detect_focused_target()
    else {
        panic!("test editor is not available")
    };
    let stale = ReplacementPlan {
        target: &authorized,
        original: "teh",
        replacement: "the",
        following: "",
        selected_text: false,
        stamp: InputStamp {
            position: super::super::input_listener::current_position_generation(),
            sequence: super::super::input_listener::current_input_sequence(),
        },
    };
    unsafe {
        let title = wide("Private document");
        assert_ne!(
            SendMessageW(
                GetAncestor(edit as _, GA_ROOT),
                WM_SETTEXT,
                0,
                title.as_ptr() as isize
            ),
            0
        );
    }
    for method in [ReplacementMethod::Clipboard, ReplacementMethod::SendInput] {
        let refused = run_strategies(&stale, &mut [&mut native::NativeStrategy(method)], true);
        assert!(!refused.success && !refused.may_have_changed);
        assert!(refused.range.is_none());
    }
    unsafe {
        let title = wide("AutoFix replacement test");
        SendMessageW(
            GetAncestor(edit as _, GA_ROOT),
            WM_SETTEXT,
            0,
            title.as_ptr() as isize,
        );
    }
    // Unchanged results use the real caret proof but never enter native replacement.
    for trigger in [
        TriggerKind::ManualShortcut,
        TriggerKind::WordCount,
        TriggerKind::Character,
        TriggerKind::FinalFixBeforeReanchor,
    ] {
        let mut config = AppConfig::default();
        config.context.informative_context_max_chars = 5;
        let frozen = matches!(trigger, TriggerKind::WordCount | TriggerKind::Character);
        let following = if frozen { " newer" } else { "" };
        let initial = format!("old hello{following} AFTER");
        let caret = format!("old hello{following}").encode_utf16().count();
        unsafe {
            SendMessageW(GetAncestor(edit as _, GA_ROOT), WM_APP + 2, 0, 0);
            let text = wide(&initial);
            SendMessageW(edit as _, WM_SETTEXT, 0, text.as_ptr() as isize);
            SendMessageW(edit as _, EM_SETSEL, caret, caret as isize);
        }
        let super::super::target::TargetDetection::Available(target) =
            super::super::target::detect_focused_target()
        else {
            panic!("test editor lost focus")
        };
        let mut manager = SessionManager::new(config.context.clone());
        manager.focus(&target);
        manager.set_informative_context("old ".into());
        manager.input(TypedInput::Text("hello".into()));
        let session = manager.active().unwrap();
        let mut request = super::super::triggers::manual(
            session.id(),
            session.informative_context(),
            &session.editable_context(),
            session.versions(),
            &super::super::context_capture::SelectionCapture::NoSelection,
            &config,
        )
        .unwrap();
        request.trigger = trigger;
        request.language_info = crate::correction::LanguageInfo {
            primary_language: Some("en".into()),
            detected_languages: vec!["en".into()],
        };
        if frozen {
            request.pending_segment_id = manager
                .active_mut()
                .unwrap()
                .freeze_pending(&config.context)
                .0;
            manager.input(TypedInput::Text(following.into()));
        }
        let stamp = InputStamp {
            position: super::super::input_listener::current_position_generation(),
            sequence: super::super::input_listener::current_input_sequence(),
        };
        let mut pipeline = super::super::pipeline::CorrectionPipeline::new().unwrap();
        assert!(pipeline.submit(
            request,
            manager.active().unwrap(),
            target.clone(),
            stamp,
            &config,
            vec![]
        ));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            pipeline.wait_manual();
            if pipeline.finish(
                &mut manager,
                &config.context,
                || stamp,
                |_| Some(target.clone()),
                |target, _, known| {
                    super::super::context_capture::read_before_caret(target, &config.context, known)
                },
                |_, _, _| -> bool { panic!("unchanged result reached native replacement") },
            ) {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "unchanged live result did not commit: {trigger:?}"
            );
        }
        assert_eq!(manager.active().unwrap().informative_context(), "hello");
        assert_eq!(manager.active().unwrap().editable_context(), following);
        assert!(manager.active().unwrap().undo_target().is_none());
        let mut observed = [0u16; 128];
        let mut caret_start = 0u32;
        let mut caret_end = 0u32;
        unsafe {
            let length = SendMessageW(
                edit as _,
                WM_GETTEXT,
                observed.len(),
                observed.as_mut_ptr() as isize,
            );
            assert_eq!(
                String::from_utf16_lossy(&observed[..length as usize]),
                initial
            );
            SendMessageW(
                edit as _,
                0x00b0,
                &mut caret_start as *mut u32 as usize,
                &mut caret_end as *mut u32 as isize,
            );
        }
        assert_eq!((caret_start as usize, caret_end as usize), (caret, caret));
    }
    // Exercise configured word thresholds through the runtime owner, live gates,
    // real pre-caret proof, native replacement, metadata and app-level undo.
    for (original, corrected, medium_silent) in [
        ("teh word ", "the word ", false),
        ("hello word ", "hello word ", false),
        ("accomodate word ", "accomodate word ", false),
        ("accomodate word ", "accommodate word ", true),
    ] {
        let following = "newer";
        let initial = format!("old {original}{following} AFTER");
        let caret = format!("old {original}{following}").encode_utf16().count();
        unsafe {
            SendMessageW(GetAncestor(edit as _, GA_ROOT), WM_APP + 2, 0, 0);
            let text = wide(&initial);
            SendMessageW(edit as _, WM_SETTEXT, 0, text.as_ptr() as isize);
            SendMessageW(edit as _, EM_SETSEL, caret, caret as isize);
        }
        let super::super::target::TargetDetection::Available(target) =
            super::super::target::detect_focused_target()
        else {
            panic!("test editor lost focus")
        };
        let path = std::env::temp_dir().join(format!(
            "autofix-native-word-count-{}-{}.sqlite",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let database = crate::storage::Database::open(&path).unwrap();
        let mut config = AppConfig::default();
        config.correction.preferred_language = Some("en".into());
        config.triggers.character_trigger_enabled = false;
        config.triggers.word_count = 2;
        config.correction.uncertain_language_policy =
            crate::correction::UncertainLanguagePolicy::CorrectNormally;
        config.replacement.clipboard_enabled = false;
        if medium_silent {
            config.correction.medium_confidence_behavior = ConfidenceBehavior::Silent;
        }
        let mut processor = super::super::InputProcessor {
            feedback: super::super::feedback::Feedback::default(),
            learner: crate::dictionary::Learner::default(),
            pipeline: super::super::pipeline::CorrectionPipeline::with_database(&database).unwrap(),
            processed_input_sequence: super::super::input_listener::current_input_sequence(),
            session_manager: SessionManager::new(config.context.clone()),
            config,
            database,
        };
        processor.session_manager.focus(&target);
        processor
            .session_manager
            .set_informative_context("old ".into());
        let mut pending = Vec::new();
        for character in original.chars() {
            processor.track_input(TypedInput::Text(character.to_string()), &mut pending);
        }
        assert_eq!(pending.len(), 1);
        let request = pending.remove(0).request;
        let id = request.pending_segment_id.unwrap();
        assert_eq!(request.trigger, TriggerKind::WordCount);
        processor.dispatch_trigger(request, super::super::InputProcessor::input_stamp());
        assert!(processor.pipeline.is_correcting());
        processor.track_input(TypedInput::Text(following.into()), &mut pending);
        assert!(pending.is_empty());
        assert_eq!(
            processor
                .session_manager
                .active()
                .unwrap()
                .editable_context(),
            following
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while processor.pipeline.is_correcting() {
            processor.pipeline.wait_manual();
            processor.finish_correction();
            assert!(
                std::time::Instant::now() < deadline,
                "word-count flow did not complete"
            );
        }
        let mut observed = [0u16; 128];
        let length = unsafe {
            SendMessageW(
                edit as _,
                WM_GETTEXT,
                observed.len(),
                observed.as_mut_ptr() as isize,
            )
        };
        assert_eq!(
            String::from_utf16_lossy(&observed[..length as usize]),
            format!("old {corrected}{following} AFTER")
        );
        let session = processor.session_manager.active().unwrap();
        assert_eq!(session.informative_context(), format!("old {corrected}"));
        assert_eq!(session.editable_context(), following);
        assert!(!session.pending_matches(id, original));
        assert_eq!(session.undo_target().is_some(), original != corrected);
        let connection = rusqlite::Connection::open(&path).unwrap();
        let metadata: (String, String, String) = connection
            .query_row(
                "select trigger_type, result_reason, replacement_method from correction_metadata",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            metadata,
            (
                "word_count".into(),
                if original != corrected {
                    "correction_applied"
                } else if original.starts_with("accomodate") {
                    "confidence_below_configured_behavior"
                } else {
                    "no_correction_needed"
                }
                .into(),
                if original != corrected {
                    "send_input"
                } else {
                    "none"
                }
                .into(),
            )
        );
        if original != corrected {
            processor.process_shortcut(2);
            let length = unsafe {
                SendMessageW(
                    edit as _,
                    WM_GETTEXT,
                    observed.len(),
                    observed.as_mut_ptr() as isize,
                )
            };
            assert_eq!(
                String::from_utf16_lossy(&observed[..length as usize]),
                initial
            );
            assert_eq!(
                processor
                    .session_manager
                    .active()
                    .unwrap()
                    .editable_context(),
                following
            );
            assert!(processor
                .session_manager
                .active()
                .unwrap()
                .undo_target()
                .is_none());
        }
        drop(connection);
        drop(processor);
        std::fs::remove_file(path).unwrap();
    }
    // Exercise the runtime shortcut owner, real security/policy gates, local
    // engine, native replacement, session commit, undo and metadata together.
    for (selected, arbitrary, allowed) in [
        (false, false, true),
        (true, false, true),
        (true, true, true),
        (true, false, false),
    ] {
        let initial = "old teh AFTER";
        unsafe {
            SendMessageW(GetAncestor(edit as _, GA_ROOT), WM_APP + 2, 0, 0);
            let text = wide(initial);
            SendMessageW(edit as _, WM_SETTEXT, 0, text.as_ptr() as isize);
            SendMessageW(edit as _, EM_SETSEL, if selected { 4 } else { 7 }, 7);
        }
        let super::super::target::TargetDetection::Available(target) =
            super::super::target::detect_focused_target()
        else {
            panic!("test editor lost focus")
        };
        let path = std::env::temp_dir().join(format!(
            "autofix-native-shortcut-{}-{}.sqlite",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let database = crate::storage::Database::open(&path).unwrap();
        let mut config = AppConfig::default();
        config.correction.preferred_language = Some("en".into());
        config.shortcuts.correct_arbitrary_selection = arbitrary;
        config.replacement.clipboard_enabled = false;
        config.feedback.show_blocked_app_notice = false;
        let mut processor = super::super::InputProcessor {
            feedback: super::super::feedback::Feedback::default(),
            learner: crate::dictionary::Learner::default(),
            pipeline: super::super::pipeline::CorrectionPipeline::with_database(&database).unwrap(),
            processed_input_sequence: super::super::input_listener::current_input_sequence(),
            session_manager: SessionManager::new(config.context.clone()),
            config,
            database,
        };
        processor.session_manager.focus(&target);
        if !arbitrary {
            processor
                .session_manager
                .set_informative_context("old ".into());
            processor.session_manager.input(TypedInput::Text(
                if !allowed {
                    "other"
                } else if selected {
                    "teh AFTER"
                } else {
                    "teh"
                }
                .into(),
            ));
        }
        processor.process_shortcut(1);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while processor.pipeline.is_correcting() {
            processor.pipeline.wait_manual();
            processor.finish_correction();
            assert!(
                std::time::Instant::now() < deadline,
                "manual shortcut did not complete"
            );
        }
        let mut text = [0u16; 128];
        let length = unsafe {
            SendMessageW(
                edit as _,
                WM_GETTEXT,
                text.len(),
                text.as_mut_ptr() as isize,
            )
        };
        assert_eq!(
            String::from_utf16_lossy(&text[..length as usize]),
            if allowed { "old the AFTER" } else { initial }
        );
        let session = processor.session_manager.active().unwrap();
        if allowed {
            assert_eq!(session.informative_context(), "old the");
            assert!(session.editable_context().is_empty());
            assert_eq!(session.undo_target().unwrap().original, "teh");
            let connection = rusqlite::Connection::open(&path).unwrap();
            let count: u64 = connection.query_row(
                "select count(*) from correction_metadata where result_reason = 'correction_applied' and replacement_method = 'send_input'",
                [], |row| row.get(0)).unwrap();
            assert_eq!(count, 1);
            processor.process_shortcut(2);
            let length = unsafe {
                SendMessageW(
                    edit as _,
                    WM_GETTEXT,
                    text.len(),
                    text.as_mut_ptr() as isize,
                )
            };
            assert_eq!(String::from_utf16_lossy(&text[..length as usize]), initial);
            assert_eq!(
                processor
                    .session_manager
                    .active()
                    .unwrap()
                    .informative_context(),
                "old teh"
            );
            assert!(processor
                .session_manager
                .active()
                .unwrap()
                .undo_target()
                .is_none());
        } else {
            assert_eq!(session.editable_context(), "other");
            assert!(session.undo_target().is_none());
        }
        drop(processor);
        std::fs::remove_file(path).unwrap();
    }
    for (method, original, replacement, following, selected_text) in [
        (ReplacementMethod::Clipboard, "teh", "the", "", false),
        (ReplacementMethod::Clipboard, "teh", "the", " newer", false),
        (ReplacementMethod::SendInput, "teh", "the", "", false),
        (ReplacementMethod::SendInput, "teh", "the", " newer", false),
        (ReplacementMethod::SendInput, "teh", "", " newer", false),
        (ReplacementMethod::SendInput, "teh", "é😃", "", false),
        (ReplacementMethod::SendInput, "teh", "العربية", "", false),
        (ReplacementMethod::Clipboard, "teh", "the", "", true),
        (ReplacementMethod::SendInput, "teh", "é😃", "", true),
        (ReplacementMethod::SendInput, "teh", "", "", true),
    ] {
        let initial = format!("old {original}{following} AFTER");
        let initial_caret = format!("old {original}{following}").encode_utf16().count();
        unsafe {
            let root = GetAncestor(edit as _, GA_ROOT);
            SendMessageW(root, WM_APP + 2, 0, 0);
            let title = wide("AutoFix replacement test");
            SendMessageW(root, WM_SETTEXT, 0, title.as_ptr() as isize);
            let text = wide(&initial);
            assert_ne!(
                SendMessageW(edit as _, WM_SETTEXT, 0, text.as_ptr() as isize),
                0
            );
            SendMessageW(edit as _, EM_SETSEL, initial_caret, initial_caret as isize);
        }
        let super::super::target::TargetDetection::Available(target) =
            super::super::target::detect_focused_target()
        else {
            panic!("test editor is not available")
        };
        assert_eq!(
            target.correction_eligibility(),
            CorrectionEligibility::Allowed
        );
        assert_eq!(target.process_id, _editor.child.id());
        assert_eq!(
            super::super::context_capture::read_before_caret(
                &target,
                &AppConfig::default().context,
                32
            )
            .as_deref(),
            Some(format!("old {original}{following}").as_str())
        );
        let stamp = InputStamp {
            position: super::super::input_listener::current_position_generation(),
            sequence: super::super::input_listener::current_input_sequence(),
        };
        if selected_text {
            // A backward selection has its active caret at the start. It must
            // never edit the selected text after that caret.
            unsafe {
                SendMessageW(edit as _, EM_SETSEL, initial_caret, initial_caret as isize);
                SendMessageW(
                    GetAncestor(edit as _, GA_ROOT),
                    WM_APP + 3,
                    original.chars().count(),
                    0,
                );
            };
            let backward = super::super::context_capture::read_selection(
                &target,
                "old ",
                original,
                &AppConfig::default().context,
            );
            assert!(
                matches!(backward, SelectionCapture::Unavailable),
                "{backward:?}"
            );
            let unsafe_plan = ReplacementPlan {
                target: &target,
                original,
                replacement,
                following,
                selected_text: true,
                stamp,
            };
            let refused = run_strategies(
                &unsafe_plan,
                &mut [&mut native::NativeStrategy(method)],
                true,
            );
            assert!(!refused.success && !refused.may_have_changed);
            unsafe { SendMessageW(edit as _, EM_SETSEL, 4, initial_caret as isize) };
            assert!(matches!(super::super::context_capture::read_selection(
                &target, "old ", original, &AppConfig::default().context),
                SelectionCapture::Selected { executable_prefix: Some(prefix), .. } if prefix.is_empty()));
        }
        let plan = ReplacementPlan {
            target: &target,
            original,
            replacement,
            following,
            selected_text,
            stamp,
        };
        unsafe { SendMessageW(GetAncestor(edit as _, GA_ROOT), WM_APP + 2, 1, 0) };
        let mut result = run_strategies(&plan, &mut [&mut native::NativeStrategy(method)], true);
        for _ in 0..5 {
            if result.success || result.may_have_changed {
                break;
            }
            thread::sleep(std::time::Duration::from_millis(50));
            result = run_strategies(&plan, &mut [&mut native::NativeStrategy(method)], true);
        }
        let mut caret_start = 0u32;
        let mut caret_end = 0u32;
        unsafe {
            SendMessageW(
                edit as _,
                0x00b0,
                &mut caret_start as *mut u32 as usize,
                &mut caret_end as *mut u32 as isize,
            );
        }
        let mut observed = [0u16; 128];
        let observed_length = unsafe {
            SendMessageW(
                edit as _,
                WM_GETTEXT,
                observed.len(),
                observed.as_mut_ptr() as isize,
            )
        };
        if method == ReplacementMethod::Clipboard
            && !result.may_have_changed
            && result.reason.as_deref().is_some_and(|reason| {
                reason.contains("clipboard format") && reason.contains("cannot be preserved")
            })
        {
            assert_eq!(
                String::from_utf16_lossy(&observed[..observed_length as usize]),
                initial
            );
            eprintln!("clipboard path safely refused an owner-managed format; checking SendInput fallback");
            continue;
        }
        assert!(
            result.success,
            "{result:?}; test editor text: {:?}; caret: {caret_start}..{caret_end}",
            String::from_utf16_lossy(&observed[..observed_length as usize])
        );
        assert_eq!(result.method, Some(method));
        let super::super::target::TargetDetection::Available(after) =
            super::super::target::detect_focused_target()
        else {
            panic!("test editor lost focus after mutation")
        };
        assert_eq!(after.window_title, "*AutoFix replacement test");
        assert_eq!(
            result.range,
            Some(ReplacedRange {
                start_back: original.chars().count() + following.chars().count(),
                end_back: following.chars().count()
            })
        );
        let mut text = [0u16; 128];
        let length = unsafe {
            SendMessageW(
                edit as _,
                WM_GETTEXT,
                text.len(),
                text.as_mut_ptr() as isize,
            )
        };
        assert_eq!(
            String::from_utf16_lossy(&text[..length as usize]),
            format!("old {replacement}{following} AFTER")
        );
        let expected_caret = format!("old {replacement}{following}")
            .encode_utf16()
            .count() as u32;
        assert_eq!((caret_start, caret_end), (expected_caret, expected_caret));

        // Exercise app undo using committed history, never target Ctrl+Z.
        let limits = AppConfig::default().context;
        let mut manager = SessionManager::new(limits.clone());
        manager.focus(&after);
        manager.set_informative_context("old ".into());
        manager.input(TypedInput::Text(original.into()));
        let segment = manager
            .active_mut()
            .unwrap()
            .freeze_pending(&limits)
            .0
            .unwrap();
        manager.input(TypedInput::Text(following.into()));
        assert!(manager
            .active_mut()
            .unwrap()
            .complete_pending(segment, replacement, &limits));
        let undo = manager.active().unwrap().undo_target().unwrap();
        let undone =
            ReplacementEngine::undo(&after, &undo, stamp, method == ReplacementMethod::Clipboard);
        assert!(undone.success, "{undone:?}");
        assert!(manager.active_mut().unwrap().undo_last_correction(&limits));
        assert_eq!(
            manager.active().unwrap().informative_context(),
            format!("old {original}")
        );
        assert_eq!(manager.active().unwrap().editable_context(), following);
        let restored_length = unsafe {
            SendMessageW(
                edit as _,
                WM_GETTEXT,
                observed.len(),
                observed.as_mut_ptr() as isize,
            )
        };
        assert_eq!(
            String::from_utf16_lossy(&observed[..restored_length as usize]),
            initial
        );
        unsafe {
            SendMessageW(
                edit as _,
                0x00b0,
                &mut caret_start as *mut u32 as usize,
                &mut caret_end as *mut u32 as isize,
            );
        }
        assert_eq!(
            (caret_start as usize, caret_end as usize),
            (initial_caret, initial_caret)
        );
    }
}

#[cfg(windows)]
static NATIVE_TEST_PARENT_PROC: std::sync::atomic::AtomicIsize =
    std::sync::atomic::AtomicIsize::new(0);

/// Simulate editors that synchronously add a modified-title marker on EN_CHANGE.
#[cfg(windows)]
unsafe extern "system" fn native_test_editor_window_proc(
    window: windows_sys::Win32::Foundation::HWND,
    message: u32,
    wparam: usize,
    lparam: isize,
) -> isize {
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    if message == WM_APP + 2 {
        SetWindowLongPtrW(window, GWLP_USERDATA, wparam as isize);
        return 0;
    }
    if message == WM_APP + 3 {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
            GetKeyboardState, SetKeyboardState, VK_LEFT, VK_SHIFT,
        };
        let mut previous = [0u8; 256];
        assert_ne!(GetKeyboardState(previous.as_mut_ptr()), 0);
        let mut shift = previous;
        shift[VK_SHIFT as usize] = 0x80;
        assert_ne!(SetKeyboardState(shift.as_ptr()), 0);
        let edit = GetWindow(window, GW_CHILD);
        for _ in 0..wparam {
            SendMessageW(edit, WM_KEYDOWN, VK_LEFT as usize, 1);
            SendMessageW(edit, WM_KEYUP, VK_LEFT as usize, 1);
        }
        assert_ne!(SetKeyboardState(previous.as_ptr()), 0);
        return 0;
    }
    if message == WM_COMMAND
        && wparam >> 16 == 0x0300
        && GetWindowLongPtrW(window, GWLP_USERDATA) != 0
    {
        let title: Vec<u16> = "*AutoFix replacement test"
            .encode_utf16()
            .chain(Some(0))
            .collect();
        SetWindowTextW(window, title.as_ptr());
    }
    let previous: WNDPROC =
        std::mem::transmute(NATIVE_TEST_PARENT_PROC.load(std::sync::atomic::Ordering::Relaxed));
    CallWindowProcW(previous, window, message, wparam, lparam)
}

/// Child-owned editor supplies real notifications without reading or editing another app.
#[cfg(windows)]
#[test]
#[ignore = "helper process for native_edit_replacement_smoke"]
fn native_edit_test_host() {
    if std::env::var("AUTOFIX_NATIVE_TEST_HOST").as_deref() != Ok("1") {
        return;
    }
    use std::{io::Write, ptr};
    use windows_sys::Win32::{
        System::Threading::GetCurrentThreadId,
        UI::{Input::KeyboardAndMouse::SetFocus, WindowsAndMessaging::*},
    };
    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(Some(0)).collect()
    }
    unsafe {
        let window = CreateWindowExW(
            0,
            wide("STATIC").as_ptr(),
            wide("AutoFix replacement test").as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            100,
            100,
            450,
            180,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null(),
        );
        assert!(!window.is_null());
        let previous = SetWindowLongPtrW(
            window,
            GWLP_WNDPROC,
            native_test_editor_window_proc as *const () as isize,
        );
        assert_ne!(previous, 0);
        NATIVE_TEST_PARENT_PROC.store(previous, std::sync::atomic::Ordering::Relaxed);
        let edit = CreateWindowExW(
            0,
            wide("EDIT").as_ptr(),
            wide("old teh AFTER").as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | ES_MULTILINE as u32,
            10,
            10,
            400,
            90,
            window,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null(),
        );
        assert!(!edit.is_null());
        SetForegroundWindow(window);
        PostMessageW(window, WM_APP + 1, 0, 0);
        let mut message: MSG = std::mem::zeroed();
        while GetMessageW(&mut message, ptr::null_mut(), 0, 0) > 0 {
            if message.message == WM_APP + 1 {
                SetFocus(edit);
                SendMessageW(edit, 0x00b1, 7, 7);
                println!("AUTOFIX_EDITOR {} {}", GetCurrentThreadId(), edit as isize);
                std::io::stdout().flush().unwrap();
                continue;
            }
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        DestroyWindow(window);
    }
}
