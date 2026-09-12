# Hermes one-command setup for Windows clones.
# Builds the app, downloads whisper runtime + model, and adds hermes.exe to user PATH.
$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot

$pythonCommand = $null
if (Get-Command python -ErrorAction SilentlyContinue) {
    $pythonCommand = @("python")
} elseif (Get-Command py -ErrorAction SilentlyContinue) {
    $pythonCommand = @("py", "-3")
}

if (-not $pythonCommand) {
    throw "Python 3 was not found. Install Python 3 and make sure python or py is on PATH."
}

& @pythonCommand "scripts/ptt_tooling.py" setup @args
exit $LASTEXITCODE
