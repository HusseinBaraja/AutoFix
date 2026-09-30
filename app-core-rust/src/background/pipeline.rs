//! Async correction ownership and the single validation gate before completion.
//! One running job and one replaceable queued job bound work during fast typing.

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use super::{
    security::TriggerKind,
    session::{Session, SessionManager},
    target::{CorrectionEligibility, FocusedTarget},
    triggers::CorrectionRequest,
};
use crate::{
    correction::{
        ApiEngineConfig, ConfidenceBehavior, ConfidenceTier, CorrectionEngines, CorrectionInput,
        CorrectionOutput, EngineStatus, NoChangeReason, TriggerType,
    },
    settings::{AppConfig, ContextConfig},
};

pub(super) const MANUAL_WAIT: Duration = Duration::from_millis(20);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct InputStamp {
    pub(super) position: u64,
    pub(super) sequence: u64,
}

struct Job {
    id: u64,
    request: CorrectionRequest,
    input: CorrectionInput,
    api: ApiEngineConfig,
    cancelled: Arc<AtomicBool>,
}

struct Completion {
    id: u64,
    output: CorrectionOutput,
}

#[derive(Default)]
struct Mailbox {
    job: Option<Job>,
    completion: Option<Completion>,
    stopped: bool,
}

struct ActiveRequest {
    id: u64,
    request: CorrectionRequest,
    target: FocusedTarget,
    editable_snapshot: String,
    stamp: InputStamp,
    cancelled: Arc<AtomicBool>,
}

impl ActiveRequest {
    fn valid(&self, session: &Session, stamp: InputStamp) -> bool {
        !self.cancelled.load(Ordering::Acquire)
            && self.request.session_id == session.id()
            && self.request.versions == session.versions()
            && self.editable_snapshot == session.editable_context()
            && self.stamp == stamp
            && !self.request.executable_context.is_empty()
            && (self.request.selected_text
                || self
                    .editable_snapshot
                    .ends_with(&self.request.executable_context))
            && (self.request.selected_text || !session.position_uncertain())
    }
}

pub(super) struct CorrectionPipeline {
    mailbox: Arc<(Mutex<Mailbox>, Condvar)>,
    worker: Option<JoinHandle<()>>,
    active: Option<ActiveRequest>,
    next_id: u64,
}

impl CorrectionPipeline {
    pub(super) fn new() -> std::io::Result<Self> {
        Self::start(|job| {
            CorrectionEngines::with_api_config(job.api.clone())
                .correct_with(job.request.engine, &job.input)
        })
    }

    fn start(correct: impl Fn(&Job) -> CorrectionOutput + Send + 'static) -> std::io::Result<Self> {
        let mailbox = Arc::new((Mutex::new(Mailbox::default()), Condvar::new()));
        let worker_mailbox = Arc::clone(&mailbox);
        let worker = thread::Builder::new()
            .name("autofix-correction".into())
            .spawn(move || {
                loop {
                    let job = {
                        let (lock, ready) = &*worker_mailbox;
                        let mut state = lock.lock().unwrap();
                        while state.job.is_none() && !state.stopped {
                            state = ready.wait(state).unwrap();
                        }
                        if state.stopped {
                            break;
                        }
                        state.job.take().unwrap()
                    };
                    if job.cancelled.load(Ordering::Acquire) {
                        continue;
                    }
                    let output = correct(&job);
                    let (lock, ready) = &*worker_mailbox;
                    let mut state = lock.lock().unwrap();
                    // Transport may finish after cancellation; never publish that result.
                    if !state.stopped && !job.cancelled.load(Ordering::Acquire) {
                        state.completion = Some(Completion { id: job.id, output });
                        ready.notify_all();
                    }
                }
            })?;
        Ok(Self {
            mailbox,
            worker: Some(worker),
            active: None,
            next_id: 1,
        })
    }

    pub(super) fn submit(
        &mut self,
        request: CorrectionRequest,
        session: &Session,
        target: FocusedTarget,
        stamp: InputStamp,
        config: &AppConfig,
        dictionary: Vec<String>,
    ) {
        self.cancel();
        let id = self.next_id;
        self.next_id = self.next_id.checked_add(1).expect("request ID exhausted");
        let cancelled = Arc::new(AtomicBool::new(false));
        let input = CorrectionInput {
            // Following selected text can inform the engine, but remains read-only.
            informative_context: if request.following_context.is_empty() {
                request.informative_context.clone()
            } else {
                format!(
                    "{}\n[Following selection]\n{}",
                    request.informative_context, request.following_context
                )
            },
            executable_context: request.executable_context.clone(),
            mode: request.mode,
            enabled_grammar_categories: if request.mode
                == crate::correction::CorrectionMode::TyposOnly
            {
                Vec::new()
            } else {
                config.correction.enabled_grammar_categories.clone()
            },
            language_info: request.language_info.clone(),
            mixed_language_policy: request.mixed_language_policy,
            uncertain_language_policy: request.uncertain_language_policy,
            protected_terms: dictionary.clone(),
            custom_dictionary: dictionary,
            trigger_type: match request.trigger {
                TriggerKind::ManualShortcut => TriggerType::ManualShortcut,
                TriggerKind::WordCount => TriggerType::WordCount,
                TriggerKind::Character => TriggerType::Character,
                TriggerKind::FinalFixBeforeReanchor => TriggerType::FinalFixBeforeReanchor,
                TriggerKind::Tracking | TriggerKind::Undo => {
                    unreachable!("not a correction trigger")
                }
            },
            confidence_behavior: request.confidence_behavior.clone(),
            suggestion_ui_available: false,
        };
        self.active = Some(ActiveRequest {
            id,
            request: request.clone(),
            target,
            editable_snapshot: session.editable_context(),
            stamp,
            cancelled: Arc::clone(&cancelled),
        });
        let (lock, ready) = &*self.mailbox;
        let mut state = lock.lock().unwrap();
        state.completion = None;
        state.job = Some(Job {
            id,
            request,
            input,
            api: (&config.api).into(),
            cancelled,
        });
        ready.notify_all();
    }

    pub(super) fn cancel(&mut self) {
        if let Some(active) = self.active.take() {
            active.cancelled.store(true, Ordering::Release);
        }
        let (lock, _) = &*self.mailbox;
        let mut state = lock.lock().unwrap();
        state.job = None;
        state.completion = None;
    }

    /// V1 cancels on every input change; retaining work needs a verified range strategy.
    pub(super) fn invalidate(&mut self, manager: &SessionManager, stamp: InputStamp) {
        if self.active.as_ref().is_some_and(|active| {
            manager
                .active()
                .is_none_or(|session| !active.valid(session, stamp))
        }) {
            self.cancel();
        }
    }

    pub(super) fn wait_manual(&self) {
        let (lock, ready) = &*self.mailbox;
        let state = lock.lock().unwrap();
        let _ = ready
            .wait_timeout_while(state, MANUAL_WAIT, |state| {
                state.completion.is_none() && !state.stopped
            })
            .unwrap();
    }

    /// Take once: duplicates and reordered completions cannot reach the mutation owner.
    pub(super) fn finish(
        &mut self,
        manager: &mut SessionManager,
        limits: &ContextConfig,
        current_stamp: impl Fn() -> InputStamp,
        check_target: impl FnOnce(TriggerKind) -> Option<FocusedTarget>,
        replace: impl FnOnce(&FocusedTarget, &CorrectionRequest, &CorrectionOutput) -> bool,
    ) -> bool {
        self.invalidate(manager, current_stamp());
        let completion = self.mailbox.0.lock().unwrap().completion.take();
        let Some(completion) = completion else {
            return false;
        };
        let Some(active) = self.active.as_ref() else {
            return false;
        };
        if completion.id != active.id {
            return false;
        }
        let active = self.active.take().unwrap();
        let output = completion.output;
        if output.status != EngineStatus::Completed {
            return false;
        }
        let Some(target) = check_target(active.request.trigger) else {
            return false;
        };
        // Security/UIA calls can race with queued typing or focus changes.
        if target.correction_eligibility() != CorrectionEligibility::Allowed
            || target.process_id != active.target.process_id
            || target.window_handle != active.target.window_handle
            || target.session_key() != active.target.session_key()
            || !manager.active_matches(&target)
            || manager
                .active()
                .is_none_or(|session| !active.valid(session, current_stamp()))
        {
            return false;
        }
        let original = &active.request.executable_context;
        if output.changes_needed {
            if output.behavior != ConfidenceBehavior::Silent
                || output.confidence == ConfidenceTier::Low
                || output.corrected_executable_text == *original
            {
                return false;
            }
        } else if output.corrected_executable_text != *original
            || !matches!(
                output.no_change_reason,
                Some(NoChangeReason::NoCorrectionNeeded | NoChangeReason::AllCandidatesProtected)
            )
        {
            return false;
        }
        // Even unchanged selections need target confirmation of the selection-end
        // caret before session completion. Do not infer it from engine success.
        if (output.changes_needed || active.request.selected_text)
            && !replace(&target, &active.request, &output)
        {
            return false;
        }
        let session = manager.active_mut().unwrap();
        if active.request.selected_text {
            session.complete_selected_correction(
                original,
                &output.corrected_executable_text,
                &active.request.informative_context,
                active.request.versions,
                limits,
            )
        } else if output.changes_needed {
            session.queue_correction(original.clone(), output.corrected_executable_text)
                && session.apply_next_correction(limits)
        } else {
            session.complete_without_changes(limits);
            true
        }
    }
}

impl Drop for CorrectionPipeline {
    fn drop(&mut self) {
        self.cancel();
        let (lock, ready) = &*self.mailbox;
        lock.lock().unwrap().stopped = true;
        ready.notify_all();
        // A synchronous provider can finish in its own timeout; never delay shutdown.
        if self
            .worker
            .as_ref()
            .is_some_and(|worker| worker.is_finished())
            && self.worker.take().unwrap().join().is_err()
        {
            tracing::error!("correction worker panicked");
        }
    }
}

#[cfg(test)]
mod tests;
