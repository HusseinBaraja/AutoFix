$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $repoRoot

$runningShell = Get-Process -Name "Autofix" -ErrorAction SilentlyContinue | Select-Object -First 1
if ($runningShell) {
    $settingsShell = Join-Path $repoRoot "ui\settings-ui\bin\Debug\net8.0-windows\Autofix.exe"
    if (Test-Path $settingsShell) {
        Start-Process -FilePath $settingsShell
    }

    Write-Host "AutoFix is already running."
    return
}

& .\scripts\clear-stale-dev-outputs.ps1
dotnet build .\AutoFix.sln
cargo build -p background-engine
cargo run -p background-engine
