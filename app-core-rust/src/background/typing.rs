use super::target::SessionKey;

const MAX_TYPED_CHARS: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MovementSignal {
    MouseClick,
    VerticalArrow,
    HomeEnd,
    ControlArrow,
    Page,
    FocusChange,
    UnknownPosition,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TypedInput {
    Text(String),
    Backspace,
    Delete,
    Left,
    Right,
    Uncertain(MovementSignal),
}

/// Only characters observed during this process lifetime live here. `caret` is
/// an offset in that text, never an offset in the target document.
pub(crate) struct TypedSession {
    target: Option<(isize, SessionKey)>,
    typed: Vec<char>,
    caret: usize,
    movement: Option<MovementSignal>,
}

impl TypedSession {
    pub(crate) fn new() -> Self {
        Self {
            target: None,
            typed: Vec::new(),
            caret: 0,
            movement: None,
        }
    }

    pub(crate) fn focus(&mut self, target: Option<(isize, SessionKey)>) {
        if self.target != target {
            self.target = target;
            self.invalidate(MovementSignal::FocusChange);
        }
    }

    pub(crate) fn input(&mut self, input: TypedInput) {
        if self.target.is_none() {
            return;
        }
        match input {
            TypedInput::Text(value) => {
                for character in value.chars() {
                    self.typed.insert(self.caret, character);
                    self.caret += 1;
                }
                if self.typed.len() > MAX_TYPED_CHARS {
                    let excess = self.typed.len() - MAX_TYPED_CHARS;
                    if self.caret < excess {
                        self.invalidate(MovementSignal::UnknownPosition);
                        return;
                    }
                    self.typed.drain(..excess);
                    self.caret -= excess;
                }
            }
            TypedInput::Backspace if self.caret > 0 => {
                self.caret -= 1;
                self.typed.remove(self.caret);
            }
            TypedInput::Delete if self.caret < self.typed.len() => {
                self.typed.remove(self.caret);
            }
            TypedInput::Left if self.caret > 0 => self.caret -= 1,
            TypedInput::Right if self.caret < self.typed.len() => self.caret += 1,
            TypedInput::Uncertain(reason) => self.invalidate(reason),
            _ => self.invalidate(MovementSignal::UnknownPosition),
        }
    }

    pub(crate) fn invalidate(&mut self, reason: MovementSignal) {
        self.typed.clear();
        self.caret = 0;
        self.movement = Some(reason);
    }

    /// Executable context stops at the caret. The known typed suffix is kept
    /// only to resolve later Left/Right/Delete events.
    pub(crate) fn executable_context(&self) -> String {
        self.typed[..self.caret].iter().collect()
    }

    pub(crate) fn latest_movement(&self) -> Option<MovementSignal> {
        self.movement
    }

    /// Replace only the known text immediately before the caret. The tracked
    /// suffix remains untouched and is never submitted to a correction engine.
    pub(crate) fn replace_executable_suffix(&mut self, original: &str, replacement: &str) -> bool {
        let original: Vec<char> = original.chars().collect();
        let replacement: Vec<char> = replacement.chars().collect();
        if original.len() > self.caret
            || self.typed[self.caret - original.len()..self.caret] != original
            || self.typed.len() - original.len() + replacement.len() > MAX_TYPED_CHARS
        {
            return false;
        }
        self.typed.splice(
            self.caret - original.len()..self.caret,
            replacement.iter().copied(),
        );
        self.caret = self.caret - original.len() + replacement.len();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> TypedSession {
        let mut session = TypedSession::new();
        session.focus(Some((1, SessionKey::WindowHandle(1))));
        session
    }

    #[test]
    fn tracks_only_typed_text_before_caret() {
        let mut session = session();
        session.input(TypedInput::Text("abcd".into()));
        session.input(TypedInput::Left);
        session.input(TypedInput::Left);
        assert_eq!(session.executable_context(), "ab");
        session.input(TypedInput::Delete);
        session.input(TypedInput::Text("X".into()));
        assert_eq!(session.executable_context(), "abX");
        session.input(TypedInput::Right);
        assert_eq!(session.executable_context(), "abXd");
        session.input(TypedInput::Backspace);
        assert_eq!(session.executable_context(), "abX");
    }

    #[test]
    fn uncertain_movement_and_focus_drop_executable_text() {
        let mut session = session();
        session.input(TypedInput::Text("hello".into()));
        session.input(TypedInput::Uncertain(MovementSignal::MouseClick));
        assert_eq!(session.latest_movement(), Some(MovementSignal::MouseClick));
        assert!(session.executable_context().is_empty());
        session.input(TypedInput::Text("new".into()));
        session.focus(Some((2, SessionKey::WindowHandle(2))));
        assert_eq!(session.latest_movement(), Some(MovementSignal::FocusChange));
        assert!(session.executable_context().is_empty());
    }

    #[test]
    fn field_change_in_same_window_starts_new_session() {
        let mut session = session();
        session.input(TypedInput::Text("old".into()));
        session.focus(Some((1, SessionKey::FocusedElement("second".into()))));
        assert!(session.executable_context().is_empty());
        assert_eq!(session.latest_movement(), Some(MovementSignal::FocusChange));
        session.focus(None);
        session.input(TypedInput::Text("blocked".into()));
        assert!(session.executable_context().is_empty());
    }

    #[test]
    fn crossing_known_boundary_invalidates() {
        let mut session = session();
        session.input(TypedInput::Text("x".into()));
        session.input(TypedInput::Right);
        assert_eq!(
            session.latest_movement(),
            Some(MovementSignal::UnknownPosition)
        );
        assert!(session.executable_context().is_empty());
    }

    #[test]
    fn overflow_keeps_only_known_prefix_before_caret() {
        let mut session = session();
        session.input(TypedInput::Text("x".repeat(MAX_TYPED_CHARS + 1)));
        assert_eq!(
            session.executable_context().chars().count(),
            MAX_TYPED_CHARS
        );
        session.input(TypedInput::Left);
        assert_eq!(
            session.executable_context().chars().count(),
            MAX_TYPED_CHARS - 1
        );
    }

    #[test]
    fn correction_changes_only_prefix_before_caret() {
        let mut session = session();
        session.input(TypedInput::Text("abxcd".into()));
        session.input(TypedInput::Left);
        session.input(TypedInput::Left);
        assert!(session.replace_executable_suffix("abx", "AB"));
        assert_eq!(session.executable_context(), "AB");
        session.input(TypedInput::Right);
        assert_eq!(session.executable_context(), "ABc");
        assert!(!session.replace_executable_suffix("ab", "bad"));
    }
}
