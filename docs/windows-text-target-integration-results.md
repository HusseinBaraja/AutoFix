# Integration verification — 2026-10-04

Verification of the scenarios/runner added on `windows-integration-scenarios`.
This is automated harness verification, not certification of real target apps.

| Check | Result | Evidence / limitation |
| --- | --- | --- |
| Full deterministic Rust library suite | PASS | 430 passed, 0 failed, 7 opt-in tests ignored; includes five new `windows_target` scenarios |
| Isolated native clipboard preservation/recovery | PASS | One native test passed, covering both child shutdown outcomes |
| Rust engine build | PASS | `cargo build -p background-engine` |
| WPF target fixture build | PASS | `dotnet build tests/windows-targets/wpf/AutoFix.TextTargetFixture.csproj`; zero warnings/errors |
| Runner failure reporting | PASS | Stubbed cargo output with zero native matches was rejected; failed suite plus all unexecuted suites recorded |
| Interactive native Edit correction/undo | NOT VERIFIED | Existing smoke failed at foreground acquisition: `test editor could not acquire foreground focus`; replacement assertions were not reached |
| Interactive native automatic trigger / caret movement | NOT RUN | Runner stopped after foreground failure; requires unlocked interactive desktop with foreground permission |
| Manual cases in actual target apps | NOT RUN | Use the [manual matrix](windows-text-target-integration.md); no actual app/control pass claimed |

Runner logs and machine-readable results are generated under
`target/windows-integration-results` (deterministic + isolated clipboard) and
`target/windows-integration-native-results` (desktop attempt). These directories
are ignored build artifacts and are not committed. The runner preserves a
failure exit code; the desktop limitation was not converted into a pass.
