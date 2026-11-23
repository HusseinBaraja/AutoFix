# Runs the end-to-end check: the real AutoFix.exe against a mock AI server, typing into a test window.
# It takes keyboard focus for about 20 seconds. Do not type or click while it runs.
$ErrorActionPreference = "Stop"
$root = Resolve-Path (Join-Path $PSScriptRoot "..")

dotnet build (Join-Path $root "AutoFix.sln") -c Debug
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

dotnet (Join-Path $root "tests\AutoFix.E2E\bin\Debug\net8.0-windows\AutoFix.E2E.dll") `
    (Join-Path $root "src\AutoFix\bin\Debug\net8.0-windows\AutoFix.exe")
exit $LASTEXITCODE
