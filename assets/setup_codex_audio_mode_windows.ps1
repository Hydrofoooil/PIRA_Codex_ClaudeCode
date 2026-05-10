<#
Install optional Codex audio notifications for Windows.

This script plays local audio files instead of using text-to-speech. It creates
non-blocking PowerShell hooks and updates Codex config.toml so normal completion
can play complete_msg.m4a and focus-aware waiting states play waiting_msg.m4a. By
default it also installs a PowerShell profile wrapper that plays start_msg.m4a
when launching `codex`.

Project default audio set: $HOME\agent\PIRA_Voice\Samantha

Example:
  powershell.exe -ExecutionPolicy Bypass -File "$HOME\agent\assets\setup_codex_audio_mode_windows.ps1" `
    -ConfigPath "$HOME\.codex\config.toml"

Local custom audio example:
  powershell.exe -ExecutionPolicy Bypass -File "$HOME\agent\assets\setup_codex_audio_mode_windows.ps1" `
    -ConfigPath "$HOME\.codex\config.toml" `
    -AudioDir "$HOME\agent\PIRA_Voice\Debbie"

Use -NoStartupWrapper to install only completion/waiting notifications.
#>

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$ConfigPath,

    [string]$AudioDir = "$HOME\agent\PIRA_Voice\Samantha",

    [string]$StartupAudio = "",

    [string]$FinishedAudio = "",

    [string]$WaitingAudio = "",

    [string]$PowerShellCmd = "powershell.exe",

    [switch]$NoStartupWrapper,

    [switch]$Force
)

$ErrorActionPreference = "Stop"

$StartMarker = "# BEGIN PIRA Codex speech notifications"
$EndMarker = "# END PIRA Codex speech notifications"
$StartupStartMarker = "# BEGIN PIRA Codex startup speech wrapper"
$StartupEndMarker = "# END PIRA Codex startup speech wrapper"

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

function ConvertTo-PowerShellDoubleQuotedArg {
    param([Parameter(Mandatory = $true)][AllowEmptyString()][string]$Value)
    return '"' + $Value.Replace('\', '\\').Replace('"', '\"') + '"'
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
    $block = "$StartMarker`r`n# Non-blocking status audio for Codex on Windows.`r`n$NotifyLine`r`n$EndMarker`r`n`r`n"
    $match = [regex]::Match($Text, '(?m)^\[')
    if ($match.Success) {
        return $Text.Insert($match.Index, $block)
    }
    if ($Text.Length -gt 0 -and -not $Text.EndsWith("`n")) { $Text += "`r`n" }
    return $Text + "`r`n" + $block
}

function Ensure-HooksFeature {
    param([Parameter(Mandatory = $true)][string]$Text)
    $featuresMatch = [regex]::Match($Text, '(?ms)^\[features\]\s*\r?\n(?<body>.*?)(?=^\[|\z)')
    if ($featuresMatch.Success) {
        $body = $featuresMatch.Groups['body'].Value
        if ($body -match '(?m)^\s*hooks\s*=') {
            $body = [regex]::Replace($body, '(?m)^\s*hooks\s*=.*$', 'hooks = true')
        } else {
            $body = "hooks = true`r`n" + $body
        }
        $body = [regex]::Replace($body, '(?m)^\s*codex_hooks\s*=.*\r?\n?', '')
        $replacement = "[features]`r`n" + $body
        return $Text.Substring(0, $featuresMatch.Index) + $replacement + $Text.Substring($featuresMatch.Index + $featuresMatch.Length)
    }
    $block = "[features]`r`nhooks = true`r`n`r`n"
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
# Play waiting audio unless the coding UI is focused.
[[hooks.PermissionRequest]]
matcher = "*"

[[hooks.PermissionRequest.hooks]]
type = "command"
command = $(ConvertTo-TomlBasicString $command)
timeout = 1
statusMessage = "Checking waiting status audio"
$EndMarker
"@
    return $Text + $block
}

function Get-PowerShellProfilePaths {
    $documents = [Environment]::GetFolderPath('MyDocuments')
    @(
        (Join-Path $documents 'PowerShell\Microsoft.PowerShell_profile.ps1'),
        (Join-Path $documents 'WindowsPowerShell\Microsoft.PowerShell_profile.ps1')
    ) | Select-Object -Unique
}

function Install-StartupWrapper {
    param(
        [Parameter(Mandatory = $true)][string]$PlayScript,
        [Parameter(Mandatory = $true)][string]$StartupAudioPath,
        [Parameter(Mandatory = $true)][string]$PowerShellCmd
    )

    $codexCommand = Get-Command codex.cmd -ErrorAction SilentlyContinue
    if (-not $codexCommand) {
        $codexCommand = Get-Command codex.exe -ErrorAction SilentlyContinue
    }
    if (-not $codexCommand) {
        Write-Warning "Could not find codex.cmd or codex.exe; skipping Windows startup audio wrapper."
        return @()
    }

    $codexPath = $codexCommand.Path
    $audioArgLine = '-NoProfile -ExecutionPolicy Bypass -File "' + $PlayScript + '" -AudioPath "' + $StartupAudioPath + '"'

    $block = @"
$StartupStartMarker
# Play a short startup audio notification, then delegate to the real Codex CLI.
# Remove this block or restore the .bak file to disable startup audio.
function codex {
    `$piraCodexCmd = $(ConvertTo-PowerShellSingleQuotedString $codexPath)
    `$piraPlayScript = $(ConvertTo-PowerShellSingleQuotedString $PlayScript)
    if (Test-Path -LiteralPath `$piraPlayScript) {
        `$piraAudioArgs = $(ConvertTo-PowerShellSingleQuotedString $audioArgLine)
        Start-Process -FilePath $(ConvertTo-PowerShellSingleQuotedString $PowerShellCmd) -ArgumentList `$piraAudioArgs -WindowStyle Hidden | Out-Null
    }
    & `$piraCodexCmd @args
}
$StartupEndMarker
"@

    $changedProfiles = @()
    foreach ($profilePath in (Get-PowerShellProfilePaths)) {
        $profileDir = Split-Path -Parent $profilePath
        New-Item -ItemType Directory -Force -Path $profileDir | Out-Null

        $profileText = ""
        if (Test-Path -LiteralPath $profilePath) {
            $profileBackup = "$profilePath.bak.$(Get-Date -Format 'yyyyMMdd-HHmmss')"
            Copy-Item -LiteralPath $profilePath -Destination $profileBackup
            $profileText = Get-Content -LiteralPath $profilePath -Raw
            if ($null -eq $profileText) { $profileText = "" }
        }

        $start = [regex]::Escape($StartupStartMarker)
        $end = [regex]::Escape($StartupEndMarker)
        $pattern = "(?ms)^$start.*?^$end\r?\n?"
        $profileText = [regex]::Replace($profileText, $pattern, "")
        if ($profileText.Length -gt 0 -and -not $profileText.EndsWith("`n")) {
            $profileText += "`r`n"
        }
        $profileText = $profileText.TrimEnd() + "`r`n`r`n" + $block + "`r`n"
        Write-Utf8NoBom $profilePath $profileText
        $changedProfiles += $profilePath
    }
    return $changedProfiles
}

$config = Resolve-UserPath $ConfigPath
$audioDirResolved = Resolve-UserPath $AudioDir
if ([string]::IsNullOrWhiteSpace($StartupAudio)) { $StartupAudio = Join-Path $audioDirResolved 'start_msg.m4a' } else { $StartupAudio = Resolve-UserPath $StartupAudio }
if ([string]::IsNullOrWhiteSpace($FinishedAudio)) { $FinishedAudio = Join-Path $audioDirResolved 'complete_msg.m4a' } else { $FinishedAudio = Resolve-UserPath $FinishedAudio }
if ([string]::IsNullOrWhiteSpace($WaitingAudio)) { $WaitingAudio = Join-Path $audioDirResolved 'waiting_msg.m4a' } else { $WaitingAudio = Resolve-UserPath $WaitingAudio }

foreach ($audio in @($StartupAudio, $FinishedAudio, $WaitingAudio)) {
    if (-not (Test-Path -LiteralPath $audio -PathType Leaf)) {
        Write-Error "Audio file is missing: $audio"
    }
}

$configDir = Split-Path -Parent $config
if ([string]::IsNullOrWhiteSpace($configDir)) { $configDir = "." }
$hooksDir = Join-Path $configDir "hooks"
$notifyScript = Join-Path $hooksDir "speak_notify.ps1"
$waitingScript = Join-Path $hooksDir "speak_waiting.ps1"
$playScript = Join-Path $hooksDir "pira_play_audio.ps1"

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

$playContent = @'
# Detached local audio helper for PIRA Codex notifications on Windows.
param(
    [Parameter(Mandatory = $true)]
    [string]$AudioPath
)

$ErrorActionPreference = "SilentlyContinue"
$resolved = (Resolve-Path -LiteralPath $AudioPath).Path
Add-Type -AssemblyName PresentationCore
$player = New-Object System.Windows.Media.MediaPlayer
$player.Open([Uri]$resolved)
Start-Sleep -Milliseconds 150
$player.Play()

$maxSeconds = 15
$started = Get-Date
while (((Get-Date) - $started).TotalSeconds -lt $maxSeconds) {
    Start-Sleep -Milliseconds 100
    if ($player.NaturalDuration.HasTimeSpan) {
        $duration = $player.NaturalDuration.TimeSpan
        if ($player.Position -ge $duration -and $duration.TotalMilliseconds -gt 0) { break }
    }
}
$player.Close()
'@
Write-Utf8NoBom $playScript $playContent

$notifyContent = @'
# Non-blocking Codex audio notification installed by PIRA for Windows.
param([Parameter(ValueFromRemainingArguments = $true)][string[]]$ArgsFromCodex)

$ErrorActionPreference = "SilentlyContinue"
$FinishedAudio = __FINISHED_AUDIO__
$WaitingAudio = __WAITING_AUDIO__
$PlayScript = Join-Path $PSScriptRoot "pira_play_audio.ps1"

function Quote-ProcessArg {
    param([Parameter(Mandatory = $true)][AllowEmptyString()][string]$Value)
    '"' + $Value.Replace('\', '\\').Replace('"', '\"') + '"'
}

function Get-ForegroundProcessName {
    try {
        $typeDefinition = @"
using System;
using System.Runtime.InteropServices;
public static class PiraForegroundWindow {
    [DllImport("user32.dll")]
    public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")]
    public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);
}
"@
        Add-Type -TypeDefinition $typeDefinition -ErrorAction SilentlyContinue | Out-Null
        $processId = 0
        $hwnd = [PiraForegroundWindow]::GetForegroundWindow()
        if ($hwnd -eq [IntPtr]::Zero) { return "" }
        [void][PiraForegroundWindow]::GetWindowThreadProcessId($hwnd, [ref]$processId)
        if ($processId -eq 0) { return "" }
        return (Get-Process -Id $processId -ErrorAction SilentlyContinue).ProcessName
    } catch {
        return ""
    }
}

function Test-CodexUiSeemsFocused {
    $processName = (Get-ForegroundProcessName).ToLowerInvariant()
    if ([string]::IsNullOrWhiteSpace($processName)) { return $false }
    return @(
        "windowsterminal", "terminal", "wt", "powershell", "pwsh", "cmd",
        "conhost", "wezterm-gui", "wezterm", "warp", "alacritty", "kitty",
        "hyper", "tabby", "rio", "mintty", "conemu64", "conemu", "cmder",
        "mobaxterm", "code", "code-insiders", "vscodium", "cursor", "windsurf",
        "zed", "sublime_text", "notepad++", "notepad++64", "gvim", "neovide",
        "emacs", "devenv", "idea64", "idea", "pycharm64", "pycharm",
        "webstorm64", "webstorm", "clion64", "clion", "goland64", "goland",
        "phpstorm64", "phpstorm", "rider64", "rider", "rubymine64", "rubymine",
        "rustrover64", "rustrover", "datagrip64", "datagrip", "androidstudio64",
        "studio64"
    ) -contains $processName
}

function Start-Audio {
    param(
        [Parameter(Mandatory = $true)][string]$AudioPath,
        [switch]$Always
    )
    if (-not $Always -and (Test-CodexUiSeemsFocused)) { return }
    $args = @(
        "-NoProfile",
        "-ExecutionPolicy", "Bypass",
        "-File", (Quote-ProcessArg $PlayScript),
        "-AudioPath", (Quote-ProcessArg $AudioPath)
    )
    Start-Process -FilePath __POWERSHELL_CMD__ -ArgumentList ($args -join " ") -WindowStyle Hidden | Out-Null
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

# Match the macOS behavior: inspect only the final assistant message; if
# extraction fails, default to "finished" rather than guessing from the full
# payload. Only a question mark is treated as waiting. Waiting audio uses the
# same focus check as completion audio.
if (-not [string]::IsNullOrWhiteSpace($message) -and $message.Contains("?")) {
    Start-Audio $WaitingAudio
} else {
    Start-Audio $FinishedAudio
}
'@
$notifyContent = $notifyContent.Replace('__FINISHED_AUDIO__', (ConvertTo-PowerShellSingleQuotedString $FinishedAudio))
$notifyContent = $notifyContent.Replace('__WAITING_AUDIO__', (ConvertTo-PowerShellSingleQuotedString $WaitingAudio))
$notifyContent = $notifyContent.Replace('__POWERSHELL_CMD__', (ConvertTo-PowerShellSingleQuotedString $PowerShellCmd))
Write-Utf8NoBom $notifyScript $notifyContent

$waitingContent = @'
# Play audio when Codex is waiting for user action, unless the coding UI is focused.
$ErrorActionPreference = "SilentlyContinue"
$PlayScript = Join-Path $PSScriptRoot "pira_play_audio.ps1"
$WaitingAudio = __WAITING_AUDIO__

function Quote-ProcessArg {
    param([Parameter(Mandatory = $true)][AllowEmptyString()][string]$Value)
    '"' + $Value.Replace('\', '\\').Replace('"', '\"') + '"'
}

function Get-ForegroundProcessName {
    try {
        $typeDefinition = @"
using System;
using System.Runtime.InteropServices;
public static class PiraForegroundWindowWaiting {
    [DllImport("user32.dll")]
    public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")]
    public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);
}
"@
        Add-Type -TypeDefinition $typeDefinition -ErrorAction SilentlyContinue | Out-Null
        $processId = 0
        $hwnd = [PiraForegroundWindowWaiting]::GetForegroundWindow()
        if ($hwnd -eq [IntPtr]::Zero) { return "" }
        [void][PiraForegroundWindowWaiting]::GetWindowThreadProcessId($hwnd, [ref]$processId)
        if ($processId -eq 0) { return "" }
        return (Get-Process -Id $processId -ErrorAction SilentlyContinue).ProcessName
    } catch {
        return ""
    }
}

function Test-CodexUiSeemsFocused {
    $processName = (Get-ForegroundProcessName).ToLowerInvariant()
    if ([string]::IsNullOrWhiteSpace($processName)) { return $false }
    return @(
        "windowsterminal", "terminal", "wt", "powershell", "pwsh", "cmd",
        "conhost", "wezterm-gui", "wezterm", "warp", "alacritty", "kitty",
        "hyper", "tabby", "rio", "mintty", "conemu64", "conemu", "cmder",
        "mobaxterm", "code", "code-insiders", "vscodium", "cursor", "windsurf",
        "zed", "sublime_text", "notepad++", "notepad++64", "gvim", "neovide",
        "emacs", "devenv", "idea64", "idea", "pycharm64", "pycharm",
        "webstorm64", "webstorm", "clion64", "clion", "goland64", "goland",
        "phpstorm64", "phpstorm", "rider64", "rider", "rubymine64", "rubymine",
        "rustrover64", "rustrover", "datagrip64", "datagrip", "androidstudio64",
        "studio64"
    ) -contains $processName
}

if (-not (Test-CodexUiSeemsFocused)) {
    $args = @(
        "-NoProfile",
        "-ExecutionPolicy", "Bypass",
        "-File", (Quote-ProcessArg $PlayScript),
        "-AudioPath", (Quote-ProcessArg $WaitingAudio)
    )
    Start-Process -FilePath __POWERSHELL_CMD__ -ArgumentList ($args -join " ") -WindowStyle Hidden | Out-Null
}
Write-Output "{}"
'@
$waitingContent = $waitingContent.Replace('__WAITING_AUDIO__', (ConvertTo-PowerShellSingleQuotedString $WaitingAudio))
$waitingContent = $waitingContent.Replace('__POWERSHELL_CMD__', (ConvertTo-PowerShellSingleQuotedString $PowerShellCmd))
Write-Utf8NoBom $waitingScript $waitingContent

$newText = Remove-TopLevelNotify $textWithoutManaged
$notifyLine = 'notify = [' + (ConvertTo-TomlBasicString $PowerShellCmd) + ', "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", ' + (ConvertTo-TomlBasicString $notifyScript) + ']'
$newText = Insert-TopLevelNotify $newText $notifyLine
$newText = Ensure-HooksFeature $newText
$newText = Add-PermissionHook $newText $waitingScript $PowerShellCmd
Write-Utf8NoBom $config $newText

$changedProfiles = @()
if (-not $NoStartupWrapper) {
    $changedProfiles = Install-StartupWrapper -PlayScript $playScript -StartupAudioPath $StartupAudio -PowerShellCmd $PowerShellCmd
}

Write-Output "Codex audio notification mode installed for Windows."
Write-Output "Config: $config"
Write-Output "Audio directory: $audioDirResolved"
Write-Output "Startup audio: $StartupAudio"
Write-Output "Finished audio: $FinishedAudio"
Write-Output "Waiting audio: $WaitingAudio"
Write-Output "Notify script: $notifyScript"
Write-Output "Waiting hook: $waitingScript"
Write-Output "Audio helper: $playScript"
if ($NoStartupWrapper) {
    Write-Output "Startup wrapper: skipped by -NoStartupWrapper"
} elseif ($changedProfiles.Count -gt 0) {
    foreach ($changedProfile in $changedProfiles) {
        Write-Output "Startup wrapper profile: $changedProfile"
    }
} else {
    Write-Output "Startup wrapper: not installed; codex.cmd/codex.exe was not found."
}
if ($backup) { Write-Output "Backup: $backup" }
Write-Output "Restart Codex to load the new notification settings. Open a new PowerShell window for startup audio."
