<#
Install optional Codex speech notifications for Windows.

This script uses Windows built-in SAPI text-to-speech through PowerShell. It
creates non-blocking PowerShell hooks and updates Codex config.toml so future
Codex turns say either "Pyra finished." or "Pyra waiting for action.".

Example:
  powershell.exe -ExecutionPolicy Bypass -File "$HOME\agent\assets\setup_codex_audio_mode_windows.ps1" `
    -ConfigPath "$HOME\.codex\config.toml"
#>

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$ConfigPath,

    [string]$PowerShellCmd = "powershell.exe",

    [string]$VoiceName = "",

    [switch]$Force
)

$ErrorActionPreference = "Stop"

$StartMarker = "# BEGIN PIRA Codex speech notifications"
$EndMarker = "# END PIRA Codex speech notifications"

function Resolve-UserPath {
    param([Parameter(Mandatory = $true)][string]$Path)
    $expanded = [Environment]::ExpandEnvironmentVariables($Path)
    if ($expanded -eq "~") { return $HOME }
    if ($expanded.StartsWith("~\") -or $expanded.StartsWith("~/")) {
        return (Join-Path $HOME $expanded.Substring(2))
    }
    return $expanded
}

function ConvertTo-TomlBasicString {
    param([Parameter(Mandatory = $true)][string]$Value)
    $escaped = $Value.Replace('\', '\\').Replace('"', '\"')
    return '"' + $escaped + '"'
}

function ConvertTo-PowerShellSingleQuotedString {
    param([Parameter(Mandatory = $true)][AllowEmptyString()][string]$Value)
    return "'" + $Value.Replace("'", "''") + "'"
}

function Write-Utf8NoBom {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][AllowEmptyString()][string]$Value
    )
    $encoding = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($Path, $Value, $encoding)
}

function Remove-ManagedBlocks {
    param([Parameter(Mandatory = $true)][string]$Text)
    $start = [regex]::Escape($StartMarker)
    $end = [regex]::Escape($EndMarker)
    $pattern = "(?ms)^$start.*?^$end\r?\n?"
    return [regex]::Replace($Text, $pattern, "")
}

function Test-TopLevelNotify {
    param([Parameter(Mandatory = $true)][string]$Text)
    return [regex]::IsMatch($Text, '(?m)^notify\s*=')
}

function Remove-TopLevelNotify {
    param([Parameter(Mandatory = $true)][string]$Text)
    return [regex]::Replace($Text, '(?m)^notify\s*=.*\r?\n?', '')
}

function Insert-TopLevelNotify {
    param(
        [Parameter(Mandatory = $true)][string]$Text,
        [Parameter(Mandatory = $true)][string]$NotifyLine
    )
    $block = "$StartMarker`r`n# Non-blocking status speech for Codex on Windows.`r`n$NotifyLine`r`n$EndMarker`r`n`r`n"
    $match = [regex]::Match($Text, '(?m)^\[')
    if ($match.Success) {
        return $Text.Insert($match.Index, $block)
    }
    if ($Text.Length -gt 0 -and -not $Text.EndsWith("`n")) { $Text += "`r`n" }
    return $Text + "`r`n" + $block
}

function Ensure-CodexHooksFeature {
    param([Parameter(Mandatory = $true)][string]$Text)
    $featuresMatch = [regex]::Match($Text, '(?ms)^\[features\]\s*\r?\n(?<body>.*?)(?=^\[|\z)')
    if ($featuresMatch.Success) {
        $body = $featuresMatch.Groups['body'].Value
        if ($body -match '(?m)^\s*codex_hooks\s*=') {
            $body = [regex]::Replace($body, '(?m)^\s*codex_hooks\s*=.*$', 'codex_hooks = true')
        } else {
            $body = "codex_hooks = true`r`n" + $body
        }
        $replacement = "[features]`r`n" + $body
        return $Text.Substring(0, $featuresMatch.Index) + $replacement + $Text.Substring($featuresMatch.Index + $featuresMatch.Length)
    }
    $block = "[features]`r`ncodex_hooks = true`r`n`r`n"
    $match = [regex]::Match($Text, '(?m)^\[')
    if ($match.Success) {
        return $Text.Insert($match.Index, $block)
    }
    if ($Text.Length -gt 0 -and -not $Text.EndsWith("`n")) { $Text += "`r`n" }
    return $Text + $block
}

function Add-PermissionHook {
    param(
        [Parameter(Mandatory = $true)][string]$Text,
        [Parameter(Mandatory = $true)][string]$WaitingScript,
        [Parameter(Mandatory = $true)][string]$PowerShellCmd
    )
    $command = "$PowerShellCmd -NoProfile -ExecutionPolicy Bypass -File `"$WaitingScript`""
    $block = @"

$StartMarker
# Speak when Codex is waiting for approval/action.
[[hooks.PermissionRequest]]
matcher = "*"

[[hooks.PermissionRequest.hooks]]
type = "command"
command = $(ConvertTo-TomlBasicString $command)
timeout = 1
statusMessage = "Speaking waiting status"
$EndMarker
"@
    return $Text + $block
}

$config = Resolve-UserPath $ConfigPath
$configDir = Split-Path -Parent $config
if ([string]::IsNullOrWhiteSpace($configDir)) { $configDir = "." }
$hooksDir = Join-Path $configDir "hooks"
$notifyScript = Join-Path $hooksDir "speak_notify.ps1"
$waitingScript = Join-Path $hooksDir "speak_waiting.ps1"

New-Item -ItemType Directory -Force -Path $configDir | Out-Null
if (-not (Test-Path -LiteralPath $config)) { New-Item -ItemType File -Force -Path $config | Out-Null }

$text = Get-Content -LiteralPath $config -Raw
if ($null -eq $text) { $text = "" }
$textWithoutManaged = Remove-ManagedBlocks $text

if ((Test-TopLevelNotify $textWithoutManaged) -and -not $Force) {
    Write-Error "Refusing to replace an existing top-level notify entry without -Force. Inspect it first, then rerun with -Force if replacement is acceptable."
}

$backup = $null
if ((Get-Item -LiteralPath $config).Length -gt 0) {
    $backup = "$config.bak.$(Get-Date -Format 'yyyyMMdd-HHmmss')"
    Copy-Item -LiteralPath $config -Destination $backup
}

New-Item -ItemType Directory -Force -Path $hooksDir | Out-Null

$notifyContent = @'
# Non-blocking Codex speech notification installed by PIRA for Windows.
param([Parameter(ValueFromRemainingArguments = $true)][string[]]$ArgsFromCodex)

$ErrorActionPreference = "SilentlyContinue"
$FinishedText = "Pyra finished."
$WaitingText = "Pyra waiting for action."
$VoiceName = __VOICE_NAME__

function Speak-Async {
    param([Parameter(Mandatory = $true)][string]$Text)
    $speaker = New-Object -ComObject SAPI.SpVoice
    if (-not [string]::IsNullOrWhiteSpace($VoiceName)) {
        foreach ($voice in $speaker.GetVoices()) {
            if ($voice.GetDescription() -like "*$VoiceName*") {
                $speaker.Voice = $voice
                break
            }
        }
    }
    # 1 = SVSFlagsAsync, so Codex does not wait for speech to finish.
    [void]$speaker.Speak($Text, 1)
}

$payload = ($ArgsFromCodex -join " ")
if ([string]::IsNullOrWhiteSpace($payload)) {
    $payload = [Console]::In.ReadToEnd()
}

$message = ""
try {
    if (-not [string]::IsNullOrWhiteSpace($payload)) {
        $json = $payload | ConvertFrom-Json
        if ($json.PSObject.Properties.Name -contains "last-assistant-message") {
            $message = [string]$json.'last-assistant-message'
        } elseif ($json.PSObject.Properties.Name -contains "last_assistant_message") {
            $message = [string]$json.last_assistant_message
        }
    }
} catch {
    $message = ""
}

$waitingPattern = "\?|confirm|confirmation|approve|approval|permission|do you want|would you like|should i|shall i|may i|please confirm|please approve|waiting for|need your|needs your|reply|respond|choose|select|pick|can i|could i"
if (-not [string]::IsNullOrWhiteSpace($message) -and $message -match $waitingPattern) {
    Speak-Async $WaitingText
} else {
    Speak-Async $FinishedText
}
'@
$notifyContent = $notifyContent.Replace('__VOICE_NAME__', (ConvertTo-PowerShellSingleQuotedString $VoiceName))
Write-Utf8NoBom $notifyScript $notifyContent

$waitingContent = @'
# Speak when Codex is waiting for user action, without blocking.
$ErrorActionPreference = "SilentlyContinue"
$VoiceName = __VOICE_NAME__
$speaker = New-Object -ComObject SAPI.SpVoice
if (-not [string]::IsNullOrWhiteSpace($VoiceName)) {
    foreach ($voice in $speaker.GetVoices()) {
        if ($voice.GetDescription() -like "*$VoiceName*") {
            $speaker.Voice = $voice
            break
        }
    }
}
[void]$speaker.Speak("Pyra waiting for action.", 1)
Write-Output "{}"
'@
$waitingContent = $waitingContent.Replace('__VOICE_NAME__', (ConvertTo-PowerShellSingleQuotedString $VoiceName))
Write-Utf8NoBom $waitingScript $waitingContent

$newText = Remove-TopLevelNotify $textWithoutManaged
$notifyLine = 'notify = [' + (ConvertTo-TomlBasicString $PowerShellCmd) + ', "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", ' + (ConvertTo-TomlBasicString $notifyScript) + ']'
$newText = Insert-TopLevelNotify $newText $notifyLine
$newText = Ensure-CodexHooksFeature $newText
$newText = Add-PermissionHook $newText $waitingScript $PowerShellCmd
Write-Utf8NoBom $config $newText

Write-Output "Codex speech notification mode installed for Windows."
Write-Output "Config: $config"
Write-Output "Notify script: $notifyScript"
Write-Output "Waiting hook: $waitingScript"
if ($backup) { Write-Output "Backup: $backup" }
Write-Output "Restart Codex to load the new notification settings."
