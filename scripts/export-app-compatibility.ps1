[CmdletBinding()]
param(
    [string] $OutputDirectory = (Join-Path $PSScriptRoot '..\target\app-compatibility'),
    [string] $InventoryFile
)

$ErrorActionPreference = 'Stop'
$catalog = Get-Content -LiteralPath (Join-Path $PSScriptRoot '..\shared-schema\windows-writing-apps.json') -Raw | ConvertFrom-Json
if ($catalog.scope -ne 'installed_laptop' -or !$catalog.apps.Count) { throw 'Expected a nonempty laptop writing scope.' }
if (($catalog.apps.id | Sort-Object -Unique).Count -ne $catalog.apps.Count) { throw 'Duplicate writing catalog IDs.' }

$inventory = [System.Collections.Generic.List[object]]::new()
$limitations = [System.Collections.Generic.List[string]]::new()
if ($InventoryFile) {
    # Optional recorded inventory makes the exporter reproducible and testable.
    $recorded = Get-Content -LiteralPath $InventoryFile -Raw | ConvertFrom-Json
    foreach ($entry in $recorded.inventory) { $inventory.Add($entry) }
    foreach ($entry in $recorded.limitations) { $limitations.Add([string]$entry) }
}
else {
    $uninstallRoots = @(
        'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*',
        'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*',
        'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*'
    )
    foreach ($entry in (Get-ItemProperty $uninstallRoots -ErrorAction SilentlyContinue)) {
        if ($entry.DisplayName -and !$entry.SystemComponent -and !$entry.ParentKeyName) {
            $inventory.Add([pscustomobject]@{ name = $entry.DisplayName; version = $entry.DisplayVersion; source = 'uninstall'; identity = '' })
        }
    }
    try {
        foreach ($entry in (Get-StartApps -ErrorAction Stop)) {
            if ($entry.Name -notmatch '^(Uninstall|Documentation|Visit |Check For Updates)') {
                $inventory.Add([pscustomobject]@{ name = $entry.Name; version = ''; source = 'start_menu'; identity = $entry.AppID })
            }
        }
    }
    catch { $limitations.Add('Start menu inventory unavailable.') }
    try {
        foreach ($entry in (Get-AppxPackage -ErrorAction Stop | Where-Object { !$_.IsFramework -and !$_.NonRemovable })) {
            $inventory.Add([pscustomobject]@{ name = $entry.Name; version = [string]$entry.Version; source = 'package'; identity = $entry.PackageFamilyName })
        }
    }
    catch { $limitations.Add('Packaged-app inventory unavailable.') }
    $notepad = Join-Path $env:WINDIR 'System32\notepad.exe'
    if (Test-Path -LiteralPath $notepad -PathType Leaf) {
        $inventory.Add([pscustomobject]@{ name = 'Notepad'; version = ''; source = 'system_executable'; identity = 'notepad.exe' })
    }
    $limitations.Add('Portable apps without registrations and authenticated web services may not be detected. Detection does not prove text-control support. Run from your normal Windows user session, not an isolated sandbox profile.')
}

$inventory = @($inventory | Sort-Object name, source, identity -Unique)
$coverage = @(foreach ($app in $catalog.apps) {
    $namePatterns = @(@($app.name) + @($app.name_aliases) | Where-Object { $_ } | ForEach-Object {
        '^' + [regex]::Escape($_) + '(?:\s+\d|\s*\(|$)'
    })
    $processCandidates = @(@($app.process) + @($app.process_aliases) | Where-Object { $_ -and $_ -ne 'browser' })
    $candidates = @($inventory | Where-Object {
        $registration = $_
        @($namePatterns | Where-Object { $registration.name -match $_ }).Count -gt 0 -or
        ($registration.identity -and @($processCandidates | Where-Object {
            $registration.identity.IndexOf($_, [StringComparison]::OrdinalIgnoreCase) -ge 0
        }).Count -gt 0)
    })
    # Only detected writing apps belong to this laptop's current coverage.
    if (!$candidates.Count) { continue }
    [pscustomobject]@{
        id = $app.id; app = $app.name; category = $app.category; surface = $app.surface;
        host = $app.host; process_candidate = $app.process; policy = $app.policy;
        detected = ($candidates.Count -gt 0); registrations = $candidates;
        verification = 'not_verified'; evidence = @()
    }
})
$report = [pscustomobject]@{
    schema_version = 2; generated_at = [DateTimeOffset]::UtcNow.ToString('o');
    scope = 'Only installed laptop apps with sustained prose typing. No fixed count or category quota. Browser targets cover writing fields, not search or address bars.';
    limitations = @($limitations); inventory = $inventory; coverage = @($coverage)
}
New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
$report | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'inventory.json') -Encoding UTF8
$coverage | Select-Object app, category, surface, host, policy, detected, verification |
    Export-Csv -LiteralPath (Join-Path $OutputDirectory 'coverage.csv') -NoTypeInformation -Encoding UTF8
$lines = [System.Collections.Generic.List[string]]::new()
$lines.Add('# Writing-app compatibility inventory')
$lines.Add('')
$lines.Add('Detected means registered on this machine. No entry is marked supported without real correction and undo evidence. This report never opens apps, reads document text, or sends messages.')
$lines.Add('')
$lines.Add('| App | Group | Writing surface | Detected | Verification |')
$lines.Add('| --- | --- | --- | --- | --- |')
foreach ($row in $coverage) {
    $lines.Add('| ' + $row.app.Replace('|', '\|') + ' | ' + $row.category + ' | ' + $row.surface + ' | ' + $row.detected + ' | NOT VERIFIED |')
}
$lines.Add('')
$lines.Add('## Inventory limitations')
foreach ($row in $limitations) { $lines.Add('- ' + $row) }
$lines | Set-Content -LiteralPath (Join-Path $OutputDirectory 'report.md') -Encoding UTF8
Write-Host ('Writing targets: ' + $coverage.Count + '; detected: ' + @($coverage | Where-Object detected).Count + '; verified: 0')
Write-Host ('Report: ' + (Join-Path $OutputDirectory 'report.md'))
