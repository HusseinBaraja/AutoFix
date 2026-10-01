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

`src/dictionary` owns persistent exclusions and optional learning. SQLite stores
word/phrase entries and active pair-specific rules. App process matching ignores
ASCII case. `und` and null language scopes protect every language; base tags such
as `en` apply to variants such as `en-US`. Regional tags match the resolved tag.
Unknown or mixed language conservatively protects all entries for the app.
Both engine routes enforce whole-term boundaries, including phrases, and filter
blocked pairs while preserving unrelated edits. Exclusions are snapshotted for
execution. Broad edits that cross a pair's boundary are refused when unchanged
context cannot prove a different replacement. Exclusions are reread
under the replacement writer reservation; a newly
blocked edit refuses the in-flight replacement.

Learning defaults to off. Only a verified, successful AutoFix undo supplies a
rejection, using the original/replacement span and the correction's recorded
language. It never captures document text or native Ctrl+Z. The smallest changed
word/phrase is learned; multiple separated changes form one phrase. Ask mode uses
a separate, coalesced native Yes/No prompt, defaults to No, and stores no text
until consent. Input processing remains available while the prompt is open.
Changing learning settings cancels pending consent. Automatic mode writes the
selected dictionary or pair exclusion after successful undo. Disabling learning
does not disable existing rules. Suggestion rejection remains unavailable with
the planned suggestion UI; failed or refused undo never learns.
`LocalRuleEngine`, `LocalMlEngine`, `OpenAiCompatibleApiEngine`, and
`CustomApiEngine` implement the same interface. `LocalRuleEngine` provides fast,
deterministic English correction for a conservative list of clear misspellings.
The engine interface reports supported BCP 47 tags. Language selection uses
read-only informative context and executable context, then resolves a primary
language from a per-app override, global preference, or the session detection.
The conservative detector recognizes English from function words and script
families for other text; it leaves ambiguous Latin text unknown. Detection is
memory-only per focused session. `correction.preferred_language` accepts a BCP
47 tag, and `correction.app_language_overrides` accepts entries such as
`notepad.exe=fr-FR`. Unknown and mixed text default to high-confidence typo
edits only, with no grammar or translation. `correction.uncertain_language_policy`
can instead skip correction or use normal correction. For mixed text,
`correction.mixed_language_policy` defaults to correcting only the dominant
language; it can disable correction or correct per token with the API engine.
Structured tokens, names, explicit protected terms, and words in other scripts
remain protected. The local rule engine supports English only; API engines can
receive other language tags. The async pipeline invokes the selected engine with
these policies. Validated changed results can now use the native replacement
strategies described below.
In grammar mode it applies only enabled categories. Conservative local rules
cover capitalization, sentence-ending punctuation on manual correction, extra
punctuation, repeated words, subject-verb agreement, a/an articles, a few
prepositions and homophones, spacing, contractions, and tense. Clarity and
word-order rules are API-only. It preserves
custom-dictionary entries, explicit protected terms, and detectable names,
emails, URLs, paths, handles, hashtags, code identifiers, and product names.
`OpenAiCompatibleApiEngine` and `CustomApiEngine` send non-streaming
chat-completion requests through WinHTTP. Presets include OpenAI, Groq, and
DeepSeek; `custom` requires a base URL. HTTPS is required except for loopback
HTTP. The provider preset selects a generic Windows Credential Manager entry
named `AutoFix/provider-profile/<preset>`; callers can store it with
`secrets::set_secret`. API keys are never put in TOML or diagnostic logs.
Manual and automatic requests default to 3000 ms and 700 ms respectively.
`api.retry_count` accepts only 0 or 1 (default 1); retries share one total
deadline, including credential access, transport, and response validation.
Loading legacy settings reduces retry counts above 1 to 1 without rewriting
the file. Saving still rejects unsupported retry counts.
The caller stops waiting at that deadline and discards late transport results.
WinHTTP operations also use the remaining budget, including each response read,
so trickling responses cannot renew the timeout.
`ApiCorrectionEngine::submit` runs the
request on a worker thread so the caller can keep processing typing. Fallback
to the local rule engine is off by default. API results must include categorized
edits for the executable span. The engine rejects disabled grammar categories,
unlisted changes, invalid offsets, and changed protected terms. API typo edits
must match the local engine's known spelling replacements.
Automatic timeouts silently skip correction and release frozen queue capacity.
Valid manual API timeouts show a small, disabled, no-activate notice near the
bottom-right work area for 2.5 seconds. `feedback.show_timeout_notice` defaults
to true and can disable it. Repeated notices coalesce; cancelled, stale, and
secure-target results cannot show a notice. Opt-in local fallback follows the
same confidence and completion checks as normal local correction and suppresses
the timeout notice when it completes.
`ApiCorrectionEngine::notice_for` distinguishes manual timeout and failure
feedback from silent automatic handling. The local ML engine remains
a placeholder. Live triggers invoke these engines on the correction worker.
The native replacement path verifies the session-owned pre-caret range.
Engine selection is explicit per request; neither local nor API routing depends
on task difficulty, and both correction modes are accepted by every engine.

Confidence decisions are shared by the local and API engines. High confidence
defaults to silent apply. Medium confidence defaults to a suggestion only for a
manual shortcut with an available suggestion UI; word-count, character, and
final-fix triggers do nothing. Setting `correction.medium_confidence_behavior`
to `silent` enables medium corrections for every trigger. Low confidence is
always blocked, including requests that bypass config validation.
`CorrectionInput.suggestion_ui_available` defaults to false; v1 has no suggestion
UI. `feedback.show_medium_confidence_suggestions` can suppress manual suggestions
when building the runtime policy, but never enables silent apply. Outputs include
an explicit `behavior`: only `silent` authorizes replacement; `suggestion`
requires user acceptance. Suppressed outputs preserve the original executable
text and discard edit details. Local results prioritize silent edits over
suggested edits when both occur in one request. The pipeline snapshots confidence
policy and rejects suggestions, suppressed edits, and low-confidence results at
completion before allowing target replacement.

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
freeze the entire current executable segment, including earlier skipped
boundaries. The manual shortcut accepts
a selected span when UI Automation proves it belongs to the typed segment.
An outside or unreadable selection blocks
the shortcut by default. The `shortcuts.correct_arbitrary_selection` setting is
off by default; when enabled, a selected span outside the typed segment becomes
temporary executable context for that request, while text before and after it
stays informative. V1 discards selected results because it cannot prove which
selection end is the live caret.
App rules and the hard security gate are checked before routing. Application
policy must be readable: a failed app-rule read blocks capture and correction
for every trigger and engine, even if dictionary reads still work. A matching app
rule permits typed-input tracking only when it allows a word-count or character
trigger. Manual-shortcut permission alone does not enable continuous capture.
Manual-only rules can still use the opt-in arbitrary-selection shortcut path.
The correction pipeline invokes the selected engine and validates completions.
Successful unchanged caret-range results commit the original executable segment into
informative context. Changed caret-range results require confirmation from
the replacement owner before session completion or undo recording. If the provider cannot
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
context. Automatic correction retires only its frozen segment and preserves newer
executable text. A manual trigger or final fix with no changes
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
the final-fix security gate for the old executable text. Final fix remains
unavailable because the old caret cannot be safely targeted after movement. Pending
corrections are invalidated on movement. Runtime ticks delete sessions when
their owning process exits. All session state disappears on engine exit or
termination and is never written to disk. Session completion methods do not
themselves change target text; the replacement engine must confirm mutation first.

The correction pipeline uses one worker, a FIFO of requests, and one completion
slot with backpressure so results cannot overwrite each other. Each session has
one active executable context and a bounded pending correction queue.
`context.pending_queue_size` defaults to 1 and accepts 1 through 16; the limit
includes running work. Automatic triggers freeze the current executable context
before processing subsequent typing, including keys in the same input batch.
New typing starts a fresh executable context while correction runs asynchronously.
`context.pending_queue_full_behavior` defaults to `skip_new`, which skips the
new automatic trigger and retains current typing. `cancel_oldest` cancels the
oldest pending request, retires its original text into informative context, and
admits the new segment. `merge_newest` cancels the newest pending request, merges
its typed text back into the active executable context, and waits for the next
trigger. Manual correction cancels pending work and restores its original text
to the active context before taking its selection or caret snapshot.
Automatic triggers never wait for correction. Manual shortcuts
retain strict input snapshots. Frozen dispatch checks the position generation
before and after live policy and dictionary reads, so newer typing does not
cancel earlier work. A refused dispatch restores that segment and newer
reservations to the active context while preserving older admitted segments.
Manual shortcuts
can wait up to 20 ms on the input processor, then continue asynchronously. Hooks
and the Windows message loop remain independent. Pending hook input is drained
before a manual snapshot. Every request includes a unique memory-only session ID,
context/executable/caret-anchor versions, trigger, engine kind, and correction mode.
Grammar, language, confidence, dictionary, and API settings are snapshotted for
engine execution. Following selected text stays informative.

Frozen results survive processed typing and backspace confined to the new active
context. The tracker retains the original typed ranges and supplies the replacement
owner with the known following text up to the current caret, which must be verified
and preserved when replacing an earlier segment. Manual snapshots still require
exact versions, editable text, and input sequence. Frozen results are discarded if
their range is no longer known, the session changes or disappears, focus/caret
generation changes, movement occurs, backspace crosses a frozen boundary, the
typed buffer evicts its anchor, executable context commits, or configuration reloads.
Cancellation prevents queued work from
starting and suppresses late publication; a running synchronous API transport can
finish within its configured timeout. API jobs also recheck cancellation and the
current app rules immediately before each outbound send, including retries.
The transport acquires a SQLite writer reservation before reading policy and
holds it until the send returns. This serializes authorization with app-rule
writes from IPC and the settings UI, including when SQLite uses WAL mode.
Revocation takes effect when the rule write commits: a send already holding the
reservation may finish first, and transmitted data cannot be recalled. Missing,
unreadable, or busy policy storage denies the send without waiting or retrying.
Denied frozen jobs retire their original text and release queue capacity through
normal failed completion. Completion rechecks the live security gate
and focused target, then input generations again after those checks. Completions
wait for hook input to be processed and retire frozen ranges in document order.
Before a changed result reaches replacement, completion also requires a stable
focused control identity and a fresh read-only TextPattern capture. The entire
known session region must match exactly at the live caret, including text typed
after a frozen segment. Missing captures or mismatches discard the result; no
fuzzy search or replacement is attempted. Selected-text results are discarded
because their replacement range cannot be proven before the caret, even when
arbitrary selected-text correction is enabled.
Failed, suppressed, or refused frozen results release their slot and retire the
original text without applying engine output. Results are
consumed once. Only completed silent corrections above low confidence reach the
replacement boundary. Failed and suppressed manual edits do not commit executable
context. Tests cover delayed engines, frozen queues and overflow policies,
manual override, stale and reordered results, queued input,
secure targets, failed replacement, selection boundaries, and commit/undo after
confirmed replacement.

The replacement feature lives in `src/background/replacement`. Its strategy
interface tries direct text APIs, UI Automation replacement, clipboard paste,
then SendInput. Direct APIs/TSF and UI Automation mutation currently report
unavailable and can be implemented progressively. UI Automation TextPattern
must first prove a collapsed caret, the exact executable span and any newer
typed text between that span and the caret. Only that span is selected; text
after the original caret remains untouched. Missing or unreliable providers
refuse replacement rather than guessing a keystroke distance.

Clipboard replacement uses synchronous `WM_PASTE` on recognized native Edit
and RichEdit controls. It snapshots every enumerated clipboard format, including
registered binary formats, bitmaps, palettes and metafiles, before changing the
clipboard. Owner-managed or unreadable formats refuse this strategy before any
clipboard write. It preallocates a complete restoration set, proves the target
selection while the clipboard stays untouched, then installs temporary Unicode
text immediately before synchronous paste. Privacy markers are installed before
text to disable clipboard history and cloud upload. Restoration runs immediately
after paste (including paste failure), before target verification, and on scope
exit. Success requires both verified replacement and complete restoration.
Ownership and independent copies of published data distinguish Windows-synthesized formats from
an external clipboard change. A user's newer copy is retained and the attempt
fails rather than overwriting it.

`replacement.clipboard_enabled` defaults to true. The settings UI exposes
**Correction > Use clipboard for correction**; disabling it skips clipboard
preparation for both correction and app-level undo. Restoration failures emit
metadata-only warnings and skip clipboard replacement for that process name
(case-insensitive) until the engine exits. A failed restore removes temporary
text when the clipboard lock is available, keeps the full saved snapshot in
memory, and retries recovery on a dedicated worker until restored or superseded
by a new copy, with at most eight active attempts over two seconds. The worker pumps
clipboard-owner messages while retrying and waiting for the clipboard lock.
Exhaustion releases ownership but retains originals in memory and quarantines
clipboard replacement across all apps. Read-only probes recognize a newer copy
or resume one restoration when a previously busy lock clears. Partial restorations
are not emptied and republished on each probe. The gate clears only after full
restoration or an external copy. Other clipboard attempts use fallback while
recovery is pending or quarantined. The tray sends a
process-scoped graceful stop signal and waits for the engine to drain recovery
before closing its process job. Shutdown makes one final attempt for quarantined
data and reports incomplete cleanup separately from a clean exit. A permanently
locked clipboard, failed final restoration, forced termination, or a crash can
leave clipboard cleanup incomplete; metadata-only warnings report that outcome. Clipboard contents
are never persisted for crash recovery.

SendInput fallback inserts UTF-16 Unicode key pairs into the same proved
selection; an empty replacement deletes the selection with Backspace. It does
not use or change the clipboard. The entire batch is validated and allocated
before selection. It runs only after all safer available methods refuse, or the
clipboard method is disabled. Only standard Unicode Edit controls with known
insert semantics are supported. Read-only, password, numeric, case-transforming,
ANSI, custom and RichEdit controls refuse this fallback before selection.
It refuses all control characters and held shortcut
modifiers (after a bounded release wait). Immediately before mutation it rechecks
the exact selected text, input generations, field security, modifiers and native
keyboard focus. Partial input, failed paste, or failed
verification stops the strategy chain and invalidates the tracked session.
If a partial batch ends on a keydown, only its matching keyup is attempted;
deletion and typing are never retried.
Verification reacquires a fresh UI Automation provider after mutation and restores
the caret beyond newer typed text only after checking that text exactly.
Range proof accepts scalar or UTF-16 provider units only when the text matches
exactly at its fixed anchor; it never searches for similar text. App rules are
reread under a SQLite writer reservation held through replacement and app undo.
Missing or busy policy storage skips mutation. Final native checks require all
authorized target attributes, including process name and window title, to remain
unchanged before mutation; title changes refuse correction and undo even without
caret movement. After mutation, verification and caret restoration permit an
editor's modified-title marker while still requiring every other identity and
safety attribute and the input generations to match.
Provider error messages are
discarded so failure logs cannot contain document text.

Every replacement returns success/failure, the attempted method (or none for
pre-strategy rejection), the exact range when proved, and a failure reason.
Failure/skip metadata is logged at warning level under default logging, including
the attempted method and whether the tracked selection may have changed.
Range offsets count Unicode characters backwards from the original caret,
with `start_back >= end_back >= 0`; they are not document byte or UTF-16 offsets.
App-level undo uses only a recorded app correction that remains fully retained
in informative context. It verifies the live anchor and restores that recorded
span, preserving newer executable text. Memory truncation, movement, unknown
selection geometry and secure fields block native undo.

Focused tests cover strategy ordering, the clipboard disable preference,
fail-closed authorization, mutation failures, Unicode ranges, rich/binary format
restoration, restoration failures and retry, Windows-synthesized formats,
per-app fallback preference, clipboard handle copying and app-owned undo spans.
The opt-in `native_clipboard_preservation_smoke` runs in a child with a separate
noninteractive Windows window station and seeds rich text, HTML, binary data
and a bitmap. It verifies actual native paste, restoration, recovery from a
locked clipboard, and retention of a newer copy without touching the desktop
clipboard. It refuses a visible station or unrelated existing clipboard data.
The opt-in `native_edit_replacement_smoke` test owns a separate native editor;
it checks clipboard/SendInput and preservation of the document suffix on an
interactive Windows desktop. It safely refuses unsupported clipboard formats.

Feature code should be organized by product behavior, not technical layer. Keep modules small, private by default, and colocate tests with the behavior they verify.

Do not implement optional helper mode here until it becomes a committed product requirement.
