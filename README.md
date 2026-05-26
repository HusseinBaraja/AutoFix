# AutoFix

AutoFix is a native Windows typo-correction app for text fields. It is designed to run quietly from the system tray, correct only the text the user typed in the current session, and avoid changing text after the caret.

Planned capabilities include:

- Configurable shortcut, word-count, and character triggers.
- Local or API-backed correction engines.
- Typos-only and typos-plus-grammar modes.
- Custom dictionaries, app rules, blocklists, and allowlists.
- Clipboard preservation, app-level undo, and secure-field blocking.

This repository is currently in setup stage.

## Run App

From the repository root, build and run AutoFix:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\run-app.ps1
```

`Autofix.exe` owns the Windows tray icon and supervises the background engine.
The same executable hosts every normal runtime role so Windows can group the
processes together:

```powershell
.\ui\settings-ui\bin\Debug\net8.0-windows\Autofix.exe
.\ui\settings-ui\bin\Debug\net8.0-windows\Autofix.exe --engine
.\ui\settings-ui\bin\Debug\net8.0-windows\Autofix.exe --shutdown-all
```

The Rust engine is loaded through `autofix_core.dll`; `AF-BG-Engine.exe` is only
kept as a direct Rust dev entry point. AutoFix keeps running until you choose
`Exit` from the tray menu.

The app creates its settings file at:

```text
%LOCALAPPDATA%\AutoFix\settings.toml
```

## Repository Structure

- `app-core-rust/` - Rust background engine and shared core logic.
- `ui/settings-ui/` - WPF settings application.
- `shared-schema/` - Shared config schemas, IPC contracts, and documentation.
- `installer/` - Installer scripts and packaging assets later.
- `docs/` - Architecture and design notes.

## Rust testing

Use `#[cfg(test)]` for test modules, unit tests placed next to the code, and helper functions or mock types that exist only for tests. Do not use it for integration tests in the `tests/` directory, normal production logic, public APIs, or alternate implementations that make code behave differently in tests than it does in real builds.

## When you are unsure

use context7 to get the official documentation and check official internet sources if you need them.
