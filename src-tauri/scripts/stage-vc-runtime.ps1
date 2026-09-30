<#
.SYNOPSIS
    Stages the Microsoft Visual C++ runtime DLLs into src-tauri/vc-runtime,
    so the Windows bundle ships them next to the app (app-local deployment).

.DESCRIPTION
    The app needs msvcp140.dll (C++ standard library): the RAR support
    (unrar) is C++, and the STEP variant's OCCT DLLs are built against the
    dynamic C++ runtime. A fresh Windows installation does not have it, so
    without these files the app does not start there ("MSVCP140.dll was not
    found"). Microsoft allows redistributing these files next to the
    application; they are taken from the Visual Studio installation that
    builds the app, so they match the compiler.

    Run before `npm run tauri dev` or `npm run tauri build` on Windows (CI does
    this in build-windows*.yml); tauri.windows.conf.json requires the files. The files are not committed
    (see src-tauri/.gitignore).
#>
$ErrorActionPreference = "Stop"

$vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
if (-not (Test-Path $vswhere)) { throw "vswhere.exe not found - is Visual Studio (Build Tools) installed?" }
$vs = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (-not $vs) { throw "No Visual Studio installation with the C++ tools found." }

# VC\Redist\MSVC\<version>\x64\Microsoft.VC14x.CRT - take the newest version.
$crt = Get-ChildItem -Path (Join-Path $vs "VC\Redist\MSVC") -Directory |
    Where-Object { $_.Name -match '^\d+\.\d+\.\d+' } |
    Sort-Object { [version]($_.Name -replace '^(\d+\.\d+\.\d+).*', '$1') } -Descending |
    ForEach-Object { Get-ChildItem -Path (Join-Path $_.FullName "x64") -Directory -Filter "Microsoft.VC14*.CRT" -ErrorAction SilentlyContinue } |
    Select-Object -First 1
if (-not $crt) { throw "No x64 Microsoft.VC14x.CRT folder found below '$vs\VC\Redist\MSVC'." }

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$targetDir = Join-Path (Split-Path -Parent $scriptDir) "vc-runtime"
New-Item -ItemType Directory -Force -Path $targetDir | Out-Null
Get-ChildItem -Path $crt.FullName -File -Include "msvcp140*.dll", "vcruntime140*.dll", "concrt140.dll" -Recurse |
    Copy-Item -Destination $targetDir -Force

foreach ($required in "msvcp140.dll", "vcruntime140.dll", "vcruntime140_1.dll") {
    if (-not (Test-Path (Join-Path $targetDir $required))) { throw "$required missing in '$($crt.FullName)'." }
}
$files = (Get-ChildItem -Path $targetDir -Filter "*.dll" | ForEach-Object Name) -join ", "
Write-Host "Staged VC++ runtime from '$($crt.FullName)' to '$targetDir': $files"
