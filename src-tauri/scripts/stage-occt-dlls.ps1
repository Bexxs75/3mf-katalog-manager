<#
.SYNOPSIS
    Stages the OCCT runtime DLLs from a vcpkg installation into
    src-tauri/occt-runtime, so Tauri's bundler can package them as
    resources for a STEP-preview-enabled Windows build.

.DESCRIPTION
    Windows offers no OS-provided OpenCASCADE package equivalent to the
    Linux distro packages or the macOS Homebrew formula, so the DLLs
    have to be built via vcpkg and staged into the project tree before
    `npm run tauri build`. They are intentionally NOT committed to the
    repository (see vendor/opencascade-sys/VENDORING.md) - this script
    re-creates them from a vcpkg installation on demand, matching the
    same pattern build-linux.yml uses to install OCCT at CI time
    instead of vendoring binaries.

    OCCT must be pinned to the same version documented in
    vendor/opencascade-sys/VENDORING.md (7.8 or 7.9) via a vcpkg
    manifest with an "overrides" entry - see that file for the exact
    vcpkg.json used to build the reference OCCT 7.9.3 install this
    script was verified against.

.PARAMETER VcpkgInstalledDir
    Path to the vcpkg triplet install directory that contains the
    OCCT runtime DLLs under its `bin` subfolder, e.g.
    C:\occt-build\vcpkg_installed\x64-windows. Defaults to that same
    path, matching the reference setup in VENDORING.md.

.EXAMPLE
    .\src-tauri\scripts\stage-occt-dlls.ps1
    .\src-tauri\scripts\stage-occt-dlls.ps1 -VcpkgInstalledDir C:\vcpkg\installed\x64-windows
#>
param(
    [string]$VcpkgInstalledDir = "C:\occt-build\vcpkg_installed\x64-windows"
)

$ErrorActionPreference = "Stop"

$binDir = Join-Path $VcpkgInstalledDir "bin"
if (-not (Test-Path $binDir)) {
    throw "OCCT bin directory not found at '$binDir'. Build OCCT via vcpkg first (see vendor/opencascade-sys/VENDORING.md for the pinned version and manifest)."
}

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$targetDir = Join-Path (Split-Path -Parent $scriptDir) "occt-runtime"

New-Item -ItemType Directory -Force -Path $targetDir | Out-Null
Copy-Item -Path (Join-Path $binDir "*.dll") -Destination $targetDir -Force

$count = (Get-ChildItem -Path $targetDir -Filter "*.dll").Count
Write-Host "Staged $count OCCT runtime DLLs from '$binDir' to '$targetDir'."
