$ErrorActionPreference = "Continue"

function Invoke-Check {
    param(
        [string] $Name,
        [scriptblock] $Command
    )

    Write-Host "==> $Name"
    & $Command
    if ($LASTEXITCODE -ne 0) {
        Write-Host "FAILED: $Name"
        exit $LASTEXITCODE
    }
}

Invoke-Check "Rust toolchain" { rustc --version; cargo --version }
Invoke-Check "Git" { git --version }
Invoke-Check "SQLite CLI" { sqlite3 --version }

$vswhere = "C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe"
if (Test-Path $vswhere) {
    Invoke-Check "Visual Studio Build Tools C++ workload" {
        & $vswhere -products * -requires Microsoft.VisualStudio.Workload.VCTools -property installationPath
    }
} else {
    Write-Host "FAILED: vswhere.exe not found"
    exit 1
}

Invoke-Check "Rust tests" { cargo test }
Invoke-Check "Rust console app" { cargo run -p background-engine }
Invoke-Check ".NET SDK and WPF build" { dotnet build .\AutoFix.sln }
