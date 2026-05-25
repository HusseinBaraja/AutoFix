$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $repoRoot

& .\scripts\clear-stale-dev-outputs.ps1
dotnet build .\AutoFix.sln
cargo build -p background-engine
cargo run -p background-engine
