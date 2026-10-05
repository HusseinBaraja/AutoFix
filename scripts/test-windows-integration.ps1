# Desktop tests own their editor but briefly change focus. They are opt-in.
[CmdletBinding()]
param(
    [switch] $NativeDesktop,
    [switch] $WpfDesktop,
    [switch] $IsolatedClipboard,
    [string] $ResultsDirectory = (Join-Path ([System.IO.Path]::GetTempPath()) ("AutoFix-integration-" + [guid]::NewGuid().ToString("N")))
)

$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$resultsPath = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($ResultsDirectory)
New-Item -ItemType Directory -Path $resultsPath -Force | Out-Null
$results = [System.Collections.Generic.List[object]]::new()

function Invoke-IntegrationSuite {
    param([string] $Name, [string[]] $CargoArguments, [switch] $SingleTest)
    $log = Join-Path $resultsPath ($Name + ".log")
    Write-Host "Running $Name"
    # PowerShell 5 treats native stderr as an error record. Capture it without
    # turning cargo progress messages into terminating script errors.
    $savedPreference = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        & cargo @CargoArguments 2>&1 | ForEach-Object { $_.ToString() } | Tee-Object -FilePath $log | Out-Host
        $code = $LASTEXITCODE
    }
    finally { $ErrorActionPreference = $savedPreference }
    # Cargo succeeds with zero matching tests. Do not report that as a native pass.
    $matched = !$SingleTest -or ((Get-Content -LiteralPath $log -Raw) -match 'test result: ok\. 1 passed;')
    $results.Add([pscustomobject]@{
        suite = $Name
        status = $(if ($code -eq 0 -and $matched) { "passed" } else { "failed" })
        exitCode = $code
        command = "cargo " + ($CargoArguments -join " ")
        log = $log
    })
    if ($code -ne 0) { throw "$Name failed with exit code $code. See $log" }
    if (!$matched) { throw "$Name did not execute exactly one passing test. See $log" }
}

Push-Location $repoRoot
try {
    Invoke-IntegrationSuite "deterministic" @("test", "-p", "background-engine", "--lib")
    $native = @(
        "background::replacement::tests::native_edit_replacement_smoke",
        "background::replacement::tests::native_automatic_trigger_smoke",
        "background::replacement::tests::native_caret_movement_smoke"
    )
    foreach ($test in $native) {
        $name = $test.Split(":")[-1]
        if ($NativeDesktop) {
            Invoke-IntegrationSuite $name @("test", "-p", "background-engine", "--lib", $test, "--", "--exact", "--ignored", "--nocapture", "--test-threads=1") -SingleTest
        }
        else { $results.Add([pscustomobject]@{ suite = $name; status = "not_run"; reason = "Enable -NativeDesktop on an unlocked Windows desktop" }) }
    }
    $clipboard = "background::replacement::clipboard::windows::tests::native_clipboard_preservation_smoke"
    if ($IsolatedClipboard) {
        Invoke-IntegrationSuite "native_clipboard_preservation_smoke" @("test", "-p", "background-engine", "--lib", $clipboard, "--", "--exact", "--ignored", "--nocapture", "--test-threads=1") -SingleTest
    }
    else { $results.Add([pscustomobject]@{ suite = "native_clipboard_preservation_smoke"; status = "not_run"; reason = "Enable -IsolatedClipboard on Windows" }) }
    if ($WpfDesktop) {
        $fixtureProject = Join-Path $repoRoot 'tests\windows-targets\wpf\AutoFix.TextTargetFixture.csproj'
        & dotnet build $fixtureProject
        if ($LASTEXITCODE -ne 0) { throw 'WPF fixture build failed.' }
        $previousFixture = $env:AUTOFIX_WPF_FIXTURE
        try {
            $env:AUTOFIX_WPF_FIXTURE = Join-Path $repoRoot 'tests\windows-targets\wpf\bin\Debug\net8.0-windows\AutoFix.TextTargetFixture.exe'
            Invoke-IntegrationSuite 'wpf_uia_replacement_and_app_undo' @('test', '-p', 'background-engine', '--lib', 'background::replacement::wpf_tests::wpf_uia_replacement_and_app_undo', '--', '--exact', '--ignored', '--nocapture', '--test-threads=1') -SingleTest
        }
        finally { $env:AUTOFIX_WPF_FIXTURE = $previousFixture }
    }
    else { $results.Add([pscustomobject]@{ suite = 'wpf_uia_replacement_and_app_undo'; status = 'not_run'; reason = 'Enable -WpfDesktop on an unlocked Windows desktop' }) }
}
finally {
    Pop-Location
    # Keep a complete report even when a failed suite stops the run early.
    foreach ($name in @("deterministic", "native_edit_replacement_smoke", "native_automatic_trigger_smoke", "native_caret_movement_smoke", "native_clipboard_preservation_smoke", "wpf_uia_replacement_and_app_undo")) {
        if (!($results | Where-Object { $_.suite -eq $name })) {
            $results.Add([pscustomobject]@{ suite = $name; status = "not_run"; reason = "Run stopped before this suite" })
        }
    }
    ConvertTo-Json -InputObject @($results.ToArray()) -Depth 4 | Set-Content -LiteralPath (Join-Path $resultsPath "results.json") -Encoding UTF8
    Write-Host "Integration results: $resultsPath"
}
