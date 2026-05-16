# Settings UI WPF

WPF settings application for AutoFix.

This component owns settings mode:

- Flow Launcher-style settings window.
- Structured settings editing.
- Communication with the background process.
- User-facing configuration for triggers, correction behavior, dictionaries, app rules, privacy, and engine selection.

The UI should remain a thin product surface over explicit config and IPC contracts from `shared-schema`. Background correction behavior belongs in `app-core-rust`.
