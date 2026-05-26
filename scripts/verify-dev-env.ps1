$ErrorActionPreference = "Stop"

function Invoke-Check {
    param(
        [string] $Name,
        [scriptblock] $Command
    )

    Write-Host "==> $Name"
    $global:LASTEXITCODE = 0
    & $Command
    if ($LASTEXITCODE -ne 0) {
        Write-Host "FAILED: $Name"
        exit $LASTEXITCODE
    }
}

function Get-InnoCompiler {
    $command = Get-Command iscc.exe -ErrorAction SilentlyContinue
    if ($command) {
        return $command.Source
    }

    $candidates = @(
        "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe",
        "$env:ProgramFiles\Inno Setup 6\ISCC.exe",
        "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe"
    )

    foreach ($candidate in $candidates) {
        if (Test-Path $candidate) {
            return $candidate
        }
    }

    return $null
}

Invoke-Check "Rust toolchain" { rustc --version; cargo --version }
Invoke-Check "Git" { git --version }
Invoke-Check "SQLite CLI" { sqlite3 --version }
Invoke-Check ".NET SDK" { dotnet --info }
Invoke-Check "Stale dev shell cleanup" { & .\scripts\clear-stale-dev-outputs.ps1 }

$vswhere = "C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe"
if (Test-Path $vswhere) {
    Invoke-Check "Visual Studio Build Tools C++ workload" {
        $vswhereOutput = & $vswhere -products * -requires Microsoft.VisualStudio.Workload.VCTools -property installationPath
        $vswhereExitCode = $LASTEXITCODE
        if ($vswhereExitCode -ne 0 -or -not $vswhereOutput) {
            $global:LASTEXITCODE = if ($vswhereExitCode -ne 0) { $vswhereExitCode } else { 1 }
            return
        }

        $vswhereOutput
    }
} else {
    Write-Host "FAILED: vswhere.exe not found"
    exit 1
}

$innoCompiler = Get-InnoCompiler
if ($innoCompiler) {
    Invoke-Check "Inno Setup compiler" {
        $version = (Get-Item $innoCompiler).VersionInfo.ProductVersion
        Write-Host "$innoCompiler $version"
    }
} else {
    Write-Host "FAILED: Inno Setup compiler not found"
    exit 1
}

Invoke-Check ".NET SDK and WPF build" { dotnet build .\AutoFix.sln }
Invoke-Check "Rust tests" { cargo test }
Invoke-Check "Rust native library build" { cargo build -p background-engine }
