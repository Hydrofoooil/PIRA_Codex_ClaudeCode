<#
Install optional Codex speech notifications for Windows.

This script uses Windows built-in SAPI text-to-speech through PowerShell. It
creates non-blocking PowerShell hooks and updates Codex config.toml so normal
completion can say "Pira finished." and waiting states always say
"Pira standing by." By default it also installs a PowerShell profile wrapper
that says "Pira online." when launching `codex`.

Example:
  powershell.exe -ExecutionPolicy Bypass -File "$HOME\agent\assets\setup_codex_audio_mode_windows.ps1" `
    -ConfigPath "$HOME\.codex\config.toml"

Use -NoStartupWrapper to install only completion/waiting notifications.
#>

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$ConfigPath,

    [string]$PowerShellCmd = "powershell.exe",

    [string]$VoiceName = "",

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
# Always speak when Codex is waiting for approval/action.
[[hooks.PermissionRequest]]
matcher = "*"

[[hooks.PermissionRequest.hooks]]
type = "command"
command = $(ConvertTo-TomlBasicString $command)
timeout = 1
statusMessage = "Checking waiting status speech"
$EndMarker
"@
    return $Text + $block
}

function ConvertTo-PowerShellDoubleQuotedArg {
    param([Parameter(Mandatory = $true)][AllowEmptyString()][string]$Value)
    return '"' + $Value.Replace('\', '\\').Replace('"', '\"') + '"'
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
        [Parameter(Mandatory = $true)][string]$SayScript,
        [Parameter(Mandatory = $true)][string]$PowerShellCmd
    )

    $codexCommand = Get-Command codex.cmd -ErrorAction SilentlyContinue
    if (-not $codexCommand) {
        $codexCommand = Get-Command codex.exe -ErrorAction SilentlyContinue
    }
    if (-not $codexCommand) {
        Write-Warning "Could not find codex.cmd or codex.exe; skipping Windows startup speech wrapper."
        return @()
    }

    $codexPath = $codexCommand.Path
    $speechArgLine = '-NoProfile -ExecutionPolicy Bypass -File "' + $SayScript + '" -Text "Pira online." -Rate 1'
    if (-not [string]::IsNullOrWhiteSpace($VoiceName)) {
        $speechArgLine += ' -VoiceName ' + (ConvertTo-PowerShellDoubleQuotedArg $VoiceName)
    }

    $block = @"
$StartupStartMarker
# Say a short startup notification, then delegate to the real Codex CLI.
# Remove this block or restore the .bak file to disable startup speech.
function codex {
    `$piraCodexCmd = $(ConvertTo-PowerShellSingleQuotedString $codexPath)
    `$piraSayScript = $(ConvertTo-PowerShellSingleQuotedString $SayScript)
    if (Test-Path -LiteralPath `$piraSayScript) {
        `$piraSpeechArgs = $(ConvertTo-PowerShellSingleQuotedString $speechArgLine)
        Start-Process -FilePath $(ConvertTo-PowerShellSingleQuotedString $PowerShellCmd) -ArgumentList `$piraSpeechArgs -WindowStyle Hidden | Out-Null
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
$configDir = Split-Path -Parent $config
if ([string]::IsNullOrWhiteSpace($configDir)) { $configDir = "." }
$hooksDir = Join-Path $configDir "hooks"
$notifyScript = Join-Path $hooksDir "speak_notify.ps1"
$waitingScript = Join-Path $hooksDir "speak_waiting.ps1"
$sayScript = Join-Path $hooksDir "pira_say.ps1"

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

$sayContent = @'
# Detached speech helper for PIRA Codex notifications on Windows.
param(
    [Parameter(Mandatory = $true)]
    [string]$Text,
    [string]$VoiceName = "",
    [int]$Rate = 0,
    [int]$Volume = 100
)

$ErrorActionPreference = "SilentlyContinue"
$speaker = New-Object -ComObject SAPI.SpVoice
if ($Rate -ne 0) { $speaker.Rate = $Rate }
if ($Volume -ge 0 -and $Volume -le 100) { $speaker.Volume = $Volume }
if (-not [string]::IsNullOrWhiteSpace($VoiceName)) {
    foreach ($voice in $speaker.GetVoices()) {
        if ($voice.GetDescription() -like "*$VoiceName*") {
            $speaker.Voice = $voice
            break
        }
    }
}
# Synchronous speech is reliable here because this script is launched in a
# detached process by the hook scripts. Codex does not wait for this to finish.
[void]$speaker.Speak($Text, 0)
'@
Write-Utf8NoBom $sayScript $sayContent

$notifyContent = @'
# Non-blocking Codex speech notification installed by PIRA for Windows.
param([Parameter(ValueFromRemainingArguments = $true)][string[]]$ArgsFromCodex)

$ErrorActionPreference = "SilentlyContinue"
$FinishedText = "Pira finished."
$WaitingText = "Pira standing by."
$VoiceName = __VOICE_NAME__
$SayScript = Join-Path $PSScriptRoot "pira_say.ps1"

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

function Start-Speech {
    param(
        [Parameter(Mandatory = $true)][string]$Text,
        [int]$Rate = 0,
        [int]$Volume = 100,
        [switch]$Always
    )
    if (-not $Always -and (Test-CodexUiSeemsFocused)) { return }
    $args = @(
        "-NoProfile",
        "-ExecutionPolicy", "Bypass",
        "-File", (Quote-ProcessArg $SayScript),
        "-Text", (Quote-ProcessArg $Text),
        "-Volume", ([string]$Volume)
    )
    if ($Rate -ne 0) {
        $args += @("-Rate", ([string]$Rate))
    }
    if (-not [string]::IsNullOrWhiteSpace($VoiceName)) {
        $args += @("-VoiceName", (Quote-ProcessArg $VoiceName))
    }
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
# payload, which can contain the user's prompt text.
$waitingPattern = "\?|confirm|confirmation|approve|approval|permission|do you want|would you like|should i|shall i|may i|please confirm|please approve|waiting for|need your|needs your|reply|respond|choose|select|pick|can i|could i"
if (-not [string]::IsNullOrWhiteSpace($message) -and $message -match $waitingPattern) {
    Start-Speech $WaitingText -Rate 2 -Volume 85 -Always
} else {
    Start-Speech $FinishedText -Rate 1
}
'@
$notifyContent = $notifyContent.Replace('__VOICE_NAME__', (ConvertTo-PowerShellSingleQuotedString $VoiceName))
$notifyContent = $notifyContent.Replace('__POWERSHELL_CMD__', (ConvertTo-PowerShellSingleQuotedString $PowerShellCmd))
Write-Utf8NoBom $notifyScript $notifyContent

$waitingContent = @'
# Always speak when Codex is waiting for user action, without blocking.
$ErrorActionPreference = "SilentlyContinue"
$SayScript = Join-Path $PSScriptRoot "pira_say.ps1"
$VoiceName = __VOICE_NAME__

function Quote-ProcessArg {
    param([Parameter(Mandatory = $true)][AllowEmptyString()][string]$Value)
    '"' + $Value.Replace('\', '\\').Replace('"', '\"') + '"'
}

$args = @(
    "-NoProfile",
    "-ExecutionPolicy", "Bypass",
    "-File", (Quote-ProcessArg $SayScript),
    "-Text", (Quote-ProcessArg "Pira standing by."),
    "-Rate", "2",
    "-Volume", "85"
)
if (-not [string]::IsNullOrWhiteSpace($VoiceName)) {
    $args += @("-VoiceName", (Quote-ProcessArg $VoiceName))
}
Start-Process -FilePath __POWERSHELL_CMD__ -ArgumentList ($args -join " ") -WindowStyle Hidden | Out-Null
Write-Output "{}"
'@
$waitingContent = $waitingContent.Replace('__VOICE_NAME__', (ConvertTo-PowerShellSingleQuotedString $VoiceName))
$waitingContent = $waitingContent.Replace('__POWERSHELL_CMD__', (ConvertTo-PowerShellSingleQuotedString $PowerShellCmd))
Write-Utf8NoBom $waitingScript $waitingContent

$newText = Remove-TopLevelNotify $textWithoutManaged
$notifyLine = 'notify = [' + (ConvertTo-TomlBasicString $PowerShellCmd) + ', "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", ' + (ConvertTo-TomlBasicString $notifyScript) + ']'
$newText = Insert-TopLevelNotify $newText $notifyLine
$newText = Ensure-CodexHooksFeature $newText
$newText = Add-PermissionHook $newText $waitingScript $PowerShellCmd
Write-Utf8NoBom $config $newText

$changedProfiles = @()
if (-not $NoStartupWrapper) {
    $changedProfiles = Install-StartupWrapper -SayScript $sayScript -PowerShellCmd $PowerShellCmd
}

Write-Output "Codex speech notification mode installed for Windows."
Write-Output "Config: $config"
Write-Output "Notify script: $notifyScript"
Write-Output "Waiting hook: $waitingScript"
Write-Output "Speech helper: $sayScript"
if ($NoStartupWrapper) {
    Write-Output "Startup wrapper: skipped by -NoStartupWrapper"
} elseif ($changedProfiles.Count -gt 0) {
    foreach ($changedProfile in $changedProfiles) {
        Write-Output "Startup wrapper profile: $changedProfile"
    }
    Write-Output "Startup phrase: Pira online."
} else {
    Write-Output "Startup wrapper: not installed; codex.cmd/codex.exe was not found."
}
if ($backup) { Write-Output "Backup: $backup" }
Write-Output "Restart Codex to load the new notification settings. Open a new PowerShell window for startup speech."
