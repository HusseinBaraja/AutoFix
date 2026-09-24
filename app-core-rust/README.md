# App Core Rust

Rust background engine and shared core logic for AutoFix.

This component owns the installed product's background mode:

- Tray icon lifecycle.
- Global shortcut listener.
- Keyboard/session tracker.
- Trigger manager.
- Context manager.
- Correction engine router.
- Replacement engine.
- App rules and security layer.

The keyboard session tracker is implemented. It keeps up to 4,096 characters
typed during the current engine run in memory and exposes only the known text
before the caret as executable context. Backspace, Delete, and plain Left/Right
update that buffer. Mouse clicks, Up/Down, Home/End, Ctrl+Arrow,
PageUp/PageDown, and focus changes
clear it and signal uncertain position. The listener checks the target security
gate before translating key codes to text. It does not read or rewrite document
content. Paste and other input that cannot be mapped safely invalidate the known
position rather than importing document text. IME composition is not yet supported.

The session manager keeps separate memory-only sessions for focused text
elements, falling back to the window handle, process ID and title, then a
temporary active key. Keys are scoped to the owning process. It stores
informative context separately from editable executable context, plus pending
corrections, correction undo history, and context, executable, and caret-anchor
versions. Informative context can only be captured from text observed in the
current engine run; correction and undo operations change only executable
context before the caret. Uncertain movement invalidates executable context and
pending corrections. Runtime ticks delete sessions when their owning process
exits. All session state disappears on engine exit or termination and is never
written to disk. The correction router and replacement engine are still
placeholders; they do not yet submit or apply corrections.

Feature code should be organized by product behavior, not technical layer. Keep modules small, private by default, and colocate tests with the behavior they verify.

Do not implement optional helper mode here until it becomes a committed product requirement.
