$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $repoRoot

function Invoke-BuildStep {
    param([string] $Name, [scriptblock] $Command)

    Write-Host "==> $Name"
    & $Command
    if ($LASTEXITCODE -ne 0) {
        Write-Host "FAILED: $Name"
        exit $LASTEXITCODE
    }
}

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
Invoke-BuildStep "Rust background-engine build" { cargo build -p background-engine }
Invoke-BuildStep ".NET solution build" { dotnet build .\AutoFix.sln }
& .\ui\settings-ui\bin\Debug\net8.0-windows\Autofix.exe
