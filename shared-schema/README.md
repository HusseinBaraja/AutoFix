# Shared Schema

Shared config schemas, IPC contracts, and schema documentation for AutoFix.

This component owns stable contracts between:

- Background mode in `app-core-rust`.
- Settings mode in `ui/settings-ui`.
- Future installer and migration tooling.

Keep schemas explicit, versioned, and documented. Avoid storing executable runtime state here; this area is for contracts, structured settings, and compatibility notes.

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
and waits for the next automatic trigger. Older settings files default to one
pending correction and skipping new triggers. Queue contents remain memory-only.

API settings retain `timeout_manual_ms = 3000`, `timeout_auto_ms = 700`,
`retry_count = 1`, and `fallback_to_local = false` defaults. Retry count accepts
only 0 or 1 for saves in both Rust and the settings UI. Both loaders normalize
legacy counts above 1 to 1; loading does not rewrite the original file.
Both attempts share the trigger's
total timeout budget. `feedback.show_timeout_notice` defaults to true and applies
only to a valid manual API timeout; automatic timeouts always stay silent.

Unavailable application rules deny authorization for typed capture and every
correction trigger, including API execution. An empty successfully read rule
list is distinct from a failed read.

Queued API work must refresh app-rule authorization and cancellation at every
outbound send, including retries. The runtime holds a SQLite writer reservation
from the fresh policy read until the send returns, so IPC and settings UI rule
writes serialize with transmission. A revocation is effective when its write
commits; previously transmitted data cannot be recalled. Missing, unreadable,
or busy policy storage denies sending immediately. Denied frozen work releases
its slot and retires its original text through failed completion.

The live replacement consumer is still a placeholder. Before enabling it, the
mutation owner must enforce `silent` or explicit acceptance of a suggestion and
recheck process, focused target, security gate, caret, and context/executable
versions immediately before manual replacement. Frozen automatic replacements
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
replacement remains unavailable; these tests cannot establish target mutation,
rollback, clipboard recovery, or target undo until the native consumer exists.
