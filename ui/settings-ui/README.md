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
per-app entries (`process.exe=language-tag`), a policy for unknown text, and a
separate mixed-language policy. Mixed text defaults to dominant-language-only,
high-confidence typo edits. Correction can be disabled on mixed text; per-token
correction requires the API engine.

Confidence controls expose high and medium behavior (`silent`, `suggestion`,
or `do_nothing`). High defaults to silent apply. Medium defaults to manual
suggestions when suggestion UI is available; automatic triggers do nothing
unless silent apply is selected. V1 has no suggestion UI, so medium suggestions
currently do nothing. Low confidence is fixed to do nothing in a disabled
control, and importing other low-confidence behaviors is rejected.
