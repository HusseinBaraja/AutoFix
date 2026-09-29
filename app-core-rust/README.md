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

The correction engine contract is defined in `src/correction`. It keeps
read-only informative context separate from editable executable text and carries
the correction mode, enabled grammar categories, language and mixed-language
policy, dictionary and protected terms, trigger, and confidence behavior. Its
result includes corrected executable text, change need and optional details,
confidence, no-change reason, latency, and completion/error/timeout status.
`LocalRuleEngine`, `LocalMlEngine`, `OpenAiCompatibleApiEngine`, and
`CustomApiEngine` implement the same interface. `LocalRuleEngine` provides fast,
deterministic English correction for a conservative list of clear misspellings.
In grammar mode it also supports enabled capitalization, punctuation, agreement,
and tense rules; clarity and word-order rules are not implemented. It preserves
custom-dictionary entries, explicit protected terms, and detectable names,
emails, URLs, paths, handles, hashtags, code identifiers, and product names.
`OpenAiCompatibleApiEngine` and `CustomApiEngine` send non-streaming
chat-completion requests through WinHTTP. Presets include OpenAI, Groq, and
DeepSeek; `custom` requires a base URL. HTTPS is required except for loopback
HTTP. The provider preset selects a generic Windows Credential Manager entry
named `AutoFix/provider-profile/<preset>`; callers can store it with
`secrets::set_secret`. API keys are never put in TOML or diagnostic logs.
Manual and automatic requests default to 3000 ms and 700 ms respectively;
retries share the request's time budget. `ApiCorrectionEngine::submit` runs the
request on a worker thread so the caller can keep processing typing. Fallback
to the local rule engine is off by default. API results contain only
replacement text for the executable span and are rejected if protected terms
disappear. `ApiCorrectionEngine::notice_for` marks manual failures for a small
notice and automatic failures for silent handling. The local ML engine remains
a placeholder. The background router and replacement path are still
placeholders, so this engine is not yet invoked by live typing.
Engine selection is explicit per request; neither local nor API routing depends
on task difficulty, and both correction modes are accepted by every engine.

The keyboard session tracker is implemented. It keeps up to 4,096 characters
typed during the current engine run in memory and exposes only the known text
before the caret as executable context. Backspace, Delete, and plain Left/Right
update that buffer. Mouse clicks, Up/Down, Home/End, Ctrl+Arrow,
PageUp/PageDown, and focus changes mark the caret position uncertain.
The message loop forwards input batches to a dedicated processing thread; slow
UI Automation providers cannot hold up polling or the separate input hooks.
The work queue is bounded. If processing falls behind, stale batches are
discarded and the typed session is invalidated before later input is handled.
Keys carry the focus and caret generation observed by the hooks, so delayed
keys are rejected after a focus change or mouse click, even within one window.
Live captures are accepted only while the keyboard event sequence remains
unchanged, so typing still queued for processing cannot enter informative context.
Re-anchoring waits until typing resumes. The listener checks the target security
gate before translating key codes to text. On initial focus or resumed typing
after caret movement, the context manager attempts a read-only UI Automation TextPattern
capture ending at a collapsed caret. It keeps text after the nearest configured
boundary (default `.`), up to the previous configured number of words (default
25), and stops at the start of the text provider's document range. The capture
reads the informative character cap plus the known typed segment so that new
typing cannot crowd the anchor out. Stored informative context remains capped.

The trigger manager now builds correction requests for the configured manual
shortcut, completed word-count thresholds, and configured characters. Each
request carries read-only informative context and known executable text before
the caret, except for the explicit selected-text option below. Character triggers
scope the request to the latest completed segment. The manual shortcut accepts
a selected span when UI Automation proves it belongs to the typed segment.
An outside or unreadable selection blocks
the shortcut by default. The `shortcuts.correct_arbitrary_selection` setting is
off by default; when enabled, a selected span outside the typed segment becomes
temporary executable context for that request, while text before and after it
stays informative. The session has a completion path to commit corrected selected
text into informative context and record undo after target replacement succeeds.
App rules and the hard security gate are checked before routing. A matching app
rule permits typed-input tracking only when it allows a word-count or character
trigger. Manual-shortcut permission alone does not enable continuous capture.
Manual-only rules can still use the opt-in arbitrary-selection shortcut path.
The correction router still only logs request metadata; it does not yet apply
corrections to target text or call the completion path. If the provider cannot
read before the caret, informative context is empty and typing continues.
Protected fields and
unavailable targets are never read. Paste and
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
shrunk in app memory after every append and commit. Shrinking stays within the
configured character budget, prefers configured sentence boundaries, and
preserves the configured minimum number of recent words when they fit. It never
edits or deletes target-application text. Backward movement within known typed
text keeps the executable context. Forward movement of at most the configured
word limit (five by default) keeps the context, adds skipped text to informative
context, and starts a fresh executable segment at the new caret. The old typed
suffix is discarded because it has not been verified at the new position.
Longer forward movement and
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
