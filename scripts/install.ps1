<#
.SYNOPSIS
  Builds BeatSlopper in release mode, installs it to %LOCALAPPDATA%\Programs\BeatSlopper and
  adds Desktop and Start menu shortcuts, plus an "Update BeatSlopper" desktop shortcut.

.DESCRIPTION
  The release build goes to its own target dir (target\install), so it neither collides with a
  running target\release\beatslopper.exe nor with the dev gate. The installed copy is what the
  shortcuts start; re-run this script (or double-click "Update BeatSlopper") to rebuild it from
  the repo's current code. Close BeatSlopper first.

  powershell -ExecutionPolicy Bypass -File scripts\install.ps1 [-NoDesktopShortcut] [-Pause]

  -Pause keeps the window open at the end so a double-clicked run shows how it went; the
  update shortcut passes it.
#>
param([switch]$NoDesktopShortcut, [switch]$Pause)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$targetDir = Join-Path $repo 'target\install'
$installDir = Join-Path $env:LOCALAPPDATA 'Programs\BeatSlopper'
$exe = Join-Path $installDir 'BeatSlopper.exe'
$failed = $false

try {
    if (Get-Process -Name BeatSlopper -ErrorAction SilentlyContinue |
            Where-Object { $_.Path -eq $exe }) {
        throw "BeatSlopper is running from $installDir; close it and run this again."
    }

    Write-Host 'Building BeatSlopper (release)...'
    cargo build --release -p app --manifest-path (Join-Path $repo 'Cargo.toml') --target-dir $targetDir
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed with exit code $LASTEXITCODE" }

    New-Item -ItemType Directory -Force $installDir | Out-Null
    Copy-Item (Join-Path $targetDir 'release\beatslopper.exe') $exe -Force

    $shell = New-Object -ComObject WScript.Shell
    function New-Shortcut([string]$path, [string]$target, [string]$arguments, [string]$description) {
        $lnk = $shell.CreateShortcut($path)
        $lnk.TargetPath = $target
        $lnk.Arguments = $arguments
        $lnk.WorkingDirectory = Split-Path -Parent $target
        $lnk.IconLocation = "$exe,0"
        $lnk.Description = $description
        $lnk.Save()
        Write-Host "Shortcut: $path"
    }

    $desktop = [Environment]::GetFolderPath('Desktop')
    $appDescription = 'BeatSlopper: make a beat'
    New-Shortcut (Join-Path ([Environment]::GetFolderPath('Programs')) 'BeatSlopper.lnk') $exe '' $appDescription
    if (-not $NoDesktopShortcut) {
        New-Shortcut (Join-Path $desktop 'BeatSlopper.lnk') $exe '' $appDescription
        New-Shortcut (Join-Path $desktop 'Update BeatSlopper.lnk') (Join-Path $PSHOME 'powershell.exe') `
            "-NoProfile -ExecutionPolicy Bypass -File `"$PSCommandPath`" -Pause" `
            'Rebuild BeatSlopper from the repo and reinstall it'
    }

    Write-Host "Installed $exe" -ForegroundColor Green
} catch {
    Write-Host $_ -ForegroundColor Red
    $failed = $true
}

if ($Pause) { Read-Host 'Press Enter to close' | Out-Null }
if ($failed) { exit 1 }
