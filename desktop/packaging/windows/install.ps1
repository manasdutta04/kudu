# Installs the latest Kudu for Windows for the current user (no admin rights needed).
#
#   irm https://github.com/manasdutta04/kudu/releases/latest/download/install.ps1 | iex
#
# Kudu isn't signed with a paid code-signing certificate, so Windows marks the download as
# coming from the internet and SmartScreen warns about it. This script removes that mark
# (Unblock-File), installs Kudu into %LOCALAPPDATA%\Programs\Kudu, adds a Start menu shortcut
# and starts it.

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'   # Invoke-WebRequest is much faster without the progress bar

$Repo = 'manasdutta04/kudu'
$Url = "https://github.com/$Repo/releases/latest/download/Kudu-Windows-x64.zip"
$Dir = Join-Path $env:LOCALAPPDATA 'Programs\Kudu'
$Exe = Join-Path $Dir 'kudu.exe'

if (-not [Environment]::Is64BitOperatingSystem) {
    throw 'Kudu needs 64-bit Windows 10 or 11.'
}

Write-Host 'Downloading Kudu...'
$Zip = Join-Path $env:TEMP 'Kudu-Windows-x64.zip'
Invoke-WebRequest -Uri $Url -OutFile $Zip -UseBasicParsing

Get-Process kudu -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Milliseconds 500

New-Item -ItemType Directory -Force -Path $Dir | Out-Null
Expand-Archive -Path $Zip -DestinationPath $Dir -Force
Remove-Item $Zip
# Unsigned app: remove the "downloaded from the internet" mark so SmartScreen doesn't block it.
Get-ChildItem -Path $Dir -Recurse | Unblock-File

$Shortcut = Join-Path ([Environment]::GetFolderPath('Programs')) 'Kudu.lnk'
$Shell = New-Object -ComObject WScript.Shell
$Link = $Shell.CreateShortcut($Shortcut)
$Link.TargetPath = $Exe
$Link.WorkingDirectory = $Dir
$Link.Description = 'Kudu: private voice dictation'
$Link.Save()

Write-Host "Kudu is installed in $Dir and in the Start menu."
Write-Host 'Hold Ctrl + Win to dictate. Kudu downloads its speech model the first time it starts.'
Start-Process $Exe
