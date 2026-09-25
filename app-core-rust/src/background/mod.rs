mod admin;
pub(crate) mod app_identity;
mod components;
mod context_capture;
mod informative_context;
mod input_listener;
mod message_loop;
mod paths;
mod process_group;
mod security;
mod session;
mod shortcuts;
mod target;
#[cfg(test)]
mod tests;
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
    components::{CorrectionEngineRouter, NamedPipeIpcServer, ReplacementEngine},
    input_listener::{InputEvent, InputListener},
    paths::RuntimePaths,
    process_group::SiblingDisappearanceMonitor,
    security::{SecurityDecision, SecurityGate, TriggerKind},
    session::{MovementResolution, SessionManager},
    shortcuts::{GlobalShortcutListener, ShortcutAction},
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
    correction_engine_router: CorrectionEngineRouter,
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
    config: AppConfig,
    session_manager: SessionManager,
    database: Database,
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
            correction_engine_router: CorrectionEngineRouter::initialize(config),
            replacement_engine: ReplacementEngine::initialize(),
            process_group_monitor: SiblingDisappearanceMonitor::new(),
            shutdown_requested,
        })
    }

    fn shutdown(self) {
        self.input_worker.shutdown();
        self.replacement_engine.shutdown();
        self.correction_engine_router.shutdown();
        drop(self.input_listener);
        self.global_shortcut.shutdown();
        self.ipc_server.shutdown();
    }

    fn run_until_exit(&mut self) {
        message_loop::run_until_exit(|event| {
            match event {
                message_loop::MessageLoopEvent::Hotkey(id) => {
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
        let queue = Arc::new((Mutex::new(VecDeque::new()), Condvar::new()));
        let worker_queue = Arc::clone(&queue);
        let (done_sender, done) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("autofix-input-processing".into())
            .spawn(move || {
                let mut processor = InputProcessor {
                    session_manager: SessionManager::new(config.context.clone()),
                    config,
                    database,
                };
                loop {
                    let work = {
                        let (lock, ready) = &*worker_queue;
                        let mut pending = lock.lock().unwrap();
                        while pending.is_empty() {
                            pending = ready.wait(pending).unwrap();
                        }
                        pending.pop_front().unwrap()
                    };
                    match work {
                        InputWork::Events(events) => processor.process_input(events),
                        InputWork::Shortcut(id) => processor.process_shortcut(id),
                        InputWork::Tick => processor.session_manager.prune_exited(),
                        InputWork::Config(config) => {
                            processor
                                .session_manager
                                .update_limits(config.context.clone());
                            processor.config = *config;
                        }
                        InputWork::Reset => {
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
    fn process_input(&mut self, events: Vec<InputEvent>) {
        let mut gate_result: Option<(isize, bool)> = None;
        let mut needs_capture = false;
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
                                    needs_capture |= self.session_manager.input(key.translate());
                                } else {
                                    self.session_manager.deactivate(MovementSignal::FocusChange);
                                }
                            }
                            continue;
                        }
                    }
                    let decision =
                        SecurityGate::check(TriggerKind::Character, &self.config, &self.database);
                    if generation != input_listener::current_position_generation() {
                        gate_result = None;
                        self.session_manager.deactivate(MovementSignal::FocusChange);
                        continue;
                    }
                    match decision {
                        SecurityDecision::Allowed { target } if target.window_handle == window => {
                            needs_capture |= self.session_manager.focus(&target);
                            gate_result = Some((window, true));
                            needs_capture |= self.session_manager.input(key.translate());
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
                SecurityGate::check(TriggerKind::Character, &self.config, &self.database)
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
                                "smart final-fix eligible at reanchor; correction pipeline is a placeholder"
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
                SecurityGate::check(TriggerKind::Character, &self.config, &self.database)
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
                    self.session_manager.set_informative_context(context);
                }
            }
        }
        if last_key_generation
            .is_some_and(|generation| generation != input_listener::current_position_generation())
        {
            self.session_manager.deactivate(MovementSignal::FocusChange);
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
                if self.security_allows(TriggerKind::ManualShortcut, &self.database) {
                    tracing::info!("correction pipeline placeholder triggered by shortcut");
                } else {
                    tracing::info!("correction shortcut ignored because context is blocked");
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
