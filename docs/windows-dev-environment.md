# Windows Development Environment

## Installed or Verified

- Rust stable: `rustc --version`, `cargo --version`
- Git: `git --version`
- SQLite CLI: `sqlite3 --version`
- Visual Studio Build Tools with C++ desktop workload:
  `C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe -products * -requires Microsoft.VisualStudio.Workload.VCTools -property installationPath`

## Manual Installs Over 50 MB

Run these from PowerShell when you want the large downloads to proceed:

```powershell
winget install --id Microsoft.DotNet.SDK.8 --exact --accept-package-agreements --accept-source-agreements
winget install --id JRSoftware.InnoSetup --exact --accept-package-agreements --accept-source-agreements
```

Visual Studio Build Tools was already installed in this setup session. If you need to reproduce it on another machine:

```powershell
winget install --id Microsoft.VisualStudio.2022.BuildTools --exact --accept-package-agreements --accept-source-agreements --override "--wait --quiet --norestart --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
```

## Verify

```powershell
rustc --version
cargo --version
git --version
sqlite3 --version
dotnet --info
dotnet build .\AutoFix.sln
cargo test
cargo run -p background-engine
```

Expected Rust coverage:

- Console app builds and runs.
- Win32 call succeeds through `GetForegroundWindow` and `GetWindowTextLengthW`.
- SQLite opens in memory through `rusqlite`.
- TOML parses through `toml`.

Expected WPF coverage:

- `dotnet build .\AutoFix.sln` builds `ui/settings-ui` after the .NET 8 SDK is installed.
