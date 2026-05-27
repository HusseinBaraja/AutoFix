$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$settingsBin = Join-Path $repoRoot "ui\settings-ui\bin"
$targetDebug = Join-Path $repoRoot "target\debug"

if (-not (Test-Path $settingsBin)) {
    return
}

$targetDebugShell = Join-Path $targetDebug "Autofix.exe"
if ((Test-Path $targetDebugShell) -and -not (Test-Path (Join-Path $targetDebug "Autofix.dll"))) {
    Write-Host "Removing stale dev shell: $targetDebugShell"
    Remove-Item -LiteralPath $targetDebugShell -Force
}

$settingsDevOutputRoots = Get-ChildItem -Path (Join-Path $settingsBin "Debug*") -Directory -ErrorAction SilentlyContinue

$settingsDevOutputRoots | ForEach-Object {
    Get-ChildItem -Path $_.FullName -Recurse -Filter "Autofix.exe" | ForEach-Object {
        $lowerPath = $_.FullName.ToLowerInvariant()
        if (
            $lowerPath.Contains("\publish\") -or
            $lowerPath.Contains("\bin\release\") -or
            $lowerPath.Contains("\release\")
        ) {
            return
        }

        $settingsDllCandidates = @(
            (Join-Path $_.DirectoryName "Autofix.dll"),
            (Join-Path $_.DirectoryName "AutoFix.SettingsUi.dll")
        )
        $hasSettingsDll = $settingsDllCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1
        if (-not $hasSettingsDll) {
            Write-Host "Removing stale dev shell: $($_.FullName)"
            Remove-Item -LiteralPath $_.FullName -Force
        }
    }
}
