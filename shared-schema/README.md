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

The live replacement consumer is still a placeholder. Before enabling it, the
mutation owner must enforce `silent` or explicit acceptance of a suggestion and
recheck process, focused target, security gate, caret, and context/executable
versions immediately before replacement. Informative context stays read-only;
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
