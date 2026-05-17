# Shared Schema

Shared config schemas, IPC contracts, and schema documentation for AutoFix.

This component owns stable contracts between:

- Background mode in `app-core-rust`.
- Settings mode in `ui/settings-ui`.
- Future installer and migration tooling.

Keep schemas explicit, versioned, and documented. Avoid storing executable runtime state here; this area is for contracts, structured settings, and compatibility notes.
