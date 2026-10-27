<#
.SYNOPSIS
  Builds BeatSlopper in release mode, installs it to %LOCALAPPDATA%\Programs\BeatSlopper and
  adds Desktop and Start menu shortcuts.

.DESCRIPTION
  The release build goes to its own target dir (target\install), so it neither collides with a
  running target\release\beatslopper.exe nor with the dev gate. The installed copy is what the
  shortcuts start; re-run this script to update it (close BeatSlopper first).

  powershell -ExecutionPolicy Bypass -File scripts\install.ps1 [-NoDesktopShortcut]
#>
param([switch]$NoDesktopShortcut)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$targetDir = Join-Path $repo 'target\install'
$installDir = Join-Path $env:LOCALAPPDATA 'Programs\BeatSlopper'
$exe = Join-Path $installDir 'BeatSlopper.exe'

if (Get-Process -Name BeatSlopper -ErrorAction SilentlyContinue |
        Where-Object { $_.Path -eq $exe }) {
    throw "BeatSlopper is running from $installDir; close it and run this script again."
}

Write-Host 'Building BeatSlopper (release)...'
cargo build --release -p app --manifest-path (Join-Path $repo 'Cargo.toml') --target-dir $targetDir
if ($LASTEXITCODE -ne 0) { throw "cargo build failed with exit code $LASTEXITCODE" }

New-Item -ItemType Directory -Force $installDir | Out-Null
Copy-Item (Join-Path $targetDir 'release\beatslopper.exe') $exe -Force

$shell = New-Object -ComObject WScript.Shell
function New-Shortcut([string]$path) {
    $lnk = $shell.CreateShortcut($path)
    $lnk.TargetPath = $exe
    $lnk.WorkingDirectory = $installDir
    $lnk.IconLocation = "$exe,0"
    $lnk.Description = 'BeatSlopper: make a beat'
    $lnk.Save()
    Write-Host "Shortcut: $path"
}

$startMenu = Join-Path ([Environment]::GetFolderPath('Programs')) 'BeatSlopper.lnk'
New-Shortcut $startMenu
if (-not $NoDesktopShortcut) {
    New-Shortcut (Join-Path ([Environment]::GetFolderPath('Desktop')) 'BeatSlopper.lnk')
}

Write-Host "Installed $exe"
