# AGENTS.md

## AutoFix invariants

AutoFix is a native Windows AI autocorrect (.NET 8, WPF). Preserve these rules in runtime changes:

- Use only text typed in the current field since the last caret move or focus change. Never read the document.
- Send earlier text only as read-only context; edit only the target text.
- Never change text after the caret. Drop any answer whose text changed while it was in flight.
- Never read or send password fields; skip blocked and elevated apps.
- Keep the clipboard intact (selection fixes restore it) and keep Backspace-undo working.
- Hook callbacks on the input thread must stay fast: no network, UI Automation, disk, or UI work there.
- Never lose or reorder user input: keys pressed during an edit or an Enter hold are replayed in order.

See `README.md` for behavior and limits.

## Code organization

- `src/AutoFix.Core`: platform-free logic with unit tests. Prefer putting decisions here.
- `src/AutoFix`: thin Windows adapters (hooks, SendInput, UI Automation, clipboard, UI).
- Keep the app small. Add settings only when users need the choice.

Use `.agents/skills/caveman/SKILL.md` in lite mode for responses.

## Verification

Add or update focused tests for changed behavior. Run `dotnet test .\tests\AutoFix.Tests` and
`dotnet build .\AutoFix.sln` (warnings are errors). For input, injection, focus or UI changes, run
`.\scripts\test-e2e.ps1` (it takes keyboard focus) and inspect screenshots for visual changes.
Report any check that could not be completed and why.

## Git

Never discard user changes. Never commit to main. For repository changes made
on main, create a branch unless the user explicitly says no branch is needed.
Use branch names without `/`. Commit completed logical checkpoints.

Always push completed, verified review fixes and their supporting tests or
documentation to the existing feature branch on `origin`. The user grants
standing authorization for these pushes to this repository without asking
again, unless the user explicitly requests local-only work.

For commits, use `.agents/skills/conventional-commit/SKILL.md` and
`.agents/skills/caveman-commit/SKILL.md`; keep the final message Conventional
Commits compliant. Close only shells and app processes started for the task.

## Pull requests

Create pull requests ready for review, never as drafts. Write clear titles and
descriptions that explain the change, why it matters, and how it was verified.

## Review fixes

For CodeRabbit findings, make the smallest correct fix and verify the affected
behavior.
