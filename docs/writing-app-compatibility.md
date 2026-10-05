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

When UIA returns an opaque native host, admission, capture and replacement now
share one composer resolver. It requires a keyboard-owned host in the foreground
process, known-safe focused descendants, and one editor identity. Repeated
references count as one editor only when both runtime IDs and UIA element
comparison agree. Different editors, identity collisions, protected descendants
and unknown ownership refuse. This is implemented behavior, not an actual-app pass.

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
access retries: its focused WinUI bridge has no TextPattern range. A later probe
reported many references sharing one editor runtime identity. On 2026-10-05,
the user-assisted WhatsApp run produced no correction or undo. Runtime diagnostics
showed descendant process-owner rejection and discarded input after slow provider
processing. Ownership metadata identified `msedgewebview2.exe` as a child of
`WhatsApp.Root.exe`. Hosted-provider admission now requires a live process chain
to the foreground application, matching token user/session, no elevation and
creation-time order to reject reused parent PIDs. Unrelated/unknown owners refuse.
Fresh bulk property snapshots reduce duplicate-reference calls, and adjacent
physical input batches coalesce without crossing shortcuts or config/reset
boundaries. Queue overflow still invalidates ownership. These changes need a
new actual-app pass; they do not establish WhatsApp compatibility.
WhatsApp composer resolution and correction remain unverified.

The user reported that Telegram manual correction and exact AutoFix undo worked
as expected in the same isolated run. Runtime diagnostics confirm successful UIA
correction and undo of a three-character span. This is one manual composer pass;
automatic triggers, Unicode, multiline text, newer typing, surrounding text,
clipboard and secure-field behavior still need actual-app passes. An earlier
Telegram range-preparation failure was observed before these changes; its exact
underlying failed provider operation was not established.
Actual app coverage and broader document adapters remain release blockers for
a claim of full compatibility or unrestricted customer readiness.

## User-assisted checks without Computer Use

After building `AutoFix.sln`, exit a normal AutoFix shell before starting:

```powershell
.\scripts\writing-app-check.ps1 -Mode Manual
# When finished with this run:
.\scripts\writing-app-check.ps1 -Stop
```

The script starts a hidden standalone engine with isolated settings/storage under
ignored `target`, local English typos-only correction, learning off, arbitrary
selection off, and metadata diagnostics. It does not copy normal settings,
dictionaries or credentials. Runtime startup holds one engine lease before
initializing storage or input hooks. The stop command signals only its recorded
process after checking executable and start time; it never force-kills a process.

In each app, physically type `teh word` into an empty unsent disposable composer,
press and release Ctrl+Alt+Space, and compare the result with `the word`. Without
moving focus/caret or typing more, press Ctrl+Alt+Z: expect exactly `teh word`.
Record correction and undo separately. Never send a message or replace an existing
user draft. The active record `target/writing-app-check-active.json` identifies
the PID, mode, settings and logs. Logs contain counts, stage names and HRESULTs;
provider error descriptions and document text are excluded.

Stop before changing modes. `-Mode WordCount` enables only a two-completed-word
trigger; `-Mode Character` enables only `.`. Follow T02/T03 and the remaining
preservation/security scenarios above. Newer following typing, middle-document
UIA replacement and multiline replacement through Unicode keyboard input still
refuse. They remain compatibility gaps, not positive preservation passes.
