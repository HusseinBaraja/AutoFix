use super::{word_char, Repository};
use crate::settings::{LearningConfig, LearningMode};
use std::{
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, SyncSender},
    thread::JoinHandle,
    time::Duration,
};

const SAVE_QUEUE_LIMIT: usize = 16;
const SAVE_BUSY_TIMEOUT: Duration = Duration::from_millis(100);

#[derive(Debug, Clone)]
pub(crate) struct Rejection {
    pub(crate) original: String,
    pub(crate) corrected: String,
    pub(crate) language: Option<String>,
    pub(crate) app: String,
}

impl Rejection {
    /// Learn the changed word/phrase, excluding unchanged leading/trailing text.
    pub(crate) fn from_undo(
        original: &str,
        corrected: &str,
        language: Option<String>,
        app: String,
    ) -> Option<Self> {
        if original == corrected {
            return None;
        }
        let a: Vec<char> = original.chars().collect();
        let b: Vec<char> = corrected.chars().collect();
        let mut start = a.iter().zip(&b).take_while(|(a, b)| a == b).count();
        // Bound common suffix before expanding the changed span, so removed
        // repeated words cannot be mistaken for unchanged suffix context.
        let mut suffix = a[start..]
            .iter()
            .rev()
            .zip(b[start..].iter().rev())
            .take_while(|(a, b)| a == b)
            .count();
        let needs_word_anchor = a[start..a.len() - suffix].iter().all(|c| c.is_whitespace());
        while start > 0 && word_char(a[start - 1]) {
            start -= 1;
        }
        while suffix > 0 && (word_char(a[a.len() - suffix]) || word_char(b[b.len() - suffix])) {
            suffix -= 1;
        }
        if needs_word_anchor {
            // Pure spacing/insertion edits need adjacent words to form a rule.
            while start > 0 && a[start - 1].is_whitespace() {
                start -= 1;
            }
            while start > 0 && !a[start - 1].is_whitespace() {
                start -= 1;
            }
        }
        let original: String = a[start..a.len() - suffix].iter().collect();
        if original.trim().is_empty() {
            return None;
        }
        Some(Self {
            original,
            corrected: b[start..b.len() - suffix].iter().collect(),
            language,
            app,
        })
    }
}

#[derive(Default)]
pub(crate) struct Learner {
    pending: Option<PendingConsent>,
    writer: Option<LearningWriter>,
}

struct Save {
    rejection: Rejection,
    config: LearningConfig,
}

struct LearningWriter {
    send: SyncSender<Save>,
    thread: JoinHandle<()>,
}

impl LearningWriter {
    /// Own a separate connection on a bounded worker; never create or migrate storage.
    fn start(path: PathBuf) -> std::io::Result<Self> {
        let (send, receive) = mpsc::sync_channel::<Save>(SAVE_QUEUE_LIMIT);
        let thread = std::thread::Builder::new()
            .name("autofix-learning-writer".into())
            .spawn(move || {
                let mut connection = None;
                for save in receive {
                    if connection.is_none() {
                        connection = rusqlite::Connection::open_with_flags(
                            &path,
                            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
                        )
                        .and_then(|connection| {
                            connection.busy_timeout(SAVE_BUSY_TIMEOUT)?;
                            Ok(connection)
                        })
                        .ok();
                    }
                    let Some(connection) = &connection else {
                        tracing::warn!("learning storage unavailable");
                        continue;
                    };
                    let repository = Repository::new(connection);
                    if repository.remember(&save.rejection, &save.config).is_err() {
                        tracing::warn!("learning could not save exclusion");
                    }
                }
            })?;
        Ok(Self { send, thread })
    }

    /// Drop a full or disconnected submission without waiting or logging captured text.
    fn submit(&self, save: Save) -> bool {
        if self.send.try_send(save).is_err() {
            tracing::warn!("learning save queue unavailable");
            return false;
        }
        true
    }

    /// Drain accepted saves during shutdown, after input processing has stopped.
    fn finish(self) {
        drop(self.send);
        if self.thread.join().is_err() {
            tracing::warn!("learning writer failed during shutdown");
        }
    }
}

struct PendingConsent {
    config: LearningConfig,
    receive: Receiver<Option<Rejection>>,
    cancelled: bool,
}

impl Learner {
    /// Handle only a successful AutoFix undo according to the current opt-in policy.
    pub(crate) fn rejected(
        &mut self,
        rejection: Rejection,
        config: &LearningConfig,
        path: Option<&Path>,
    ) {
        self.rejected_with(rejection, config, path, confirm);
    }

    /// Run at most one consent prompt while leaving input processing available.
    /// Off never prompts or writes; automatic queues the authorized rejection.
    fn rejected_with(
        &mut self,
        rejection: Rejection,
        config: &LearningConfig,
        path: Option<&Path>,
        ask: impl FnOnce(&Rejection) -> bool + Send + 'static,
    ) {
        match config.mode {
            LearningMode::Off => {}
            LearningMode::Automatic => self.remember(path, rejection, config),
            LearningMode::Ask if self.pending.is_none() => {
                let (send, receive) = mpsc::channel();
                match std::thread::Builder::new()
                    .name("autofix-learning".into())
                    .spawn(move || {
                        let accepted = ask(&rejection);
                        let _ = send.send(accepted.then_some(rejection));
                    }) {
                    Ok(_) => {
                        self.pending = Some(PendingConsent {
                            config: config.clone(),
                            receive,
                            cancelled: false,
                        })
                    }
                    Err(_) => tracing::warn!("learning prompt unavailable"),
                }
            }
            LearningMode::Ask => {}
        }
    }

    /// Changing settings cancels pending consent; refusals never persist text.
    pub(crate) fn poll(&mut self, config: &LearningConfig, path: Option<&Path>) {
        let Some(pending) = &mut self.pending else {
            return;
        };
        pending.cancelled |= pending.config != *config;
        let cancelled = pending.cancelled;
        match pending.receive.try_recv() {
            Ok(rejection) => {
                self.pending = None;
                if let Some(rejection) = rejection.filter(|_| !cancelled) {
                    self.remember(path, rejection, config);
                }
            }
            Err(mpsc::TryRecvError::Disconnected) => self.pending = None,
            Err(mpsc::TryRecvError::Empty) => {}
        }
    }

    /// Queue authorized text without performing SQLite work on the input thread.
    fn remember(&mut self, path: Option<&Path>, rejection: Rejection, config: &LearningConfig) {
        if self.writer.is_none() {
            let Some(path) = path else {
                tracing::warn!("learning storage unavailable");
                return;
            };
            match LearningWriter::start(PathBuf::from(path)) {
                Ok(writer) => self.writer = Some(writer),
                Err(_) => {
                    tracing::warn!("learning writer unavailable");
                    return;
                }
            }
        }
        self.writer.as_ref().unwrap().submit(Save {
            rejection,
            config: config.clone(),
        });
    }

    /// Stop accepting consent and finish already authorized saves on engine shutdown.
    pub(crate) fn finish(&mut self) {
        self.pending = None;
        if let Some(writer) = self.writer.take() {
            writer.finish();
        }
    }
}

/// Ask for native Yes/No consent with No selected by default; only Yes permits storage.
fn confirm(rejection: &Rejection) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        MessageBoxW, IDYES, MB_DEFBUTTON2, MB_YESNO,
    };
    let text: Vec<u16> = format!(
        "Don't correct this again?\n\n{} → {}",
        rejection.original, rejection.corrected
    )
    .encode_utf16()
    .chain(Some(0))
    .collect();
    let title: Vec<u16> = "AutoFix".encode_utf16().chain(Some(0)).collect();
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            title.as_ptr(),
            MB_YESNO | MB_DEFBUTTON2,
        ) == IDYES
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{correction::LanguageInfo, storage::Database};

    struct Fixture {
        db: Database,
        path: PathBuf,
        learner: Learner,
    }

    impl Fixture {
        /// Use a real file so the writer must own a separate SQLite connection.
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "autofix-learning-{}-{}.sqlite",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            Self {
                db: Database::open(&path).unwrap(),
                path,
                learner: Learner::default(),
            }
        }

        /// Consume the asynchronous prompt answer without waiting for persistence.
        fn poll_consent(&mut self, config: &LearningConfig) {
            let deadline = std::time::Instant::now() + Duration::from_secs(2);
            while self.learner.pending.is_some() && std::time::Instant::now() < deadline {
                self.learner.poll(config, self.db.path());
                std::thread::yield_now();
            }
            assert!(self.learner.pending.is_none());
        }
    }

    impl Drop for Fixture {
        /// Drain the worker and close both connections before removing Windows test files.
        fn drop(&mut self) {
            self.learner.finish();
            drop(std::mem::replace(
                &mut self.db,
                Database::open_memory().unwrap(),
            ));
            std::fs::remove_file(&self.path).unwrap();
        }
    }
    /// Build a successful undo rejection with English and app scope.
    fn rejection() -> Rejection {
        Rejection::from_undo(
            "I typed teh today",
            "I typed the today",
            Some("en".into()),
            "Notepad.EXE".into(),
        )
        .unwrap()
    }
    /// Count applicable saved word exclusions for the test application.
    fn terms(db: &Database) -> usize {
        let p = db
            .dictionary()
            .policy(
                "notepad.exe",
                &LanguageInfo {
                    primary_language: Some("en".into()),
                    detected_languages: vec!["en".into()],
                },
            )
            .unwrap();
        p.terms.len()
    }

    /// Learning isolates changed words and phrases, including Unicode, spacing and deletions.
    #[test]
    fn extracts_complete_changed_words_and_unicode_phrases() {
        assert_eq!(rejection().original, "teh");
        assert_eq!(rejection().corrected, "the");
        let r =
            Rejection::from_undo("é teh wierd!", "é the weird!", None, "app.exe".into()).unwrap();
        assert_eq!(
            (r.original.as_str(), r.corrected.as_str()),
            ("teh wierd", "the weird")
        );
        assert!(Rejection::from_undo("abc", "abc", None, "".into()).is_none());
        assert!(Rejection::from_undo("", "abc", None, "".into()).is_none());
        for (original, corrected, expected_original, expected_corrected) in [
            ("hello", "hello.", "hello", "hello."),
            ("very very", "very", "very very", "very"),
            ("two  words", "two words", "two  words", "two words"),
            ("cant", "can't", "cant", "can't"),
        ] {
            let r = Rejection::from_undo(original, corrected, None, "app.exe".into()).unwrap();
            assert_eq!(
                (r.original.as_str(), r.corrected.as_str()),
                (expected_original, expected_corrected)
            );
        }
    }

    /// Default-off undo never asks or persists; automatic learning stores one scoped exclusion.
    #[test]
    fn default_undo_never_writes_or_prompts_and_automatic_deduplicates() {
        let mut fixture = Fixture::new();
        let db = &fixture.db;
        let learner = &mut fixture.learner;
        learner.rejected_with(rejection(), &LearningConfig::default(), db.path(), |_| {
            panic!("default must not ask")
        });
        assert_eq!(terms(db), 0);
        assert!(learner.writer.is_none());
        let config = LearningConfig {
            mode: LearningMode::Automatic,
            rule: crate::settings::LearningRule::Dictionary,
            per_app: true,
        };
        learner.rejected_with(rejection(), &config, db.path(), |_| {
            panic!("automatic must not ask")
        });
        learner.rejected_with(rejection(), &config, db.path(), |_| panic!());
        learner.finish();
        assert_eq!(terms(db), 1);
    }

    /// Only accepted, uncancelled consent persists a rejection; changing settings revokes consent.
    #[test]
    fn ask_requires_consent_and_cancels_when_disabled() {
        let mut fixture = Fixture::new();
        let config = LearningConfig {
            mode: LearningMode::Ask,
            rule: crate::settings::LearningRule::Dictionary,
            per_app: false,
        };
        for accepted in [false, true] {
            fixture
                .learner
                .rejected_with(rejection(), &config, fixture.db.path(), move |_| accepted);
            fixture.poll_consent(&config);
            if !accepted {
                assert!(fixture.learner.writer.is_none());
            }
            fixture.learner.finish();
            assert_eq!(terms(&fixture.db), usize::from(accepted));
        }
        let (release, wait) = mpsc::channel();
        let mut other = rejection();
        other.original = "wierd".into();
        fixture
            .learner
            .rejected_with(other, &config, fixture.db.path(), move |_| {
                wait.recv().unwrap();
                true
            });
        fixture
            .learner
            .poll(&LearningConfig::default(), fixture.db.path());
        release.send(()).unwrap();
        fixture.poll_consent(&config);
        assert!(fixture.learner.writer.is_none());
        assert_eq!(terms(&fixture.db), 1);
    }

    /// API-send reservations cannot stall accepted consent or automatic learning in either journal mode.
    #[test]
    fn learning_during_api_send_keeps_input_available() {
        for journal in ["delete", "wal"] {
            for mode in [LearningMode::Ask, LearningMode::Automatic] {
                let mut fixture = Fixture::new();
                fixture
                    .db
                    .dictionary()
                    .connection
                    .execute_batch(&format!("PRAGMA journal_mode={journal}"))
                    .unwrap();
                let config = LearningConfig {
                    mode,
                    ..LearningConfig::default()
                };
                let (accepted, answer) = mpsc::channel();
                if mode == LearningMode::Ask {
                    // Install an already accepted prompt answer to isolate poll's input-path latency.
                    fixture.learner.pending = Some(PendingConsent {
                        config: config.clone(),
                        receive: answer,
                        cancelled: false,
                    });
                    accepted.send(Some(rejection())).unwrap();
                }
                // This is the same writer reservation held through each outbound API send.
                let guard = crate::storage::AppPolicyGuard::acquire(&fixture.path).unwrap();
                let started = std::time::Instant::now();
                if mode == LearningMode::Ask {
                    fixture.learner.poll(&config, fixture.db.path());
                    assert!(fixture.learner.pending.is_none());
                } else {
                    fixture.learner.rejected_with(
                        rejection(),
                        &config,
                        fixture.db.path(),
                        |_| panic!(),
                    );
                }
                assert!(
                    started.elapsed() < Duration::from_millis(250),
                    "input waited for SQLite"
                );
                // Holding the guard through worker drain proves timeout loses only the save.
                fixture.learner.finish();
                assert_eq!(terms(&fixture.db), 0);
                drop(guard);
                // Failure must not prevent a later authorized rejection from being saved.
                fixture
                    .learner
                    .remember(fixture.db.path(), rejection(), &config);
                fixture.learner.finish();
                assert!(
                    fixture
                        .db
                        .dictionary()
                        .policy(
                            "notepad.exe",
                            &LanguageInfo {
                                primary_language: Some("en".into()),
                                detected_languages: vec!["en".into()]
                            }
                        )
                        .unwrap()
                        .pairs
                        .len()
                        == 1
                );
            }
        }
    }

    /// A stalled writer has a fixed queue; overflow and disconnected workers return immediately.
    #[test]
    fn save_queue_is_bounded_and_never_waits() {
        let (send, receive) = mpsc::sync_channel(SAVE_QUEUE_LIMIT);
        let (release, wait) = mpsc::channel();
        let (count, counted) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            wait.recv().unwrap();
            count.send(receive.into_iter().count()).unwrap();
        });
        let writer = LearningWriter { send, thread };
        for _ in 0..SAVE_QUEUE_LIMIT {
            assert!(writer.submit(Save {
                rejection: rejection(),
                config: LearningConfig::default()
            }));
        }
        let started = std::time::Instant::now();
        assert!(!writer.submit(Save {
            rejection: rejection(),
            config: LearningConfig::default()
        }));
        assert!(started.elapsed() < Duration::from_millis(250));
        release.send(()).unwrap();
        writer.finish();
        assert_eq!(
            counted.recv_timeout(Duration::from_secs(2)).unwrap(),
            SAVE_QUEUE_LIMIT
        );

        let (send, receive) = mpsc::sync_channel(SAVE_QUEUE_LIMIT);
        drop(receive);
        let writer = LearningWriter {
            send,
            thread: std::thread::spawn(|| {}),
        };
        assert!(!writer.submit(Save {
            rejection: rejection(),
            config: LearningConfig::default()
        }));
        writer.finish();
    }

    /// A failed save does not kill the writer or alter later jobs' snapshotted scope.
    #[test]
    fn writer_recovers_from_save_failure_and_preserves_scope() {
        let mut fixture = Fixture::new();
        fixture
            .db
            .dictionary()
            .connection
            .execute_batch(
                "create trigger reject_test_entry before insert on custom_dictionary_entries
             when new.entry = 'failed' begin select raise(abort, 'test failure'); end;",
            )
            .unwrap();
        let config = LearningConfig {
            mode: LearningMode::Automatic,
            rule: crate::settings::LearningRule::Dictionary,
            per_app: true,
        };
        let mut failed = rejection();
        failed.original = "failed".into();
        fixture.learner.rejected(failed, &config, fixture.db.path());
        fixture
            .learner
            .rejected(rejection(), &config, fixture.db.path());
        let other_config = LearningConfig {
            per_app: false,
            ..config
        };
        let mut other = rejection();
        other.original = "wierd".into();
        fixture
            .learner
            .rejected(other, &other_config, fixture.db.path());
        fixture.learner.finish();
        let mut statement = fixture.db.dictionary().connection.prepare(
            "select entry, app_process_name, language_code from custom_dictionary_entries order by entry",
        ).unwrap();
        let saved = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(
            saved,
            vec![
                ("teh".into(), Some("notepad.exe".into()), "en".into()),
                ("wierd".into(), None, "en".into()),
            ]
        );
    }

    /// Learning cannot recreate deleted storage or run migrations against an uninitialized file.
    #[test]
    fn writer_never_creates_or_migrates_storage() {
        let fixture = Fixture::new();
        let path = fixture.path.with_extension("uninitialized.sqlite");
        let config = LearningConfig {
            mode: LearningMode::Automatic,
            rule: crate::settings::LearningRule::Dictionary,
            per_app: false,
        };
        let writer = LearningWriter::start(path.clone()).unwrap();
        assert!(writer.submit(Save {
            rejection: rejection(),
            config: config.clone()
        }));
        writer.finish();
        assert!(!path.exists());
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection
            .execute_batch("create table sentinel (value text)")
            .unwrap();
        let writer = LearningWriter::start(path.clone()).unwrap();
        assert!(writer.submit(Save {
            rejection: rejection(),
            config
        }));
        writer.finish();
        let tables: usize = connection
            .query_row(
                "select count(*) from sqlite_master where type = 'table'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(tables, 1);
        drop(connection);
        std::fs::remove_file(path).unwrap();
    }

    /// Scope changes cancel consent permanently, even if reverted before the answer arrives.
    #[test]
    fn consent_is_coalesced_and_revoked_by_scope_changes() {
        let config = LearningConfig {
            mode: LearningMode::Ask,
            ..LearningConfig::default()
        };
        for changed in [
            LearningConfig {
                per_app: !config.per_app,
                ..config.clone()
            },
            LearningConfig {
                rule: crate::settings::LearningRule::Dictionary,
                ..config.clone()
            },
        ] {
            let mut fixture = Fixture::new();
            let (release, wait) = mpsc::channel();
            fixture
                .learner
                .rejected_with(rejection(), &config, fixture.db.path(), move |_| {
                    wait.recv().unwrap();
                    true
                });
            fixture
                .learner
                .rejected_with(rejection(), &config, fixture.db.path(), |_| {
                    panic!("duplicate prompt")
                });
            fixture.learner.poll(&changed, fixture.db.path());
            release.send(()).unwrap();
            fixture.poll_consent(&config);
            assert!(fixture.learner.writer.is_none());
            assert_eq!(terms(&fixture.db), 0);
        }
    }
}
