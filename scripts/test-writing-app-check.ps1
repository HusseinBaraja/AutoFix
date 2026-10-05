# Exercise the production identity guard without launching or stopping a process.
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$scriptPath = Join-Path $PSScriptRoot 'writing-app-check.ps1'
$tokens = $null
$parseErrors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile($scriptPath, [ref] $tokens, [ref] $parseErrors)
if ($parseErrors.Count) { throw "Writing-app script failed parsing: $parseErrors" }
$assignment = $ast.FindAll({
    param($node)
    $node -is [System.Management.Automation.Language.AssignmentStatementAst] -and
    $node.Left.Extent.Text -eq '$recordedStart'
}, $true)
$guard = $ast.FindAll({
    param($node)
    $node -is [System.Management.Automation.Language.IfStatementAst] -and
    $node.Clauses[0].Item1.Extent.Text.StartsWith('$owned.Path -ne $record.executable')
}, $true)
if ($assignment.Count -ne 1 -or $guard.Count -ne 1) { throw 'Expected one production process identity guard.' }
$identityGuard = [scriptblock]::Create($assignment[0].Extent.Text + "`n" + $guard[0].Extent.Text)
$start = [datetime]::Parse('2026-10-05T14:12:07.4830396Z', [cultureinfo]::InvariantCulture, [System.Globalization.DateTimeStyles]::RoundtripKind)
$owned = [pscustomobject]@{ Path = 'C:\AutoFix\Autofix.exe'; StartTime = $start.ToLocalTime() }
$serialized = [pscustomobject]@{ executable = $owned.Path; started_utc = $start.ToString('o') } | ConvertTo-Json
$record = $serialized | ConvertFrom-Json
& $identityGuard
# Cover both older string-decoding and newer DateTime-decoding PowerShell.
foreach ($timestamp in @($start.ToString('o'), $start, $start.ToLocalTime())) {
    $record = [pscustomobject]@{ executable = $owned.Path; started_utc = $timestamp }
    & $identityGuard
}
foreach ($mismatch in @(
    [pscustomobject]@{ executable = 'C:\Other\Autofix.exe'; started_utc = $start },
    [pscustomobject]@{ executable = $owned.Path; started_utc = $start.AddTicks(1) }
)) {
    $record = $mismatch
    $refused = $false
    try { & $identityGuard } catch {
        if ($_.Exception.Message -ne 'Recorded PID belongs to a different process. No process was stopped.') { throw }
        $refused = $true
    }
    if (!$refused) { throw 'The identity guard accepted a different process.' }
}
Write-Host 'Process identity guard passed: JSON/string/date timestamps accepted; path and timestamp mismatches refused.'
