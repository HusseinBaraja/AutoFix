# Settings UI

WPF settings application for AutoFix.

This component owns settings mode:

- Flow Launcher-style settings window.
- Structured settings editing.
- Communication with the background process.
- User-facing configuration for triggers, correction behavior, dictionaries, app rules, privacy, and engine selection.

The UI should remain a thin product surface over explicit config and IPC contracts from `shared-schema`. Background correction behavior belongs in `app-core-rust`.

Correction settings offer typos-only and typos-plus-grammar modes. Grammar
category switches are available in grammar mode; the saved enabled list is
empty in typos-only mode. An older `punctuation` setting loads as `spacing`.
Language controls allow an optional global BCP 47 preference, comma-separated
per-app entries (`process.exe=language-tag`), and a policy for unknown or mixed
text. The default policy permits only high-confidence typo edits.
