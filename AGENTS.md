# AGENTS.md

## Instruction Priority

- Every instruction in this file is mandatory.
- Use `.agents/skills/caveman/SKILL.md` in lite mode.
- Use `.agents/skills/vertical-codebase/SKILL.md` to decide how to structure the codebase.
- Commit incrementally when you complete a checkpoint.
- Follow the existing non-destructive git rules: never revert user changes unless explicitly asked.


## Project Snapshot

AutoFix is a native Windows typo-correction app that works across text fields using a configurable shortcut, word-count triggers, and character triggers. It tracks only what the user types during the current session, separates read-only informative context from editable executable context, and never changes text after the caret. Corrections can run locally or through a configurable API engine, with support for typos-only or typos-plus-grammar modes, confidence-based behavior, custom dictionaries, app rules, language detection, and mixed-language safeguards.

AutoFix runs quietly from the system tray, includes a Flow Launcher-style settings UI, supports app-level undo, preserves the user’s clipboard, blocks password and secure fields, and lets users control where it runs through blocklists, allowlists, and per-trigger overrides. It is designed to be fast, private, configurable, and native to Windows.


## Testing Discipline

- Follow test-driven development when making code changes: add or update tests as you go.
- When fixing a focused small problem, determine if it actually needs a test or not before going straight to coding.
- Keep tests focused on the behavior changed.
- A task is not complete until the app compiles, runs, and has been verified with no warnings or errors.
- Fix warnings and errors properly by addressing the underlying issue; do not silence, bypass, or hide them just to make output clean.
- Only call the task done after tests pass and the app has been run cleanly.

## PR Review Fixes

- When fixing PR issues submitted by CodeRabbit, apply minimal fixes and do not go overboard.
- Minimal change does not mean taking shortcuts; if the correct fix is more involved, make the correct fix.

## Git Workflow

- Never commit to main. If the project is checked out to main and the user asks for a task, create a new branch and do the work in there.
- If the user explicitly says no branch is needed, do not create one.
- Commit incrementally when a logical checkpoint is complete.
- Close all PowerShell/CMD instances you created during the session after you are done working and the codebase is clean and committed.


## Commit Message Skill

- Follow the `conventional-commit` skill workflow instead of inventing a commit message ad hoc.
- Use `skills/caveman-commit/SKILL.md` to draft commit messages, then keep the final message Conventional Commits compliant.
- Preserve the existing non-destructive git rules in this file when handling commit requests.

## Core Priorities

- Correctness and reliability first.
- Prefer small, focused modules over large monolithic systems.
- Avoid architecture that encourages hallucinated, stale, cross-tenant, or partial state.

## Maintainability

- Long-term maintainability is a core requirement.
- Do not hesitate to refactor existing code when that produces a cleaner system.