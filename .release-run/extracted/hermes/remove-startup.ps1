$ErrorActionPreference = "Stop"

$startupDir = [Environment]::GetFolderPath("Startup")
$shortcutPath = Join-Path $startupDir "Hermes.lnk"

if (Test-Path $shortcutPath) {
    Remove-Item $shortcutPath -Force
    Write-Host "Removed startup shortcut:"
    Write-Host "  $shortcutPath"
} else {
    Write-Host "No startup shortcut found at:"
    Write-Host "  $shortcutPath"
}
