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
PageUp/PageDown, and focus changes mark the caret position uncertain.
The message loop forwards input batches to a dedicated processing thread; slow
UI Automation providers cannot hold up polling or the separate input hooks.
Re-anchoring waits until typing resumes. The listener checks the target security
gate before translating key codes to text. On initial focus or resumed typing
after caret movement, the context manager attempts a read-only UI Automation TextPattern
capture ending at a collapsed caret. It keeps text after the nearest configured
boundary (default `.`), up to the previous configured number of words (default
25), and stops at the start of the text provider's document range. The capture
reads the informative character cap plus the known typed segment so that new
typing cannot crowd the anchor out. Stored informative context remains capped.
It is never used as a replacement
target; only newly typed text enters executable context. If the provider cannot
read before the caret, informative context is empty and typing continues.
Selections, protected fields, and unavailable targets are never read. Paste and
other input that cannot be mapped safely invalidate the known position rather
than importing document text. IME composition is not yet supported.

The session manager keeps separate memory-only sessions for focused text
elements, falling back to the window handle, process ID and title, then a
temporary active key. Keys are scoped to the owning process. It stores
informative context separately from editable executable context, plus pending
corrections, correction undo history, and context, executable, and caret-anchor
versions. Captured field text may enter informative context; executable context
contains only text typed during the current engine run.
On a successful correction, corrected text becomes read-only informative
context and executable context clears. A trigger or final fix with no changes
does the same with the original text. Exceeding the configured executable word
limit also commits the current segment. New typing starts a fresh executable
segment. Undo restores the corrected span in informative context to its
original text and leaves newer executable text intact. Informative context is
bounded by the configured character limit. Backward movement within known typed
text keeps the executable context. Forward movement of at most the configured
word limit (five by default) keeps the context, adds skipped text to informative
context, and protects that text from replacement. Longer forward movement and
unmatched positions re-anchor at the new caret. A longer forward move requests
the final-fix security gate for the old executable text. The correction pipeline
is still a placeholder, so no final fix is applied to target text yet. Pending
corrections are invalidated on movement. Runtime ticks delete sessions when
their owning process exits. All session state disappears on engine exit or
termination and is never written to disk. The correction router and replacement
engine are still placeholders; lifecycle methods model their outcomes in
memory and do not change target application text.

Feature code should be organized by product behavior, not technical layer. Keep modules small, private by default, and colocate tests with the behavior they verify.

Do not implement optional helper mode here until it becomes a committed product requirement.
