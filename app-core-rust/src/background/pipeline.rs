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
    feedback::suggestion::{Preview, PreviewSuggestionUi, SuggestionUi},
    feedback::Event,
    replacement::ReplacementConfirmation,
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
    trigger_type: TriggerType,
    suggestion_ui_available: bool,
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
    feedback_event: Option<(Event, bool)>,
    suggestion_preview: Option<Preview>,
    preview_cancelled: Arc<AtomicBool>,
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
            feedback_event: None,
            suggestion_preview: None,
            preview_cancelled: Arc::new(AtomicBool::new(false)),
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
        // Check executable text before either local execution or API transmission.
        if let Some(path) = self.database_path.as_deref() {
            let allowed =
                crate::storage::AppPolicyGuard::read_rules_nowait(path).is_ok_and(|rules| {
                    matches!(
                        super::security::check_detection(
                            request.trigger,
                            config,
                            &rules,
                            super::target::TargetDetection::Available(target.clone())
                        ),
                        super::security::SecurityDecision::Allowed { .. }
                    ) && super::security::request_allowed(&rules, &target, &request)
                });
            if !allowed {
                self.feedback_event = Some((
                    Event::Blocked,
                    request.trigger == TriggerKind::ManualShortcut,
                ));
                return false;
            }
        } else if !super::security::request_allowed(&[], &target, &request) {
            return false;
        }
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
            request.clone(),
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
            suggestion_ui_available: PreviewSuggestionUi::new(&config.feedback).is_available(),
        };
        let exclusions = match self.database_path.as_deref() {
            Some(path) => match crate::dictionary::Repository::policy_nowait(
                path,
                &target.process_name,
                &request.language_info,
            ) {
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
            trigger_type: input.trigger_type,
            suggestion_ui_available: input.suggestion_ui_available,
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
        self.preview_cancelled.store(true, Ordering::Release);
        self.preview_cancelled = Arc::new(AtomicBool::new(false));
        self.timeout_notice = false;
        self.feedback_event = None;
        self.suggestion_preview = None;
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
    #[cfg(test)]
    pub(super) fn take_timeout_notice(&mut self) -> bool {
        std::mem::take(&mut self.timeout_notice)
    }

    pub(super) fn take_feedback(&mut self) -> Option<(Event, bool)> {
        self.feedback_event.take()
    }

    pub(super) fn take_suggestion(&mut self) -> Option<Preview> {
        self.suggestion_preview.take()
    }

    pub(super) fn is_correcting(&self) -> bool {
        !self.active.is_empty()
    }

    /// Take once: duplicates and reordered completions cannot reach the mutation owner.
    pub(super) fn finish<R: Into<ReplacementConfirmation>>(
        &mut self,
        manager: &mut SessionManager,
        limits: &ContextConfig,
        current_stamp: impl Fn() -> InputStamp,
        check_target: impl FnOnce(TriggerKind) -> Option<FocusedTarget>,
        read_before_caret: impl FnOnce(&FocusedTarget, usize) -> Option<String>,
        replace: impl FnOnce(&FocusedTarget, &CorrectionRequest, &CorrectionOutput) -> R,
    ) -> bool {
        self.timeout_notice = false;
        self.feedback_event = None;
        self.suggestion_preview = None;
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
        let mut native_changed = false;
        // Every completion releases its slot. Only accepted results commit;
        // failed or skipped work returns to executable context below.
        let applied = (|| {
            let output = completion.output;
            let manual = active.request.trigger == TriggerKind::ManualShortcut;
            let notify_timeout = output.status == EngineStatus::TimedOut
                && active.show_timeout_notice
                && active.request.trigger == TriggerKind::ManualShortcut
                && matches!(
                    active.request.engine,
                    EngineKind::OpenAiCompatibleApi | EngineKind::CustomApi
                );
            // Silent failures release their slot without another potentially slow UIA call.
            let notify_error = manual && matches!(output.status, EngineStatus::Error(_));
            if output.status != EngineStatus::Completed && !notify_timeout && !notify_error {
                self.feedback_event = Some((
                    if output.status == EngineStatus::TimedOut {
                        Event::Timeout
                    } else {
                        Event::Error
                    },
                    false,
                ));
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
                self.feedback_event = Some((
                    if notify_timeout {
                        Event::Timeout
                    } else {
                        Event::Error
                    },
                    manual,
                ));
                return false;
            }
            // Generic fallback contains no provider text or document contents.
            self.feedback_event = Some((
                if manual {
                    Event::Error
                } else {
                    Event::Skipped("AutoFix: could not verify the editable range.")
                },
                manual,
            ));
            let original = &active.request.executable_context;
            let no_change_reason = match output.no_change_reason {
                Some(NoChangeReason::NoCorrectionNeeded) => Some("no_correction_needed"),
                Some(NoChangeReason::AllCandidatesProtected) => Some("all_candidates_protected"),
                _ => None,
            };
            if output.changes_needed {
                if target.focused_element_id.is_none()
                    || output.behavior == ConfidenceBehavior::DoNothing
                    || output.confidence == ConfidenceTier::Low
                    || output.behavior
                        != active.request.confidence_behavior.behavior_for(
                            output.confidence,
                            active.trigger_type,
                            active.suggestion_ui_available,
                        )
                    || output.corrected_executable_text == *original
                {
                    self.feedback_event = Some((Event::Skipped("AutoFix: confidence policy requires a suggestion or skips this correction."), manual));
                    return false;
                }
            } else if output.corrected_executable_text != *original || no_change_reason.is_none() {
                let reason = match output.no_change_reason {
                    Some(NoChangeReason::UnsupportedLanguage) => {
                        "AutoFix: language is not supported by this engine."
                    }
                    Some(NoChangeReason::UncertainLanguage) => {
                        "AutoFix: language policy skipped this correction."
                    }
                    Some(NoChangeReason::ConfidenceBelowConfiguredBehavior) => {
                        "AutoFix: confidence policy skipped this correction."
                    }
                    _ => "AutoFix: correction was skipped.",
                };
                self.feedback_event = Some((Event::Skipped(reason), manual));
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
            if output.behavior == ConfidenceBehavior::Suggestion {
                // A preview never authorizes replacement or commits executable context.
                self.feedback_event = None;
                if manual && output.confidence == ConfidenceTier::Medium {
                    self.suggestion_preview = Some(Preview {
                        text: super::feedback::suggestion_preview(
                            &output.corrected_executable_text,
                        ),
                        stamp: validation_stamp,
                        target,
                        cancelled: Arc::clone(&self.preview_cancelled),
                    });
                }
                return false;
            }
            let confirmation = if output.changes_needed {
                if !session.can_complete_correction(
                    segment_id,
                    original,
                    &output.corrected_executable_text,
                ) {
                    return false;
                }
                let confirmation = replace(&target, &active.request, &output).into();
                if !confirmation.success {
                    return false;
                }
                native_changed = true;
                if current_stamp() != validation_stamp {
                    return false;
                }
                Some(confirmation)
            } else {
                None
            };
            let session = manager.active_mut().unwrap();
            let changed = output.changes_needed;
            let confidence = output.confidence;
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
                self.feedback_event = Some((Event::Applied, manual));
                session.record_undo_metadata(
                    active.request.language_info.primary_language.clone(),
                    active.request.trigger,
                    confidence,
                    confirmation.unwrap(),
                );
            } else if completed {
                self.feedback_event = Some((
                    Event::Skipped(if no_change_reason == Some("all_candidates_protected") {
                        "AutoFix: matching terms are protected."
                    } else {
                        "AutoFix: no correction needed."
                    }),
                    manual,
                ));
                self.log_no_change_commit(
                    &active.request,
                    &active.target,
                    confidence,
                    no_change_reason.unwrap(),
                    output.engine_latency_ms,
                );
            }
            completed
        })();
        if !applied {
            if native_changed {
                // The document changed but session ownership was not committed.
                // Never restore unchecked originals into a now-stale executable range.
                self.cancel();
                manager.deactivate(super::typing::MovementSignal::UnknownPosition);
                tracing::warn!("replacement bookkeeping lost session ownership");
                return false;
            }
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

    /// Record accepted commits once without waiting on SQLite policy reservations.
    /// Metadata contains no document text, even in full-debug mode.
    fn log_no_change_commit(
        &self,
        request: &CorrectionRequest,
        target: &FocusedTarget,
        confidence: ConfidenceTier,
        reason: &str,
        latency_ms: u64,
    ) {
        tracing::info!(
            session_id = request.session_id,
            trigger = request.trigger.as_str(),
            engine = ?request.engine,
            confidence = ?confidence,
            reason,
            latency_ms,
            replacement_method = "none",
            "no-change context committed"
        );
        let Some(path) = self.database_path.as_deref() else {
            return;
        };
        let metadata = crate::storage::CorrectionMetadata {
            session_id: request.session_id.to_string(),
            app_process_name: target.process_name.clone(),
            trigger_type: request.trigger.as_str().into(),
            confidence_tier: match confidence {
                ConfidenceTier::High => "high",
                ConfidenceTier::Medium => "medium",
                ConfidenceTier::Low => "low",
            }
            .into(),
            engine_used: match request.engine {
                EngineKind::LocalRule => "local_rule",
                EngineKind::LocalMl => "local_ml",
                EngineKind::OpenAiCompatibleApi => "open_ai_compatible_api",
                EngineKind::CustomApi => "custom_api",
            }
            .into(),
            replacement_method: "none".into(),
            result_reason: reason.into(),
            latency_ms,
        };
        if crate::storage::Database::record_metadata_nowait(path, &metadata).is_err() {
            tracing::warn!(
                session_id = request.session_id,
                "no-change metadata unavailable"
            );
        }
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
