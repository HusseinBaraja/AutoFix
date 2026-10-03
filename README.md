# AutoFix

AutoFix is a native Windows typo-correction app for text fields. It is designed to run quietly from the system tray, correct only the text the user typed in the current session, and avoid changing text after the caret.

Implemented capabilities include:

- Configurable shortcut, word-count, and character triggers.
- Local rule and API-backed correction engines.
- Typos-only and typos-plus-grammar modes.
- Custom dictionaries, app rules, blocklists, and allowlists.
- App-level undo and secure-field blocking.

**Word-count correction** is optional and defaults to every 10 completed words.
Under **Triggers**, enable or disable it and configure the threshold. A word is
completed by whitespace; an unfinished word does not trigger correction. Only
the current executable context counts. At the threshold, AutoFix freezes that
segment and runs correction on its worker while new typing starts a fresh
executable context. App rules and secure-field blocks apply before execution
and again before replacement. Completion requires the same session and control,
an exact live pre-caret range, and no input conflict during validation or mutation.
New typing and text after the caret are preserved.

High-confidence results apply silently by default. Medium confidence follows
the saved policy; automatic suggestion behavior currently skips because suggestion
acceptance is planned. Low confidence never applies. A verified no-change or
confidence-suppressed word-count result commits the original as informative
context and clears its pending segment without an undo entry. Changed segments
record app-level undo. Failed, timed-out, stale or unsafe work returns to executable
context for a later trigger. Logs contain metadata only.

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

**Feedback** keeps AutoFix quiet by default. The tray always stays available,
with idle, active, correcting, blocked and error states. Successful automatic
corrections show no popup. Manual failures, blocked actions and manual API
timeouts use a small notice that never takes focus. Repeated notices coalesce
and disappear after 2.5 seconds, new typing or a focus change. Secure fields do
not receive correction notices.

You can enable correction-applied notices, skipped reasons, and read-only manual
medium-confidence suggestion previews. Suggestions never apply edits; acceptance
remains planned. Blocked-action and timeout notices can be disabled independently.
Optional near-caret placement uses the Windows native caret when available,
otherwise the notice appears in the current monitor's work-area corner. Turning
off tray state keeps the tray icon and its settings/Exit menu available. Existing
saved preferences are preserved; new feedback options default to off.

When safer methods are unavailable or clipboard correction is disabled, SendInput
can replace a verified pre-caret span in supported Unicode Edit controls. Unknown
controls, unsafe selections and security refusals skip correction and log only
failure metadata.

**App Rules** adds layered terminal/editor safety. New terminal and editor rules
disable manual, word-count and character triggers. Enable **Manual** explicitly;
editors also require **Editor prose**. **Safety** defaults to automatic detection,
or can classify a custom app as `terminal` or `code_editor`. Known apps cannot be
downgraded by choosing `auto`.

Manual requests in these apps require selected prose. Heuristics reject commands,
flags, paths, URLs, identifiers, code and uncertain fragments before either engine
runs. Editor automatic correction requires explicit trigger and prose opt-ins,
and a whole segment of recognizable line-comment prose. Terminal automatic
requests still skip because the shell context cannot be proven safe.
Existing permissions survive migration; the new editor prose permission starts
off. Selected correction requires both UI Automation and the native caret to
prove the selection ends at the caret. Manual-only rules do not enable
continuous typed-input capture.

The manual correction shortcut (default **Ctrl+Alt+Space**) checks security,
app rules, selection ownership and session validity before sending read-only
informative context and editable executable context to the selected engine.
Confidence policy controls whether the result applies, previews or skips.
Successful replacement changes only the proved executable span before the caret,
moves the corrected text into informative context, clears executable context,
records app-level undo and writes metadata without document text.

Selected text must belong to the current typed segment unless **Correct arbitrary
selection** is enabled under **Shortcuts**. Even with that option, backward
selections and selections without reliable caret proof skip correction. Text after
the caret stays untouched. Unchanged or suppressed selections retain their context
and selection; suggestion acceptance remains planned.

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
