# Start one isolated local-engine check. Never changes normal settings or target drafts.
[CmdletBinding()]
param(
    [ValidateSet('Manual', 'WordCount', 'Character')]
    [string] $Mode = 'Manual',
    [switch] $Stop
)

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$recordPath = Join-Path $repoRoot 'target\writing-app-check-active.json'

if (Test-Path -LiteralPath $recordPath) {
    $record = Get-Content -LiteralPath $recordPath -Raw | ConvertFrom-Json
    $owned = Get-Process -Id $record.process_id -ErrorAction SilentlyContinue
    if ($owned) {
        $recordedStart = ([datetime] $record.started_utc).ToUniversalTime()
        if ($owned.Path -ne $record.executable -or $owned.StartTime.ToUniversalTime() -ne $recordedStart) {
            throw 'Recorded PID belongs to a different process. No process was stopped.'
        }
        if (!$Stop) { throw 'The owned check is still running. Run this script with -Stop first.' }
        $signal = [System.Threading.EventWaitHandle]::OpenExisting("Local\AutoFix.EngineStop.$($owned.Id)")
        try { [void] $signal.Set() } finally { $signal.Dispose() }
        if (!$owned.WaitForExit(6000)) { throw 'Engine cleanup is still pending. No process was force-killed.' }
    }
}
if ($Stop) {
    Write-Host 'Owned check stopped. Its logs remain under target.'
    return
}

# Older binaries predate the runtime lease. Refuse competing shells/test hosts too.
$checkSession = (Get-Process -Id $PID).SessionId
$competing = Get-Process | Where-Object { $_.SessionId -eq $checkSession -and ($_.ProcessName -eq 'Autofix' -or $_.ProcessName -like 'autofix_core-*') }
if ($competing) { throw 'Exit the existing AutoFix shell/diagnostic engine before starting this isolated check.' }

$executable = Join-Path $repoRoot 'ui\settings-ui\bin\Debug\net8.0-windows\Autofix.exe'
if (!(Test-Path -LiteralPath $executable)) { throw 'Build AutoFix.sln before starting this check.' }
$testRoot = Join-Path $repoRoot ('target\writing-app-check-' + [guid]::NewGuid().ToString('N'))
$settingsDirectory = Join-Path $testRoot 'AutoFix'
New-Item -ItemType Directory -Path $settingsDirectory -Force | Out-Null

$testSettings = @'
[general]
start_with_windows = false
run_mode = "blocklist"
[shortcuts]
correct = "Ctrl+Alt+Space"
undo = "Ctrl+Alt+Z"
correct_arbitrary_selection = false
[triggers]
word_count_enabled = false
word_count = 2
character_trigger_enabled = false
characters = ["."]
[context]
undo_history_size = 10
pending_queue_size = 1
pending_queue_full_behavior = "skip_new"
initial_context_words = 25
initial_context_boundary_chars = ["."]
forward_movement_word_limit = 5
informative_context_max_chars = 2000
informative_context_min_words = 25
executable_context_max_words = 80
[correction]
enabled = true
mode = "typos_only"
engine = "local"
preferred_language = "en"
high_confidence_behavior = "silent"
medium_confidence_behavior = "suggestion"
low_confidence_behavior = "do_nothing"
enabled_grammar_categories = []
[replacement]
clipboard_enabled = true
[learning]
mode = "off"
rule = "pair"
per_app = false
[api]
provider_preset = "openai_compatible"
model = "gpt-4.1-mini"
timeout_manual_ms = 3000
timeout_auto_ms = 700
retry_count = 1
fallback_to_local = false
temperature = 0.0
streaming = false
[feedback]
tray_state_enabled = false
show_correction_applied_notification = false
show_skipped_reason = true
show_medium_confidence_suggestions = false
show_blocked_app_notice = true
show_timeout_notice = true
show_near_caret_overlay = false
[logging]
metadata_only_logs_enabled = true
debug_mode_enabled = false
redacted_debug_mode_enabled = false
full_text_debug_mode_enabled = false
'@
if ($Mode -eq 'WordCount') { $testSettings = $testSettings.Replace('word_count_enabled = false', 'word_count_enabled = true') }
if ($Mode -eq 'Character') { $testSettings = $testSettings.Replace('character_trigger_enabled = false', 'character_trigger_enabled = true') }
Set-Content -LiteralPath (Join-Path $settingsDirectory 'settings.toml') -Value $testSettings -Encoding UTF8
$stdout = Join-Path $testRoot 'engine.log'
$stderr = Join-Path $testRoot 'engine-error.log'
$previousLocalData = $env:LOCALAPPDATA
$previousDiagnostics = $env:AUTOFIX_DIAGNOSTICS
try {
    $env:LOCALAPPDATA = $testRoot
    $env:AUTOFIX_DIAGNOSTICS = '1'
    $engine = Start-Process -FilePath $executable -ArgumentList '--engine' -WorkingDirectory (Split-Path $executable) -WindowStyle Hidden -RedirectStandardOutput $stdout -RedirectStandardError $stderr -PassThru
} finally {
    $env:LOCALAPPDATA = $previousLocalData
    $env:AUTOFIX_DIAGNOSTICS = $previousDiagnostics
}
$engine.Refresh()
if ($engine.WaitForExit(1500)) { throw "Engine did not stay running. See $stderr (another engine may already own the lease)." }
[pscustomobject]@{
    process_id = $engine.Id
    executable = $executable
    started_utc = $engine.StartTime.ToUniversalTime().ToString('o')
    mode = $Mode
    log = $stdout
    error_log = $stderr
    settings = (Join-Path $settingsDirectory 'settings.toml')
} | ConvertTo-Json | Set-Content -LiteralPath $recordPath -Encoding UTF8
Write-Host "Owned $Mode check running: PID $($engine.Id)"
Write-Host "Metadata log: $stdout"
Write-Host 'Type into an unsent disposable draft using your physical keyboard. Do not send it.'
Write-Host 'Stop with: .\scripts\writing-app-check.ps1 -Stop'
