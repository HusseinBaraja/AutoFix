mod admin;
pub(crate) mod app_identity;
mod components;
mod context_capture;
pub(crate) mod feedback;
mod informative_context;
mod input_listener;
mod message_loop;
mod paths;
mod pipeline;
mod process_group;
mod replacement;
mod security;
mod session;
mod shortcuts;
mod target;
#[cfg(test)]
mod tests;
mod triggers;
mod typing;

use std::{
    collections::VecDeque,
    error::Error,
    fmt, fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Condvar, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, SystemTime},
};

use crate::{
    settings::{save_config, AppConfig},
    storage::Database,
};

use self::{
    admin::reject_elevated_process,
    components::NamedPipeIpcServer,
    input_listener::{InputEvent, InputListener},
    paths::RuntimePaths,
    pipeline::{CorrectionPipeline, InputStamp},
    process_group::SiblingDisappearanceMonitor,
    replacement::ReplacementEngine,
    security::{SecurityDecision, SecurityGate, TriggerKind},
    session::{MovementResolution, SessionManager},
    shortcuts::{GlobalShortcutListener, ShortcutAction},
    triggers::CorrectionRequest,
    typing::MovementSignal,
};

pub(crate) struct BackgroundRuntime {
    components: RuntimeComponents,
}

struct RuntimeComponents {
    config_path: PathBuf,
    config_modified_at: Option<SystemTime>,
    config: AppConfig,
    ipc_server: NamedPipeIpcServer,
    global_shortcut: GlobalShortcutListener,
    input_listener: InputListener,
    input_worker: InputWorker,
    process_group_monitor: SiblingDisappearanceMonitor,
    shutdown_signal: process_group::ShutdownSignal,
    shutdown_requested: Arc<AtomicBool>,
}

struct InputWorker {
    queue: Arc<(Mutex<VecDeque<InputWork>>, Condvar)>,
    done: mpsc::Receiver<bool>,
    thread: Option<JoinHandle<()>>,
}

enum InputWork {
    Events(Vec<InputEvent>),
    Shortcut(usize),
    Tick,
    Config(Box<AppConfig>),
    Reset,
    Shutdown,
    #[cfg(test)]
    Probe(mpsc::Sender<thread::ThreadId>),
    #[cfg(test)]
    Pause(mpsc::Sender<()>, mpsc::Receiver<()>),
}

const INPUT_WORK_QUEUE_LIMIT: usize = 8;

/// Accept read-only capture only while the hook input sequence remains unchanged.
fn capture_if_current<T>(
    expected: u64,
    current: impl Fn() -> u64,
    capture: impl FnOnce() -> T,
) -> Option<T> {
    if current() != expected {
        return None;
    }
    let result = capture();
    (current() == expected).then_some(result)
}

struct InputProcessor {
    feedback: feedback::Feedback,
    learner: crate::dictionary::Learner,
    pipeline: CorrectionPipeline,
    processed_input_sequence: u64,
    config: AppConfig,
    session_manager: SessionManager,
    database: Database,
}

struct PendingTrigger {
    editable_snapshot: String,
    request: CorrectionRequest,
}

impl PendingTrigger {
    /// Frozen ranges survive later typing; unsegmented requests require an exact snapshot.
    fn matches_session(&self, session: &session::Session) -> bool {
        self.request.session_id == session.id()
            && self.request.pending_segment_id.map_or_else(
                || self.editable_snapshot == session.editable_context(),
                |id| session.pending_matches(id, &self.request.executable_context),
            )
    }
}

#[derive(Debug)]
pub(crate) enum BackgroundError {
    ElevatedProcess,
    CreateDirectory {
        path: std::path::PathBuf,
        source: std::io::Error,
    },
    Config(crate::settings::ConfigIoError),
    Database(rusqlite::Error),
    InputHook(u32),
    InputWorker(std::io::Error),
    ShutdownSignal(u32),
}

impl fmt::Display for BackgroundError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ElevatedProcess => write!(formatter, "AutoFix v1 must run as a normal user"),
            Self::CreateDirectory { path, source } => {
                write!(formatter, "failed to create {}: {}", path.display(), source)
            }
            Self::Config(source) => write!(formatter, "config error: {}", source),
            Self::Database(source) => write!(formatter, "database error: {}", source),
            Self::InputHook(code) => write!(
                formatter,
                "failed to install input listener: Windows error {code}"
            ),
            Self::ShutdownSignal(code) => write!(
                formatter,
                "failed to create engine stop signal: Windows error {code}"
            ),
            Self::InputWorker(source) => {
                write!(formatter, "failed to start input worker: {source}")
            }
        }
    }
}

impl Error for BackgroundError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::CreateDirectory { source, .. } => Some(source),
            Self::Config(source) => Some(source),
            Self::Database(source) => Some(source),
            Self::InputWorker(source) => Some(source),
            Self::ElevatedProcess | Self::InputHook(_) | Self::ShutdownSignal(_) => None,
        }
    }
}

pub(crate) fn run_background_mode() -> Result<(), BackgroundError> {
    let mut runtime = BackgroundRuntime::start(RuntimePaths::for_current_user()?)?;
    runtime.run_until_exit();
    runtime.shutdown();
    Ok(())
}

impl BackgroundRuntime {
    fn start(paths: RuntimePaths) -> Result<Self, BackgroundError> {
        reject_elevated_process()?;
        initialize_logging();
        ensure_parent_directory(paths.config_path())?;
        ensure_parent_directory(paths.database_path())?;

        let config = load_or_create_config(paths.config_path())?;
        let database = Database::open(paths.database_path()).map_err(BackgroundError::Database)?;
        let shutdown_requested = Arc::new(AtomicBool::new(false));
        let components = RuntimeComponents::start(&config, &paths, shutdown_requested, database)?;

        tracing::info!("AutoFix background process started");
        Ok(Self { components })
    }

    /// Report unresolved cleanup separately from a fully drained graceful exit.
    fn shutdown(self) {
        if self.components.shutdown() {
            tracing::info!("AutoFix background process exited cleanly");
        } else {
            tracing::warn!("AutoFix background process exited with incomplete cleanup");
        }
    }

    fn run_until_exit(&mut self) {
        self.components.run_until_exit();
    }
}

impl RuntimeComponents {
    fn start(
        config: &AppConfig,
        paths: &RuntimePaths,
        shutdown_requested: Arc<AtomicBool>,
        database: Database,
    ) -> Result<Self, BackgroundError> {
        let input_listener = InputListener::initialize().map_err(BackgroundError::InputHook)?;
        let shutdown_signal =
            process_group::ShutdownSignal::new().map_err(BackgroundError::ShutdownSignal)?;
        let input_worker = InputWorker::start(config.clone(), database)?;
        Ok(Self {
            config_path: paths.config_path().to_path_buf(),
            config_modified_at: modified_at(paths.config_path()),
            config: config.clone(),
            ipc_server: NamedPipeIpcServer::initialize(
                config,
                paths,
                Arc::clone(&shutdown_requested),
            )?,
            global_shortcut: GlobalShortcutListener::initialize(config),
            input_listener,
            input_worker,
            process_group_monitor: SiblingDisappearanceMonitor::new(),
            shutdown_signal,
            shutdown_requested,
        })
    }

    /// Stop input mutation first and preserve the cleanup outcome through component teardown.
    fn shutdown(self) -> bool {
        let clean = self.input_worker.shutdown();
        drop(self.input_listener);
        self.global_shortcut.shutdown();
        self.ipc_server.shutdown();
        clean
    }

    fn run_until_exit(&mut self) {
        message_loop::run_until_exit(|event| {
            match event {
                message_loop::MessageLoopEvent::Hotkey(id) => {
                    let events = self.input_listener.drain();
                    if !events.is_empty() {
                        self.input_worker.send(InputWork::Events(events));
                    }
                    self.input_worker.send(InputWork::Shortcut(id))
                }
                message_loop::MessageLoopEvent::Poll => {
                    let events = self.input_listener.drain();
                    if !events.is_empty() {
                        self.input_worker.send(InputWork::Events(events));
                    }
                }
                message_loop::MessageLoopEvent::Tick => {
                    self.reload_shortcuts_if_config_changed();
                    self.input_worker.send(InputWork::Tick);
                    if self.process_group_monitor.shutdown_requested() {
                        self.shutdown_requested.store(true, Ordering::Relaxed);
                    }
                }
            }

            self.shutdown_requested.load(Ordering::Relaxed) || self.shutdown_signal.requested()
        });
    }

    fn reload_shortcuts_if_config_changed(&mut self) {
        let modified_at = modified_at(&self.config_path);
        if modified_at == self.config_modified_at {
            return;
        }

        self.config_modified_at = modified_at;
        match crate::settings::load_config(&self.config_path) {
            Ok(config) => {
                if shortcuts::detect_conflict(&config) {
                    tracing::warn!("shortcut conflict detected while reloading config");
                }
                self.config = config.clone();
                self.global_shortcut.reload(&config);
                self.input_worker.send(InputWork::Config(Box::new(config)));
            }
            Err(error) => tracing::warn!("failed to reload shortcuts from config: {}", error),
        }
    }
}

impl InputWorker {
    fn start(config: AppConfig, database: Database) -> Result<Self, BackgroundError> {
        let pipeline =
            CorrectionPipeline::with_database(&database).map_err(BackgroundError::InputWorker)?;
        let queue = Arc::new((Mutex::new(VecDeque::new()), Condvar::new()));
        let worker_queue = Arc::clone(&queue);
        let (done_sender, done) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("autofix-input-processing".into())
            .spawn(move || {
                let mut processor = InputProcessor {
                    feedback: feedback::Feedback::default(),
                    learner: crate::dictionary::Learner::default(),
                    pipeline,
                    processed_input_sequence: input_listener::current_input_sequence(),
                    session_manager: SessionManager::new(config.context.clone()),
                    config,
                    database,
                };
                loop {
                    let work = {
                        let (lock, ready) = &*worker_queue;
                        let mut pending = lock.lock().unwrap();
                        while pending.is_empty() {
                            let (next, timeout) = ready
                                .wait_timeout(pending, Duration::from_millis(10))
                                .unwrap();
                            pending = next;
                            if timeout.timed_out() && pending.is_empty() {
                                break;
                            }
                        }
                        pending.pop_front()
                    };
                    let Some(work) = work else {
                        processor.finish_correction();
                        continue;
                    };
                    match work {
                        InputWork::Events(events) => processor.process_input(events),
                        InputWork::Shortcut(id) => processor.process_shortcut(id),
                        InputWork::Tick => processor.session_manager.prune_exited(),
                        InputWork::Config(config) => {
                            processor.feedback.reset();
                            processor.pipeline.cancel();
                            if let Some(session) = processor.session_manager.active_mut() {
                                session.restore_pending();
                            }
                            processor
                                .session_manager
                                .update_limits(config.context.clone());
                            processor.config = *config;
                        }
                        InputWork::Reset => {
                            processor.feedback.reset();
                            processor.pipeline.cancel();
                            processor
                                .session_manager
                                .deactivate(MovementSignal::UnknownPosition);
                        }
                        InputWork::Shutdown => break,
                        #[cfg(test)]
                        InputWork::Probe(reply) => {
                            let _ = reply.send(thread::current().id());
                        }
                        #[cfg(test)]
                        InputWork::Pause(ready, release) => {
                            let _ = ready.send(());
                            let _ = release.recv();
                        }
                    }
                    processor.finish_correction();
                }
                processor.learner.finish();
                let _ = done_sender.send(replacement::finish_shutdown());
            })
            .map_err(BackgroundError::InputWorker)?;
        Ok(Self {
            queue,
            done,
            thread: Some(thread),
        })
    }

    fn send(&self, work: InputWork) {
        let (lock, ready) = &*self.queue;
        let mut pending = lock.lock().unwrap();
        if pending.len() >= INPUT_WORK_QUEUE_LIMIT {
            if matches!(work, InputWork::Tick) {
                return;
            }
            let latest_config = pending.iter().rev().find_map(|queued| match queued {
                InputWork::Config(config) => Some(config.clone()),
                _ => None,
            });
            pending.clear();
            pending.push_back(InputWork::Reset);
            if let Some(config) = latest_config {
                pending.push_back(InputWork::Config(config));
            }
            if matches!(work, InputWork::Events(_) | InputWork::Shortcut(_)) {
                tracing::warn!("discarded queued input after slow processing");
                ready.notify_one();
                return;
            }
        }
        pending.push_back(work);
        ready.notify_one();
    }

    /// Wait for processor and clipboard cleanup without treating a timeout as success.
    fn shutdown(mut self) -> bool {
        replacement::begin_shutdown();
        let (lock, ready) = &*self.queue;
        {
            let mut pending = lock.lock().unwrap();
            pending.clear();
            pending.push_back(InputWork::Shutdown);
            ready.notify_one();
        }
        if let Ok(clean) = self.done.recv_timeout(Duration::from_secs(4)) {
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
            clean
        } else {
            tracing::warn!("input processor still waiting on UI Automation during shutdown");
            false
        }
    }
}

impl InputProcessor {
    /// Snapshot hook position and input generations without reading target text.
    fn input_stamp() -> InputStamp {
        InputStamp {
            position: input_listener::current_position_generation(),
            sequence: input_listener::current_input_sequence(),
        }
    }

    /// Defer completion until hook input is drained, then validate the live target and caret.
    fn finish_correction(&mut self) {
        self.learner
            .poll(&self.config.learning, self.database.path());
        // Frozen ranges may survive processed typing, but never guess what keys
        // still queued in the hooks did to the target.
        if Self::input_stamp().sequence != self.processed_input_sequence {
            self.feedback
                .publish(&self.config.feedback, false, self.pipeline.is_correcting());
            return;
        }
        let config = &self.config;
        let database = &self.database;
        let replacement_stamp = Self::input_stamp();
        let mut replacement_uncertain = false;
        self.pipeline.finish(
            &mut self.session_manager,
            &config.context,
            Self::input_stamp,
            |trigger| match SecurityGate::check(trigger, config, database) {
                SecurityDecision::Allowed { target } if config.correction.enabled => Some(target),
                _ => None,
            },
            |target, request, known_chars| {
                context_capture::read_correction_context(
                    target,
                    request,
                    &config.context,
                    known_chars,
                )
            },
            |target, request, output| {
                let Some(_policy_guard) = SecurityGate::authorize_correction_replacement(
                    request, config, database, target,
                ) else {
                    tracing::warn!(
                        success = false,
                        method = "none",
                        reason = "replacement policy unavailable or denied",
                        "replacement skipped"
                    );
                    return replacement::ReplacementConfirmation::default();
                };
                // Recheck live exclusions under the same writer reservation as mutation.
                let Ok(policy) = database
                    .dictionary()
                    .policy(&target.process_name, &request.language_info)
                else {
                    tracing::warn!("replacement skipped: exclusions unavailable");
                    return replacement::ReplacementConfirmation::default();
                };
                if policy.filter(&request.executable_context, output.clone()) != *output {
                    return replacement::ReplacementConfirmation::default();
                }
                let result = ReplacementEngine::replace(
                    target,
                    request,
                    output,
                    replacement_stamp,
                    config.replacement.clipboard_enabled,
                );
                replacement_uncertain = !result.success && result.may_have_changed;
                result.log_outcome(false);
                result.into()
            },
        );
        if replacement_uncertain {
            self.pipeline.cancel();
            self.session_manager
                .deactivate(MovementSignal::UnknownPosition);
        }
        if let Some((event, manual)) = self.pipeline.take_feedback() {
            self.feedback.event(event, manual, &self.config.feedback);
        }
        if let Some(preview) = self.pipeline.take_suggestion() {
            self.feedback.suggestion(preview, &self.config.feedback);
        }
        self.feedback.publish(
            &self.config.feedback,
            self.config.correction.enabled
                && self.session_manager.active().is_some_and(|session| {
                    !session.position_uncertain() && !session.executable_context().is_empty()
                }),
            self.pipeline.is_correcting(),
        );
    }

    /// Processes a guarded input batch, captures context, and snapshots correction policies.
    fn process_input(&mut self, events: Vec<InputEvent>) {
        let mut pending_requests = Vec::new();
        let mut gate_result: Option<(isize, bool)> = None;
        let mut needs_capture = false;
        let mut captured_prefix = None;
        let mut last_key_generation = None;
        let mut last_key_sequence = None;
        for event in events {
            match event {
                InputEvent::FocusChange => {
                    self.feedback.reset();
                    gate_result = None;
                    self.session_manager
                        .input(typing::TypedInput::Uncertain(MovementSignal::FocusChange));
                }
                InputEvent::MouseClick => {
                    self.feedback.reset();
                    gate_result = None;
                    self.session_manager
                        .input(typing::TypedInput::Uncertain(MovementSignal::MouseClick));
                }
                InputEvent::Key(key) => {
                    self.processed_input_sequence = key.input_sequence;
                    if key.matches_shortcut(&self.config.shortcuts.correct)
                        || key.matches_shortcut(&self.config.shortcuts.undo)
                    {
                        continue;
                    }
                    let generation = key.position_generation;
                    if generation != input_listener::current_position_generation() {
                        gate_result = None;
                        self.session_manager.deactivate(MovementSignal::FocusChange);
                        continue;
                    }
                    last_key_generation = Some(generation);
                    last_key_sequence = Some(key.input_sequence);
                    let window = key.window;
                    if window == 0 || window != target::active_window_handle_value() {
                        gate_result = None;
                        self.session_manager.deactivate(MovementSignal::FocusChange);
                        continue;
                    }
                    if let Some((checked_window, allowed)) = gate_result {
                        if checked_window == window {
                            if allowed {
                                if generation == input_listener::current_position_generation() {
                                    needs_capture |=
                                        self.track_input(key.translate(), &mut pending_requests);
                                } else {
                                    self.session_manager.deactivate(MovementSignal::FocusChange);
                                }
                            }
                            continue;
                        }
                    }
                    let decision =
                        SecurityGate::check(TriggerKind::Tracking, &self.config, &self.database);
                    if generation != input_listener::current_position_generation() {
                        gate_result = None;
                        self.session_manager.deactivate(MovementSignal::FocusChange);
                        continue;
                    }
                    match decision {
                        SecurityDecision::Allowed { target } if target.window_handle == window => {
                            self.feedback.blocked(false);
                            needs_capture |= self.session_manager.focus(&target);
                            gate_result = Some((window, true));
                            needs_capture |=
                                self.track_input(key.translate(), &mut pending_requests);
                        }
                        _ => {
                            self.feedback.blocked(true);
                            gate_result = Some((window, false));
                            self.session_manager.deactivate(MovementSignal::FocusChange);
                        }
                    }
                }
            }
        }
        if last_key_generation
            .is_some_and(|generation| generation != input_listener::current_position_generation())
        {
            self.session_manager.deactivate(MovementSignal::FocusChange);
            return;
        }
        let capture_sequence =
            last_key_sequence.unwrap_or_else(input_listener::current_input_sequence);
        if self.session_manager.needs_movement_resolution() {
            if let SecurityDecision::Allowed { target } =
                SecurityGate::check(TriggerKind::Tracking, &self.config, &self.database)
            {
                self.session_manager.focus(&target);
                if let Some(preceding) = capture_if_current(
                    capture_sequence,
                    input_listener::current_input_sequence,
                    || {
                        context_capture::read_before_caret(
                            &target,
                            &self.config.context,
                            self.session_manager.movement_capture_extra_chars(),
                        )
                    },
                ) {
                    if let MovementResolution::Reanchor {
                        final_fix: Some(old),
                    } = self.session_manager.resolve_movement(preceding.as_deref())
                    {
                        if self.final_fix_before_reanchor_allowed(&self.database) {
                            tracing::info!(
                                typed_chars = old.chars().count(),
                                "smart final-fix eligible; old caret replacement unavailable after movement"
                            );
                        }
                    }
                }
            } else {
                self.session_manager.deactivate(MovementSignal::FocusChange);
            }
        } else if needs_capture
            && !self
                .session_manager
                .active()
                .is_some_and(|session| session.position_uncertain())
        {
            if let SecurityDecision::Allowed { target } =
                SecurityGate::check(TriggerKind::Tracking, &self.config, &self.database)
            {
                self.session_manager.focus(&target);
                let executable = self
                    .session_manager
                    .active()
                    .map_or_else(String::new, |session| session.executable_context());
                if let Some(preceding) = capture_if_current(
                    capture_sequence,
                    input_listener::current_input_sequence,
                    || {
                        context_capture::read_before_caret(
                            &target,
                            &self.config.context,
                            executable.chars().count(),
                        )
                    },
                ) {
                    let context = context_capture::captured_context(
                        preceding.as_deref(),
                        &executable,
                        &self.config.context,
                    );
                    self.session_manager
                        .set_captured_informative_context(context);
                    captured_prefix = self
                        .session_manager
                        .active()
                        .map(|session| session.informative_context().to_owned());
                }
            }
        }
        if last_key_generation
            .is_some_and(|generation| generation != input_listener::current_position_generation())
        {
            self.session_manager.deactivate(MovementSignal::FocusChange);
            return;
        }
        let mut ready_requests = Vec::new();
        if let Some(session) = self.session_manager.active() {
            if !session.position_uncertain() {
                for mut pending in pending_requests {
                    if pending.matches_session(session) {
                        let request = &mut pending.request;
                        if request.pending_segment_id.is_none() {
                            request.informative_context = session.informative_context().to_owned();
                        } else if let Some(prefix) = &captured_prefix {
                            request.informative_context.insert_str(0, prefix);
                        }
                        request.versions = session.versions();
                        ready_requests.push(pending.request);
                    }
                }
            }
        }
        for request in ready_requests {
            self.dispatch_trigger(
                request,
                InputStamp {
                    position: last_key_generation
                        .unwrap_or_else(input_listener::current_position_generation),
                    sequence: capture_sequence,
                },
            );
        }
        if let Some(signal) = self
            .session_manager
            .active()
            .and_then(|session| session.latest_movement())
        {
            tracing::debug!(?signal, "typed session position changed");
        }
        let typed_chars = self
            .session_manager
            .active()
            .map_or(0, |session| session.executable_context().chars().count());
        tracing::debug!(typed_chars, "typed session updated");
    }

    /// Route configured shortcuts through security and snapshot validation.
    fn process_shortcut(&mut self, id: usize) {
        match GlobalShortcutListener::action_for_id(id) {
            Some(ShortcutAction::Correct) => {
                let stamp = Self::input_stamp();
                if stamp.sequence != self.processed_input_sequence {
                    return;
                }
                let decision =
                    SecurityGate::check(TriggerKind::ManualShortcut, &self.config, &self.database);
                if let SecurityDecision::Blocked { reason, .. } = decision {
                    self.feedback.blocked(true);
                    if feedback::is_app_block(reason) && Self::input_stamp() == stamp {
                        self.feedback
                            .event(feedback::Event::Blocked, true, &self.config.feedback);
                    }
                    return;
                }
                self.feedback.blocked(false);
                if let SecurityDecision::Allowed { target } = decision {
                    if self.config.shortcuts.correct_arbitrary_selection
                        && !self.session_manager.active_matches(&target)
                    {
                        self.session_manager.focus(&target);
                    }
                    if self.session_manager.active_matches(&target) {
                        self.pipeline.cancel();
                        if let Some(session) = self.session_manager.active_mut() {
                            session.restore_pending();
                        }
                        if let Some(session) = self.session_manager.active() {
                            let executable = session.editable_context();
                            let sequence = input_listener::current_input_sequence();
                            let selected = capture_if_current(
                                sequence,
                                input_listener::current_input_sequence,
                                || {
                                    context_capture::read_selection(
                                        &target,
                                        session.informative_context(),
                                        &executable,
                                        &self.config.context,
                                    )
                                },
                            );
                            if let Some(selected) = selected {
                                if matches!(
                                    selected,
                                    context_capture::SelectionCapture::NoSelection
                                ) && session.position_uncertain()
                                {
                                    self.feedback.event(
                                        feedback::Event::Blocked,
                                        true,
                                        &self.config.feedback,
                                    );
                                    return;
                                }
                                if let Some(request) = triggers::manual(
                                    session.id(),
                                    session.informative_context(),
                                    &executable,
                                    session.versions(),
                                    &selected,
                                    &self.config,
                                ) {
                                    self.dispatch_trigger(request, stamp);
                                } else if Self::input_stamp() == stamp {
                                    self.feedback.event(
                                        feedback::Event::Blocked,
                                        true,
                                        &self.config.feedback,
                                    );
                                }
                            }
                        }
                    } else if Self::input_stamp() == stamp {
                        self.feedback
                            .event(feedback::Event::Blocked, true, &self.config.feedback);
                    }
                }
            }
            Some(ShortcutAction::Undo) => {
                self.undo_correction();
            }
            None => {}
        }
    }

    /// Restore a verified session correction, committing bookkeeping before optional learning.
    /// Any uncertain native result or input race revokes the session's editable ownership.
    fn undo_correction(&mut self) {
        let stamp = Self::input_stamp();
        if stamp.sequence != self.processed_input_sequence {
            return;
        }
        let decision = SecurityGate::check(TriggerKind::Undo, &self.config, &self.database);
        if let SecurityDecision::Blocked { reason, .. } = decision {
            self.feedback.blocked(true);
            if feedback::is_app_block(reason) && Self::input_stamp() == stamp {
                self.feedback
                    .event(feedback::Event::Blocked, true, &self.config.feedback);
            }
            return;
        }
        let SecurityDecision::Allowed { target } = decision else {
            unreachable!()
        };
        self.feedback.blocked(false);
        if !self.session_manager.active_matches(&target) {
            if Self::input_stamp() == stamp {
                self.feedback
                    .event(feedback::Event::Blocked, true, &self.config.feedback);
            }
            return;
        }
        let Some(session) = self.session_manager.active() else {
            return;
        };
        let Some(undo) = session.undo_target() else {
            if Self::input_stamp() == stamp {
                self.feedback
                    .event(feedback::Event::Blocked, true, &self.config.feedback);
            }
            return;
        };
        let known = format!(
            "{}{}",
            session.informative_context(),
            session.executable_context()
        );
        let Some(live) = context_capture::read_before_caret(
            &target,
            &self.config.context,
            known.chars().count(),
        ) else {
            if Self::input_stamp() == stamp {
                self.feedback
                    .event(feedback::Event::Blocked, true, &self.config.feedback);
            }
            return;
        };
        if Self::input_stamp() != stamp || !live.ends_with(&known) {
            return;
        }
        let Some(_policy_guard) = SecurityGate::authorize_replacement(
            TriggerKind::Undo,
            &self.config,
            &self.database,
            &target,
        ) else {
            tracing::warn!(
                success = false,
                method = "none",
                reason = "replacement policy unavailable or denied",
                "app correction undo skipped"
            );
            return;
        };
        self.pipeline.cancel();
        if let Some(session) = self.session_manager.active_mut() {
            session.restore_pending();
        }
        let result = ReplacementEngine::undo(
            &target,
            &undo,
            stamp,
            self.config.replacement.clipboard_enabled,
        );
        result.log_outcome(true);
        drop(_policy_guard);
        if result.success {
            self.complete_undo(undo, &target, stamp, Self::input_stamp());
        } else if result.may_have_changed {
            self.session_manager
                .deactivate(MovementSignal::UnknownPosition);
        }
        if !result.success && Self::input_stamp() == stamp {
            self.feedback
                .event(feedback::Event::Error, true, &self.config.feedback);
        }
    }

    /// Commit a successful native undo before learning; lost ownership drops the session.
    fn complete_undo(
        &mut self,
        undo: session::CorrectionUndoTarget,
        target: &target::FocusedTarget,
        stamp: InputStamp,
        current_stamp: InputStamp,
    ) {
        if current_stamp != stamp
            || !self
                .session_manager
                .active_mut()
                .is_some_and(|session| session.undo_last_correction(&self.config.context))
        {
            self.session_manager
                .deactivate(MovementSignal::UnknownPosition);
            tracing::warn!("undo bookkeeping lost session ownership");
            return;
        }
        if self.config.learning.mode != crate::settings::LearningMode::Off {
            if let Some(rejection) = crate::dictionary::Rejection::from_undo(
                &undo.original,
                &undo.corrected,
                undo.language,
                target.process_name.clone(),
            ) {
                self.learner
                    .rejected(rejection, &self.config.learning, self.database.path());
            }
        }
    }

    /// Update the typed session and retain trigger requests with their full scope.
    fn track_input(
        &mut self,
        input: typing::TypedInput,
        pending: &mut Vec<PendingTrigger>,
    ) -> bool {
        let before = self
            .session_manager
            .active()
            .map(|session| session.editable_context());
        let inserted = match &input {
            typing::TypedInput::Text(text) => Some(text.clone()),
            _ => None,
        };
        let needs_capture = self.session_manager.input(input);
        if let (Some(before), Some(inserted), Some(session)) =
            (before, inserted, self.session_manager.active_mut())
        {
            if !session.position_uncertain() {
                let editable_snapshot = session.editable_context();
                if let Some(mut request) = triggers::automatic(
                    session.id(),
                    &before,
                    &editable_snapshot,
                    &inserted,
                    &session.correction_informative_context(),
                    session.versions(),
                    &self.config,
                ) {
                    // Freeze the entire current context, including earlier
                    // skipped boundaries. New keys belong to a fresh context.
                    let (segment, cancelled) = session.freeze_pending(&self.config.context);
                    for id in cancelled {
                        self.pipeline.cancel_segment(id);
                    }
                    if let Some(id) = segment {
                        // Overflow can restore older unchecked text. Snapshot
                        // the admitted range after reservation, not before it.
                        (request.informative_context, request.executable_context) =
                            session.pending_context(id).unwrap();
                        request.pending_segment_id = Some(id);
                        pending.push(PendingTrigger {
                            editable_snapshot: request.executable_context.clone(),
                            request,
                        });
                    }
                }
            }
        }
        needs_capture
    }

    /// Recheck the focused target and trigger permission before routing.
    fn dispatch_trigger(&mut self, request: CorrectionRequest, stamp: InputStamp) {
        self.dispatch_trigger_with(request, stamp, Self::input_stamp, SecurityGate::check);
    }

    /// Dispatch through live input and security checks, releasing only the failed suffix.
    fn dispatch_trigger_with(
        &mut self,
        request: CorrectionRequest,
        stamp: InputStamp,
        current_stamp: impl Fn() -> InputStamp,
        check_target: impl FnOnce(TriggerKind, &AppConfig, &Database) -> SecurityDecision,
    ) -> bool {
        let segment_id = request.pending_segment_id;
        let session_id = request.session_id;
        let dispatched = self.try_dispatch_trigger(request, stamp, current_stamp, check_target);
        if !dispatched {
            if let (Some(id), Some(session)) = (segment_id, self.session_manager.active_mut()) {
                if session.id() == session_id {
                    for cancelled in session.restore_pending_from(id) {
                        self.pipeline.cancel_segment(cancelled);
                    }
                }
            }
        }
        dispatched
    }

    /// Check input both before and after slow policy, UIA, and dictionary reads.
    fn try_dispatch_trigger(
        &mut self,
        mut request: CorrectionRequest,
        stamp: InputStamp,
        current_stamp: impl Fn() -> InputStamp,
        check_target: impl FnOnce(TriggerKind, &AppConfig, &Database) -> SecurityDecision,
    ) -> bool {
        self.pipeline
            .invalidate(&mut self.session_manager, current_stamp());
        let frozen = request.pending_segment_id.is_some();
        if !self.config.correction.enabled || !stamp.permits(current_stamp(), frozen) {
            return false;
        }
        if let SecurityDecision::Allowed { target } =
            check_target(request.trigger, &self.config, &self.database)
        {
            if self.session_manager.active_matches(&target) {
                let saved_override = match self
                    .database
                    .language_overrides()
                    .find(&target.process_name)
                {
                    Ok(value) => value,
                    Err(error) => {
                        tracing::warn!(%error, "failed to read app language override");
                        None
                    }
                };
                let configured_override = crate::correction::language::app_override(
                    &self.config.correction.app_language_overrides,
                    &target.process_name,
                );
                let session_detected = self
                    .session_manager
                    .active()
                    .and_then(|session| session.detected_language());
                let selection = crate::correction::language::resolve(
                    &request.informative_context,
                    &request.executable_context,
                    session_detected,
                    configured_override.or(saved_override.as_deref()),
                    self.config.correction.preferred_language.as_deref(),
                    self.config.correction.uncertain_language_policy,
                );
                if let Some(session) = self.session_manager.active_mut() {
                    session.set_detected_language(selection.session_detected);
                }
                request.language_info = selection.info;
                request.uncertain_language_policy = selection.policy;
                request.mixed_language_policy = self.config.correction.mixed_language_policy;
                request.confidence_behavior = self.config.confidence_behavior();
                let dictionary = match self
                    .database
                    .dictionary()
                    .policy(&target.process_name, &request.language_info)
                {
                    Ok(policy) => policy.terms,
                    Err(error) => {
                        tracing::warn!(%error, "correction skipped: dictionary unavailable");
                        return false;
                    }
                };
                if !stamp.permits(current_stamp(), frozen) {
                    return false;
                }
                if let Some(session) = self.session_manager.active() {
                    if request.session_id != session.id()
                        || request.pending_segment_id.map_or_else(
                            || request.versions != session.versions(),
                            |id| !session.pending_matches(id, &request.executable_context),
                        )
                    {
                        return false;
                    }
                    let manual = request.trigger == TriggerKind::ManualShortcut;
                    tracing::debug!(session_id = request.session_id, ?request.versions,
                        ?request.engine, ?request.mode, trigger = request.trigger.as_str(),
                        "correction queued");
                    if !self.pipeline.submit(
                        request,
                        session,
                        target,
                        stamp,
                        &self.config,
                        dictionary,
                    ) {
                        return false;
                    }
                    if manual {
                        self.pipeline.wait_manual();
                        self.finish_correction();
                    }
                    return true;
                }
            }
        }
        false
    }

    fn security_allows(&self, trigger: TriggerKind, database: &Database) -> bool {
        match SecurityGate::check(trigger, &self.current_config(), database) {
            SecurityDecision::Allowed { target } => {
                let session_key = target.session_key();
                tracing::debug!(
                    process_name = %target.process_name,
                    process_id = target.process_id,
                    trigger = trigger.as_str(),
                    session_key_kind = session_key_kind(&session_key),
                    "security gate allowed target"
                );
                true
            }
            SecurityDecision::Blocked { reason, target } => {
                if let Some(target) = target {
                    let session_key = target.session_key();
                    tracing::info!(
                        process_name = %target.process_name,
                        process_id = target.process_id,
                        trigger = trigger.as_str(),
                        session_key_kind = session_key_kind(&session_key),
                        block_reason = reason.as_str(),
                        "security gate blocked target"
                    );
                } else {
                    tracing::info!(
                        trigger = trigger.as_str(),
                        block_reason = reason.as_str(),
                        "security gate blocked target"
                    );
                }
                false
            }
        }
    }

    fn current_config(&self) -> AppConfig {
        self.config.clone()
    }

    #[allow(dead_code)]
    fn word_count_trigger_allowed(&self, database: &Database) -> bool {
        self.security_allows(TriggerKind::WordCount, database)
    }

    #[allow(dead_code)]
    fn character_trigger_allowed(&self, database: &Database) -> bool {
        self.security_allows(TriggerKind::Character, database)
    }

    #[allow(dead_code)]
    fn final_fix_before_reanchor_allowed(&self, database: &Database) -> bool {
        self.security_allows(TriggerKind::FinalFixBeforeReanchor, database)
    }
}

fn initialize_logging() {
    let _ = tracing_subscriber::fmt().with_target(false).try_init();
}

fn ensure_parent_directory(path: &Path) -> Result<(), BackgroundError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| BackgroundError::CreateDirectory {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    Ok(())
}

fn load_or_create_config(path: &Path) -> Result<AppConfig, BackgroundError> {
    ensure_parent_directory(path)?;

    if !path.exists() {
        save_config(path, &AppConfig::default()).map_err(BackgroundError::Config)?;
    }

    crate::settings::load_config(path).map_err(BackgroundError::Config)
}

fn modified_at(path: &Path) -> Option<SystemTime> {
    path.metadata()
        .and_then(|metadata| metadata.modified())
        .ok()
}

fn session_key_kind(session_key: &target::SessionKey) -> &'static str {
    match session_key {
        target::SessionKey::FocusedElement(_) => "focused_element",
        target::SessionKey::WindowHandle(_) => "window_handle",
        target::SessionKey::ProcessTitle { .. } => "process_title",
        target::SessionKey::TemporaryActiveSession => "temporary_active_session",
    }
}
