//! Refresh app authorization at the last boundary before transmitting captured text.

use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use super::{check_detection, request_allowed, SecurityDecision};
use crate::{
    background::target::{FocusedTarget, TargetDetection},
    correction::SendAuthorization,
    settings::{AppConfig, CorrectionEngine},
    storage::AppPolicyGuard,
};

/// Bind a request to live policy and cancellation; missing or busy storage denies sends.
pub(in crate::background) fn api_send_authorization(
    path: Option<PathBuf>,
    mut config: AppConfig,
    target: FocusedTarget,
    request: crate::background::triggers::CorrectionRequest,
    cancelled: Arc<AtomicBool>,
) -> SendAuthorization {
    config.correction.engine = CorrectionEngine::Api;
    Arc::new(move || {
        if cancelled.load(Ordering::Acquire) || !config.correction.enabled {
            return None;
        }
        let guard = AppPolicyGuard::acquire(path.as_deref()?).ok()?;
        let rules = guard.rules().ok()?;
        if !matches!(
            check_detection(
                request.trigger,
                &config,
                &rules,
                TargetDetection::Available(target.clone())
            ),
            SecurityDecision::Allowed { .. }
        ) || !request_allowed(&rules, &target, &request)
            || cancelled.load(Ordering::Acquire)
        {
            return None;
        }
        Some(Box::new(guard) as Box<dyn Send>)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::background::{
        context_capture::SelectionCapture, security::TriggerKind, session::ContextVersions,
        triggers,
    };
    use crate::storage::{AppRule, Database};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn target() -> FocusedTarget {
        FocusedTarget {
            process_id: 42,
            process_name: "notepad.exe".into(),
            window_handle: 123,
            window_title: "Notes".into(),
            focused_element_id: None,
            is_elevated: false,
            is_password_or_protected: false,
            is_hidden_or_unavailable: false,
            field_safety_known: true,
            is_secure_desktop: false,
            is_lock_screen: false,
            is_credential_dialog: false,
        }
    }

    fn allow_rule() -> AppRule {
        AppRule {
            process_name: "notepad.exe".into(),
            window_title_pattern: None,
            list_behavior: "allowlist".into(),
            manual_shortcut_allowed: true,
            word_count_trigger_allowed: true,
            character_trigger_allowed: true,
            local_engine_allowed: true,
            api_engine_allowed: true,
            safety_mode: "auto".into(),
            prose_context_allowed: false,
        }
    }

    struct Fixture {
        database: Database,
        path: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "autofix-outbound-{}-{}.sqlite",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            Self {
                database: Database::open(&path).unwrap(),
                path,
            }
        }

        fn authorize(&self, trigger: TriggerKind, cancelled: Arc<AtomicBool>) -> SendAuthorization {
            let mut request = triggers::manual(
                1,
                "",
                "This is teh sentence.",
                ContextVersions::default(),
                &SelectionCapture::NoSelection,
                &AppConfig::default(),
            )
            .unwrap();
            request.trigger = trigger;
            api_send_authorization(
                Some(self.path.clone()),
                AppConfig::default(),
                target(),
                request,
                cancelled,
            )
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            // Windows requires the SQLite connection to close before removing its file.
            let database = std::mem::replace(&mut self.database, Database::open_memory().unwrap());
            drop(database);
            std::fs::remove_file(&self.path).unwrap();
        }
    }

    #[test]
    fn prose_revocation_and_code_like_content_deny_outbound_requests() {
        let fixture = Fixture::new();
        let mut target = target();
        target.process_name = "code.exe".into();
        let mut rule = allow_rule();
        rule.process_name = "code.exe".into();
        rule.prose_context_allowed = true;
        fixture.database.app_rules().upsert(&rule).unwrap();
        let mut request = triggers::manual(
            1,
            "",
            "This is teh sentence.",
            ContextVersions::default(),
            &SelectionCapture::NoSelection,
            &AppConfig::default(),
        )
        .unwrap();
        request.selected_text = true;
        assert!(
            super::super::SecurityGate::authorize_correction_replacement(
                &request,
                &AppConfig::default(),
                &fixture.database,
                &target
            )
            .is_some()
        );
        let authorize = api_send_authorization(
            Some(fixture.path.clone()),
            AppConfig::default(),
            target.clone(),
            request.clone(),
            Arc::new(AtomicBool::new(false)),
        );
        assert!(authorize().is_some());
        rule.prose_context_allowed = false;
        fixture.database.app_rules().upsert(&rule).unwrap();
        assert!(authorize().is_none());
        assert!(
            super::super::SecurityGate::authorize_correction_replacement(
                &request,
                &AppConfig::default(),
                &fixture.database,
                &target
            )
            .is_none()
        );
        rule.prose_context_allowed = true;
        fixture.database.app_rules().upsert(&rule).unwrap();
        request.executable_context = "This is user_name.".into();
        assert!(
            super::super::SecurityGate::authorize_correction_replacement(
                &request,
                &AppConfig::default(),
                &fixture.database,
                &target
            )
            .is_none()
        );
        let authorize = api_send_authorization(
            Some(fixture.path.clone()),
            AppConfig::default(),
            target,
            request,
            Arc::new(AtomicBool::new(false)),
        );
        assert!(authorize().is_none());
    }

    /// Permission is read at every send, including changes to trigger and title rules.
    #[test]
    fn revocation_blocks_existing_authorizations_for_every_correction_trigger() {
        for trigger in [
            TriggerKind::ManualShortcut,
            TriggerKind::WordCount,
            TriggerKind::Character,
            TriggerKind::FinalFixBeforeReanchor,
        ] {
            for revocation in 0..4 {
                let fixture = Fixture::new();
                let mut rule = allow_rule();
                fixture.database.app_rules().upsert(&rule).unwrap();
                let authorize = fixture.authorize(trigger, Arc::new(AtomicBool::new(false)));
                assert!(authorize().is_some());
                match revocation {
                    0 => rule.api_engine_allowed = false,
                    1 => rule.list_behavior = "blocklist".into(),
                    2 => {
                        rule.manual_shortcut_allowed = false;
                        rule.word_count_trigger_allowed = false;
                        rule.character_trigger_allowed = false;
                    }
                    _ => {
                        fixture
                            .database
                            .app_rules()
                            .delete("notepad.exe", None)
                            .unwrap();
                        rule.window_title_pattern = Some("Notes".into());
                        rule.api_engine_allowed = false;
                    }
                }
                fixture.database.app_rules().upsert(&rule).unwrap();
                assert!(authorize().is_none());
            }
        }
    }

    /// Both SQLite journal modes serialize a rule commit with the actual send guard.
    #[test]
    fn send_guard_blocks_policy_writers_until_released() {
        for mode in ["DELETE", "WAL"] {
            let fixture = Fixture::new();
            let writer = rusqlite::Connection::open(&fixture.path).unwrap();
            writer.pragma_update(None, "journal_mode", mode).unwrap();
            writer.busy_timeout(std::time::Duration::ZERO).unwrap();
            let authorize = fixture.authorize(
                TriggerKind::ManualShortcut,
                Arc::new(AtomicBool::new(false)),
            );
            let guard = authorize().unwrap();
            let update = "INSERT INTO app_rules (process_name, window_title_pattern, list_behavior,
                manual_shortcut_allowed, word_count_trigger_allowed, character_trigger_allowed,
                local_engine_allowed, api_engine_allowed) VALUES ('notepad.exe', '', 'allowlist', 1, 1, 1, 1, 0)";
            assert!(
                matches!(writer.execute(update, []), Err(rusqlite::Error::SqliteFailure(error, _))
                if error.code == rusqlite::ErrorCode::DatabaseBusy)
            );
            drop(guard);
            writer.execute(update, []).unwrap();
            assert!(authorize().is_none());
        }
    }

    /// Uncommitted writers, unreadable policy, missing files and cancellation all deny sends.
    #[test]
    fn unavailable_or_contended_policy_and_cancelled_jobs_deny_sends() {
        let fixture = Fixture::new();
        let cancelled = Arc::new(AtomicBool::new(false));
        let authorize = fixture.authorize(TriggerKind::ManualShortcut, Arc::clone(&cancelled));
        let writer = rusqlite::Connection::open(&fixture.path).unwrap();
        writer.execute_batch("BEGIN IMMEDIATE").unwrap();
        assert!(authorize().is_none());
        writer.execute_batch("ROLLBACK").unwrap();
        assert!(authorize().is_some());
        cancelled.store(true, Ordering::Release);
        assert!(authorize().is_none());
        cancelled.store(false, Ordering::Release);
        writer.execute("DROP TABLE app_rules", []).unwrap();
        assert!(authorize().is_none());
        let missing = fixture.path.with_extension("missing");
        let authorize = api_send_authorization(
            Some(missing.clone()),
            AppConfig::default(),
            target(),
            triggers::manual(
                1,
                "",
                "This is teh sentence.",
                ContextVersions::default(),
                &SelectionCapture::NoSelection,
                &AppConfig::default(),
            )
            .unwrap(),
            cancelled,
        );
        assert!(authorize().is_none());
        assert!(!missing.exists());
    }
}
