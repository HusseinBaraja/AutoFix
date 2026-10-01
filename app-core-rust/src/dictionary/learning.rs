use super::{word_char, Repository};
use crate::settings::{LearningConfig, LearningMode};
use std::sync::mpsc::{self, Receiver};

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
}

struct PendingConsent {
    config: LearningConfig,
    receive: Receiver<Option<Rejection>>,
    cancelled: bool,
}

impl Learner {
    pub(crate) fn rejected(
        &mut self,
        rejection: Rejection,
        config: &LearningConfig,
        repository: &Repository<'_>,
    ) {
        self.rejected_with(rejection, config, repository, confirm);
    }

    fn rejected_with(
        &mut self,
        rejection: Rejection,
        config: &LearningConfig,
        repository: &Repository<'_>,
        ask: impl FnOnce(&Rejection) -> bool + Send + 'static,
    ) {
        match config.mode {
            LearningMode::Off => {}
            LearningMode::Automatic => remember(repository, &rejection, config),
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
    pub(crate) fn poll(&mut self, config: &LearningConfig, repository: &Repository<'_>) {
        let Some(pending) = &mut self.pending else {
            return;
        };
        pending.cancelled |= pending.config != *config;
        let cancelled = pending.cancelled;
        match pending.receive.try_recv() {
            Ok(rejection) => {
                self.pending = None;
                if let Some(rejection) = rejection.filter(|_| !cancelled) {
                    remember(repository, &rejection, config);
                }
            }
            Err(mpsc::TryRecvError::Disconnected) => self.pending = None,
            Err(mpsc::TryRecvError::Empty) => {}
        }
    }
}

fn remember(repository: &Repository<'_>, rejection: &Rejection, config: &LearningConfig) {
    if repository.remember(rejection, config).is_err() {
        tracing::warn!("learning could not save exclusion");
    }
}

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
    fn rejection() -> Rejection {
        Rejection::from_undo(
            "I typed teh today",
            "I typed the today",
            Some("en".into()),
            "Notepad.EXE".into(),
        )
        .unwrap()
    }
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

    #[test]
    fn default_undo_never_writes_or_prompts_and_automatic_deduplicates() {
        let db = Database::open_memory().unwrap();
        let mut learner = Learner::default();
        learner.rejected_with(
            rejection(),
            &LearningConfig::default(),
            &db.dictionary(),
            |_| panic!("default must not ask"),
        );
        assert_eq!(terms(&db), 0);
        let config = LearningConfig {
            mode: LearningMode::Automatic,
            rule: crate::settings::LearningRule::Dictionary,
            per_app: true,
        };
        learner.rejected_with(rejection(), &config, &db.dictionary(), |_| {
            panic!("automatic must not ask")
        });
        learner.rejected_with(rejection(), &config, &db.dictionary(), |_| panic!());
        assert_eq!(terms(&db), 1);
    }

    #[test]
    fn ask_requires_consent_and_cancels_when_disabled() {
        let db = Database::open_memory().unwrap();
        let mut learner = Learner::default();
        let config = LearningConfig {
            mode: LearningMode::Ask,
            rule: crate::settings::LearningRule::Dictionary,
            per_app: false,
        };
        for accepted in [false, true] {
            learner.rejected_with(rejection(), &config, &db.dictionary(), move |_| accepted);
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
            while learner.pending.is_some() && std::time::Instant::now() < deadline {
                learner.poll(&config, &db.dictionary());
                std::thread::yield_now();
            }
            assert!(learner.pending.is_none());
            assert_eq!(terms(&db), usize::from(accepted));
        }
        let (release, wait) = mpsc::channel();
        let mut other = rejection();
        other.original = "wierd".into();
        learner.rejected_with(other, &config, &db.dictionary(), move |_| {
            wait.recv().unwrap();
            true
        });
        learner.poll(&LearningConfig::default(), &db.dictionary());
        release.send(()).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while learner.pending.is_some() && std::time::Instant::now() < deadline {
            learner.poll(&config, &db.dictionary());
            std::thread::yield_now();
        }
        assert!(learner.pending.is_none());
        assert_eq!(terms(&db), 1);
    }
}
