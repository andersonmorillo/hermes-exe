param(
    [Parameter(Mandatory = $true)]
    [string]$ToolingScriptPath,
    [Parameter(Mandatory = $true)]
    [string]$ModelPath,
    [Parameter(Mandatory = $true)]
    [string]$WhisperCliPath,
    [Parameter(Mandatory = $true)]
    [string]$Language,
    [Parameter(Mandatory = $true)]
    [string]$HotkeyModifier,
    [Parameter(Mandatory = $true)]
    [string]$HotkeyKey,
    [Parameter(Mandatory = $true)]
    [UInt64]$MinRecordMs,
    [Parameter(Mandatory = $true)]
    [string]$AutoPunctuation,
    [Parameter(Mandatory = $true)]
    [string]$TypeOutput,
    [Parameter(Mandatory = $true)]
    [string]$StreamOutput,
    [Parameter(Mandatory = $true)]
    [string]$AllowTerminalOutput,
    [Parameter(Mandatory = $true)]
    [string]$SttBackend,
    [Parameter(Mandatory = $true)]
    [string]$WhisperxModel,
    [Parameter(Mandatory = $true)]
    [string]$WhisperxPython
)

$ErrorActionPreference = "Stop"

Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

$form = New-Object System.Windows.Forms.Form
$form.Text = "Hermes Settings"
$form.StartPosition = "CenterScreen"
$form.ClientSize = New-Object System.Drawing.Size(680, 640)
$form.FormBorderStyle = [System.Windows.Forms.FormBorderStyle]::FixedDialog
$form.MaximizeBox = $false
$form.MinimizeBox = $false
$form.TopMost = $true

$labelWidth = 170
$inputLeft = 185
$inputWidth = 470
$rowHeight = 32
$rowY = 16
$script:downloadProcess = $null
$script:downloadOutputPath = $null
$script:downloadStdoutPath = $null
$script:downloadStderrPath = $null
$script:currentCustomModelPath = $ModelPath
$script:saveAfterDownload = $false
$script:knownModelVariants = @("tiny.en", "base.en", "small.en", "medium.en", "large-v3", "custom")
$script:knownWhisperxModels = @("tiny", "base", "small", "medium", "large-v2", "large-v3")
$script:knownBackends = @("whisperx", "whispercpp")

function Set-DownloadUiState {
    param(
        [bool]$IsBusy,
        [string]$StatusText
    )

    $downloadModelButton.Enabled = -not $IsBusy
    $saveButton.Enabled = -not $IsBusy
    $cancelButton.Enabled = -not $IsBusy
    $modelVariantCombo.Enabled = -not $IsBusy
    $downloadModelRow.Label.Visible = $true
    $downloadModelRow.Button.Visible = $true
    $downloadModelButton.Text = if ($IsBusy) { "Downloading..." } else { "Download Selected Model" }
    $form.UseWaitCursor = $IsBusy
    if ($IsBusy) {
        $statusLabel.Text = $StatusText
    } else {
        Update-ModelSelectionUi
    }
    $form.Refresh()
}

function Escape-TomlString {
    param([string]$Value)
    return $Value.Replace('\', '\\').Replace('"', '\"')
}

function Add-TextField {
    param(
        [string]$FieldLabel,
        [string]$Value
    )

    $labelControl = New-Object System.Windows.Forms.Label
    $labelControl.Text = $FieldLabel
    $labelControl.AutoSize = $false
    $labelControl.Location = New-Object System.Drawing.Point(10, ($script:rowY + 6))
    $labelControl.Size = New-Object System.Drawing.Size($script:labelWidth, 20)
    $form.Controls.Add($labelControl)

    $textbox = New-Object System.Windows.Forms.TextBox
    $textbox.Location = New-Object System.Drawing.Point($script:inputLeft, ($script:rowY + 3))
    $textbox.Size = New-Object System.Drawing.Size($script:inputWidth, 23)
    $textbox.Text = $Value
    $form.Controls.Add($textbox)

    $script:rowY += $script:rowHeight
    return $textbox
}

function Add-ComboField {
    param(
        [string]$FieldLabel,
        [string[]]$Options,
        [string]$SelectedValue
    )

    $labelControl = New-Object System.Windows.Forms.Label
    $labelControl.Text = $FieldLabel
    $labelControl.AutoSize = $false
    $labelControl.Location = New-Object System.Drawing.Point(10, ($script:rowY + 6))
    $labelControl.Size = New-Object System.Drawing.Size($script:labelWidth, 20)
    $form.Controls.Add($labelControl)

    $combo = New-Object System.Windows.Forms.ComboBox
    $combo.Location = New-Object System.Drawing.Point($script:inputLeft, ($script:rowY + 3))
    $combo.Size = New-Object System.Drawing.Size($script:inputWidth, 23)
    $combo.DropDownStyle = [System.Windows.Forms.ComboBoxStyle]::DropDownList
    foreach ($option in $Options) {
        $null = $combo.Items.Add($option)
    }
    $index = $combo.Items.IndexOf($SelectedValue)
    if ($index -lt 0) {
        $index = 0
    }
    $combo.SelectedIndex = $index
    $form.Controls.Add($combo)

    $script:rowY += $script:rowHeight
    return $combo
}

function Add-ActionButton {
    param(
        [string]$FieldLabel,
        [string]$ButtonText
    )

    $labelControl = New-Object System.Windows.Forms.Label
    $labelControl.Text = $FieldLabel
    $labelControl.AutoSize = $false
    $labelControl.Location = New-Object System.Drawing.Point(10, ($script:rowY + 6))
    $labelControl.Size = New-Object System.Drawing.Size($script:labelWidth, 20)
    $form.Controls.Add($labelControl)

    $button = New-Object System.Windows.Forms.Button
    $button.Location = New-Object System.Drawing.Point($script:inputLeft, ($script:rowY + 2))
    $button.Size = New-Object System.Drawing.Size(150, 24)
    $button.Text = $ButtonText
    $form.Controls.Add($button)

    $script:rowY += $script:rowHeight
    return [pscustomobject]@{
        Label = $labelControl
        Button = $button
    }
}

function Add-CheckField {
    param(
        [string]$FieldLabel,
        [bool]$Checked
    )

    $labelControl = New-Object System.Windows.Forms.Label
    $labelControl.Text = $FieldLabel
    $labelControl.AutoSize = $false
    $labelControl.Location = New-Object System.Drawing.Point(10, ($script:rowY + 6))
    $labelControl.Size = New-Object System.Drawing.Size($script:labelWidth, 20)
    $form.Controls.Add($labelControl)

    $checkbox = New-Object System.Windows.Forms.CheckBox
    $checkbox.Location = New-Object System.Drawing.Point($script:inputLeft, ($script:rowY + 6))
    $checkbox.Size = New-Object System.Drawing.Size($script:inputWidth, 20)
    $checkbox.Checked = $Checked
    $form.Controls.Add($checkbox)

    $script:rowY += $script:rowHeight
    return $checkbox
}

function Get-ModelFilename {
    param([string]$Variant)

    switch ($Variant) {
        "tiny.en" { return "ggml-tiny.en.bin" }
        "base.en" { return "ggml-base.en.bin" }
        "small.en" { return "ggml-small.en.bin" }
        "medium.en" { return "ggml-medium.en.bin" }
        "large-v3" { return "ggml-large-v3.bin" }
        default { throw "Unknown model variant: $Variant" }
    }
}

function Get-DefaultModelsDirectory {
    if ([string]::IsNullOrWhiteSpace($env:LOCALAPPDATA)) {
        throw "LOCALAPPDATA is not set."
    }

    return Join-Path $env:LOCALAPPDATA "Hermes\Hermes\data\models"
}

function Get-ModelVariantFromPath {
    param([string]$PathValue)

    if ([string]::IsNullOrWhiteSpace($PathValue)) {
        return "medium.en"
    }

    $filename = [System.IO.Path]::GetFileName($PathValue).ToLowerInvariant()
    switch ($filename) {
        "ggml-tiny.en.bin" { return "tiny.en" }
        "ggml-base.en.bin" { return "base.en" }
        "ggml-small.en.bin" { return "small.en" }
        "ggml-medium.en.bin" { return "medium.en" }
        "ggml-large-v3.bin" { return "large-v3" }
        default { return "custom" }
    }
}

function Resolve-SelectedModelPath {
    $variant = $modelVariantCombo.SelectedItem.ToString()
    if ($variant -eq "custom") {
        return $script:currentCustomModelPath
    }

    $modelsDir = Get-DefaultModelsDirectory
    return Join-Path $modelsDir (Get-ModelFilename $variant)
}

function Update-ModelSelectionUi {
    if (-not $modelVariantCombo) {
        return
    }

    $variant = $modelVariantCombo.SelectedItem.ToString()
    if ($variant -eq "custom") {
        $downloadModelRow.Label.Visible = $false
        $downloadModelRow.Button.Visible = $false
        $downloadModelButton.Enabled = $false
        if ([string]::IsNullOrWhiteSpace($script:currentCustomModelPath)) {
            $statusLabel.Text = "Custom model path is empty. Edit config.toml manually if you need a custom file."
        } else {
            $statusLabel.Text = "Custom model path preserved: $script:currentCustomModelPath"
        }
        return
    }

    $downloadModelButton.Enabled = $true
    $modelPath = Resolve-SelectedModelPath
    if (Test-Path -LiteralPath $modelPath) {
        $downloadModelRow.Label.Visible = $false
        $downloadModelRow.Button.Visible = $false
        $statusLabel.Text = "Installed: $variant"
    } else {
        $downloadModelRow.Label.Visible = $true
        $downloadModelRow.Button.Visible = $true
        $statusLabel.Text = "Not downloaded yet: $variant. Hermes can download it automatically."
    }
}

function Complete-Save {
    $modelPath = Resolve-SelectedModelPath
    $whisperCliPath = $whisperCliPathBox.Text.Trim()
    $language = $languageBox.Text.Trim()
    $hotkeyModifier = $hotkeyModifierBox.Text.Trim()
    $hotkeyKey = $hotkeyKeyBox.Text.Trim()

    if ([string]::IsNullOrWhiteSpace($modelPath)) {
        [System.Windows.Forms.MessageBox]::Show("Model selection is invalid.", "Hermes Settings")
        return
    }
    if ([string]::IsNullOrWhiteSpace($whisperCliPath)) {
        [System.Windows.Forms.MessageBox]::Show("Whisper CLI path cannot be empty.", "Hermes Settings")
        return
    }
    if ([string]::IsNullOrWhiteSpace($language)) {
        [System.Windows.Forms.MessageBox]::Show("Language cannot be empty.", "Hermes Settings")
        return
    }
    if ([string]::IsNullOrWhiteSpace($hotkeyKey)) {
        [System.Windows.Forms.MessageBox]::Show("Hotkey key cannot be empty.", "Hermes Settings")
        return
    }

    $minRecordMs = 0
    if (-not [uint64]::TryParse($minRecordMsBox.Text.Trim(), [ref]$minRecordMs)) {
        [System.Windows.Forms.MessageBox]::Show("Min Record (ms) must be a non-negative integer.", "Hermes Settings")
        return
    }

    $autoPunctuationValue = if ($autoPunctuationBox.Checked) { "true" } else { "false" }
    $typeOutputValue = if ($typeOutputBox.Checked) { "true" } else { "false" }
    $streamOutputValue = if ($streamOutputBox.Checked) { "true" } else { "false" }
    $allowTerminalOutputValue = if ($allowTerminalOutputBox.Checked) { "true" } else { "false" }

    $sttBackend = $sttBackendCombo.SelectedItem.ToString()
    $whisperxModel = $whisperxModelCombo.SelectedItem.ToString()
    $whisperxPython = $whisperxPythonBox.Text.Trim()
    if ([string]::IsNullOrWhiteSpace($whisperxPython)) {
        [System.Windows.Forms.MessageBox]::Show("WhisperX python path cannot be empty.", "Hermes Settings")
        return
    }

    $tomlLines = @(
        "stt_backend = `"$($(Escape-TomlString $sttBackend))`"",
        "model_path = `"$($(Escape-TomlString $modelPath))`"",
        "whisper_cli_path = `"$($(Escape-TomlString $whisperCliPath))`"",
        "whisperx_python = `"$($(Escape-TomlString $whisperxPython))`"",
        "whisperx_model = `"$($(Escape-TomlString $whisperxModel))`"",
        'whisperx_language = "en"',
        'whisperx_device = "auto"',
        'whisperx_compute_type = "auto"',
        'whisperx_model_dir = ""',
        "min_record_ms = $minRecordMs",
        "auto_punctuation = $autoPunctuationValue",
        "type_output = $typeOutputValue",
        "stream_output = $streamOutputValue",
        "allow_terminal_output = $allowTerminalOutputValue",
        "language = `"$($(Escape-TomlString $language))`"",
        "",
        "[hotkey]",
        "modifier = `"$($(Escape-TomlString $hotkeyModifier))`"",
        "key = `"$($(Escape-TomlString $hotkeyKey))`""
    )

    $form.Tag = ($tomlLines -join "`n")
    $form.DialogResult = [System.Windows.Forms.DialogResult]::OK
    $form.Close()
}

function Resolve-PythonCommand {
    $candidates = @(
        @{ File = "py"; Args = @("-3") },
        @{ File = "python"; Args = @() }
    )

    foreach ($candidate in $candidates) {
        if (Get-Command $candidate.File -ErrorAction SilentlyContinue) {
            return $candidate
        }
    }

    throw "Python 3 was not found. Install Python and make sure `py` or `python` is on PATH."
}

function Update-BackendUi {
    $isWhisperx = ($sttBackendCombo.SelectedItem.ToString() -eq "whisperx")
    $modelVariantCombo.Enabled = -not $isWhisperx
    $whisperxModelCombo.Enabled = $isWhisperx
    $downloadModelButton.Enabled = -not $isWhisperx
    $streamOutputBox.Enabled = -not $isWhisperx
    if ($isWhisperx -and $streamOutputBox.Checked) {
        $streamOutputBox.Checked = $false
    }
    if ($isWhisperx) {
        $note.Text = "WhisperX uses GPU when available. Live stream output is only available with whisper.cpp."
    } else {
        $note.Text = "whisper.cpp runs in CPU-only mode. Standard ggml models are stored in your local app data folder."
    }
}

$selectedModelVariant = Get-ModelVariantFromPath $ModelPath
$selectedBackend = if ($script:knownBackends -contains $SttBackend) { $SttBackend } else { "whispercpp" }
$selectedWhisperxModel = if ($script:knownWhisperxModels -contains $WhisperxModel) { $WhisperxModel } else { "small" }
$sttBackendCombo = Add-ComboField "STT Backend" $script:knownBackends $selectedBackend
$whisperCliPathBox = Add-TextField "Whisper CLI Path" $WhisperCliPath
$whisperxPythonBox = Add-TextField "WhisperX Python" $WhisperxPython
$modelVariantCombo = Add-ComboField "whisper.cpp Model" $script:knownModelVariants $selectedModelVariant
$whisperxModelCombo = Add-ComboField "WhisperX Model" $script:knownWhisperxModels $selectedWhisperxModel
$languageBox = Add-TextField "Language" $Language
$hotkeyModifierBox = Add-TextField "Hotkey Modifier" $HotkeyModifier
$hotkeyKeyBox = Add-TextField "Hotkey Key" $HotkeyKey
$downloadModelRow = Add-ActionButton "Model Download" "Download Selected Model"
$downloadModelButton = $downloadModelRow.Button
$minRecordMsBox = Add-TextField "Min Record (ms)" $MinRecordMs.ToString()
$autoPunctuationBox = Add-CheckField "Auto Punctuation" ($AutoPunctuation.ToLowerInvariant() -eq "true")
$typeOutputBox = Add-CheckField "Type Output" ($TypeOutput.ToLowerInvariant() -eq "true")
$streamOutputBox = Add-CheckField "Stream Output While Recording" ($StreamOutput.ToLowerInvariant() -eq "true")
$allowTerminalOutputBox = Add-CheckField "Allow Terminal Output (Ctrl+Shift+V)" ($AllowTerminalOutput.ToLowerInvariant() -eq "true")

$note = New-Object System.Windows.Forms.Label
$note.Text = "Choose a backend and model. WhisperX prefers GPU; whisper.cpp is CPU-only."
$note.AutoSize = $false
$note.Location = New-Object System.Drawing.Point(10, ($rowY + 4))
$note.Size = New-Object System.Drawing.Size(650, 30)
$form.Controls.Add($note)

$statusLabel = New-Object System.Windows.Forms.Label
$statusLabel.Text = ""
$statusLabel.AutoSize = $false
$statusLabel.Location = New-Object System.Drawing.Point(10, ($rowY + 34))
$statusLabel.Size = New-Object System.Drawing.Size(650, 30)
$form.Controls.Add($statusLabel)

$buttonTop = $rowY + 74

$saveButton = New-Object System.Windows.Forms.Button
$saveButton.Text = "Save"
$saveButton.Location = New-Object System.Drawing.Point(470, $buttonTop)
$saveButton.Size = New-Object System.Drawing.Size(90, 30)

$cancelButton = New-Object System.Windows.Forms.Button
$cancelButton.Text = "Cancel"
$cancelButton.Location = New-Object System.Drawing.Point(570, $buttonTop)
$cancelButton.Size = New-Object System.Drawing.Size(90, 30)
$cancelButton.DialogResult = [System.Windows.Forms.DialogResult]::Cancel

$form.AcceptButton = $saveButton
$form.CancelButton = $cancelButton

$modelVariantCombo.Add_SelectedIndexChanged({
    if (-not ($script:downloadProcess -and -not $script:downloadProcess.HasExited)) {
        Update-ModelSelectionUi
    }
})

$downloadPollTimer = New-Object System.Windows.Forms.Timer
$downloadPollTimer.Interval = 400
$downloadPollTimer.Add_Tick({
    if (-not $script:downloadProcess) {
        $downloadPollTimer.Stop()
        return
    }

    if (-not $script:downloadProcess.HasExited) {
        return
    }

    $downloadPollTimer.Stop()
    Set-DownloadUiState -IsBusy $false -StatusText ""

    $exitCode = $script:downloadProcess.ExitCode
    $stdoutText = ""
    $stderrText = ""
    if ($script:downloadStdoutPath -and (Test-Path -LiteralPath $script:downloadStdoutPath)) {
        $stdoutText = [System.IO.File]::ReadAllText($script:downloadStdoutPath).Trim()
    }
    if ($script:downloadStderrPath -and (Test-Path -LiteralPath $script:downloadStderrPath)) {
        $stderrText = [System.IO.File]::ReadAllText($script:downloadStderrPath).Trim()
    }

    $outputPath = $script:downloadOutputPath

    $script:downloadProcess.Dispose()
    $script:downloadProcess = $null
    $script:downloadOutputPath = $null

    if ($script:downloadStdoutPath) {
        Remove-Item -LiteralPath $script:downloadStdoutPath -Force -ErrorAction SilentlyContinue
        $script:downloadStdoutPath = $null
    }
    if ($script:downloadStderrPath) {
        Remove-Item -LiteralPath $script:downloadStderrPath -Force -ErrorAction SilentlyContinue
        $script:downloadStderrPath = $null
    }

    if ($exitCode -ne 0) {
        $script:saveAfterDownload = $false
        $message = if (-not [string]::IsNullOrWhiteSpace($stderrText)) {
            $stderrText
        } elseif (-not [string]::IsNullOrWhiteSpace($stdoutText)) {
            $stdoutText
        } else {
            "The download helper exited with code $exitCode."
        }
        [System.Windows.Forms.MessageBox]::Show(
            "Model download failed:`n$message",
            "Hermes Settings"
        )
        return
    }

    if ($script:saveAfterDownload) {
        $script:saveAfterDownload = $false
        Complete-Save
        return
    }

    [System.Windows.Forms.MessageBox]::Show(
        "Model downloaded successfully to:`n$outputPath",
        "Hermes Settings"
    )
})

$form.Add_FormClosing({
    param($sender, $eventArgs)

    if ($script:downloadProcess -and -not $script:downloadProcess.HasExited) {
        $eventArgs.Cancel = $true
        [System.Windows.Forms.MessageBox]::Show(
            "Wait for the current model download to finish before closing Settings.",
            "Hermes Settings"
        )
    }
})

$downloadModelButton.Add_Click({
    try {
        if ($script:downloadProcess -and -not $script:downloadProcess.HasExited) {
            return
        }

        $variant = $modelVariantCombo.SelectedItem.ToString()
        if ($variant -eq "custom") {
            [System.Windows.Forms.MessageBox]::Show(
                "Custom model paths are preserved as-is. Edit config.toml manually if you need a different custom file.",
                "Hermes Settings"
            )
            return
        }

        $outputPath = Resolve-SelectedModelPath
        $targetDirectory = Split-Path $outputPath -Parent
        $null = New-Item -ItemType Directory -Path $targetDirectory -Force
        $pythonCommand = Resolve-PythonCommand

        if (-not (Test-Path -LiteralPath $ToolingScriptPath)) {
            throw "The embedded Hermes Python helper is unavailable."
        }

        $arguments = @()
        $arguments += $pythonCommand.Args
        $arguments += @(
            $ToolingScriptPath,
            "download-model",
            "--variant", $variant,
            "--output", $outputPath
        )

        $script:downloadStdoutPath = Join-Path $env:TEMP ("hermes-model-download-{0}-stdout.log" -f ([guid]::NewGuid().ToString("N")))
        $script:downloadStderrPath = Join-Path $env:TEMP ("hermes-model-download-{0}-stderr.log" -f ([guid]::NewGuid().ToString("N")))

        $argumentLine = [string]::Join(' ', ($arguments | ForEach-Object {
            if ($_ -match '[\s"]') {
                '"' + ($_.Replace('"', '\"')) + '"'
            } else {
                $_
            }
        }))

        Set-DownloadUiState -IsBusy $true -StatusText "Downloading $variant. The settings window will stay responsive."
        $script:downloadOutputPath = $outputPath
        $script:downloadProcess = Start-Process `
            -FilePath $pythonCommand.File `
            -ArgumentList $argumentLine `
            -WindowStyle Hidden `
            -RedirectStandardOutput $script:downloadStdoutPath `
            -RedirectStandardError $script:downloadStderrPath `
            -PassThru
        $downloadPollTimer.Start()
    } catch {
        if ($script:downloadProcess) {
            try {
                if (-not $script:downloadProcess.HasExited) {
                    $script:downloadProcess.Kill()
                }
            } catch {
            }
            $script:downloadProcess.Dispose()
            $script:downloadProcess = $null
        }
        $script:downloadOutputPath = $null
        $script:saveAfterDownload = $false
        if ($script:downloadStdoutPath) {
            Remove-Item -LiteralPath $script:downloadStdoutPath -Force -ErrorAction SilentlyContinue
            $script:downloadStdoutPath = $null
        }
        if ($script:downloadStderrPath) {
            Remove-Item -LiteralPath $script:downloadStderrPath -Force -ErrorAction SilentlyContinue
            $script:downloadStderrPath = $null
        }
        Set-DownloadUiState -IsBusy $false -StatusText ""
        [System.Windows.Forms.MessageBox]::Show(
            "Model download failed:`n$($_.Exception.Message)",
            "Hermes Settings"
        )
    }
})

$saveButton.Add_Click({
    if ($script:downloadProcess -and -not $script:downloadProcess.HasExited) {
        return
    }

    $variant = $modelVariantCombo.SelectedItem.ToString()
    if ($variant -ne "custom") {
        $modelPath = Resolve-SelectedModelPath
        if (-not (Test-Path -LiteralPath $modelPath)) {
            $script:saveAfterDownload = $true
            $downloadModelButton.PerformClick()
            return
        }
    }

    Complete-Save
})

$form.Controls.Add($saveButton)
$form.Controls.Add($cancelButton)

$sttBackendCombo.Add_SelectedIndexChanged({ Update-BackendUi })
Update-ModelSelectionUi
Update-BackendUi

$null = $form.ShowDialog()
if ($form.DialogResult -eq [System.Windows.Forms.DialogResult]::OK -and $form.Tag) {
    Write-Output $form.Tag
}
