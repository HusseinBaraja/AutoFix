# Builds and starts AutoFix from source.
$ErrorActionPreference = "Stop"
$root = Resolve-Path (Join-Path $PSScriptRoot "..")

dotnet build (Join-Path $root "src\AutoFix\AutoFix.csproj") -c Debug
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Start-Process (Join-Path $root "src\AutoFix\bin\Debug\net8.0-windows\AutoFix.exe")
