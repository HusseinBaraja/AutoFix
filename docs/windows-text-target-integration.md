# Windows text-target integration scenarios

This suite checks v1 behavior in [README](../README.md) and the
[engine documentation](../app-core-rust/README.md). It has three layers:

- Deterministic integration tests use real trigger construction, app-rule
  storage, dispatcher, local worker, session commits and undo records.
  Focus/capture/mutation are adapters, not real application providers.
- Opt-in native tests own a Windows Edit control and exercise real range proof,
  direct insertion, clipboard/SendInput, undo and caret movement. A separate WPF
  fixture tests the UIA adapter. Clipboard recovery
  tests use an isolated, noninteractive window station.
- Manual scenarios below verify actual installed targets. No real target has
  a recorded manual pass until a tester runs it.

See the [initial harness verification](windows-text-target-integration-results.md)
for checks performed and desktop limitations.

## Run automation

From the repository root:

```powershell
# Deterministic; never injects desktop input or changes the desktop clipboard.
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\test-windows-integration.ps1

# Add isolated native clipboard preservation/recovery tests.
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\test-windows-integration.ps1 -IsolatedClipboard

# Also test native replacement on an unlocked interactive Windows desktop.
# Tests briefly focus their own editor. Avoid typing during the run.
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\test-windows-integration.ps1 -NativeDesktop -WpfDesktop -IsolatedClipboard

# Only the new target contracts:
cargo test -p background-engine windows_target --lib
```

The runner fails on a failed suite, writes logs and `results.json` to a new
temporary directory and prints its path. Use `-ResultsDirectory <path>` to choose
the destination. Omitted native suites are `not_run`; ignored tests in the
default Rust run are not desktop passes. Run the desktop tier without another
AutoFix engine competing for shortcuts/capture. Native tests use internal
shortcut dispatch; global hotkey registration/delivery needs manual verification.

## Prepare the desktop

1. Use a normal Windows session. Record Windows build, AutoFix commit, app
   version, actual focused process/title, control type and keyboard layout.
   Start AutoFix with `scripts/run-app.ps1`.
2. Record settings/rules for restoration. Use local engine, English, typos-only,
   high-confidence silent correction, learning off and **Correct arbitrary
   selection** off. Disable the automatic trigger not under test. Account for
   dictionary exclusions protecting `teh` or `teh` → `the`.
3. Use disposable text. Never send chat messages, navigate test address text,
   execute test commands or save source-file edits. Close only fixtures/apps
   started for testing; leave existing processes open.
4. Start each case with a fresh field/session (switch focus away/back). Type
   executable text character by character. Pasted setup text is not executable
   typing. Wait up to three seconds for a local result; inspect metadata for
   refusals. Silence alone is not proof that a trigger or security gate worked.

Open `tests/windows-targets/browser-text-targets.html` locally in each browser.
It provides input, textarea, contenteditable, password and read-only controls
without network requests or submission. For an optional WPF fixture:

```powershell
dotnet build .\tests\windows-targets\wpf\AutoFix.TextTargetFixture.csproj
& .\tests\windows-targets\wpf\bin\Debug\net8.0-windows\AutoFix.TextTargetFixture.exe
```

Use the fixture executable, whose process identity is distinct. A WPF TextBox
hosted in `powershell.exe` remains a terminal target and is blocked by default.

## Target matrix

`Conditional` means policy permits an ordinary, unelevated, known-safe field;
replacement still needs reliable UI Automation caret/range proof and supported
mutation. Clipboard paste supports recognized Edit/RichEdit controls; SendInput
supports standard Unicode Edit. Direct selected-range insertion supports recognized
Unicode Edit/RichEdit controls. The UIA keyboard adapter requires a proved writable
range ending at the document end and no newer following text. Custom
browser/messaging controls may refuse all triggers safely.
Record those as `UNSUPPORTED`, not successful corrections.

| Target / field | Shortcut | Word count | Character | Cases / setup |
| --- | --- | --- | --- | --- |
| Notepad document | Conditional | Conditional | Conditional | T01–T11; record modern/classic control |
| Windows Search query | Conditional | Conditional | Conditional | T01–T09, T11; actual SearchHost/SearchApp/host; never open results |
| File Explorer search | Conditional | Conditional | Conditional | T01–T09, T11; empty disposable folder |
| Run command edit | Conditional | Conditional | Conditional | T01–T09, T11; Win+R; never press Enter |
| Browser input / textarea / contenteditable | Conditional | Conditional | Conditional | T01–T11 per fixture field/browser |
| WhatsApp composer | Conditional | Conditional | Conditional | T01–T09, T11; unsent disposable draft; clear afterward |
| Telegram composer | Conditional | Conditional | Conditional | T01–T09, T11; unsent disposable draft; clear afterward |
| CMD | Blocked by default | Blocked by default | Blocked by default | T05, T06, T12; never execute test text |
| Windows PowerShell / pwsh | Blocked by default | Blocked by default | Blocked by default | T05, T06, T12 |
| Windows Terminal, each shell tab | Blocked by default | Blocked by default | Blocked by default | T05, T06, T12; record actual host/tab title |
| Code editor (VS Code / Visual Studio / installed editor) | Blocked by default | Blocked by default | Blocked by default | T05, T06, T12; unsaved scratch buffer |
| WPF fixture single/multiline TextBox | Conditional | Conditional | Conditional | T01–T11 |
| Browser address bar | Conditional | Conditional | Conditional | T01–T09, T11; plain prose; never Enter; URLs stay unchanged |
| Password/protected field | Blocked | Blocked | Blocked | T10; explicit rules cannot override security |
| Elevated/admin target | Unsupported v1 | Unsupported v1 | Unsupported v1 | T11; AutoFix stays unelevated |

Policy uses process/title; Run and address bars have no separate safety category.
These tests use prose, not command syntax, paths or URLs. Single-line controls
use `N/A` only for multiline-specific cases. Missing apps are `NOT INSTALLED`.

## Manual scenarios

Run T01–T04 per permitted target/control. Repeat T05–T12 with each relevant
trigger, including blocked targets.

| ID | Steps | Exact expected result |
| --- | --- | --- |
| T01 Shortcut | Disable automatic triggers. Type `teh word`; press Ctrl+Alt+Space or configured correction shortcut. | Supported target becomes `the word` once. Unsupported range/mutation skips unchanged. Confirm hotkey delivery. |
| T02 Word count | Enable only word-count, threshold 2. Type `teh word`, pause, then one space. | No request before completing space; supported target becomes `the word `. Preexisting/informative words do not count. Restore threshold. |
| T03 Character | Enable only character trigger, `.`. Type `teh word`, pause, then `.`. Repeat with configured `?` or `。`. | No request before boundary; supported target becomes `the word.` with boundary preserved. |
| T04 Continuation | Immediately after T02/T03 boundary type ` newer` before the result. Use a delayed test provider if local completion is too fast. | Frozen original corrects; ` newer` remains exactly typed. One undo record per change. Mark concurrency untested if delay was not observed. |
| T05 Rules | Blocklist actual process, repeat T01–T03. Then allow it and disable one trigger at a time, keeping another automatic trigger enabled so typing remains tracked. Deny selected engine separately. Test allowlist mode without/with a matching rule. | Blocklist, denied trigger/engine and missing allowlist entry refuse. Other permitted triggers work only when safety/native support permit. Manual-only rules do not enable continuous capture. Restore settings/rules. |
| T06 Default terminals/editors | Restore default rules; try T01–T03 prose. Also type `git status`, `Get-ChildItem`, `C:\Temp\teh.txt`, `user_name`, `let teh = 1;` without executing/saving. | All default triggers blocked; text unchanged; no correction/undo record. Check metadata to distinguish blocking from a missed shortcut. |
| T07 Clipboard | Copy `AUTOFIX_CLIPBOARD_SENTINEL_42` in scratch app before a fresh correction. Correct, then paste into scratch. Repeat separately for undo, rich HTML/RTF and an image; compare formatted/plain paste. Check Win+V if enabled. Repeat with clipboard correction disabled. | Original content, formatting/image remain visible; temporary correction is absent from clipboard history. Disabled clipboard uses safe fallback or skips. Unreadable formats refuse safely. Complete undo before switching focus for clipboard inspection. |
| T08 App undo | Correct with each trigger; keep focus/caret stable, type ` newer`, press Ctrl+Alt+Z or configured undo. Repeat with two corrections and undo newest first. Press again with no history. | Only recorded correction restores `teh`; newer text survives. Each undo consumes one record. Empty history does nothing. Use AutoFix undo, not native Ctrl+Z. Changed focus/caret or lost proof may safely refuse; record separately. |
| T09 Caret/context | Preload `Earlier teh words:  AFTER teh suffix`. Put caret between spaces, switch away/back for fresh session, type T01/T02/T03 text. Record full field/caret before/after. Repeat with old `foreign teh` selection ending at caret, arbitrary selection off. Multiline fields also test prefix/suffix on separate lines. | Only new executable text changes. Preexisting prefix and complete post-caret suffix remain identical Unicode text. Old selection refuses by default. Correction/undo preserve suffix and newer typing. |
| T10 Password/read-only | Use synthetic text in browser/WPF password controls, invoke all triggers under explicit allow rule. Attempt read-only-field correction. | Password has no correction, capture, engine request, undo or correction notice. Read-only text unchanged; provider refusal reason may vary. |
| T11 Admin/uncertain provider | With normal AutoFix, open disposable elevated Notepad/editor using normal UAC and repeat all triggers under allow rule. Separately test unreliable caret/range provider. | Elevated target unsupported v1; no capture/edit/undo; rules cannot bypass. Uncertain providers refuse instead of guessing. If testing elevated AutoFix startup itself, it must reject startup. |
| T12 Optional prose opt-in | Enable terminal Manual; editor also needs Editor prose. Select recognizable current-session prose ending at caret. Test editor automatic opt-in with whole `// This is teh sentence.` or `# This is teh sentence.` segment. Repeat command/code/path/URL samples. | Only proved selection/prose with native support may correct. Unsafe syntax refuses before either engine. Terminal automatic requests still refuse despite permissions. Editor automatic requires trigger + prose opt-in + whole comment prose. Known app safety cannot downgrade via `auto`. Restore rules. |

Run T07/T08/T09 on every target where an edit actually applies. A refused edit
must not create undo; do not claim clipboard/undo exercised when every correction
refused. Test mid-flight policy revocation with a delayed provider: block app
before completion; replacement must refuse. API revocation cannot recall text
already transmitted. Baseline needs no API credentials.

## Automated coverage map

| Requirement | Automated coverage | Remaining desktop evidence |
| --- | --- | --- |
| Target identities, three triggers, app rules, hard blocks | `pipeline::tests::windows_targets` matrices and allowed/denied flows | Real provider and global hotkey delivery |
| Trigger timing, worker concurrency, stale results | `pipeline::tests::word_count`, `character`, `context_versions` | T02–T04 per app |
| Terminal/editor prose admission | Target matrices, `security::text_safety`, restricted-text pipeline tests | Actual identity/selection/provider |
| Password/admin safety | Target matrices, `background::admin::tests` | Real password/elevated controls |
| Clipboard preservation/recovery | `replacement::clipboard::tests`, isolated `native_clipboard_preservation_smoke`, `native_edit_replacement_smoke` | Visible paste/formats/history per app |
| App undo | Allowed-flow matrix, `replacement::tests::app_undo_*`, native editor smoke | Configured global undo hotkey |
| Pre-caret range, suffix/newer typing | Allowed-flow matrix, pipeline manual/automatic/movement tests, three native editor smokes | Complete field and caret observation |
| Informative/foreign text ownership | Foreign-selection matrix, manual ownership tests, native movement smoke | T09 old text/selection |

## Record results

Copy this header into a separate result file. Use one row per target, control,
trigger and case. Attach runner `results.json`/logs; evidence uses synthetic text.

```text
Date/time/timezone / tester:
Windows build / keyboard layout:
AutoFix commit / build:
Settings/rules snapshot / restored afterward:
Automation results directory:
```

| Target/version | Process/title/control | Case | Trigger | Expected | Actual text/caret/metadata | Status | Evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Fill after testing | | | | | | NOT RUN | |

Statuses: `PASS`, `FAIL`, `UNSUPPORTED`, `NOT RUN`, `NOT INSTALLED`, `N/A`.
`PASS` requires the exact applicable oracle. Safe refusal remains `UNSUPPORTED`
for positive correction coverage. Password/admin edits, clipboard loss, altered
suffix/prefix, unauthorized engine work or incorrect undo are failures.
