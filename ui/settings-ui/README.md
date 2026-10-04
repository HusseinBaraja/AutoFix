# Settings UI

WPF settings application for AutoFix.

The completed product pages autosave validated config and notify the background
engine. Save and validation results appear below every page. Invalid config edits
remain visible without replacing the last saved config.

Search is available across the top of the window. Ctrl+F focuses it and selects
the current query; Escape in the search box or Clear search restores browsing.
Matching is case-insensitive and requires all query words. Setting names,
descriptions, option labels and config paths are searchable, including nested
thresholds and queue behaviors (for example `word count threshold`,
`pending queue cancel oldest` or `api.timeout_auto_ms`). Matching sections remain
in the sidebar, individual nonmatching settings are hidden, and matching terms
are highlighted. Section-name searches expose that whole section. A no-results
view replaces stale content and offers a clear action. Searching never removes
config fields or changes saved values; edits in filtered results still save the
complete config. Active search is reapplied after config load or import.

Dictionary editing, per-trigger app rules, secure API-key management, automatic
language detection and the log viewer have indexed names and descriptions too.
Search indexes public UI metadata only, never keys, configured values, dictionary
contents, app-rule rows, typed text or log payloads.

**Privacy & Security** contains the existing arbitrary-selection and clipboard
switches, plus read-only explanations of unconditional secure-field blocking,
current-session scope and caret safety. These controls use the same config paths
and defaults as before. All product sections, including Context's nested limits,
are searchable.

**App Rules** owns the global blocklist/allowlist run mode. Its table exposes each
app's list behavior, three trigger permissions, local/API permissions and prose
safety. Add a process name and optional window-title pattern; identity columns are
read-only to prevent edits from leaving old rules behind. Delete and recreate a
rule to change its scope. New offline databases receive terminal/editor and
sensitive-app safety defaults once. Reset defaults restores the complete list.
Existing choices and deletions are preserved on later settings loads.

**Engines** configures local/API selection, provider preset, optional
OpenAI-compatible base URL, model, manual/automatic timeouts, retries, temperature
and local fallback. Selecting a named provider clears a previous custom endpoint
so it cannot silently override that provider. Keys can be saved, replaced or
removed through a masked editor backed by Windows Credential Manager, using the
same profile targets and UTF-8 bytes as the engine. Keys never enter TOML, SQLite,
IPC or export. The editor clears after save, on provider changes and when hidden;
saved keys are never revealed.

**Languages** owns the global preference, per-app language overrides, uncertain
language policy and mixed-language policy. Empty preference uses automatic
detection by default; Use automatic detection clears it. Per-app overrides take
priority and are preserved when returning to automatic detection. Language tags
and duplicate app scopes are validated using the shared config contract.

**Logs / Debug** reads the latest 500 correction metadata rows from SQLite,
newest first, with UTC timestamps. It never selects debug messages or typed-text
payloads. Refresh works while the engine is stopped. Clear logs requires
confirmation and atomically deletes correction metadata and debug events while
preserving dictionary entries and app rules. New events may arrive while the
engine is running. Enabling redacted or full-text diagnostics enables the parent
debug switch; turning that switch off disables both. Enabling full-text debug
requires explicit acceptance of a privacy warning, including when importing a
config that enables it. Declining import preserves existing settings. These
controls configure the existing engine logging policy; they add no text capture.

**Advanced → Settings import/export** creates a portable ZIP bundle. It always
includes `settings.toml`, app rules (including trigger and engine permissions),
custom protected words/phrases with language/app scopes, and effective per-app
language overrides. TOML overrides take priority over legacy SQLite language
overrides; export puts the effective list in both TOML and the language file.
**Include learned rules in export** defaults off. Enabling it includes the
separate learned-rule table, including disabled rules and saved rejected text
pairs. Protected originals saved by dictionary-style learning already live in
the custom dictionary, which has no manual/learned provenance; these remain
dictionary entries in the bundle. The checkbox controls pair-rule storage.

Export serializes only typed settings and explicitly selected product columns.
It never copies SQLite files, credentials, logs, session buffers, undo records or
correction-history text. Unknown fields from legacy TOML are not retained. API
endpoint validation rejects embedded credentials, query strings and fragments.
Secure provider keys remain in Windows Credential Manager and must be configured
separately on another device. Export cannot overwrite the live config or database.

Import accepts a ZIP bundle or a legacy settings-only TOML file. It validates
every imported setting, row, language and scope before displaying a read-only
preview with current/imported values and additions/removals. Long cell values
are available in tooltips; learned-pair text is hidden in the preview. ZIP imports
replace settings, app rules, dictionary and language overrides. An included
learned-rule file replaces those rules; omission preserves them. TOML imports
leave all SQLite product rows intact. Cancel or Escape makes no changes. Full-text
debug imports require the separate explicit privacy warning after Apply import.

The reviewed payload stays in memory, so changes to the source file cannot alter
the confirmed import. If saved settings or participating product data change
after preview, import rejects the stale preview. The preview diff and fingerprint
use the same settings-file snapshot. SQLite replacements use one transaction;
handled file/database failures roll back rows and restore replaced settings. If
restoration fails, the error includes the original failure and the path to a
retained backup of the original settings. Cleanup failures also preserve the
original error. Imports also persist a recovery record before replacing settings.
The record chooses the original settings until SQLite commits the imported rows;
that same commit chooses the imported settings. WPF and the Rust engine reconcile
any surviving record before loading settings. Settings access uses a shared
Windows file lock, and correction admission, outbound sends and replacements
refuse pending recovery. File contents are flushed and replaced with write-through
semantics before the record is cleared. Interrupted imports therefore recover
the settings and participating product rows to the same committed generation,
including when the settings file is missing. Failed recovery retains the record
and blocks correction until a later attempt succeeds. A stale save that triggers
recovery requires reloading settings; IPC edits read the latest recovered config.
The journal is local recovery data and is never included in export.
After success the UI refreshes and requests an engine config reload,
even if Windows startup registration fails. With the engine stopped, settings
take effect on its next start. Existing credentials and diagnostic/history tables
are untouched. Engine migration versions remain owned by the engine.

Bundle format version 1 contains exactly these files (UTF-8):

| File | Payload |
| --- | --- |
| `manifest.json` | `format_version: 1`, `learned_rules_included: bool` |
| `settings.toml` | Validated settings contract |
| `app-rules.json` | Array of app-rule IPC fields; no IDs or timestamps |
| `dictionary.json` | Array of `word`, `language`, nullable `app` |
| `language-overrides.json` | Array of `process`, `language`; agrees with TOML |
| `learned-rules.json` (optional) | Array of `enabled`, `original`, nullable `replacement`, `rule_type`, nullable `language` and `app` |

Import rejects unknown archive members/JSON fields, duplicate files/scopes,
unsupported versions, inconsistent manifests, malformed entries and invalid
settings. It reads members without extracting paths. Files are limited to 8 MiB
each, and every rule list to 10,000 entries. Export uses the same limits and writes
the destination only after the complete bundle succeeds.

**App Rules** offers **Safety** (`auto`, `terminal`, `code_editor`) and
**Editor prose**. New rules and terminal/editor defaults disable all triggers.
Enable Manual and, for editors, Editor prose to admit selected prose. Known app
detection remains active under auto. Heuristics reject commands, flags, paths,
URLs, code, identifiers and uncertain text. Automatic editor line comments also
need Words/Chars permission; terminal automatic requests skip. Migration preserves
existing choices and defaults Editor prose to off. Native selected-text replacement
still requires proof of the live caret end.

**Shortcuts** configures correction and app-level undo hotkeys (undo defaults to
Ctrl+Alt+Z), plus undo history capacity (default 10, range 1–1000). Capacity saves
as `context.undo_history_size`; legacy settings keep the default. History itself
is memory-only and deleted with its session.

This component owns settings mode:

- Flow Launcher-style settings window.
- Structured settings editing.
- Communication with the background process.
- User-facing configuration for triggers, correction behavior, dictionaries, app rules, privacy, and engine selection.

**Feedback** exposes seven independent switches. Tray states, manual blocked-action
notices and manual API timeout notices default on. Applied notices, skipped reasons,
medium-confidence suggestion previews and near-caret placement default off. Every
switch round-trips through TOML and IPC, including older files without the new
`feedback.show_near_caret_overlay` key. Near-caret placement changes only where an
enabled notice appears. The tray polls metadata-only engine state and changes a
small icon badge and tooltip; it stays available when state is disabled or the
engine cannot restart. Correction feedback never uses modal dialogs or balloons.

The UI should remain a thin product surface over explicit config and IPC contracts from `shared-schema`. Background correction behavior belongs in `app-core-rust`.

The **Dictionary** section lists and edits SQLite word/phrase exclusions and
pair-specific rules. New entry, Save, Delete selected, and Refresh work with the
background process running or stopped. Language uses a BCP 47 tag (`und` for all
languages); blank app scope means all apps. Invalid edits preserve the saved row.
The engine sees saved exclusions on future requests and rechecks before mutation.
Learning controls round-trip through TOML: off (default), ask after AutoFix undo,
or automatic; protect the original or only the rejected pair; optionally scope
new learned entries to the current app. Refresh shows newly learned entries.
The optional consent prompt is owned by the engine and works while settings are
closed. Native Ctrl+Z and the planned suggestion UI do not supply rejections.

Correction settings offer typos-only and typos-plus-grammar modes. Grammar
category switches are available in grammar mode; the saved enabled list is
empty in typos-only mode. An older `punctuation` setting loads as `spacing`.
Language controls allow an optional global BCP 47 preference, comma-separated
per-app entries (`process.exe=language-tag`), a policy for unknown text, and a
separate mixed-language policy. Mixed text defaults to dominant-language-only,
high-confidence typo edits. Correction can be disabled on mixed text; per-token
correction requires the API engine.

Confidence controls expose high and medium behavior (`silent`, `suggestion`,
or `do_nothing`). High defaults to silent apply. Medium defaults to manual
suggestions when the Feedback preview switch is enabled; automatic triggers do
nothing unless silent apply is selected. Previews are read-only and never accept
or apply edits. Low confidence is fixed to do nothing in a disabled
control, and importing other low-confidence behaviors is rejected.

Context settings expose the pending correction queue. Capacity defaults to one
and accepts 1 through 16 running or waiting corrections per session. The full
queue choice defaults to skipping the new automatic trigger; alternatives cancel
the oldest correction or merge the newest pending segment with current typing
and wait for the next trigger. These values round-trip through TOML and IPC.

API retry count accepts zero or one when saving. Loading or importing older
settings reduces counts above one to one so settings remain accessible.
Loading leaves the file unchanged; saving or importing persists the normalized value.
