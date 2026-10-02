# Shared Schema

Shared config schemas, IPC contracts, and schema documentation for AutoFix.

This component owns stable contracts between:

- Background mode in `app-core-rust`.
- Settings mode in `ui/settings-ui`.
- Future installer and migration tooling.

Keep schemas explicit, versioned, and documented. Avoid storing executable runtime state here; this area is for contracts, structured settings, and compatibility notes.

`[learning]` is optional in TOML. `mode` accepts `off` (default), `ask`, or
`automatic`; `rule` accepts `pair` (default) or `dictionary`; `per_app` defaults
to false. Rust and WPF use the same defaults and reject unknown choices.
Learning changes only future exclusions after successful app-level undo. Accepted
consent and automatic learning queue saves on a bounded background writer with
its own connection. Settings changes revoke pending consent; save failures warn
without captured text and never block input processing.
Exclusions remain active independently of these settings.

The WPF editor and Rust engine share `custom_dictionary_entries` and
`learned_correction_rules` in `autofix.sqlite`. Null app scope means all apps;
`und` dictionary language and null/`und` pair language mean all languages.
Active pairs use `learning_enabled = 1` and `rule_type = 'pair'`. SQLite row IDs
identify editor updates and deletes; dictionary/rule edits use transactions.
The engine owns migration versions and accepts tables initialized by settings.
No dictionary IPC is required: the settings editor writes the same local database.

Confidence settings retain the existing TOML values: `silent`, `suggestion`, and
`do_nothing`. High defaults to `silent`, medium to `suggestion`, and low is fixed
to `do_nothing`. In v1, medium `suggestion` means manual-only with available
suggestion UI, otherwise no action; medium `silent` enables both automatic and
manual corrections. The Rust correction contract adds an optional
`suggestion_ui_available` input capability (default false) and an output
`behavior` disposition (default `do_nothing` for older serialized outputs).
Consumers must replace text only for `silent` results; `suggestion` requires
explicit user acceptance. These engine contracts are separate from IPC.

Language preferences and per-app overrides validate RFC 5646 tag structure in
both the Rust engine and settings UI, including complete extension and private-use
subtags. Registry membership is not checked. `language-tag-cases.json` is the shared
validation corpus; both test suites cover it through the config storage paths.

Pending correction settings live in the existing `[context]` TOML section:
`pending_queue_size` defaults to 1 and accepts 1 through 16, counting running and
waiting requests per session. `pending_queue_full_behavior` accepts `skip_new`
(default), `cancel_oldest`, and `merge_newest`. Merge cancels the newest pending
request, restores its typed text into the active context alongside new typing,
and waits for the next automatic trigger. Cancel-oldest cancels the oldest and
dependent newer requests, then resubmits all unchecked text with the new segment.
It does not commit cancelled text into informative context. Older settings files
default to one pending correction and skipping new triggers. Queue contents remain memory-only.

API settings retain `timeout_manual_ms = 3000`, `timeout_auto_ms = 700`,
`retry_count = 1`, and `fallback_to_local = false` defaults. Retry count accepts
only 0 or 1 for saves in both Rust and the settings UI. Both loaders normalize
legacy counts above 1 to 1; loading does not rewrite the original file.
Both attempts share the trigger's
total timeout budget. `feedback.show_timeout_notice` defaults to true and applies
only to a valid manual API timeout; automatic timeouts always stay silent.

Feedback defaults to tray states and small manual error/blocked/timeout notices.
`show_correction_applied_notification`, `show_skipped_reason`,
`show_medium_confidence_suggestions` and `show_near_caret_overlay` default false;
`tray_state_enabled`, `show_blocked_app_notice` and `show_timeout_notice` default
true. Missing feedback fields use these defaults; explicit saved values survive.
Medium suggestions are read-only previews of validated executable text, never
authorization to mutate. Near-caret placement does not enable any notice.
`app_status` and `config_reloaded` include a text-free `tray_state` with one of
`idle`, `active`, `correcting`, `blocked`, or `error`. Disabling state publishes
idle while keeping the tray accessible.
The status also carries `tray_state_enabled` so the shell can preserve that
preference when the engine becomes unavailable. Hard secure-field/desktop refusals,
cancelled and stale results cannot display a notice or suggestion preview.

The Windows IPC pipe rejects remote clients and uses a protected ACL granting
access only to its owner. The .NET client verifies that the server has the same
Windows owner and elevation context before sending a request. Processes running
as that account share the configuration trust boundary; IPC does not authenticate
individual executables. Anonymous local clients cannot connect, including for
read-only access.

Each connected client has a one-second pipe-I/O deadline covering request reads,
response writes and response consumption. Expiry cancels only that pipe's I/O so
an idle or nonreading client cannot monopolize status polling. Responses drain
before disconnect because Windows discards unread pipe bytes on disconnect.
Both byte-stream clients reading to EOF and message-mode clients are supported.
The .NET client retains its separate 400 ms connect and request deadlines; legacy
status payloads without tray fields default to idle with state display enabled.

Unavailable application rules deny authorization for typed capture and every
correction trigger, including API execution. An empty successfully read rule
list is distinct from a failed read.

Queued API work must refresh app-rule authorization and cancellation at every
outbound send, including retries. The runtime holds a SQLite writer reservation
from the fresh policy read until the send returns, so IPC and settings UI rule
writes serialize with transmission. A revocation is effective when its write
commits; previously transmitted data cannot be recalled. Missing, unreadable,
or busy policy storage denies sending immediately. Denied frozen work releases
its slot and restores its original text and dependent newer segments to active
executable context through failed completion.

The native replacement consumer enforces `silent` results; suggestion acceptance
remains planned. The mutation owner must recheck process, focused target,
security gate, caret, and context/executable versions immediately before manual
replacement. Frozen automatic replacements
validate their session, segment identity, original typed range, caret anchor,
and known following text instead of requiring unchanged active context versions.
They wait for queued hook input and recheck generations after live security calls.
Informative context stays read-only;
replacement stays within the verified executable span and never alters text
after the caret. Preserve the clipboard and record app-level undo only after
successful replacement.

Cancelled, duplicate, stale, or reordered completions must be discarded before
mutation. Integration verification must cover typing or moving focus while an
API request is pending, cancellation followed by a late response, duplicate and
out-of-order results, a target becoming secure, and interrupted replacement with
rollback and clipboard/undo recovery. The async pipeline now tests stale and
reordered result rejection, typing/focus changes, security revalidation, and
session completion only after a replacement callback confirms success. Native
clipboard paste and SendInput replacement are implemented for supported controls;
direct text APIs and UI Automation mutation remain planned. Focused native tests
verify correction, recorded app-level undo, clipboard preservation and recovery,
and preservation of newer typing, caret position, and text after the caret.
