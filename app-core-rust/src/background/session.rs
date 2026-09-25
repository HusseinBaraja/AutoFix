//! In-memory text sessions. No session state is written to the database.

use std::collections::{HashMap, VecDeque};

use super::{
    target::{FocusedTarget, SessionKey},
    typing::{MovementSignal, TypedInput, TypedSession},
};
use crate::settings::ContextConfig;

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
    informative_start: usize,
    caret_anchor: u64,
}

pub(crate) struct Session {
    // Informative text is known session text before the executable segment.
    // It is never an editable replacement target.
    informative_context: String,
    executable: TypedSession,
    pending_corrections: VecDeque<PendingCorrection>,
    correction_undo_history: Vec<CorrectionUndo>,
    versions: ContextVersions,
    pending_movement: Option<PendingMovement>,
    correction_floor: usize,
}

struct PendingMovement {
    old_executable: String,
    typed_after: String,
    tracked_arrows_only: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum MovementResolution {
    Continued,
    Reanchor { final_fix: Option<String> },
}

const MIN_MOVEMENT_ANCHOR_CHARS: usize = 8;

// Preserve the entire known executable prefix when a bounded capture has
// dropped the oldest informative text.
fn matching_anchor<'a>(
    candidate: &'a str,
    executable_chars: usize,
    informative_chars: usize,
    available_chars: usize,
) -> Option<&'a str> {
    let candidate_chars = candidate.chars().count();
    let anchor_chars = candidate_chars.min(available_chars);
    let required_informative = informative_chars.min(4);
    if anchor_chars < executable_chars + required_informative
        || (informative_chars > 0 && anchor_chars < MIN_MOVEMENT_ANCHOR_CHARS)
    {
        return None;
    }
    let start = candidate
        .char_indices()
        .nth(candidate_chars - anchor_chars)
        .map_or(candidate.len(), |(index, _)| index);
    Some(&candidate[start..])
}

fn forward_skipped_start(before_typing: &str, informative: &str, old: &str) -> Option<usize> {
    if old.is_empty() {
        return None;
    }
    let informative_chars = informative.chars().count();
    let candidate = format!("{informative}{old}");
    let mut match_end = None;
    for (start, _) in before_typing.match_indices(old) {
        let end = start + old.len();
        let prefix = &before_typing[..end];
        let Some(anchor) = matching_anchor(
            &candidate,
            old.chars().count(),
            informative_chars,
            prefix.chars().count(),
        ) else {
            continue;
        };
        if prefix.ends_with(anchor) {
            if match_end.replace(end).is_some() {
                return None;
            }
        }
    }
    match_end
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
            pending_movement: None,
            correction_floor: 0,
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

    pub(crate) fn position_uncertain(&self) -> bool {
        self.pending_movement.is_some()
    }

    fn needs_movement_resolution(&self) -> bool {
        self.pending_movement
            .as_ref()
            .is_some_and(|pending| !pending.typed_after.is_empty())
    }

    fn mark_movement(&mut self, tracked_arrows_only: bool) {
        let old_executable: String = self
            .executable_context()
            .chars()
            .skip(self.correction_floor)
            .collect();
        self.pending_movement = Some(PendingMovement {
            old_executable,
            typed_after: String::new(),
            tracked_arrows_only,
        });
        self.pending_corrections.clear();
        self.correction_undo_history.clear();
        self.versions.caret_anchor = self.versions.caret_anchor.wrapping_add(1);
    }

    fn input(&mut self, input: TypedInput, limits: &ContextConfig) {
        if matches!(
            &input,
            TypedInput::Left | TypedInput::Right | TypedInput::Uncertain(_)
        ) {
            let tracked_arrows_only = matches!(&input, TypedInput::Left | TypedInput::Right)
                && self.pending_movement.as_ref().is_none_or(|pending| {
                    pending.tracked_arrows_only && pending.typed_after.is_empty()
                });
            self.mark_movement(tracked_arrows_only);
            if matches!(&input, TypedInput::Left | TypedInput::Right) {
                self.executable.input(input);
                if self.executable.latest_movement() == Some(MovementSignal::UnknownPosition) {
                    // A plain arrow crossed the known text boundary.
                    let pending = self.pending_movement.as_mut().unwrap();
                    pending.old_executable.clear();
                    pending.tracked_arrows_only = false;
                }
            }
            return;
        }
        if let Some(pending) = self.pending_movement.as_mut() {
            if let TypedInput::Text(value) = &input {
                pending.typed_after.push_str(value);
                return;
            }
            // An edit before locating the caret cannot be attributed safely.
            self.executable.invalidate(MovementSignal::UnknownPosition);
            self.pending_movement = None;
            self.correction_floor = 0;
            self.informative_context.clear();
            return;
        }
        if matches!(input, TypedInput::Backspace)
            && self.correction_floor > 0
            && self.executable_context().chars().count() <= self.correction_floor
        {
            self.executable.invalidate(MovementSignal::UnknownPosition);
            self.correction_floor = 0;
            self.informative_context.clear();
            self.pending_corrections.clear();
            return;
        }
        if matches!(&input, TypedInput::Backspace) && self.executable_context().is_empty() {
            // The user may have deleted text we only keep as read-only context.
            self.informative_context.clear();
            self.correction_undo_history.clear();
        }
        let is_text_edit = matches!(&input, TypedInput::Text(value) if !value.is_empty())
            || matches!(&input, TypedInput::Backspace | TypedInput::Delete);
        self.executable.input(input);
        if is_text_edit {
            self.versions.context = self.versions.context.wrapping_add(1);
            self.versions.executable = self.versions.executable.wrapping_add(1);
            self.pending_corrections.clear();
        }
        if self.executable_context().split_whitespace().count()
            > usize::from(limits.executable_context_max_words)
        {
            self.commit_executable(limits);
        }
    }

    fn resolve_movement(
        &mut self,
        preceding: Option<&str>,
        limits: &ContextConfig,
    ) -> MovementResolution {
        let Some(pending) = self.pending_movement.take() else {
            return MovementResolution::Continued;
        };
        if preceding.is_none() && pending.tracked_arrows_only {
            self.executable.input(TypedInput::Text(pending.typed_after));
            self.versions.context = self.versions.context.wrapping_add(1);
            self.versions.executable = self.versions.executable.wrapping_add(1);
            return MovementResolution::Continued;
        }
        let before_typing = preceding.and_then(|text| text.strip_suffix(&pending.typed_after));
        let old = pending.old_executable.as_str();
        if let Some(before_typing) = before_typing {
            // Only a prefix of the known editable text can prove a backward move.
            if !old.is_empty() {
                let mut matching_caret = None;
                let mut ambiguous = false;
                for (offset, _) in old.char_indices().rev() {
                    let candidate = format!("{}{}", self.informative_context, &old[..offset]);
                    let anchored = if self.informative_context.is_empty() {
                        before_typing == candidate
                    } else {
                        matching_anchor(
                            &candidate,
                            old[..offset].chars().count(),
                            self.informative_context.chars().count(),
                            before_typing.chars().count(),
                        )
                        .is_some_and(|anchor| before_typing.ends_with(anchor))
                    };
                    if anchored {
                        let caret = self.correction_floor + old[..offset].chars().count();
                        if matching_caret.replace(caret).is_some() {
                            ambiguous = true;
                            break;
                        }
                    }
                }
                if ambiguous {
                    return self.reanchor_after_movement(
                        Some(before_typing),
                        pending.typed_after,
                        None,
                        limits,
                    );
                }
                if let Some(caret) = matching_caret {
                    if self.executable.set_caret(caret) {
                        self.executable.input(TypedInput::Text(pending.typed_after));
                        self.versions.context = self.versions.context.wrapping_add(1);
                        self.versions.executable = self.versions.executable.wrapping_add(1);
                        return MovementResolution::Continued;
                    }
                }
            }
            if let Some(end) = forward_skipped_start(before_typing, &self.informative_context, old)
            {
                let skipped = &before_typing[end..];
                if skipped.split_whitespace().count()
                    <= usize::from(limits.forward_movement_word_limit)
                {
                    self.append_informative(old, limits);
                    self.append_informative(skipped, limits);
                    let floor = self.executable_context().chars().count();
                    self.executable.input(TypedInput::Text(pending.typed_after));
                    self.correction_floor = floor;
                    self.versions.context = self.versions.context.wrapping_add(1);
                    self.versions.executable = self.versions.executable.wrapping_add(1);
                    return MovementResolution::Continued;
                }
                return self.reanchor_after_movement(
                    Some(before_typing),
                    pending.typed_after,
                    Some(old.to_owned()),
                    limits,
                );
            }
        }
        self.reanchor_after_movement(before_typing, pending.typed_after, None, limits)
    }

    fn reanchor_after_movement(
        &mut self,
        before_typing: Option<&str>,
        typed_after: String,
        final_fix: Option<String>,
        limits: &ContextConfig,
    ) -> MovementResolution {
        self.executable.clear_executable();
        self.correction_floor = 0;
        self.informative_context = before_typing
            .map(|text| super::context_capture::trim_before_caret(text, limits).to_owned())
            .unwrap_or_default();
        self.trim_informative(limits);
        self.executable.input(TypedInput::Text(typed_after));
        self.pending_corrections.clear();
        self.versions.context = self.versions.context.wrapping_add(1);
        self.versions.executable = self.versions.executable.wrapping_add(1);
        MovementResolution::Reanchor {
            final_fix: final_fix.filter(|text| !text.is_empty()),
        }
    }

    fn append_informative(&mut self, text: &str, limits: &ContextConfig) {
        if text.is_empty() {
            return;
        }
        self.informative_context.push_str(text);
        self.trim_informative(limits);
    }

    fn trim_informative(&mut self, limits: &ContextConfig) {
        let max = limits.informative_context_max_chars as usize;
        let excess = self.informative_context.chars().count().saturating_sub(max);
        if excess > 0 {
            let boundary = self
                .informative_context
                .char_indices()
                .nth(excess)
                .map_or(self.informative_context.len(), |(index, _)| index);
            self.informative_context.drain(..boundary);
            self.correction_undo_history.clear();
        }
    }

    /// Move only known text before the caret into read-only context.
    fn commit_executable(&mut self, limits: &ContextConfig) {
        let observed: String = self
            .executable_context()
            .chars()
            .skip(self.correction_floor)
            .collect();
        if observed.is_empty() {
            if self.correction_floor > 0 {
                self.executable.clear_executable();
                self.correction_floor = 0;
            }
            return;
        }
        self.append_informative(&observed, limits);
        self.executable.clear_executable();
        self.correction_floor = 0;
        self.pending_corrections.clear();
        self.versions.context = self.versions.context.wrapping_add(1);
        self.versions.executable = self.versions.executable.wrapping_add(1);
    }

    /// Called only after a trigger or final-fix completed with no changes.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "called by the upcoming correction router")
    )]
    pub(crate) fn complete_without_changes(&mut self, limits: &ContextConfig) {
        if !self.position_uncertain() {
            self.commit_executable(limits);
        }
    }

    fn deactivate(&mut self, reason: MovementSignal) {
        self.executable.focus(None);
        self.executable.invalidate(reason);
        self.pending_movement = None;
        self.correction_floor = 0;
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

    /// Reanchor the read-only prefix at the current caret. Executable typing
    /// already observed in this input batch remains a separate segment.
    fn set_informative_context(&mut self, context: String, limits: &ContextConfig) {
        self.informative_context = context;
        self.trim_informative(limits);
        self.pending_corrections.clear();
        self.correction_undo_history.clear();
        self.versions.context = self.versions.context.wrapping_add(1);
        self.versions.caret_anchor = self.versions.caret_anchor.wrapping_add(1);
    }

    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "called by the upcoming correction router")
    )]
    pub(crate) fn queue_correction(&mut self, original: String, replacement: String) -> bool {
        if original.is_empty()
            || !self.executable_context().ends_with(&original)
            || original.chars().count()
                > self
                    .executable_context()
                    .chars()
                    .count()
                    .saturating_sub(self.correction_floor)
            || self.position_uncertain()
        {
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
    pub(crate) fn apply_next_correction(&mut self, limits: &ContextConfig) -> bool {
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
        let corrected_empty = self.executable_context().is_empty();
        self.commit_executable(limits);
        if corrected_empty {
            self.pending_corrections.clear();
            self.versions.context = self.versions.context.wrapping_add(1);
            self.versions.executable = self.versions.executable.wrapping_add(1);
        }
        let replacement: String = correction
            .replacement
            .chars()
            .rev()
            .take(limits.informative_context_max_chars as usize)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        if let Some(informative_start) = self
            .informative_context
            .len()
            .checked_sub(replacement.len())
            .filter(|start| self.informative_context.get(*start..) == Some(replacement.as_str()))
        {
            self.correction_undo_history.push(CorrectionUndo {
                original: correction.original,
                replacement,
                informative_start,
                caret_anchor: self.versions.caret_anchor,
            });
        }
        true
    }

    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "called after target undo succeeds")
    )]
    pub(crate) fn undo_last_correction(&mut self, limits: &ContextConfig) -> bool {
        let Some(last) = self.correction_undo_history.last() else {
            return false;
        };
        if last.caret_anchor != self.versions.caret_anchor {
            return false;
        }
        let start = last.informative_start;
        let end = start + last.replacement.len();
        if self.informative_context.get(start..end) != Some(last.replacement.as_str()) {
            return false;
        }
        self.informative_context
            .replace_range(start..end, &last.original);
        self.correction_undo_history.pop();
        self.trim_informative(limits);
        self.pending_corrections.clear();
        self.versions.context = self.versions.context.wrapping_add(1);
        true
    }
}

pub(crate) struct SessionManager {
    sessions: HashMap<SessionIdentity, Session>,
    active: Option<SessionIdentity>,
    limits: ContextConfig,
}

impl SessionManager {
    pub(crate) fn new(limits: ContextConfig) -> Self {
        Self {
            sessions: HashMap::new(),
            active: None,
            limits,
        }
    }

    pub(crate) fn update_limits(&mut self, limits: ContextConfig) {
        self.limits = limits;
        let active_limits = self.limits.clone();
        if let Some(session) = self.active_mut() {
            session.trim_informative(&active_limits);
            if !session.position_uncertain()
                && session.executable_context().split_whitespace().count()
                    > usize::from(active_limits.executable_context_max_words)
            {
                session.commit_executable(&active_limits);
            }
        }
    }

    pub(crate) fn focus(&mut self, target: &FocusedTarget) -> bool {
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
            return false;
        }
        self.deactivate(MovementSignal::FocusChange);
        let session = self
            .sessions
            .entry(identity.clone())
            .or_insert_with(|| Session::new(target.window_handle, identity.key.clone()));
        session.reactivate(target.window_handle, identity.key.clone());
        self.active = Some(identity);
        true
    }

    pub(crate) fn deactivate(&mut self, reason: MovementSignal) {
        if let Some(identity) = self.active.take() {
            if let Some(mut session) = self.sessions.remove(&identity) {
                session.deactivate(reason);
            }
        }
    }

    pub(crate) fn input(&mut self, input: TypedInput) -> bool {
        if let Some(identity) = self.active.as_ref() {
            if let Some(session) = self.sessions.get_mut(identity) {
                let backspace_into_informative = matches!(input, TypedInput::Backspace)
                    && session.executable_context().is_empty();
                session.input(input, &self.limits);
                return backspace_into_informative;
            }
        }
        false
    }

    pub(crate) fn needs_movement_resolution(&self) -> bool {
        self.active()
            .is_some_and(Session::needs_movement_resolution)
    }

    pub(crate) fn resolve_movement(&mut self, preceding: Option<&str>) -> MovementResolution {
        let limits = self.limits.clone();
        self.active_mut()
            .map_or(MovementResolution::Continued, |session| {
                session.resolve_movement(preceding, &limits)
            })
    }

    pub(crate) fn set_informative_context(&mut self, context: String) {
        let limits = self.limits.clone();
        if let Some(session) = self.active_mut() {
            session.set_informative_context(context, &limits);
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
        let mut manager = SessionManager::new(ContextConfig::default());
        let first = target(1, 10, Some("editor"));
        let other_field = target(1, 10, Some("search"));
        let other_process = target(2, 10, Some("editor"));
        manager.focus(&first);
        manager.input(TypedInput::Text("one".into()));
        manager.focus(&other_field);
        manager.input(TypedInput::Text("two".into()));
        manager.focus(&other_process);
        manager.input(TypedInput::Text("three".into()));
        assert_eq!(manager.sessions.len(), 1);
        manager.focus(&first);
        assert_eq!(manager.active().unwrap().executable_context(), "");
        assert_eq!(manager.active().unwrap().informative_context(), "");
    }

    #[test]
    fn uncertain_focus_invalidates_editable_text_and_pending_work() {
        let mut manager = SessionManager::new(ContextConfig::default());
        manager.focus(&target(1, 10, None));
        manager.input(TypedInput::Text("word".into()));
        assert!(manager
            .active_mut()
            .unwrap()
            .queue_correction("word".into(), "Word".into()));
        manager.deactivate(MovementSignal::MouseClick);
        assert!(manager.sessions.is_empty());
        manager.focus(&target(1, 10, None));
        let active = manager.active().unwrap();
        assert!(active.executable_context().is_empty());
        assert!(active.pending_corrections.is_empty());
        assert_eq!(active.versions(), ContextVersions::default());
    }

    #[test]
    fn uncertain_navigation_waits_for_typing_then_reanchors_if_capture_fails() {
        let mut manager = SessionManager::new(ContextConfig::default());
        manager.focus(&target(1, 10, None));
        manager.input(TypedInput::Text("earlier".into()));
        manager.input(TypedInput::Uncertain(MovementSignal::VerticalArrow));
        let active = manager.active().unwrap();
        assert!(active.position_uncertain());
        assert_eq!(active.executable_context(), "earlier");
        manager.input(TypedInput::Text("new".into()));
        assert!(manager.needs_movement_resolution());
        assert_eq!(
            manager.resolve_movement(None),
            MovementResolution::Reanchor { final_fix: None }
        );
        assert_eq!(manager.active().unwrap().executable_context(), "new");
        assert_eq!(manager.active().unwrap().informative_context(), "");
        manager.deactivate(MovementSignal::FocusChange);
        manager.focus(&target(1, 10, None));
        assert_eq!(manager.active().unwrap().informative_context(), "");
    }

    #[test]
    fn tracked_arrows_keep_known_caret_when_capture_fails() {
        let mut manager = SessionManager::new(ContextConfig::default());
        manager.focus(&target(1, 10, None));
        manager.input(TypedInput::Text("typed".into()));
        manager.input(TypedInput::Left);
        manager.input(TypedInput::Left);
        manager.input(TypedInput::Right);
        manager.input(TypedInput::Text("X".into()));
        assert_eq!(
            manager.resolve_movement(None),
            MovementResolution::Continued
        );
        assert_eq!(manager.active().unwrap().executable_context(), "typeX");
        manager.input(TypedInput::Right);
        assert_eq!(manager.active().unwrap().executable_context(), "typeXd");
    }

    #[test]
    fn uncertain_signal_after_arrow_still_reanchors_without_capture() {
        let mut manager = SessionManager::new(ContextConfig::default());
        manager.focus(&target(1, 10, None));
        manager.input(TypedInput::Text("typed".into()));
        manager.input(TypedInput::Left);
        manager.input(TypedInput::Uncertain(MovementSignal::MouseClick));
        manager.input(TypedInput::Right);
        manager.input(TypedInput::Text("X".into()));
        assert_eq!(
            manager.resolve_movement(None),
            MovementResolution::Reanchor { final_fix: None }
        );
        assert_eq!(manager.active().unwrap().executable_context(), "X");
    }

    #[test]
    fn arrow_past_known_text_does_not_reuse_tracked_caret() {
        let mut manager = SessionManager::new(ContextConfig::default());
        manager.focus(&target(1, 10, None));
        manager.input(TypedInput::Text("typed".into()));
        manager.input(TypedInput::Right);
        manager.input(TypedInput::Text("X".into()));
        assert_eq!(
            manager.resolve_movement(None),
            MovementResolution::Reanchor { final_fix: None }
        );
        assert_eq!(manager.active().unwrap().executable_context(), "X");
    }

    #[test]
    fn backward_movement_resumes_inside_same_executable_context() {
        let mut manager = SessionManager::new(ContextConfig::default());
        manager.focus(&target(1, 10, None));
        manager.input(TypedInput::Text("alpha beta".into()));
        manager.input(TypedInput::Uncertain(MovementSignal::ControlArrow));
        assert!(manager.active().unwrap().position_uncertain());
        manager.input(TypedInput::Text("X".into()));
        assert_eq!(manager.active().unwrap().executable_context(), "alpha beta");
        assert_eq!(
            manager.resolve_movement(Some("alpha X")),
            MovementResolution::Continued
        );
        assert_eq!(manager.active().unwrap().executable_context(), "alpha X");
        assert_eq!(manager.active().unwrap().informative_context(), "");
    }

    #[test]
    fn weak_backward_suffix_does_not_prove_a_new_caret() {
        let mut manager = SessionManager::new(ContextConfig::default());
        manager.focus(&target(1, 10, None));
        manager.input(TypedInput::Text("typed".into()));
        manager.input(TypedInput::Uncertain(MovementSignal::MouseClick));
        manager.input(TypedInput::Text("X".into()));
        assert_eq!(
            manager.resolve_movement(Some("unrelatX")),
            MovementResolution::Reanchor { final_fix: None }
        );
        assert_eq!(manager.active().unwrap().executable_context(), "X");
    }

    #[test]
    fn ambiguous_backward_anchor_reanchors() {
        let mut manager = SessionManager::new(ContextConfig::default());
        manager.focus(&target(1, 10, None));
        manager.set_informative_context("aaaaaaaa".into());
        manager.input(TypedInput::Text("aa".into()));
        manager.input(TypedInput::Uncertain(MovementSignal::MouseClick));
        manager.input(TypedInput::Text("X".into()));
        assert_eq!(
            manager.resolve_movement(Some("aaaaaaaaaX")),
            MovementResolution::Reanchor { final_fix: None }
        );
        assert_eq!(manager.active().unwrap().executable_context(), "X");
    }

    #[test]
    fn capped_informative_context_still_proves_backward_move() {
        let mut manager = SessionManager::new(ContextConfig {
            informative_context_max_chars: 16,
            ..ContextConfig::default()
        });
        manager.focus(&target(1, 10, None));
        manager.set_informative_context("abcdefghijklmnop".into());
        manager.input(TypedInput::Text("typed".into()));
        manager.input(TypedInput::Uncertain(MovementSignal::MouseClick));
        manager.input(TypedInput::Text("X".into()));
        assert_eq!(
            manager.resolve_movement(Some("efghijklmnoptypX")),
            MovementResolution::Continued
        );
        assert_eq!(manager.active().unwrap().executable_context(), "typX");
    }

    #[test]
    fn capped_informative_context_still_proves_forward_move() {
        let mut manager = SessionManager::new(ContextConfig {
            informative_context_max_chars: 16,
            ..ContextConfig::default()
        });
        manager.focus(&target(1, 10, None));
        manager.set_informative_context("abcdefghijklmnop".into());
        manager.input(TypedInput::Text("typed".into()));
        manager.input(TypedInput::Uncertain(MovementSignal::MouseClick));
        manager.input(TypedInput::Text("X".into()));
        assert_eq!(
            manager.resolve_movement(Some("klmnoptyped oneX")),
            MovementResolution::Continued
        );
        assert_eq!(manager.active().unwrap().executable_context(), "typedX");
        assert_eq!(
            manager.active().unwrap().informative_context(),
            "jklmnoptyped one"
        );
    }

    #[test]
    fn capped_informative_context_still_requests_long_forward_fix() {
        let mut manager = SessionManager::new(ContextConfig {
            informative_context_max_chars: 64,
            ..ContextConfig::default()
        });
        manager.focus(&target(1, 10, None));
        manager.set_informative_context("a".repeat(64));
        manager.input(TypedInput::Text("typed".into()));
        manager.input(TypedInput::Uncertain(MovementSignal::MouseClick));
        manager.input(TypedInput::Text("X".into()));
        let full = format!("{}typed one two three four five six X", "a".repeat(64));
        let captured: String = full.chars().skip(full.chars().count() - 64).collect();
        assert_eq!(
            manager.resolve_movement(Some(&captured)),
            MovementResolution::Reanchor {
                final_fix: Some("typed".into())
            }
        );
    }

    #[test]
    fn backward_movement_to_start_keeps_known_suffix_after_caret() {
        let mut manager = SessionManager::new(ContextConfig::default());
        manager.focus(&target(1, 10, None));
        manager.input(TypedInput::Text("tail".into()));
        manager.input(TypedInput::Uncertain(MovementSignal::HomeEnd));
        manager.input(TypedInput::Text("X".into()));
        assert_eq!(
            manager.resolve_movement(Some("X")),
            MovementResolution::Continued
        );
        assert_eq!(manager.active().unwrap().executable_context(), "X");
        manager.input(TypedInput::Right);
        assert_eq!(
            manager.resolve_movement(Some("Xt")),
            MovementResolution::Continued
        );
        assert_eq!(manager.active().unwrap().executable_context(), "Xt");
    }

    #[test]
    fn short_forward_movement_preserves_context_and_protects_skipped_text() {
        let mut manager = SessionManager::new(ContextConfig::default());
        manager.focus(&target(1, 10, None));
        manager.input(TypedInput::Text("typed".into()));
        manager.input(TypedInput::Uncertain(MovementSignal::MouseClick));
        manager.input(TypedInput::Text("X".into()));
        assert_eq!(
            manager.resolve_movement(Some("typed one two three four five X")),
            MovementResolution::Continued
        );
        let session = manager.active_mut().unwrap();
        assert_eq!(
            session.informative_context(),
            "typed one two three four five "
        );
        assert_eq!(session.executable_context(), "typedX");
        assert!(!session.queue_correction("typedX".into(), "bad".into()));
        assert!(session.queue_correction("X".into(), "Y".into()));
        session.complete_without_changes(&ContextConfig::default());
        assert_eq!(
            session.informative_context(),
            "typed one two three four five X"
        );
        assert_eq!(session.executable_context(), "");
        session.input(TypedInput::Text("next".into()), &ContextConfig::default());
        assert!(session.queue_correction("next".into(), "Next".into()));
    }

    #[test]
    fn forward_limit_uses_configured_word_count() {
        let mut manager = SessionManager::new(ContextConfig {
            forward_movement_word_limit: 1,
            ..ContextConfig::default()
        });
        manager.focus(&target(1, 10, None));
        manager.input(TypedInput::Text("typed".into()));
        manager.input(TypedInput::Uncertain(MovementSignal::MouseClick));
        manager.input(TypedInput::Text("X".into()));
        assert_eq!(
            manager.resolve_movement(Some("typed one two X")),
            MovementResolution::Reanchor {
                final_fix: Some("typed".into())
            }
        );
    }

    #[test]
    fn long_forward_movement_requests_final_fix_and_reanchors() {
        let mut manager = SessionManager::new(ContextConfig::default());
        manager.focus(&target(1, 10, None));
        manager.input(TypedInput::Text("typed".into()));
        manager.input(TypedInput::Uncertain(MovementSignal::MouseClick));
        manager.input(TypedInput::Text("X".into()));
        assert_eq!(
            manager.resolve_movement(Some("typed one two three four five six X")),
            MovementResolution::Reanchor {
                final_fix: Some("typed".into())
            }
        );
        let session = manager.active().unwrap();
        assert_eq!(session.executable_context(), "X");
        assert_eq!(
            session.informative_context(),
            "typed one two three four five six "
        );
    }

    #[test]
    fn different_area_reanchors_without_unsafe_final_fix() {
        let mut manager = SessionManager::new(ContextConfig::default());
        manager.focus(&target(1, 10, None));
        manager.input(TypedInput::Text("typed".into()));
        manager.input(TypedInput::Uncertain(MovementSignal::VerticalArrow));
        manager.input(TypedInput::Text("X".into()));
        assert_eq!(
            manager.resolve_movement(Some("unrelated area X")),
            MovementResolution::Reanchor { final_fix: None }
        );
        assert_eq!(manager.active().unwrap().executable_context(), "X");
        assert_eq!(
            manager.active().unwrap().informative_context(),
            "unrelated area "
        );
    }

    #[test]
    fn correction_commits_context_and_undo_preserves_new_typing() {
        let limits = ContextConfig::default();
        let mut manager = SessionManager::new(ContextConfig::default());
        manager.focus(&target(1, 10, None));
        manager.input(TypedInput::Text("old ".into()));
        manager
            .active_mut()
            .unwrap()
            .complete_without_changes(&limits);
        manager.input(TypedInput::Text("teh".into()));
        let session = manager.active_mut().unwrap();
        assert_eq!(session.executable_context(), "teh");
        assert_eq!(session.informative_context(), "old ");
        assert!(session.queue_correction("teh".into(), "the".into()));
        assert!(session.apply_next_correction(&limits));
        assert_eq!(session.executable_context(), "");
        assert_eq!(session.informative_context(), "old the");
        session.input(TypedInput::Text(" next".into()), &limits);
        assert!(session.undo_last_correction(&limits));
        assert_eq!(session.informative_context(), "old teh");
        assert_eq!(session.executable_context(), " next");
    }

    #[test]
    fn trigger_and_final_fix_without_changes_commit_each_segment() {
        let limits = ContextConfig::default();
        let mut manager = SessionManager::new(limits.clone());
        manager.focus(&target(1, 10, None));
        manager.input(TypedInput::Text("first ".into()));
        let session = manager.active_mut().unwrap();
        session.complete_without_changes(&limits); // trigger
        assert_eq!(session.informative_context(), "first ");
        assert_eq!(session.executable_context(), "");
        session.input(TypedInput::Text("second".into()), &limits);
        session.complete_without_changes(&limits); // final fix
        assert_eq!(session.informative_context(), "first second");
        assert_eq!(session.executable_context(), "");
    }

    #[test]
    fn configurable_word_limit_commits_on_exceeding() {
        let limits = ContextConfig {
            executable_context_max_words: 2,
            ..ContextConfig::default()
        };
        let mut manager = SessionManager::new(limits.clone());
        manager.focus(&target(1, 10, None));
        manager.input(TypedInput::Text("one two".into()));
        assert_eq!(manager.active().unwrap().executable_context(), "one two");
        manager.input(TypedInput::Text(" three".into()));
        assert_eq!(
            manager.active().unwrap().informative_context(),
            "one two three"
        );
        assert_eq!(manager.active().unwrap().executable_context(), "");
        manager.input(TypedInput::Text(" four".into()));
        assert_eq!(manager.active().unwrap().executable_context(), " four");
    }

    #[test]
    fn updated_limit_commits_active_segment() {
        let mut manager = SessionManager::new(ContextConfig::default());
        manager.focus(&target(1, 10, None));
        manager.input(TypedInput::Text("one two".into()));
        manager.update_limits(ContextConfig {
            executable_context_max_words: 1,
            ..ContextConfig::default()
        });
        assert_eq!(manager.active().unwrap().informative_context(), "one two");
        assert_eq!(manager.active().unwrap().executable_context(), "");
    }

    #[test]
    fn deletion_correction_can_be_undone() {
        let limits = ContextConfig::default();
        let mut manager = SessionManager::new(limits.clone());
        manager.focus(&target(1, 10, None));
        manager.input(TypedInput::Text("extra".into()));
        let session = manager.active_mut().unwrap();
        assert!(session.queue_correction("extra".into(), "".into()));
        assert!(session.apply_next_correction(&limits));
        assert_eq!(session.informative_context(), "");
        assert_eq!(session.executable_context(), "");
        assert!(session.undo_last_correction(&limits));
        assert_eq!(session.informative_context(), "extra");
    }

    #[test]
    fn undo_restores_original_suffix_after_informative_limit() {
        let limits = ContextConfig {
            informative_context_max_chars: 4,
            ..ContextConfig::default()
        };
        let mut manager = SessionManager::new(limits.clone());
        manager.focus(&target(1, 10, None));
        manager.input(TypedInput::Text("abcdef".into()));
        let session = manager.active_mut().unwrap();
        assert!(session.queue_correction("abcdef".into(), "uvwxyz".into()));
        assert!(session.apply_next_correction(&limits));
        assert_eq!(session.informative_context(), "wxyz");
        assert!(session.undo_last_correction(&limits));
        assert_eq!(session.informative_context(), "cdef");
    }

    #[test]
    fn correction_never_commits_known_text_after_caret() {
        let limits = ContextConfig::default();
        let mut manager = SessionManager::new(limits.clone());
        manager.focus(&target(1, 10, None));
        manager.input(TypedInput::Text("teh tail".into()));
        for _ in 0..5 {
            manager.input(TypedInput::Left);
        }
        manager.input(TypedInput::Text(String::new()));
        assert_eq!(
            manager.resolve_movement(Some("teh")),
            MovementResolution::Continued
        );
        let session = manager.active_mut().unwrap();
        assert_eq!(session.executable_context(), "teh");
        assert!(session.queue_correction("teh".into(), "the".into()));
        assert!(session.apply_next_correction(&limits));
        assert_eq!(session.informative_context(), "the");
        assert_eq!(session.executable_context(), "");
    }

    #[test]
    fn reanchor_replaces_read_only_context_and_preserves_new_typing() {
        let limits = ContextConfig {
            informative_context_max_chars: 5,
            ..ContextConfig::default()
        };
        let mut manager = SessionManager::new(limits.clone());
        manager.focus(&target(1, 10, None));
        manager.input(TypedInput::Text("abc".into()));
        manager
            .active_mut()
            .unwrap()
            .complete_without_changes(&limits);
        manager.input(TypedInput::Text("éxyz".into()));
        let session = manager.active_mut().unwrap();
        session.set_informative_context("old cé".into(), &limits);
        assert_eq!(session.informative_context(), "ld cé");
        assert_eq!(session.executable_context(), "éxyz");
    }

    #[test]
    fn backspace_beyond_new_segment_invalidates_read_only_context() {
        let limits = ContextConfig::default();
        let mut manager = SessionManager::new(limits.clone());
        manager.focus(&target(1, 10, None));
        manager.input(TypedInput::Text("old".into()));
        manager
            .active_mut()
            .unwrap()
            .complete_without_changes(&limits);
        assert!(manager.input(TypedInput::Backspace));
        assert_eq!(manager.active().unwrap().informative_context(), "");
    }

    #[test]
    fn stale_correction_and_undo_are_rejected() {
        let limits = ContextConfig::default();
        let mut manager = SessionManager::new(ContextConfig::default());
        manager.focus(&target(1, 10, None));
        manager.input(TypedInput::Text("teh".into()));
        let session = manager.active_mut().unwrap();
        assert!(session.queue_correction("teh".into(), "the".into()));
        session.input(TypedInput::Text("!".into()), &limits);
        assert!(!session.apply_next_correction(&limits));
        assert!(session.queue_correction("teh!".into(), "the!".into()));
        assert!(session.apply_next_correction(&limits));
        session.input(TypedInput::Text("next".into()), &limits);
        session.input(TypedInput::Left, &limits);
        assert!(!session.undo_last_correction(&limits));
    }

    #[test]
    fn exited_process_sessions_are_removed() {
        let mut manager = SessionManager::new(ContextConfig::default());
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
        let mut manager = SessionManager::new(ContextConfig::default());
        manager.focus(&target(1, 10, Some("Editor")));
        manager.input(TypedInput::Text("first".into()));
        manager.focus(&target(1, 20, Some("Editor")));
        assert_eq!(manager.sessions.len(), 1);
        assert_eq!(manager.active().unwrap().executable_context(), "");
        assert_eq!(manager.active.as_ref().unwrap().element_window, Some(20));
    }

    #[test]
    fn temporary_fallback_lasts_only_for_one_activation() {
        let mut manager = SessionManager::new(ContextConfig::default());
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
        let limits = ContextConfig::default();
        let mut manager = SessionManager::new(ContextConfig::default());
        manager.focus(&target(1, 10, None));
        manager.input(TypedInput::Text("teh".into()));
        let session = manager.active_mut().unwrap();
        assert!(session.queue_correction("teh".into(), "the".into()));
        assert!(session.apply_next_correction(&limits));
        session.input(TypedInput::Text(" next".into()), &limits);
        assert!(session.queue_correction("next".into(), "Next".into()));
        assert!(session.apply_next_correction(&limits));
        assert_eq!(session.informative_context(), "the Next");
        assert!(session.undo_last_correction(&limits));
        assert_eq!(session.informative_context(), "the next");
        assert!(session.undo_last_correction(&limits));
        assert_eq!(session.informative_context(), "teh next");
        assert_eq!(session.executable_context(), "");
    }
}
