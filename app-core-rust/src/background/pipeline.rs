//! Async correction ownership and the single validation gate before completion.
//! FIFO work and completions are bounded by each session's frozen queue.

use std::{
    collections::VecDeque,
    path::PathBuf,
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
        ApiCorrectionEngine, ApiEngineConfig, ConfidenceBehavior, ConfidenceTier, CorrectionEngine,
        CorrectionEngines, CorrectionInput, CorrectionOutput, EngineKind, EngineStatus,
        NoChangeReason, SendAuthorization, TriggerType,
    },
    settings::{AppConfig, ContextConfig},
};

pub(super) const MANUAL_WAIT: Duration = Duration::from_millis(20);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct InputStamp {
    pub(super) position: u64,
    pub(super) sequence: u64,
}

impl InputStamp {
    /// Frozen text tolerates later typing, but every request requires the same position.
    pub(super) fn permits(self, current: Self, frozen: bool) -> bool {
        self.position == current.position && (frozen || self.sequence == current.sequence)
    }
}

struct Job {
    id: u64,
    request: CorrectionRequest,
    input: CorrectionInput,
    api: ApiEngineConfig,
    cancelled: Arc<AtomicBool>,
    authorization: SendAuthorization,
    exclusions: crate::dictionary::Policy,
}

struct Completion {
    id: u64,
    output: CorrectionOutput,
}

#[derive(Default)]
struct Mailbox {
    jobs: VecDeque<Job>,
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
    show_timeout_notice: bool,
}

impl ActiveRequest {
    /// Require the admitted session and range; only frozen work tolerates later typing.
    fn valid(&self, session: &Session, stamp: InputStamp) -> bool {
        if let Some(id) = self.request.pending_segment_id {
            return !self.cancelled.load(Ordering::Acquire)
                && self.request.session_id == session.id()
                && self.stamp.permits(stamp, true)
                && session.pending_matches(id, &self.request.executable_context);
        }
        !self.cancelled.load(Ordering::Acquire)
            && self.request.session_id == session.id()
            && self.request.versions == session.versions()
            && self.editable_snapshot == session.editable_context()
            && self.stamp.permits(stamp, false)
            && !self.request.executable_context.is_empty()
            && (self.request.selected_text
                || self
                    .editable_snapshot
                    .ends_with(&self.request.executable_context))
            && (self.request.selected_text || !session.position_uncertain())
    }
}

pub(super) struct CorrectionPipeline {
    database_path: Option<PathBuf>,
    mailbox: Arc<(Mutex<Mailbox>, Condvar)>,
    worker: Option<JoinHandle<()>>,
    active: VecDeque<ActiveRequest>,
    next_id: u64,
    timeout_notice: bool,
}

impl CorrectionPipeline {
    /// Start the correction worker using the configured local or API engine.
    pub(super) fn new() -> std::io::Result<Self> {
        Self::start(|job| match job.request.engine {
            EngineKind::OpenAiCompatibleApi | EngineKind::CustomApi => {
                ApiCorrectionEngine::new(job.request.engine, job.api.clone())
                    .with_send_authorization(Arc::clone(&job.authorization))
                    .correct(&job.input)
            }
            _ => CorrectionEngines::default().correct_with(job.request.engine, &job.input),
        })
    }

    /// Bind runtime API jobs to the same database used by app-rule writers.
    pub(super) fn with_database(database: &crate::storage::Database) -> std::io::Result<Self> {
        let mut pipeline = Self::new()?;
        pipeline.database_path = database.path().map(PathBuf::from);
        Ok(pipeline)
    }

    /// Start FIFO execution with one completion slot so no result can be overwritten.
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
                        while (state.jobs.is_empty() || state.completion.is_some())
                            && !state.stopped
                        {
                            state = ready.wait(state).unwrap();
                        }
                        if state.stopped {
                            break;
                        }
                        state.jobs.pop_front().unwrap()
                    };
                    if job.cancelled.load(Ordering::Acquire) {
                        continue;
                    }
                    let output = job
                        .exclusions
                        .filter(&job.input.executable_context, correct(&job));
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
            database_path: None,
            mailbox,
            worker: Some(worker),
            active: VecDeque::new(),
            next_id: 1,
            timeout_notice: false,
        })
    }

    /// Snapshot engine settings and dictionary terms before handing work to the worker.
    pub(super) fn submit(
        &mut self,
        request: CorrectionRequest,
        session: &Session,
        target: FocusedTarget,
        stamp: InputStamp,
        config: &AppConfig,
        dictionary: Vec<String>,
    ) -> bool {
        if request.pending_segment_id.is_none() {
            self.cancel();
        }
        let id = self.next_id;
        self.next_id = self.next_id.checked_add(1).expect("request ID exhausted");
        let cancelled = Arc::new(AtomicBool::new(false));
        let authorization = super::security::api_send_authorization(
            self.database_path.clone(),
            config.clone(),
            target.clone(),
            request.trigger,
            Arc::clone(&cancelled),
        );
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
        let exclusions = match self.database_path.as_deref() {
            Some(path) => match crate::storage::Database::open(path).and_then(|db| {
                db.dictionary()
                    .policy(&target.process_name, &request.language_info)
            }) {
                Ok(policy) => policy,
                Err(_) => {
                    tracing::warn!("correction skipped: exclusions unavailable");
                    return false;
                }
            },
            None => crate::dictionary::Policy::default(),
        };
        self.active.push_back(ActiveRequest {
            id,
            request: request.clone(),
            target,
            editable_snapshot: session.editable_context(),
            stamp,
            cancelled: Arc::clone(&cancelled),
            show_timeout_notice: config.feedback.show_timeout_notice,
        });
        let (lock, ready) = &*self.mailbox;
        let mut state = lock.lock().unwrap();
        state.jobs.push_back(Job {
            id,
            request,
            input,
            api: (&config.api).into(),
            cancelled,
            authorization,
            exclusions,
        });
        ready.notify_all();
        true
    }

    /// Cancel all admitted work and clear queued jobs, completions, and timeout feedback.
    pub(super) fn cancel(&mut self) {
        self.timeout_notice = false;
        for active in self.active.drain(..) {
            active.cancelled.store(true, Ordering::Release);
        }
        let (lock, ready) = &*self.mailbox;
        let mut state = lock.lock().unwrap();
        state.jobs.clear();
        state.completion = None;
        ready.notify_all();
    }

    /// Cancel one frozen segment without discarding other admitted requests.
    pub(super) fn cancel_segment(&mut self, segment_id: u64) {
        self.active.retain(|active| {
            if active.request.pending_segment_id == Some(segment_id) {
                active.cancelled.store(true, Ordering::Release);
                false
            } else {
                true
            }
        });
        self.purge_cancelled();
    }

    /// Remove cancelled jobs and orphan completions, waking the worker if blocked.
    fn purge_cancelled(&self) {
        let (lock, ready) = &*self.mailbox;
        let mut state = lock.lock().unwrap();
        state
            .jobs
            .retain(|job| !job.cancelled.load(Ordering::Acquire));
        if state
            .completion
            .as_ref()
            .is_some_and(|completion| !self.active.iter().any(|active| active.id == completion.id))
        {
            state.completion = None;
        }
        ready.notify_all();
    }

    /// Frozen work survives newer typing; manual snapshots remain strict.
    pub(super) fn invalidate(&mut self, manager: &mut SessionManager, stamp: InputStamp) {
        // Losing worker ownership does not mean the typed segment was checked.
        // Return it and its dependent suffix to the active context when present.
        for active in &self.active {
            if let Some(session) = manager.active_mut() {
                if session.id() == active.request.session_id && !active.valid(session, stamp) {
                    if let Some(id) = active.request.pending_segment_id {
                        session.restore_pending_from(id);
                    }
                }
            }
        }
        self.active.retain(|active| {
            if manager
                .active()
                .is_none_or(|session| !active.valid(session, stamp))
            {
                active.cancelled.store(true, Ordering::Release);
                false
            } else {
                true
            }
        });
        self.purge_cancelled();
    }

    /// Allow a short manual wait without blocking the input hook or message loop.
    pub(super) fn wait_manual(&self) {
        let (lock, ready) = &*self.mailbox;
        let state = lock.lock().unwrap();
        let _ = ready
            .wait_timeout_while(state, MANUAL_WAIT, |state| {
                state.completion.is_none() && !state.stopped
            })
            .unwrap();
    }

    /// Consume eligible manual timeout feedback exactly once.
    pub(super) fn take_timeout_notice(&mut self) -> bool {
        std::mem::take(&mut self.timeout_notice)
    }

    /// Take once: duplicates and reordered completions cannot reach the mutation owner.
    pub(super) fn finish(
        &mut self,
        manager: &mut SessionManager,
        limits: &ContextConfig,
        current_stamp: impl Fn() -> InputStamp,
        check_target: impl FnOnce(TriggerKind) -> Option<FocusedTarget>,
        read_before_caret: impl FnOnce(&FocusedTarget, usize) -> Option<String>,
        replace: impl FnOnce(&FocusedTarget, &CorrectionRequest, &CorrectionOutput) -> bool,
    ) -> bool {
        self.timeout_notice = false;
        self.invalidate(manager, current_stamp());
        let completion = {
            let (lock, ready) = &*self.mailbox;
            let result = lock.lock().unwrap().completion.take();
            ready.notify_all();
            result
        };
        let Some(completion) = completion else {
            return false;
        };
        let Some(active) = self.active.front() else {
            return false;
        };
        if completion.id != active.id {
            return false;
        }
        let mut active = self.active.pop_front().unwrap();
        let validation_stamp = current_stamp();
        let segment_id = active.request.pending_segment_id;
        // Every completion releases its slot. Only accepted results commit;
        // failed or skipped work returns to executable context below.
        let applied = (|| {
            let output = completion.output;
            let notify_timeout = output.status == EngineStatus::TimedOut
                && active.show_timeout_notice
                && active.request.trigger == TriggerKind::ManualShortcut
                && matches!(
                    active.request.engine,
                    EngineKind::OpenAiCompatibleApi | EngineKind::CustomApi
                );
            // Silent failures release their slot without another potentially slow UIA call.
            if output.status != EngineStatus::Completed && !notify_timeout {
                return false;
            }
            let Some(target) = check_target(active.request.trigger) else {
                return false;
            };
            // Security/UIA calls can race with queued typing or focus changes.
            if target.correction_eligibility() != CorrectionEligibility::Allowed
                || target.process_id != active.target.process_id
                || target.process_name != active.target.process_name
                || target.window_handle != active.target.window_handle
                || target.focused_element_id != active.target.focused_element_id
                || target.session_key() != active.target.session_key()
                || !manager.active_matches(&target)
                || current_stamp() != validation_stamp
                || manager
                    .active()
                    .is_none_or(|session| !active.valid(session, current_stamp()))
            {
                return false;
            }
            if output.status != EngineStatus::Completed {
                self.timeout_notice = notify_timeout;
                return false;
            }
            let original = &active.request.executable_context;
            if output.changes_needed {
                if target.focused_element_id.is_none()
                    || output.behavior != ConfidenceBehavior::Silent
                    || output.confidence == ConfidenceTier::Low
                    || output.corrected_executable_text == *original
                {
                    return false;
                }
            } else if output.corrected_executable_text != *original
                || !matches!(
                    output.no_change_reason,
                    Some(
                        NoChangeReason::NoCorrectionNeeded | NoChangeReason::AllCandidatesProtected
                    )
                )
            {
                return false;
            }
            if let Some(id) = segment_id {
                let Some(following) = manager.active().unwrap().pending_following_text(id) else {
                    return false;
                };
                active.request.replacement_following_text = following;
            }
            // A selection does not prove which end contains the live caret.
            // V1 only completes corrections of collapsed, pre-caret ranges.
            if active.request.selected_text {
                return false;
            }
            let session = manager.active().unwrap();
            let known_before_caret = format!(
                "{}{}",
                session.informative_context(),
                session.executable_context()
            );
            let Some(live_before_caret) =
                read_before_caret(&target, known_before_caret.chars().count())
            else {
                return false;
            };
            // The exact tracked region must still end at the live caret.
            // Never search elsewhere in the document for a similar span.
            if !exact_range_before_caret(
                &live_before_caret,
                &known_before_caret,
                original,
                &active.request.replacement_following_text,
            ) || current_stamp() != validation_stamp
                || !active.valid(session, current_stamp())
            {
                return false;
            }
            if output.changes_needed && !replace(&target, &active.request, &output) {
                return false;
            }
            let session = manager.active_mut().unwrap();
            let changed = output.changes_needed;
            let completed = if let Some(id) = segment_id {
                session.complete_pending(id, &output.corrected_executable_text, limits)
            } else if output.changes_needed {
                session.queue_correction(original.clone(), output.corrected_executable_text)
                    && session.apply_next_correction(limits)
            } else {
                session.complete_without_changes(limits);
                true
            };
            if completed && changed {
                session.record_undo_language(active.request.language_info.primary_language.clone());
            }
            completed
        })();
        if !applied {
            if let (Some(id), Some(session)) = (segment_id, manager.active_mut()) {
                if session.id() == active.request.session_id {
                    for cancelled in session.restore_pending_from(id) {
                        self.cancel_segment(cancelled);
                    }
                }
            }
        }
        applied
    }
}

/// Require the tracked original and following text at the live caret without searching.
fn exact_range_before_caret(
    live_before_caret: &str,
    known_before_caret: &str,
    original: &str,
    following: &str,
) -> bool {
    !original.is_empty()
        && known_before_caret.ends_with(&format!("{original}{following}"))
        && live_before_caret.ends_with(known_before_caret)
}

impl Drop for CorrectionPipeline {
    /// Stop publication and wake the worker without waiting for a synchronous provider.
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
