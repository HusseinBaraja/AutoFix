# AutoFix

AutoFix is a native Windows typo-correction app for text fields. It is designed to run quietly from the system tray, correct only the text the user typed in the current session, and avoid changing text after the caret.

Implemented capabilities include:

- Configurable shortcut, word-count, and character triggers.
- Local rule and API-backed correction engines.
- Typos-only and typos-plus-grammar modes.
- Custom dictionaries, app rules, blocklists, and allowlists.
- App-level undo and secure-field blocking.

Local ML correction, suggestion acceptance, IME composition, and direct text API
or UI Automation mutation remain planned. App-level undo restores only recorded
corrections whose exact span and caret can still be verified.

Undo defaults to **Ctrl+Alt+Z**. Under **Shortcuts**, configure the hotkey and
**Undo history entries** (default 10, range 1–1000). Each session keeps its own
history in memory and deletes it with the session. Repeated undo restores the
latest recorded correction first, preserving newly typed text and text after
the caret. Restored originals become informative context; only new typing stays
executable. AutoFix replaces the verified corrected span directly instead of
sending the target application's Ctrl+Z.

**Dictionary** settings edit words and phrases that AutoFix must never correct,
plus specific pairs such as `teh` → `the` that must never be applied. Entries live
in `%LOCALAPPDATA%\AutoFix\autofix.sqlite`, with a language tag and optional app
process scope. `und` protects all languages; an empty app scope applies everywhere.

Learning defaults to **Off — undo only**. App-level undo only restores the
correction unless you choose **Ask after undo** or **Automatically learn**.
Ask shows “Don't correct this again?” after a successful undo; only Yes saves an
exclusion. Choose whether to protect the original word/phrase or only the rejected
pair, and whether learning applies just to that app. Existing exclusions stay
active when learning is off. Native Ctrl+Z and suggestion rejection are not tracked;
use AutoFix's undo shortcut to reject an applied correction.

Native clipboard replacement preserves all readable formats and restores them
before reporting success. Disable it under **Correction > Use clipboard for
correction** or set `replacement.clipboard_enabled = false` in settings TOML.
See [the engine documentation](app-core-rust/README.md) for runtime boundaries.

When safer methods are unavailable or clipboard correction is disabled, SendInput
can replace a verified pre-caret span in supported Unicode Edit controls. Unknown
controls, unsafe selections and security refusals skip correction and log only
failure metadata.

## Run App

From the repository root, build and run AutoFix:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\run-app.ps1
```

`Autofix.exe` owns the Windows tray icon and supervises the background engine.
The same executable hosts every normal runtime role so Windows can group the
processes together:

```powershell
.\ui\settings-ui\bin\Debug\net8.0-windows\Autofix.exe
.\ui\settings-ui\bin\Debug\net8.0-windows\Autofix.exe --engine
```

The Rust engine is loaded through `autofix_core.dll`. AutoFix keeps running until
you choose `Exit` from the tray menu.

The app creates its settings file at:

```text
%LOCALAPPDATA%\AutoFix\settings.toml
```

## Repository Structure

- `app-core-rust/` - Rust background engine and shared core logic.
- `ui/settings-ui/` - WPF settings application.
- `shared-schema/` - Shared config schemas, IPC contracts, and documentation.
- `installer/` - Installer scripts and packaging assets later.
- `docs/` - Architecture and design notes.

## Rust testing

Use `#[cfg(test)]` for test modules, unit tests placed next to the code, and helper functions or mock types that exist only for tests. Do not use it for integration tests in the `tests/` directory, normal production logic, public APIs, or alternate implementations that make code behave differently in tests than it does in real builds.

## When you are unsure

use context7 to get the official documentation and check official internet sources if you need them.
