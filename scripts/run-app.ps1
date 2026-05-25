$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $repoRoot

dotnet build .\AutoFix.sln
cargo build -p background-engine
cargo run -p background-engine
