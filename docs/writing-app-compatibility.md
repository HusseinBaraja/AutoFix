# Writing-app compatibility

Compatibility means actual correction and AutoFix undo work in the intended
writing surface while preserving session ownership, prefix/suffix, newer typing,
clipboard and secure-field blocking. A process name, installed registration,
policy test or successful test in another app is not sufficient evidence.

## Target selection

[The scope](../shared-schema/windows-writing-apps.json) contains only writing-heavy
apps observed on this laptop. There is no fixed target count or category quota.
The 2026-10-05 inventory contains 25 targets. Re-exporting filters out scoped apps
that are no longer detected. No uninstalled app or standalone web service counts
as a target. An unfamiliar newly installed writing app needs a reviewed scope entry.

| Writing use | Installed targets |
| --- | --- |
| Messaging | WhatsApp, WhatsApp Beta, Telegram, Microsoft Teams, Skype for Business |
| Notes and documents | Obsidian, OneNote, Sticky Notes, Notepad, Word, Outlook (classic), PowerPoint, Publisher |
| Developer prose | Notepad++, WebStorm, RustRover, Zed, Antigravity, T3 Code, SciTE, GitHub Desktop, Codex desktop (registered as ChatGPT) |
| Web writing fields | Microsoft Edge, Chromium, Zen |

Search-only tools, OBS, launchers, converters, terminals, installers and developer
build tools are excluded. Excel is excluded from this scope because its primary
input is numbers and formulas. Browser targets cover sustained writing in message,
email and document fields; address bars and search boxes are not compatibility
priorities. A browser installation proves neither service access nor provider
support. PowerPoint/Publisher targets cover prose, not object names or searches.
Codex targets cover prompt prose; tool restrictions can prevent automated UI tests.

Developer editors remain blocked by default. Explicit editor-prose and trigger
opt-ins permit only selected prose or eligible line-comment prose according to
the existing safety policy. Catalog membership grants no runtime permission.
Terminal commands and source code are not general correction targets.

## Implemented capabilities

| Adapter | Admission | Limits |
| --- | --- | --- |
| Direct text API | Known writable Unicode native Edit/RichEdit; exact UIA span and native UTF-16 selection; enough capacity | Native HWND required; protected/unknown range attributes refuse; modern RichEdit class recognition alone is not a tested-app pass |
| UIA selection + Unicode input | Editable Edit/Document control; same authorized provider/foreground/keyboard process; exact writable range at document end | No newer following text or after-caret suffix; no control characters; custom selection/caret providers can refuse |
| Clipboard | Existing synchronous native paste adapter with complete format restoration | Unknown/asynchronous paste APIs refuse |
| SendInput | Existing standard Unicode Edit adapter | Unknown insert semantics refuse |

All methods recheck live selection, input generations, field safety and target
identity. Uncertain mutation never falls through to another method. App-level
undo runs through the same replacement boundary and never sends host Ctrl+Z.
No adapter uses whole-field setters or searches document text for an approximate
match. Arbitrary middle-document UIA mutation, TSF, app-specific document APIs,
and IME composition are still incomplete.

## Laptop inventory

Run from the normal Windows user session:

```powershell
.\scripts\export-app-compatibility.ps1
```

The script reads uninstall registrations, Start-menu app identities and packaged
app metadata. It never opens a document or starts a target app. Output is local
under ignored `target/app-compatibility`: `inventory.json`, `coverage.csv`, and
`report.md`. Portable/unregistered apps and authenticated web services may be
missing. The report retains `not_verified` for every target; detection cannot
promote support. Reproducible exporter checks use:

```powershell
.\scripts\test-app-compatibility.ps1
```

Notepad is detected from its existing system executable. Other targets use app
registrations; duplicate registrations and private-browser launchers do not create
extra targets. Machine-local inventory is not committed.

## Verification and release gate

See the [recorded verification](writing-app-compatibility-results.md) for checks
completed and their limits.

```powershell
.\scripts\test-windows-integration.ps1 -NativeDesktop -WpfDesktop -IsolatedClipboard
```

Run on an unlocked desktop without a competing AutoFix engine. Native tests
briefly focus their own disposable windows. Default Rust runs leave interactive
tests ignored, and the runner rejects zero-test matches. WPF commands manipulate
only the fixture's synthetic text through its own stdin pipe.

Record actual app/version/control passes with the cases and exact oracles in
[the integration scenarios](windows-text-target-integration.md). Test all three
triggers, app-level undo, Unicode, multiline text, old/foreign text, newer typing,
suffix preservation, clipboard formats and password/read-only refusal. Record
safe refusals as `UNSUPPORTED` for positive coverage. Use unsent disposable chat
drafts and never send messages as a test. Keep existing user drafts/documents.

Full compatibility for the scoped laptop apps remains incomplete. Native Edit
and WPF fixture passes prove those providers only. WhatsApp was inspected after
access retries: its focused WinUI bridge has no TextPattern range and ambiguous
focused descendants. It remains unsupported by the current adapter. No composer
correction or message sending occurred. Computer Use stopped on physical Escape.
Actual app coverage and broader document adapters remain release blockers for
a claim of full compatibility or unrestricted customer readiness.
