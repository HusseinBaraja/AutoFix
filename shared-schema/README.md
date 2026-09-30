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
