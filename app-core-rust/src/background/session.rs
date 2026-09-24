//! In-memory text sessions. No session state is written to the database.

use std::collections::{HashMap, VecDeque};

use super::{
    target::{FocusedTarget, SessionKey},
    typing::{MovementSignal, TypedInput, TypedSession},
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SessionIdentity {
    // Keep the owner even for focused-element and window keys. IDs and HWNDs
    // can be reused by another process.
    process_id: u32,
    key: SessionKey,
    // Automation IDs are only guaranteed unique within their containing UI.
    element_window: Option<isize>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ContextVersions {
    pub(crate) context: u64,
    pub(crate) executable: u64,
    pub(crate) caret_anchor: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PendingCorrection {
    pub(crate) original: String,
    pub(crate) replacement: String,
    pub(crate) versions: ContextVersions,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CorrectionUndo {
    original: String,
    replacement: String,
    applied_versions: ContextVersions,
}

pub(crate) struct Session {
    // Informative text is captured from known typed text. Correction and undo
    // never write it or use it as a replacement target.
    informative_context: String,
    executable: TypedSession,
    pending_corrections: VecDeque<PendingCorrection>,
    correction_undo_history: Vec<CorrectionUndo>,
    versions: ContextVersions,
}

impl Session {
    fn new(window: isize, key: SessionKey) -> Self {
        let mut executable = TypedSession::new();
        executable.focus(Some((window, key)));
        Self {
            informative_context: String::new(),
            executable,
            pending_corrections: VecDeque::new(),
            correction_undo_history: Vec::new(),
            versions: ContextVersions::default(),
        }
    }

    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "read by the upcoming correction router")
    )]
    pub(crate) fn informative_context(&self) -> &str {
        &self.informative_context
    }

    pub(crate) fn executable_context(&self) -> String {
        self.executable.executable_context()
    }

    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "read by the upcoming correction router")
    )]
    pub(crate) fn versions(&self) -> ContextVersions {
        self.versions
    }

    pub(crate) fn latest_movement(&self) -> Option<MovementSignal> {
        self.executable.latest_movement()
    }

    fn input(&mut self, input: TypedInput) {
        if matches!(&input, TypedInput::Uncertain(_)) {
            // This text was observed in the current control before the caret
            // became uncertain. Keep it for context, never for replacement.
            let observed = self.executable.executable_context();
            if !observed.is_empty() {
                self.informative_context = observed;
            }
        }
        let is_text_edit = matches!(&input, TypedInput::Text(value) if !value.is_empty())
            || matches!(&input, TypedInput::Backspace | TypedInput::Delete);
        let is_anchor_change = matches!(
            &input,
            TypedInput::Left | TypedInput::Right | TypedInput::Uncertain(_)
        );
        self.executable.input(input);
        if is_text_edit || is_anchor_change {
            self.versions.context = self.versions.context.wrapping_add(1);
            self.versions.executable = self.versions.executable.wrapping_add(1);
            self.pending_corrections.clear();
            self.correction_undo_history.clear();
        }
        if is_anchor_change {
            self.versions.caret_anchor = self.versions.caret_anchor.wrapping_add(1);
        }
    }

    fn deactivate(&mut self, reason: MovementSignal) {
        self.executable.focus(None);
        self.executable.invalidate(reason);
        self.pending_corrections.clear();
        // A window key may stand for multiple fields. Context and undo from
        // the prior field must not survive a focus or mouse transition.
        self.informative_context.clear();
        self.correction_undo_history.clear();
        self.versions.context = self.versions.context.wrapping_add(1);
        self.versions.executable = self.versions.executable.wrapping_add(1);
        self.versions.caret_anchor = self.versions.caret_anchor.wrapping_add(1);
    }

    fn reactivate(&mut self, window: isize, key: SessionKey) {
        self.executable.focus(Some((window, key)));
    }

    /// Explicitly reanchor after verified same-field movement. Only observed
    /// current-session text can become informative context.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "called after a verified reanchor in the correction router"
        )
    )]
    pub(crate) fn capture_informative_context(&mut self) {
        self.informative_context = self.executable.executable_context();
        self.executable.invalidate(MovementSignal::UnknownPosition);
        self.pending_corrections.clear();
        self.versions.context = self.versions.context.wrapping_add(1);
        self.versions.executable = self.versions.executable.wrapping_add(1);
        self.versions.caret_anchor = self.versions.caret_anchor.wrapping_add(1);
    }

    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "called by the upcoming correction router")
    )]
    pub(crate) fn queue_correction(&mut self, original: String, replacement: String) -> bool {
        if original.is_empty() || !self.executable_context().ends_with(&original) {
            return false;
        }
        self.pending_corrections.push_back(PendingCorrection {
            original,
            replacement,
            versions: self.versions,
        });
        true
    }

    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "called after target replacement succeeds")
    )]
    pub(crate) fn apply_next_correction(&mut self) -> bool {
        let Some(correction) = self.pending_corrections.pop_front() else {
            return false;
        };
        if correction.versions != self.versions
            || !self
                .executable
                .replace_executable_suffix(&correction.original, &correction.replacement)
        {
            return false;
        }
        self.pending_corrections.clear();
        self.versions.context = self.versions.context.wrapping_add(1);
        self.versions.executable = self.versions.executable.wrapping_add(1);
        self.correction_undo_history.push(CorrectionUndo {
            original: correction.original,
            replacement: correction.replacement,
            applied_versions: self.versions,
        });
        true
    }

    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "called after target undo succeeds")
    )]
    pub(crate) fn undo_last_correction(&mut self) -> bool {
        let Some(last) = self.correction_undo_history.last() else {
            return false;
        };
        if last.applied_versions != self.versions {
            return false;
        }
        let original = last.original.clone();
        let replacement = last.replacement.clone();
        if !self
            .executable
            .replace_executable_suffix(&replacement, &original)
        {
            return false;
        }
        self.correction_undo_history.pop();
        self.pending_corrections.clear();
        self.versions.context = self.versions.context.wrapping_add(1);
        self.versions.executable = self.versions.executable.wrapping_add(1);
        if let Some(previous) = self.correction_undo_history.last_mut() {
            previous.applied_versions = self.versions;
        }
        true
    }
}

#[derive(Default)]
pub(crate) struct SessionManager {
    sessions: HashMap<SessionIdentity, Session>,
    active: Option<SessionIdentity>,
}

impl SessionManager {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn focus(&mut self, target: &FocusedTarget) {
        let identity = SessionIdentity {
            process_id: target.process_id,
            key: target.session_key(),
            element_window: target
                .focused_element_id
                .as_ref()
                .map(|_| target.window_handle)
                .filter(|window| *window != 0),
        };
        if self.active.as_ref() == Some(&identity) {
            return;
        }
        self.deactivate(MovementSignal::FocusChange);
        let session = self
            .sessions
            .entry(identity.clone())
            .or_insert_with(|| Session::new(target.window_handle, identity.key.clone()));
        session.reactivate(target.window_handle, identity.key.clone());
        self.active = Some(identity);
    }

    pub(crate) fn deactivate(&mut self, reason: MovementSignal) {
        if let Some(identity) = self.active.take() {
            if matches!(&identity.key, SessionKey::TemporaryActiveSession) {
                self.sessions.remove(&identity);
                return;
            }
            if let Some(session) = self.sessions.get_mut(&identity) {
                session.deactivate(reason);
            }
        }
    }

    pub(crate) fn input(&mut self, input: TypedInput) {
        if let Some(session) = self.active_mut() {
            session.input(input);
        }
    }

    pub(crate) fn active(&self) -> Option<&Session> {
        self.active
            .as_ref()
            .and_then(|identity| self.sessions.get(identity))
    }

    pub(crate) fn active_mut(&mut self) -> Option<&mut Session> {
        self.active
            .as_ref()
            .and_then(|identity| self.sessions.get_mut(identity))
    }

    /// Called on runtime ticks. A failed process probe is treated as dead so
    /// typed text cannot linger after its owning process becomes inaccessible.
    pub(crate) fn prune_exited(&mut self) {
        self.retain_processes(process_is_running);
    }

    fn retain_processes(&mut self, mut is_running: impl FnMut(u32) -> bool) {
        self.sessions
            .retain(|identity, _| is_running(identity.process_id));
        if self
            .active
            .as_ref()
            .is_some_and(|identity| !self.sessions.contains_key(identity))
        {
            self.active = None;
        }
    }
}

#[cfg(windows)]
fn process_is_running(process_id: u32) -> bool {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, WAIT_TIMEOUT},
        System::Threading::{OpenProcess, WaitForSingleObject},
    };

    const SYNCHRONIZE_ACCESS: u32 = 0x0010_0000;
    unsafe {
        let process = OpenProcess(SYNCHRONIZE_ACCESS, 0, process_id);
        if process.is_null() {
            return false;
        }
        let running = WaitForSingleObject(process, 0) == WAIT_TIMEOUT;
        CloseHandle(process);
        running
    }
}

#[cfg(not(windows))]
fn process_is_running(_process_id: u32) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::background::target::FocusedElementId;

    fn target(pid: u32, window: isize, element: Option<&str>) -> FocusedTarget {
        FocusedTarget {
            process_id: pid,
            process_name: "notepad.exe".into(),
            window_handle: window,
            window_title: "Notes".into(),
            focused_element_id: element.map(|value| FocusedElementId::RuntimeId(value.into())),
            is_elevated: false,
            is_password_or_protected: false,
            is_hidden_or_unavailable: false,
            field_safety_known: true,
            is_secure_desktop: false,
            is_lock_screen: false,
            is_credential_dialog: false,
        }
    }

    #[test]
    fn keys_isolate_elements_windows_and_processes() {
        let mut manager = SessionManager::new();
        let first = target(1, 10, Some("editor"));
        let other_field = target(1, 10, Some("search"));
        let other_process = target(2, 10, Some("editor"));
        manager.focus(&first);
        manager.input(TypedInput::Text("one".into()));
        manager.focus(&other_field);
        manager.input(TypedInput::Text("two".into()));
        manager.focus(&other_process);
        manager.input(TypedInput::Text("three".into()));
        assert_eq!(manager.sessions.len(), 3);
        manager.focus(&first);
        assert_eq!(manager.active().unwrap().executable_context(), "");
        assert_eq!(manager.active().unwrap().informative_context(), "");
    }

    #[test]
    fn uncertain_focus_invalidates_editable_text_and_pending_work() {
        let mut manager = SessionManager::new();
        manager.focus(&target(1, 10, None));
        manager.input(TypedInput::Text("word".into()));
        assert!(manager
            .active_mut()
            .unwrap()
            .queue_correction("word".into(), "Word".into()));
        let before = manager.active().unwrap().versions();
        manager.deactivate(MovementSignal::MouseClick);
        manager.focus(&target(1, 10, None));
        let active = manager.active().unwrap();
        assert!(active.executable_context().is_empty());
        assert!(active.pending_corrections.is_empty());
        assert!(active.versions().caret_anchor > before.caret_anchor);
    }

    #[test]
    fn uncertain_navigation_keeps_typed_text_as_read_only_context() {
        let mut manager = SessionManager::new();
        manager.focus(&target(1, 10, None));
        manager.input(TypedInput::Text("earlier".into()));
        manager.input(TypedInput::Uncertain(MovementSignal::VerticalArrow));
        let active = manager.active().unwrap();
        assert_eq!(active.informative_context(), "earlier");
        assert!(active.executable_context().is_empty());
        manager.input(TypedInput::Text("new".into()));
        assert_eq!(manager.active().unwrap().informative_context(), "earlier");
        manager.deactivate(MovementSignal::FocusChange);
        manager.focus(&target(1, 10, None));
        assert_eq!(manager.active().unwrap().informative_context(), "");
    }

    #[test]
    fn correction_and_undo_only_change_executable_context() {
        let mut manager = SessionManager::new();
        manager.focus(&target(1, 10, None));
        manager.input(TypedInput::Text("old".into()));
        manager.active_mut().unwrap().capture_informative_context();
        manager.focus(&target(1, 10, None));
        manager.input(TypedInput::Text("teh tail".into()));
        for _ in 0..5 {
            manager.input(TypedInput::Left);
        }
        let session = manager.active_mut().unwrap();
        assert_eq!(session.executable_context(), "teh");
        assert_eq!(session.informative_context(), "old");
        assert!(session.queue_correction("teh".into(), "the".into()));
        assert!(session.apply_next_correction());
        assert_eq!(session.executable_context(), "the");
        assert_eq!(session.informative_context(), "old");
        assert!(session.undo_last_correction());
        assert_eq!(session.executable_context(), "teh");
        session.input(TypedInput::Right);
        assert_eq!(session.executable_context(), "teh ");
    }

    #[test]
    fn stale_correction_and_undo_are_rejected() {
        let mut manager = SessionManager::new();
        manager.focus(&target(1, 10, None));
        manager.input(TypedInput::Text("teh".into()));
        let session = manager.active_mut().unwrap();
        assert!(session.queue_correction("teh".into(), "the".into()));
        session.input(TypedInput::Text("!".into()));
        assert!(!session.apply_next_correction());
        assert!(session.queue_correction("teh!".into(), "the!".into()));
        assert!(session.apply_next_correction());
        session.input(TypedInput::Left);
        assert!(!session.undo_last_correction());
    }

    #[test]
    fn exited_process_sessions_are_removed() {
        let mut manager = SessionManager::new();
        manager.focus(&target(1, 10, None));
        manager.input(TypedInput::Text("secret".into()));
        manager.focus(&target(2, 20, None));
        manager.retain_processes(|pid| pid == 2);
        assert_eq!(manager.sessions.len(), 1);
        manager.retain_processes(|_| false);
        assert!(manager.sessions.is_empty());
        assert!(manager.active().is_none());
    }

    #[cfg(windows)]
    #[test]
    fn process_probe_distinguishes_running_and_missing_processes() {
        assert!(process_is_running(std::process::id()));
        assert!(!process_is_running(u32::MAX));
    }

    #[test]
    fn focused_element_ids_are_scoped_to_window() {
        let mut manager = SessionManager::new();
        manager.focus(&target(1, 10, Some("Editor")));
        manager.focus(&target(1, 20, Some("Editor")));
        assert_eq!(manager.sessions.len(), 2);
    }

    #[test]
    fn temporary_fallback_lasts_only_for_one_activation() {
        let mut manager = SessionManager::new();
        let mut fallback = target(1, 0, None);
        fallback.window_title.clear();
        manager.focus(&fallback);
        manager.input(TypedInput::Text("typed".into()));
        assert_eq!(manager.sessions.len(), 1);
        manager.deactivate(MovementSignal::FocusChange);
        assert!(manager.sessions.is_empty());
        manager.focus(&fallback);
        assert!(manager.active().unwrap().executable_context().is_empty());
    }

    #[test]
    fn undo_history_can_walk_back_multiple_corrections() {
        let mut manager = SessionManager::new();
        manager.focus(&target(1, 10, None));
        manager.input(TypedInput::Text("teh".into()));
        let session = manager.active_mut().unwrap();
        assert!(session.queue_correction("teh".into(), "the".into()));
        assert!(session.apply_next_correction());
        assert!(session.queue_correction("the".into(), "The".into()));
        assert!(session.apply_next_correction());
        assert!(session.undo_last_correction());
        assert_eq!(session.executable_context(), "the");
        assert!(session.undo_last_correction());
        assert_eq!(session.executable_context(), "teh");
    }
}
