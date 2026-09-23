# AGENTS.md

## AutoFix invariants

AutoFix is a native Windows text-correction app. Preserve these boundaries in
runtime changes: use only text typed in the current session; keep read-only
informative context separate from editable executable context; never change
text after the caret; block password and secure fields; preserve the clipboard
and app-level undo.

Use `README.md` and the relevant component documentation to distinguish
implemented behavior from planned capabilities.

## Code organization

For Rust module placement or refactoring, use
`.agents/skills/vertical-codebase/SKILL.md`. Keep feature behavior and its tests
together, and expose narrow interfaces between features.

Use `.agents/skills/caveman/SKILL.md` in lite mode for responses.

## Verification

For code changes, add or update focused tests when they protect the changed
behavior. Run the affected tests and builds, fix failures caused by the change,
and rerun them. Address warnings and errors at their source; do not hide them.
For runtime behavior changes, run the app and inspect the affected flow when
feasible. Report any check that could not be completed and why.
Documentation-only changes need no app build or launch.

Useful entry points: `cargo test`, `dotnet test .\AutoFix.sln`,
`dotnet build .\AutoFix.sln`, and `.\scripts\run-app.ps1`.

## Git

Never discard user changes. Never commit to main. For repository changes made
on main, create a branch unless the user explicitly says no branch is needed.
Use branch names without `/`. Commit completed logical checkpoints.

For commits, use `.agents/skills/conventional-commit/SKILL.md` and
`.agents/skills/caveman-commit/SKILL.md`; keep the final message Conventional
Commits compliant. Close only shells and app processes started for the task.

## Review fixes

For CodeRabbit findings, make the smallest correct fix and verify the affected
behavior.
