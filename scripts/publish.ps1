# Produces dist\AutoFix.exe: one self-contained file that runs on any 64-bit Windows 10/11 PC
# without installing .NET.
param([string] $Runtime = "win-x64")
$ErrorActionPreference = "Stop"
$root = Resolve-Path (Join-Path $PSScriptRoot "..")
$out = Join-Path $root "dist"

dotnet publish (Join-Path $root "src\AutoFix\AutoFix.csproj") -c Release -r $Runtime --self-contained true `
    -p:PublishSingleFile=true -p:IncludeNativeLibrariesForSelfExtract=true -p:EnableCompressionInSingleFile=true `
    -p:DebugType=none -o $out
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$exe = Join-Path $out "AutoFix.exe"
"{0} ({1:N1} MB)" -f $exe, ((Get-Item $exe).Length / 1MB)
