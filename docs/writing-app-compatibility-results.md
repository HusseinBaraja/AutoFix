# Writing-adapter verification — 2026-10-04

Local verification on `app-compatibility`. No push or pull request requested.
These passes establish fixture/provider behavior, not compatibility with every
writing app on this laptop. Scope changed to installed writing apps on 2026-10-05.

| Check | Result | Scope |
| --- | --- | --- |
| Rust library suite | PASS | 437 passed, 8 opt-in tests ignored |
| Clippy | PASS | Workspace, all targets/features, warnings denied |
| Native Edit correction and app undo | PASS | Direct API, UIA at document end, clipboard, SendInput; Unicode, selected ranges, deletion, suffix and newer typing |
| Native automatic trigger flow | PASS | Word-count and character correction/undo through production pipeline |
| Native caret movement | PASS | Pre-caret movement/final correction and undo preserve known text |
| Isolated native clipboard preservation | PASS | Separate noninteractive station; format restoration and recovery; desktop clipboard untouched |
| Native RichEdit API tests | PASS | Owned hidden RichEdit20W and RichEdit50W controls; Unicode, suffix, deletion and inverse recorded-range replacement |
| WPF provider correction/undo | PASS | Separate fixture process; UIA TextBox adapter; Unicode/deletion; read-only and suffix refusal |
| .NET tests | PASS | 241 passed; 1 credential-manager test skipped because logon prerequisites are unavailable in the test environment |
| .NET solution and fixture builds | PASS | Zero warnings/errors |
| Inventory exporter | PASS | Synthetic registration tests; exact count; no false service or support promotion |
| Laptop inventory | COMPLETE WITH LIMITS | Registered apps only; portable apps and service access can be missing; no support inferred |
| WhatsApp composer | UNSUPPORTED BY CURRENT ADAPTER | Access retries succeeded. Focused `Microsoft.UI.Content.DesktopChildSiteBridge` has no TextPattern; descendants report ambiguous focus. No correction or message sending. Physical Escape stopped Computer Use |
| Other actual writing apps | NOT VERIFIED | Per-app/provider/version manual or end-to-end evidence remains required |

Machine-local logs and runner results are under ignored
`target/writing-app-verification`. Local inventory is under
`target/app-compatibility`. Neither is committed.

## Follow-up verification — 2026-10-05

The revised inventory reports 25 installed writing targets. Synthetic exporter
checks verify that absent apps and uninstallers are omitted, browser installations
do not add web-service targets, and no installation promotes compatibility.
The policy matrix checks each laptop target without count or category quotas.

After final field-safety changes, `cargo test --workspace --locked` passes 439
tests with 9 opt-in tests ignored. Clippy passes with all targets/features and
warnings denied. The .NET solution builds with zero warnings/errors. Native Edit
and WPF correction/undo smoke tests both pass on the host desktop. Other actual
app tests have not been completed. No new Computer Use session was started.

The previous [integration report](windows-text-target-integration-results.md)
records an earlier sandbox foreground failure. This run used the normal host
desktop for owned fixtures and passed all five opt-in suites. Actual applications
remain unverified. See [scope and release blockers](writing-app-compatibility.md).

## Composer checkpoint — 2026-10-05

Before Computer Use was stopped with physical Escape, exact disposable WhatsApp
drafts passed correction and AutoFix undo for manual (`teh`), two-completed-word
(`teh word `) and character (`teh.`) triggers. The opt-in live test uses production
pipeline/native code, isolated local policies/storage and the single-engine lease,
but seeds synthetic session input. These are not physical-keyboard/global-hotkey
end-to-end passes. The earlier user-assisted diagnostics did retain three physical
characters; the user also reported a Telegram manual correction/undo pass.

This checkpoint adds bounded context clipping detection, guarded operation-local
composer discovery reuse, hosted field-boundary proof, asynchronous selection
acknowledgment and bounded selected-character substitutions. Revalidation on the
final checkpoint and the remaining cases in [the live-draft table](writing-app-compatibility.md)
are still required before either app is verified fixed.

Commit verification: 457 Rust tests pass, 10 tests ignored; Clippy with all targets
and features passes with warnings denied; the solution builds with zero warnings
or errors; .NET tests pass 241 with one credential-manager prerequisite skip.
The owned native editor correction/undo fixture passes. The WPF fixture failed
before mutation on two attempts with `WPF fixture could not acquire authorized
focus`; its live verification remains unresolved. Its suffix oracle now reflects
the selected-character capability, while read-only refusal remains required.
No actual chat messages were sent. Machine-local diagnostics remain ignored.
