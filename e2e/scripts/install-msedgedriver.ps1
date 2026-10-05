# Downloads the msedgedriver that matches the WebView2 runtime installed on this
# machine (tauri-driver drives WebView2 through it; a version mismatch makes the
# session fail at start). Prints the path and, on GitHub Actions, exports it as
# MFK_E2E_NATIVE_DRIVER for the following steps.
$ErrorActionPreference = 'Stop'

# The WebView2 runtime registers its version under this fixed client GUID.
$webview2 = 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}'
$version = $null
if (Test-Path $webview2) { $version = (Get-ItemProperty $webview2).pv }
if (-not $version) {
  # Runner images ship Edge; its major version equals the runtime's.
  $version = (Get-ItemProperty 'HKCU:\Software\Microsoft\Edge\BLBeacon' -ErrorAction SilentlyContinue).version
}
if (-not $version) { throw 'Could not determine the installed WebView2/Edge version.' }
Write-Host "WebView2 runtime: $version"

$target = Join-Path $PSScriptRoot '..\out\msedgedriver'
New-Item -ItemType Directory -Force -Path $target | Out-Null
$zip = Join-Path $target 'edgedriver_win64.zip'
$url = "https://msedgedriver.microsoft.com/$version/edgedriver_win64.zip"
try {
  Invoke-WebRequest -Uri $url -OutFile $zip
} catch {
  # Exact build not published (runtime updates ahead of the driver): fall back to the latest driver of the same major version.
  $major = $version.Split('.')[0]
  $latest = (Invoke-WebRequest -Uri "https://msedgedriver.microsoft.com/LATEST_RELEASE_${major}_WINDOWS" -UseBasicParsing).Content
  $latest = ($latest -replace "[^\d\.]", '').Trim()
  Write-Host "Exact driver not found, using $latest"
  Invoke-WebRequest -Uri "https://msedgedriver.microsoft.com/$latest/edgedriver_win64.zip" -OutFile $zip
}
Expand-Archive -Path $zip -DestinationPath $target -Force
$driver = (Resolve-Path (Join-Path $target 'msedgedriver.exe')).Path
Write-Host "msedgedriver: $driver"
if ($env:GITHUB_ENV) { "MFK_E2E_NATIVE_DRIVER=$driver" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append }
