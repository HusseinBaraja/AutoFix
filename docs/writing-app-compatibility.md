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
| UIA selection + Unicode input | Editable Edit/Document control; verified provider ownership and native keyboard host; exact writable selection | Equal-scalar-width substitutions use at most 64 single BMP-character patches, preserving unchanged Unicode/line breaks and known following text. Other multi-character input requires a proved document/field end and no newer following text; control characters refuse |
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
boundaries. Queue overflow still invalidates ownership.

A later user-assisted run resolved the composer, retained three physically typed
characters and queued manual correction, then refused completion before mutation.
Metadata showed a valid request/input stamp but a failed live suffix comparison:
the bounded read returned 2,003 characters for three known typed characters.
The next diagnostic run confirmed clipping: the sentinel read returned 2,004
UTF-16 units; halving the endpoint movement returned a complete 1,004-unit range
whose suffix matched the typed text. Bounded context reads now detect clipping and
shrink the range until complete text fits, while preserving the original caret
endpoint. Always-clipped, unavailable or mismatched text remains a refusal.
Composer queries took roughly 1.5–2.9 seconds. Discovery is now reused only within
one guarded operation with unchanged input generations, native focus and exact
host/editor identities. Live ownership, protection, visibility and keyboard focus
are rechecked on reuse; COM references are released before the apartment ends.

Live unsent-draft tests then identified two replacement barriers: WhatsApp's
field range encloses an inner text node and its caret endpoint differs from the
field's final text endpoint; UIA Select can return before the app applies the
selection. Field-end proof now requires exact Edit ancestry, writability,
containment and an empty gap with no embedded children. Selection acknowledgment
waits up to 250 ms while retaining exact text/endpoint and input/target checks.
Equal-width substitutions can use separate selected-character writes from right
to left. Each patch and the final text are proved; partial/uncertain mutation stops
fallback and invalidates session ownership.

WhatsApp manual, two-word and `.` trigger correction/AutoFix undo passed against
exact disposable drafts on 2026-10-05. The opt-in test seeds synthetic session
input and holds the single-engine lease; it does not prove physical hook capture
or global shortcut dispatch. The earlier user-assisted run did retain three
physically typed characters. Combined end-to-end physical passes, latency,
Unicode/multiline/newer/suffix cases and secure-field coverage remain unfinished.

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
preservation/security scenarios above. Equal-width selected-character patches
are implemented, but their Unicode/multiline/newer/suffix app coverage remains
unverified. Length-changing middle-field replacements and control-character
keyboard insertion still refuse; those are compatibility gaps.

## Opt-in synthetic live-draft checkpoint

Close any AutoFix engine, prepare the exact disposable draft, keep its composer
focused, then run the ignored test on the interactive Windows desktop:

```powershell
$env:AUTOFIX_EXPECTED_PROCESS = 'WhatsApp.Root.exe' # or Telegram.exe
$env:AUTOFIX_LIVE_CASE = 'manual'
cargo test live_composer_roundtrip -- --ignored --nocapture
```

This uses production trigger, security, completion, replacement and undo code
with isolated local policies/storage and synthetic session input. It never sends
a message and refuses before mutation unless the whole draft matches the chosen
fixture. Each passing case ends with the exact original draft restored.

| Case | Exact original draft | Caret | Status on WhatsApp |
| --- | --- | --- | --- |
| `manual` | `teh` | End | PASS: correction and undo |
| `word_count` | `teh word ` (trailing space) | End | PASS: correction and undo |
| `character` | `teh.` | End | PASS: correction and undo |
| `unicode` | `é😃 العربية teh` | End | Not run |
| `multiline` | `teh` + newline + `word` | End | Not run |
| `surrounding` | `Earlier notes: teh` | End | Not run; only `teh` is session-editable |
| `newer` | `teh word next` | End | Not run; queues at the second completed word before tracking `next` |
| `suffix` | `teh AFTER` | After `teh` | Not run; ` AFTER` remains outside editable context |

All seeded cases remain unverified on Telegram. Machine-local metadata logs are
under ignored `target/live-whatsapp-*.log`; no document text is logged. Computer
Use was stopped with physical Escape before the remaining cases were exercised.
