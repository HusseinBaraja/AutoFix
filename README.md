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

From the repository root, build the WPF shell and Rust background engine:

```powershell
dotnet build .\AutoFix.sln
cargo build -p background-engine
```

Then start AutoFix through the Rust app entry point:

```powershell
cargo run -p background-engine
```

The Rust entry point launches the WPF shell, and the shell owns the Windows tray
icon and supervises the background engine. It keeps running until you choose
`Exit` from the tray menu.

To build and run the executable directly:

```powershell
cargo build -p background-engine
.\target\debug\AF-BG-Engine.exe
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
