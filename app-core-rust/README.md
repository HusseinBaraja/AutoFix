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

Feature code should be organized by product behavior, not technical layer. Keep modules small, private by default, and colocate tests with the behavior they verify.

Do not implement optional helper mode here until it becomes a committed product requirement.
