$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$settingsBin = Join-Path $repoRoot "ui\settings-ui\bin"

if (-not (Test-Path $settingsBin)) {
    return
}

Get-ChildItem -Path $settingsBin -Recurse -Filter "Autofix.exe" | ForEach-Object {
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
