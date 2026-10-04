//! Windows target policy contracts through the runtime dispatcher and local worker.
//! Focus/capture/mutation are deterministic adapters, not real application providers.
use super::*;
use crate::background::{
    replacement::{ReplacedRange, ReplacementMethod},
    security::{BlockReason, SecurityDecision},
    target::{FocusedTarget, TargetDetection},
};
use crate::{settings::RunMode, storage::AppRule};

// A control's real process/title must be recorded during desktop testing. Search
// and packaged messaging apps can have different hosts across Windows versions.
const TARGETS: &[(&str, &str, bool)] = &[
    ("notepad.exe", "Notes", false),
    ("SearchHost.exe", "Search", false),
    ("SearchApp.exe", "Search", false),
    ("explorer.exe", "Search results", false),
    ("explorer.exe", "Run", false),
    ("msedge.exe", "Browser text fixture", false),
    ("chrome.exe", "Browser text fixture", false),
    ("firefox.exe", "Browser text fixture", false),
    ("WhatsApp.exe", "WhatsApp", false),
    ("Telegram.exe", "Telegram", false),
    ("cmd.exe", "Command Prompt", true),
    ("powershell.exe", "Windows PowerShell", true),
    ("pwsh.exe", "PowerShell", true),
    ("WindowsTerminal.exe", "Windows Terminal", true),
    ("Code.exe", "notes.txt - Visual Studio Code", true),
    ("devenv.exe", "Visual Studio", true),
    (
        "AutoFix.TextTargetFixture.exe",
        "AutoFix WPF text fixture",
        false,
    ),
    ("msedge.exe", "Browser address bar", false),
];
const TRIGGERS: [TriggerKind; 3] = [
    TriggerKind::ManualShortcut,
    TriggerKind::WordCount,
    TriggerKind::Character,
];

fn focused(process: &str, title: &str) -> FocusedTarget {
    FocusedTarget {
        process_name: process.into(),
        window_title: title.into(),
        ..target()
    }
}

fn allow_rule(process: &str) -> AppRule {
    AppRule {
        process_name: process.into(),
        window_title_pattern: None,
        list_behavior: "allowlist".into(),
        manual_shortcut_allowed: true,
        word_count_trigger_allowed: true,
        character_trigger_allowed: true,
        local_engine_allowed: true,
        api_engine_allowed: true,
        safety_mode: "auto".into(),
        prose_context_allowed: true,
    }
}

fn decision(
    trigger: TriggerKind,
    config: &AppConfig,
    rules: &[AppRule],
    target: &FocusedTarget,
) -> SecurityDecision {
    security::check_detection(
        trigger,
        config,
        rules,
        TargetDetection::Available(target.clone()),
    )
}

/// App identity permissions never override password, elevation or unknown safety.
#[test]
fn windows_target_security_matrix() {
    let config = AppConfig::default();
    let database = crate::storage::Database::open_memory().unwrap();
    let defaults = database.app_rules().list().unwrap();
    for &(process, title, blocked) in TARGETS {
        for trigger in TRIGGERS {
            let target = focused(process, title);
            assert_eq!(
                matches!(
                    decision(trigger, &config, &defaults, &target),
                    SecurityDecision::Allowed { .. }
                ),
                !blocked,
                "{process}: {title}, {trigger:?}"
            );
            for reason in [
                BlockReason::PasswordField,
                BlockReason::ElevatedTarget,
                BlockReason::UnsupportedTarget,
            ] {
                let mut unsafe_target = target.clone();
                match reason {
                    BlockReason::PasswordField => unsafe_target.is_password_or_protected = true,
                    BlockReason::ElevatedTarget => unsafe_target.is_elevated = true,
                    BlockReason::UnsupportedTarget => unsafe_target.field_safety_known = false,
                    _ => unreachable!(),
                }
                assert!(
                    matches!(decision(trigger, &config, &[allow_rule(process)], &unsafe_target), SecurityDecision::Blocked { reason: actual, .. } if actual == reason),
                    "{process}: {trigger:?}, {reason:?}"
                );
            }
        }
    }
}

/// Per-trigger/engine denials, blocklists and allowlists apply to every target.
#[test]
fn windows_target_app_rule_matrix() {
    for &(process, title, _) in TARGETS {
        let target = focused(process, title);
        for trigger in TRIGGERS {
            for engine in [
                crate::settings::CorrectionEngine::Local,
                crate::settings::CorrectionEngine::Api,
            ] {
                let mut config = AppConfig::default();
                config.correction.engine = engine.clone();
                let rule = allow_rule(process);
                assert!(matches!(
                    decision(trigger, &config, &[rule.clone()], &target),
                    SecurityDecision::Allowed { .. }
                ));
                let mut trigger_denied = rule.clone();
                match trigger {
                    TriggerKind::ManualShortcut => trigger_denied.manual_shortcut_allowed = false,
                    TriggerKind::WordCount => trigger_denied.word_count_trigger_allowed = false,
                    TriggerKind::Character => trigger_denied.character_trigger_allowed = false,
                    _ => unreachable!(),
                }
                assert!(matches!(
                    decision(trigger, &config, &[trigger_denied], &target),
                    SecurityDecision::Blocked {
                        reason: BlockReason::AppRuleBlocked,
                        ..
                    }
                ));
                let mut engine_denied = rule.clone();
                engine_denied.local_engine_allowed = false;
                engine_denied.api_engine_allowed = false;
                assert!(matches!(
                    decision(trigger, &config, &[engine_denied], &target),
                    SecurityDecision::Blocked {
                        reason: BlockReason::EngineBlocked,
                        ..
                    }
                ));
                let mut blocklisted = rule;
                blocklisted.list_behavior = "blocklist".into();
                assert!(matches!(
                    decision(trigger, &config, &[blocklisted], &target),
                    SecurityDecision::Blocked {
                        reason: BlockReason::AppRuleBlocked,
                        ..
                    }
                ));
                config.general.run_mode = RunMode::Allowlist;
                assert!(matches!(
                    decision(trigger, &config, &[], &target),
                    SecurityDecision::Blocked { .. }
                ));
            }
        }
    }
}

/// Real translated-key trigger construction, dispatcher, local worker and commit.
/// The simulated document makes accidental edits to prefix/suffix observable.
#[test]
fn windows_target_allowed_trigger_flow_preserves_context_and_undo() {
    for &(process, title, blocked) in TARGETS {
        if blocked {
            continue;
        }
        for trigger in TRIGGERS {
            let mut config = AppConfig::default();
            config.triggers.word_count_enabled = trigger == TriggerKind::WordCount;
            config.triggers.character_trigger_enabled = trigger == TriggerKind::Character;
            config.triggers.word_count = 2;
            config.correction.preferred_language = Some("en".into());
            let target = focused(process, title);
            let mut processor = processor(config, CorrectionPipeline::new().unwrap());
            processor.session_manager.focus(&target);
            let prefix = "Earlier teh words: ";
            let suffix = " AFTER teh suffix";
            processor
                .session_manager
                .set_informative_context(prefix.into());
            let original = match trigger {
                TriggerKind::ManualShortcut => "teh word",
                TriggerKind::WordCount => "teh word ",
                TriggerKind::Character => "teh word.",
                _ => unreachable!(),
            };
            let mut pending = type_text(&mut processor, original);
            let request = if trigger == TriggerKind::ManualShortcut {
                assert!(pending.is_empty());
                request(&processor.session_manager, &processor.config)
            } else {
                assert_eq!(pending.len(), 1);
                pending.remove(0).request
            };
            assert_eq!(request.trigger, trigger);
            assert!(processor.dispatch_trigger_with(
                request,
                STAMP,
                || STAMP,
                |kind, config, database| decision(
                    kind,
                    config,
                    &database.app_rules().list().unwrap(),
                    &target
                )
            ));
            let newer = if trigger == TriggerKind::ManualShortcut {
                ""
            } else {
                " newer"
            };
            assert!(type_text(&mut processor, newer).is_empty());
            wait_completion(&processor.pipeline);
            let live = processor
                .session_manager
                .active()
                .unwrap()
                .known_before_caret();
            let document = std::cell::RefCell::new(format!("{prefix}{original}{newer}{suffix}"));
            let corrected = original.replacen("teh", "the", 1);
            let calls = Cell::new(0);
            assert!(
                processor.pipeline.finish(
                    &mut processor.session_manager,
                    &processor.config.context,
                    || STAMP,
                    |_| Some(target.clone()),
                    |_, _, _| Some(live),
                    |_, request, output| {
                        assert_eq!(request.informative_context, prefix);
                        assert_eq!(request.executable_context, original);
                        assert_eq!(request.replacement_following_text, newer);
                        assert_eq!(output.corrected_executable_text, corrected);
                        document.borrow_mut().replace_range(
                            prefix.len()..prefix.len() + original.len(),
                            &output.corrected_executable_text,
                        );
                        calls.set(calls.get() + 1);
                        ReplacementConfirmation {
                            success: true,
                            method: Some(ReplacementMethod::SendInput),
                            range: Some(ReplacedRange {
                                start_back: original.chars().count() + newer.chars().count(),
                                end_back: newer.chars().count(),
                            }),
                        }
                    },
                ),
                "{process}: {trigger:?}"
            );
            assert_eq!(calls.get(), 1);
            assert_eq!(
                *document.borrow(),
                format!("{prefix}{corrected}{newer}{suffix}")
            );
            let session = processor.session_manager.active_mut().unwrap();
            assert_eq!(session.editable_context(), newer);
            let undo = session.undo_target().unwrap();
            assert_eq!(undo.original, original);
            assert_eq!(undo.corrected, corrected);
            assert_eq!(undo.following, newer);
            assert!(session.undo_last_correction(&processor.config.context));
            assert_eq!(session.informative_context(), format!("{prefix}{original}"));
            assert_eq!(session.editable_context(), newer);
            assert!(session.undo_target().is_none());
        }
    }
}

/// Default arbitrary-selection ownership refuses foreign text for every target.
#[test]
fn windows_target_foreign_selection_is_not_executable_by_default() {
    let config = AppConfig::default();
    assert!(!config.shortcuts.correct_arbitrary_selection);
    for &(process, title, _) in TARGETS {
        let mut manager = SessionManager::new(config.context.clone());
        manager.focus(&focused(process, title));
        manager.input(TypedInput::Text("new typing".into()));
        let session = manager.active().unwrap();
        let selection = SelectionCapture::Selected {
            text: "foreign teh".into(),
            preceding: "old text ".into(),
            following: " AFTER".into(),
            executable_prefix: None,
        };
        assert!(
            triggers::manual(
                session.id(),
                session.informative_context(),
                &session.editable_context(),
                session.versions(),
                &selection,
                &config
            )
            .is_none(),
            "{process}"
        );
        assert_eq!(session.editable_context(), "new typing");
        assert!(session.undo_target().is_none());
    }
}

/// A denied dispatch restores frozen text and never submits engine work.
#[test]
fn windows_target_denied_dispatch_keeps_original_and_has_no_undo() {
    for &(process, title, blocked) in TARGETS {
        for trigger in TRIGGERS {
            for refusal in ["default", "password", "elevated", "rule"] {
                if refusal == "default" && !blocked {
                    continue;
                }
                let mut config = AppConfig::default();
                config.triggers.word_count_enabled = trigger == TriggerKind::WordCount;
                config.triggers.character_trigger_enabled = trigger == TriggerKind::Character;
                config.triggers.word_count = 2;
                let mut target = focused(process, title);
                target.is_password_or_protected = refusal == "password";
                target.is_elevated = refusal == "elevated";
                let pipeline =
                    CorrectionPipeline::start(|_| panic!("denied target reached engine")).unwrap();
                let mut processor = processor(config, pipeline);
                processor.session_manager.focus(&target);
                if refusal != "default" {
                    let mut rule = allow_rule(process);
                    if refusal == "rule" {
                        rule.list_behavior = "blocklist".into();
                    }
                    processor.database.app_rules().upsert(&rule).unwrap();
                }
                let original = if trigger == TriggerKind::Character {
                    "teh word."
                } else {
                    "teh word "
                };
                let mut pending = type_text(&mut processor, original);
                let request = if trigger == TriggerKind::ManualShortcut {
                    request(&processor.session_manager, &processor.config)
                } else {
                    pending.remove(0).request
                };
                assert!(
                    !processor.dispatch_trigger_with(
                        request,
                        STAMP,
                        || STAMP,
                        |kind, config, database| decision(
                            kind,
                            config,
                            &database.app_rules().list().unwrap(),
                            &target
                        )
                    ),
                    "{process}: {trigger:?}, {refusal}"
                );
                assert!(!processor.pipeline.is_correcting());
                assert!(processor.pipeline.mailbox.0.lock().unwrap().jobs.is_empty());
                let session = processor.session_manager.active().unwrap();
                assert_eq!(session.editable_context(), original);
                assert!(session.informative_context().is_empty());
                assert!(session.undo_target().is_none());
            }
        }
    }
}
