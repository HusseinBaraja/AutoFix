[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$work = Join-Path $repoRoot ('target\compatibility-export-test-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $work -Force | Out-Null
$inputPath = Join-Path $work 'recorded.json'
@{
    inventory = @(
        @{ name = 'WhatsApp'; version = '1'; source = 'start_menu'; identity = '5319275A.WhatsAppDesktop!App' },
        @{ name = 'WhatsApp Beta'; version = '2'; source = 'start_menu'; identity = 'WhatsAppBeta!App' },
        @{ name = 'WebStorm 2026.1.3'; version = '2026.1.3'; source = 'uninstall'; identity = '' },
        @{ name = 'Word'; version = ''; source = 'start_menu'; identity = 'Microsoft.Office.WINWORD.EXE.15' },
        @{ name = 'OBS Studio'; version = '1'; source = 'uninstall'; identity = 'obs64.exe' },
        @{ name = 'Telegram Uninstaller'; version = '1'; source = 'start_menu'; identity = 'uninstall.exe' },
        @{ name = 'Microsoft Edge'; version = '1'; source = 'start_menu'; identity = 'msedge.exe' }
        @{ name = 'T3 Code (Alpha) 0.0.37'; version = '0.0.37'; source = 'uninstall'; identity = '' }
    )
    limitations = @('Synthetic inventory; no real app verification.')
} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $inputPath -Encoding UTF8
& (Join-Path $PSScriptRoot 'export-app-compatibility.ps1') -InventoryFile $inputPath -OutputDirectory (Join-Path $work 'report')
$report = Get-Content -LiteralPath (Join-Path $work 'report\inventory.json') -Raw | ConvertFrom-Json
function Assert-Result([bool] $Condition, [string] $Message) {
    if (!$Condition) { throw $Message }
}
Assert-Result ($report.coverage.Count -eq 6) 'Coverage must contain only the six installed writing targets in this fixture.'
Assert-Result (@($report.coverage | Where-Object { $_.verification -ne 'not_verified' -or $_.evidence.Count -ne 0 }).Count -eq 0) 'Inventory must never promote verification.'
foreach ($id in @('whatsapp', 'whatsapp-beta', 'webstorm', 'word', 't3-code', 'edge')) {
    Assert-Result ([bool]($report.coverage | Where-Object id -eq $id).detected) "Missing registered writing target: $id"
}
Assert-Result (@($report.coverage | Where-Object id -eq 'telegram').Count -eq 0) 'An uninstaller is not the writing application.'
Assert-Result (@($report.coverage | Where-Object { $_.host -ne 'desktop' -or $_.id -in @('gmail','google-docs','outlook-web') }).Count -eq 0) 'Browser registration cannot prove a writing service.'
Assert-Result (@($report.coverage | Where-Object app -eq 'OBS Studio').Count -eq 0) 'Search-only utilities must not be writing targets.'
$csv = @(Import-Csv -LiteralPath (Join-Path $work 'report\coverage.csv'))
Assert-Result ($csv.Count -eq 6) 'CSV must match installed writing scope.'
Assert-Result (@($report.limitations | Where-Object { $_ -eq 'Synthetic inventory; no real app verification.' }).Count -eq 1) 'Recorded limitations must survive export.'
Write-Host 'Compatibility exporter: all checks passed.'
