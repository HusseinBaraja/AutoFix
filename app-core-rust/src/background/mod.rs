mod admin;
pub(crate) mod app_identity;
mod components;
mod context_capture;
mod informative_context;
mod input_listener;
mod message_loop;
mod paths;
mod pipeline;
mod process_group;
mod security;
mod session;
mod shortcuts;
mod target;
#[cfg(test)]
mod tests;
mod timeout_notice;
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
    components::{NamedPipeIpcServer, ReplacementEngine},
    input_listener::{InputEvent, InputListener},
    paths::RuntimePaths,
    pipeline::{CorrectionPipeline, InputStamp},
    process_group::SiblingDisappearanceMonitor,
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
    replacement_engine: ReplacementEngine,
    process_group_monitor: SiblingDisappearanceMonitor,
    shutdown_requested: Arc<AtomicBool>,
}

struct InputWorker {
    queue: Arc<(Mutex<VecDeque<InputWork>>, Condvar)>,
    done: mpsc::Receiver<()>,
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
            Self::ElevatedProcess | Self::InputHook(_) => None,
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

    fn shutdown(self) {
        self.components.shutdown();
        tracing::info!("AutoFix background process exited cleanly");
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
            replacement_engine: ReplacementEngine::initialize(),
            process_group_monitor: SiblingDisappearanceMonitor::new(),
            shutdown_requested,
        })
    }

    fn shutdown(self) {
        self.input_worker.shutdown();
        self.replacement_engine.shutdown();
        drop(self.input_listener);
        self.global_shortcut.shutdown();
        self.ipc_server.shutdown();
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

            self.shutdown_requested.load(Ordering::Relaxed)
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
        let pipeline = CorrectionPipeline::new().map_err(BackgroundError::InputWorker)?;
        let queue = Arc::new((Mutex::new(VecDeque::new()), Condvar::new()));
        let worker_queue = Arc::clone(&queue);
        let (done_sender, done) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("autofix-input-processing".into())
            .spawn(move || {
                let mut processor = InputProcessor {
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
                let _ = done_sender.send(());
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

    fn shutdown(mut self) {
        let (lock, ready) = &*self.queue;
        {
            let mut pending = lock.lock().unwrap();
            pending.clear();
            pending.push_back(InputWork::Shutdown);
            ready.notify_one();
        }
        if self.done.recv_timeout(Duration::from_millis(500)).is_ok() {
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        } else {
            tracing::warn!("input processor still waiting on UI Automation during shutdown");
        }
    }
}

impl InputProcessor {
    fn input_stamp() -> InputStamp {
        InputStamp {
            position: input_listener::current_position_generation(),
            sequence: input_listener::current_input_sequence(),
        }
    }

    fn finish_correction(&mut self) {
        // Frozen ranges may survive processed typing, but never guess what keys
        // still queued in the hooks did to the target.
        if Self::input_stamp().sequence != self.processed_input_sequence {
            return;
        }
        let config = &self.config;
        let database = &self.database;
        self.pipeline.finish(
            &mut self.session_manager,
            &config.context,
            Self::input_stamp,
            |trigger| match SecurityGate::check(trigger, config, database) {
                SecurityDecision::Allowed { target } if config.correction.enabled => Some(target),
                _ => None,
            },
            |target, known_chars| {
                context_capture::read_before_caret(target, &config.context, known_chars)
            },
            ReplacementEngine::replace,
        );
        if self.pipeline.take_timeout_notice() {
            timeout_notice::show();
        }
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
                    gate_result = None;
                    self.session_manager
                        .input(typing::TypedInput::Uncertain(MovementSignal::FocusChange));
                }
                InputEvent::MouseClick => {
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
                            needs_capture |= self.session_manager.focus(&target);
                            gate_result = Some((window, true));
                            needs_capture |=
                                self.track_input(key.translate(), &mut pending_requests);
                        }
                        _ => {
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

    fn process_shortcut(&mut self, id: usize) {
        match GlobalShortcutListener::action_for_id(id) {
            Some(ShortcutAction::Correct) => {
                let stamp = Self::input_stamp();
                if stamp.sequence != self.processed_input_sequence {
                    return;
                }
                if let SecurityDecision::Allowed { target } =
                    SecurityGate::check(TriggerKind::ManualShortcut, &self.config, &self.database)
                {
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
                                }
                            }
                        }
                    }
                }
            }
            Some(ShortcutAction::Undo) => {
                if self.security_allows(TriggerKind::Undo, &self.database) {
                    tracing::info!("undo pipeline placeholder triggered by shortcut");
                } else {
                    tracing::info!("undo shortcut ignored because context is blocked");
                }
            }
            None => {}
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
                    request.executable_context = editable_snapshot.clone();
                    let (segment, cancelled) = session.freeze_pending(&self.config.context);
                    if let Some(id) = cancelled {
                        self.pipeline.cancel_segment(id);
                    }
                    if let Some(id) = segment {
                        request.pending_segment_id = Some(id);
                        pending.push(PendingTrigger {
                            editable_snapshot,
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
            .invalidate(&self.session_manager, current_stamp());
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
                    .custom_dictionary()
                    .entries_for_app(&target.process_name)
                {
                    Ok(entries) => entries,
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
                    self.pipeline
                        .submit(request, session, target, stamp, &self.config, dictionary);
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
