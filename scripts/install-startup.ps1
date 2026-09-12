param(
    [string]$ExePath = ""
)

$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($ExePath)) {
    $ExePath = Join-Path $PSScriptRoot "..\target\release\hermes.exe"
}

if (-not (Test-Path $ExePath -PathType Leaf)) {
    throw "Executable not found at: $ExePath"
}
$resolvedExe = (Resolve-Path $ExePath).Path

$startupDir = [Environment]::GetFolderPath("Startup")
$shortcutPath = Join-Path $startupDir "Hermes.lnk"

$shell = New-Object -ComObject WScript.Shell
$shortcut = $shell.CreateShortcut($shortcutPath)
$shortcut.TargetPath = $resolvedExe
$shortcut.Arguments = "--background"
$shortcut.WorkingDirectory = Split-Path $resolvedExe -Parent
$shortcut.IconLocation = "$resolvedExe,0"
$shortcut.WindowStyle = 7
$shortcut.Description = "Hermes startup launcher"
$shortcut.Save()

Write-Host "Startup shortcut installed at:"
Write-Host "  $shortcutPath"
Write-Host "Launch target:"
Write-Host "  $resolvedExe --background"
