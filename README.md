# AutoFix

AutoFix is a native Windows typo-correction app for text fields. It is designed to run quietly from the system tray, correct only the text the user typed in the current session, and avoid changing text after the caret.

Planned capabilities include:

- Configurable shortcut, word-count, and character triggers.
- Local or API-backed correction engines.
- Typos-only and typos-plus-grammar modes.
- Custom dictionaries, app rules, blocklists, and allowlists.
- Clipboard preservation, app-level undo, and secure-field blocking.

This repository is currently in setup stage.

## Run Background App

From the repository root, run the Rust background app with:

```powershell
cargo run -p background-engine
```

The app starts the background process and Windows tray icon. It keeps running until
you choose `Exit` from the tray menu or stop it with `Ctrl+C` in the terminal.

To build and run the executable directly:

```powershell
cargo build -p background-engine
.\target\debug\background-engine.exe
```

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
