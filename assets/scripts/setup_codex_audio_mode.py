#!/usr/bin/env python3
"""Shared Codex audio notification setup for PIRA.

This stdlib-only helper owns config editing, backups, idempotency, audio-file
validation, generated hook scripts, and legacy startup-wrapper cleanup for macOS
and Windows. Platform shell/PowerShell files are thin compatibility wrappers.
"""

from __future__ import annotations

import argparse
import os
import platform as platform_module
import re
import shlex
import shutil
import stat
import subprocess
import sys
import tempfile
from datetime import datetime
from pathlib import Path
from typing import Literal

START = "# BEGIN PIRA Codex speech notifications"
END = "# END PIRA Codex speech notifications"
MAC_STARTUP_START = "# BEGIN PIRA Codex startup audio"
MAC_STARTUP_END = "# END PIRA Codex startup audio"
WIN_STARTUP_START = "# BEGIN PIRA Codex startup speech wrapper"
WIN_STARTUP_END = "# END PIRA Codex startup speech wrapper"

MAC_NOTIFY_TEMPLATE = r"""#!/usr/bin/env bash
# Non-blocking Codex audio notification installed by PIRA.
set -euo pipefail
PLAYER_CMD=__PLAYER_CMD__
FINISHED_AUDIO=__FINISHED_AUDIO__
WAITING_AUDIO=__WAITING_AUDIO__

# Codex sets these variables for commands run by an existing agent turn.
# If that command launches a nested Codex process, keep the child session silent.
running_under_codex_exec() {
  [ "${CODEX_CI:-}" = "1" ] || [ -n "${CODEX_THREAD_ID:-}" ]
}

if running_under_codex_exec; then
  case "${PIRA_CODEX_ALLOW_NESTED_AUDIO:-}" in
    1|true|TRUE|yes|YES) ;;
    *) exit 0 ;;
  esac
fi

payload="$*"
if [ -z "$payload" ]; then
  payload="$(cat || true)"
fi

frontmost_identity() {
  osascript <<'OSA' 2>/dev/null || true
tell application "System Events"
  set p to first application process whose frontmost is true
  set n to name of p
  set bid to ""
  try
    set bid to bundle identifier of p
  end try
  set uid to ""
  try
    set uid to unix id of p as text
  end try
  return n & linefeed & bid & linefeed & uid
end tell
OSA
}

codex_ui_seems_focused() {
  # Best-effort focus check: if a terminal/editor app is frontmost, assume the
  # user may already be looking at Codex and avoid redundant completion audio.
  # Some apps, especially VS Code-like Electron builds, may report their process
  # name as "Electron", so we also inspect bundle id and process command/path.
  identity="$(frontmost_identity)"
  [ -n "$identity" ] || return 1
  pid="$(printf '%s\n' "$identity" | sed -n '3p')"
  process_info=""
  case "$pid" in
    ''|*[!0-9]*) ;;
    *) process_info="$(ps -p "$pid" -o comm= -o args= 2>/dev/null || true)" ;;
  esac
  haystack="$(printf '%s\n%s' "$identity" "$process_info" | tr '[:upper:]' '[:lower:]')"
  case "$haystack" in
    *terminal*|*iterm2*|*warp*|*wezterm*|*ghostty*|*alacritty*|*kitty*|*hyper*|*tabby*|*rio*|*black\ box*|*gnome\ terminal*|*konsole*|*tilix*|*xterm*|*mintty*|*cmder*|*conemu*|*mobaxterm*|*visual\ studio\ code*|*com.microsoft.vscode*|*code-insiders*|*vscodium*|*code\ -\ oss*|*cursor*|*todesktop*|*windsurf*|*codeium*|*zed*|*sublime\ text*|*textmate*|*bbedit*|*nova*|*macvim*|*neovide*|*emacs*|*xcode*|*android\ studio*|*intellij\ idea*|*idea*|*pycharm*|*webstorm*|*clion*|*goland*|*phpstorm*|*rider*|*rubymine*|*rustrover*|*datagrip*)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

play_unless_focused() {
  audio="$1"
  if codex_ui_seems_focused; then
    exit 0
  fi
  "$PLAYER_CMD" "$audio" >/dev/null 2>&1 &
}

json_unescape_minimal() {
  sed -e 's/\\\\"/"/g' \\
      -e 's/\\\\n/ /g' \\
      -e 's/\\\\r/ /g' \\
      -e 's/\\\\t/ /g' \\
      -e 's/\\\\\\\\/\\\\/g'
}

extract_turn_id() {
  printf '%s' "$payload" | tr '\n' ' ' | sed -nE \\
    's/.*"turn[-_]id"[[:space:]]*:[[:space:]]*"([^"\\\\]+)".*/\\1/p' | head -n 1
}

turn_is_from_subagent() {
  turn_id="$(extract_turn_id)"
  [ -n "$turn_id" ] || return 1
  sessions_dir="${PIRA_CODEX_SESSIONS_DIR:-$HOME/.codex/sessions}"
  [ -d "$sessions_dir" ] || return 1
  session_file="$(find "$sessions_dir" -type f -name '*.jsonl' -print 2>/dev/null \\
    | xargs grep -l "$turn_id" 2>/dev/null \\
    | head -n 1 || true)"
  [ -n "$session_file" ] || return 1
  head -n 1 "$session_file" 2>/dev/null | grep -q '"subagent"'
}

extract_last_assistant_message() {
  # Best-effort extraction without Python or jq. We intentionally inspect only
  # the final assistant message, not the full payload, to avoid classifying the
  # user's prompt as a pending action request.
  printf '%s' "$payload" | tr '\n' ' ' | sed -nE \\
    's/.*"last[-_]assistant[-_]message"[[:space:]]*:[[:space:]]*"(([^"\\\\]|\\\\.)*)".*/\\1/p' \\
    | json_unescape_minimal
}

if turn_is_from_subagent; then
  exit 0
fi

message="$(extract_last_assistant_message | head -n 1)"

# If extraction fails, default to "finished" rather than guessing from the full
# JSON payload. Only a question mark in the final assistant message is treated
# as waiting. Waiting audio uses the same focus check as completion audio.
if [ -n "$message" ] && printf '%s' "$message" | grep -q '?'; then
  play_unless_focused "$WAITING_AUDIO"
else
  play_unless_focused "$FINISHED_AUDIO"
fi"""

MAC_WAITING_TEMPLATE = r"""#!/usr/bin/env bash
# Play audio when the user-facing Codex agent is waiting for user action.
set -euo pipefail
PLAYER_CMD=__PLAYER_CMD__
WAITING_AUDIO=__WAITING_AUDIO__

# Codex sets these variables for commands run by an existing agent turn.
# If that command launches a nested Codex process, keep the child session silent.
running_under_codex_exec() {
  [ "${CODEX_CI:-}" = "1" ] || [ -n "${CODEX_THREAD_ID:-}" ]
}

if running_under_codex_exec; then
  case "${PIRA_CODEX_ALLOW_NESTED_AUDIO:-}" in
    1|true|TRUE|yes|YES) ;;
    *) printf '{}\n'; exit 0 ;;
  esac
fi

payload="$*"
if [ -z "$payload" ] && [ ! -t 0 ]; then
  payload="$(cat || true)"
fi

extract_turn_id() {
  printf '%s' "$payload" | tr '\n' ' ' | sed -nE \\
    's/.*"turn[-_]id"[[:space:]]*:[[:space:]]*"([^"\\\\]+)".*/\\1/p' | head -n 1
}

turn_is_from_subagent() {
  turn_id="$(extract_turn_id)"
  [ -n "$turn_id" ] || return 1
  sessions_dir="${PIRA_CODEX_SESSIONS_DIR:-$HOME/.codex/sessions}"
  [ -d "$sessions_dir" ] || return 1
  session_file="$(find "$sessions_dir" -type f -name '*.jsonl' -print 2>/dev/null \\
    | xargs grep -l "$turn_id" 2>/dev/null \\
    | head -n 1 || true)"
  [ -n "$session_file" ] || return 1
  head -n 1 "$session_file" 2>/dev/null | grep -q '"subagent"'
}

frontmost_identity() {
  osascript <<'OSA' 2>/dev/null || true
tell application "System Events"
  set p to first application process whose frontmost is true
  set n to name of p
  set bid to ""
  try
    set bid to bundle identifier of p
  end try
  set uid to ""
  try
    set uid to unix id of p as text
  end try
  return n & linefeed & bid & linefeed & uid
end tell
OSA
}

codex_ui_seems_focused() {
  identity="$(frontmost_identity)"
  [ -n "$identity" ] || return 1
  pid="$(printf '%s\n' "$identity" | sed -n '3p')"
  process_info=""
  case "$pid" in
    ''|*[!0-9]*) ;;
    *) process_info="$(ps -p "$pid" -o comm= -o args= 2>/dev/null || true)" ;;
  esac
  haystack="$(printf '%s\n%s' "$identity" "$process_info" | tr '[:upper:]' '[:lower:]')"
  case "$haystack" in
    *terminal*|*iterm2*|*warp*|*wezterm*|*ghostty*|*alacritty*|*kitty*|*hyper*|*tabby*|*rio*|*black\ box*|*gnome\ terminal*|*konsole*|*tilix*|*xterm*|*mintty*|*cmder*|*conemu*|*mobaxterm*|*visual\ studio\ code*|*com.microsoft.vscode*|*code-insiders*|*vscodium*|*code\ -\ oss*|*cursor*|*todesktop*|*windsurf*|*codeium*|*zed*|*sublime\ text*|*textmate*|*bbedit*|*nova*|*macvim*|*neovide*|*emacs*|*xcode*|*android\ studio*|*intellij\ idea*|*idea*|*pycharm*|*webstorm*|*clion*|*goland*|*phpstorm*|*rider*|*rubymine*|*rustrover*|*datagrip*)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

if turn_is_from_subagent; then
  printf '{}\n'
  exit 0
fi

if ! codex_ui_seems_focused; then
  "$PLAYER_CMD" "$WAITING_AUDIO" >/dev/null 2>&1 &
fi
printf '{}\n'
"""

WIN_PLAY_TEMPLATE = r"""# Detached local audio helper for PIRA Codex notifications on Windows.
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
$player.Close()"""

WIN_NOTIFY_TEMPLATE = r"""# Non-blocking Codex audio notification installed by PIRA for Windows.
param([Parameter(ValueFromRemainingArguments = $true)][string[]]$ArgsFromCodex)

$ErrorActionPreference = "SilentlyContinue"
$FinishedAudio = __FINISHED_AUDIO__
$WaitingAudio = __WAITING_AUDIO__
$PlayScript = Join-Path $PSScriptRoot "pira_play_audio.ps1"

function Test-RunningUnderCodexExec {
    return ($env:CODEX_CI -eq "1") -or (-not [string]::IsNullOrWhiteSpace($env:CODEX_THREAD_ID))
}

if ((Test-RunningUnderCodexExec) -and -not (@("1", "true", "TRUE", "yes", "YES") -contains $env:PIRA_CODEX_ALLOW_NESTED_AUDIO)) {
    exit 0
}

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

function Get-TurnIdFromPayload {
    param([AllowEmptyString()][string]$Payload)
    try {
        if ([string]::IsNullOrWhiteSpace($Payload)) { return "" }
        $json = $Payload | ConvertFrom-Json
        if ($json.PSObject.Properties.Name -contains "turn_id") { return [string]$json.turn_id }
        if ($json.PSObject.Properties.Name -contains "turn-id") { return [string]$json.'turn-id' }
    } catch {}
    if ($Payload -match '"turn[-_]id"\s*:\s*"([^"\\]+)"') { return $Matches[1] }
    return ""
}

function Test-SubagentPayload {
    param([AllowEmptyString()][string]$Payload)
    $turnId = Get-TurnIdFromPayload $Payload
    if ([string]::IsNullOrWhiteSpace($turnId)) { return $false }
    $sessionsDir = $env:PIRA_CODEX_SESSIONS_DIR
    if ([string]::IsNullOrWhiteSpace($sessionsDir)) { $sessionsDir = Join-Path $HOME ".codex\sessions" }
    if (-not (Test-Path -LiteralPath $sessionsDir -PathType Container)) { return $false }
    try {
        $files = Get-ChildItem -LiteralPath $sessionsDir -Recurse -Filter "*.jsonl" -File -ErrorAction SilentlyContinue |
            Sort-Object LastWriteTime -Descending |
            Select-Object -First 200
        foreach ($file in $files) {
            if (Select-String -LiteralPath $file.FullName -SimpleMatch $turnId -Quiet -ErrorAction SilentlyContinue) {
                $firstLine = Get-Content -LiteralPath $file.FullName -TotalCount 1 -ErrorAction SilentlyContinue
                return [regex]::IsMatch([string]$firstLine, '"subagent"')
            }
        }
    } catch {}
    return $false
}

$payload = ($ArgsFromCodex -join " ")
if ([string]::IsNullOrWhiteSpace($payload)) {
    $payload = [Console]::In.ReadToEnd()
}

if (Test-SubagentPayload $payload) { exit 0 }

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
}"""

WIN_WAITING_TEMPLATE = r"""# Play audio when the user-facing Codex agent is waiting for user action.
param([Parameter(ValueFromRemainingArguments = $true)][string[]]$ArgsFromCodex)

$ErrorActionPreference = "SilentlyContinue"
$PlayScript = Join-Path $PSScriptRoot "pira_play_audio.ps1"
$WaitingAudio = __WAITING_AUDIO__

function Test-RunningUnderCodexExec {
    return ($env:CODEX_CI -eq "1") -or (-not [string]::IsNullOrWhiteSpace($env:CODEX_THREAD_ID))
}

if ((Test-RunningUnderCodexExec) -and -not (@("1", "true", "TRUE", "yes", "YES") -contains $env:PIRA_CODEX_ALLOW_NESTED_AUDIO)) {
    Write-Output "{}"
    exit 0
}

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

function Get-TurnIdFromPayload {
    param([AllowEmptyString()][string]$Payload)
    try {
        if ([string]::IsNullOrWhiteSpace($Payload)) { return "" }
        $json = $Payload | ConvertFrom-Json
        if ($json.PSObject.Properties.Name -contains "turn_id") { return [string]$json.turn_id }
        if ($json.PSObject.Properties.Name -contains "turn-id") { return [string]$json.'turn-id' }
    } catch {}
    if ($Payload -match '"turn[-_]id"\s*:\s*"([^"\\]+)"') { return $Matches[1] }
    return ""
}

function Test-SubagentPayload {
    param([AllowEmptyString()][string]$Payload)
    $turnId = Get-TurnIdFromPayload $Payload
    if ([string]::IsNullOrWhiteSpace($turnId)) { return $false }
    $sessionsDir = $env:PIRA_CODEX_SESSIONS_DIR
    if ([string]::IsNullOrWhiteSpace($sessionsDir)) { $sessionsDir = Join-Path $HOME ".codex\sessions" }
    if (-not (Test-Path -LiteralPath $sessionsDir -PathType Container)) { return $false }
    try {
        $files = Get-ChildItem -LiteralPath $sessionsDir -Recurse -Filter "*.jsonl" -File -ErrorAction SilentlyContinue |
            Sort-Object LastWriteTime -Descending |
            Select-Object -First 200
        foreach ($file in $files) {
            if (Select-String -LiteralPath $file.FullName -SimpleMatch $turnId -Quiet -ErrorAction SilentlyContinue) {
                $firstLine = Get-Content -LiteralPath $file.FullName -TotalCount 1 -ErrorAction SilentlyContinue
                return [regex]::IsMatch([string]$firstLine, '"subagent"')
            }
        }
    } catch {}
    return $false
}

$payload = ($ArgsFromCodex -join " ")
if ([string]::IsNullOrWhiteSpace($payload) -and -not [Console]::IsInputRedirected) { $payload = "" }
elseif ([string]::IsNullOrWhiteSpace($payload)) { $payload = [Console]::In.ReadToEnd() }

if (Test-SubagentPayload $payload) {
    Write-Output "{}"
    exit 0
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
"""

def _normalize_bash_template(value: str) -> str:
    lines = []
    for line in value.splitlines():
        if line.endswith("\\\\"):
            line = line[:-1]
        lines.append(line)
    return "\n".join(lines)


MAC_NOTIFY_TEMPLATE = _normalize_bash_template(MAC_NOTIFY_TEMPLATE)
MAC_WAITING_TEMPLATE = _normalize_bash_template(MAC_WAITING_TEMPLATE)


def expand_path(value: str) -> Path:
    path = Path(os.path.expandvars(os.path.expanduser(value)))
    if path.is_absolute():
        return path
    return Path.cwd() / path


def toml_basic_string(value: str) -> str:
    return '"' + value.replace('\\', '\\\\').replace('"', '\\"') + '"'


def ps_single_quoted(value: str) -> str:
    return "'" + value.replace("'", "''") + "'"


def backup_path(path: Path) -> Path:
    stamp = datetime.now().strftime("%Y%m%d-%H%M%S%f")
    candidate = path.with_name(f"{path.name}.bak.{stamp}")
    suffix = 1
    while candidate.exists() or candidate.is_symlink():
        candidate = path.with_name(f"{path.name}.bak.{stamp}.{suffix}")
        suffix += 1
    return candidate


def split_before_first_section(text: str) -> tuple[str, str]:
    match = re.search(r"(?m)^\s*\[", text)
    if not match:
        return text, ""
    return text[: match.start()], text[match.start() :]


def remove_managed_blocks(text: str) -> str:
    pattern = rf"(?ms)^{re.escape(START)}.*?^{re.escape(END)}\r?\n?"
    return re.sub(pattern, "", text)


def has_top_level_notify(text: str) -> bool:
    preamble, _ = split_before_first_section(text)
    return re.search(r"(?m)^\s*notify\s*=", preamble) is not None


def remove_top_level_notify(text: str) -> str:
    preamble, rest = split_before_first_section(text)
    preamble = re.sub(r"(?m)^\s*notify\s*=.*\r?\n?", "", preamble)
    return preamble + rest


def insert_before_first_section(text: str, block: str) -> str:
    match = re.search(r"(?m)^\s*\[", text)
    if match:
        return text[: match.start()] + block + text[match.start() :]
    if text and not text.endswith("\n"):
        text += "\n"
    return text + ("\n" if text else "") + block

def insert_top_level_notify(text: str, notify_line: str, platform_name: str) -> str:
    block = f"{START}\n# Non-blocking status audio for Codex on {platform_name}.\n{notify_line}\n{END}\n\n"
    return insert_before_first_section(text, block)

def ensure_hooks_feature(text: str) -> str:
    match = re.search(r"(?ms)^\[features\]\s*\r?\n(?P<body>.*?)(?=^\[|\Z)", text)
    if match:
        body = match.group("body")
        lines = body.splitlines(keepends=True)
        out: list[str] = []
        saw_hooks = False
        for line in lines:
            if re.match(r"\s*codex_hooks\s*=", line):
                continue
            if re.match(r"\s*hooks\s*=", line):
                if not saw_hooks:
                    out.append("hooks = true\n")
                    saw_hooks = True
                continue
            out.append(line)
        if not saw_hooks:
            out.insert(0, "hooks = true\n")
        replacement = "[features]\n" + "".join(out)
        return text[: match.start()] + replacement + text[match.end() :]
    return insert_before_first_section(text, "[features]\nhooks = true\n\n")

def add_permission_hook(text: str, command: str) -> str:
    block = f"""
{START}
# Play waiting audio only for the user-facing agent unless the coding UI is focused.
[[hooks.PermissionRequest]]
matcher = "*"

[[hooks.PermissionRequest.hooks]]
type = "command"
command = {toml_basic_string(command)}
timeout = 1
statusMessage = "Checking waiting status audio"
{END}
"""
    return text + block


def write_executable(path: Path, content: str) -> None:
    path.write_text(content, encoding="utf-8")
    mode = path.stat().st_mode
    path.chmod(mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)


def remove_legacy_block(path: Path, start: str, end: str, validate_cmd: list[str] | None = None) -> Path | None:
    if not path.exists() or start not in path.read_text(encoding="utf-8", errors="replace"):
        return None
    original = path.read_text(encoding="utf-8", errors="replace")
    pattern = rf"(?ms)^{re.escape(start)}.*?^{re.escape(end)}\r?\n?"
    cleaned = re.sub(pattern, "", original)
    with tempfile.NamedTemporaryFile("w", encoding="utf-8", delete=False) as handle:
        handle.write(cleaned)
        temp_name = handle.name
    temp_path = Path(temp_name)
    try:
        if validate_cmd:
            subprocess.run(validate_cmd + [str(temp_path)], check=True)
        backup = backup_path(path)
        shutil.copy2(path, backup)
        path.write_text(cleaned, encoding="utf-8")
        return backup
    finally:
        temp_path.unlink(missing_ok=True)


def windows_documents_dir(powershell_cmd: str) -> Path:
    try:
        completed = subprocess.run(
            [powershell_cmd, "-NoProfile", "-Command", "[Environment]::GetFolderPath('MyDocuments')"],
            check=False,
            capture_output=True,
            text=True,
            timeout=10,
        )
        value = completed.stdout.strip()
        if completed.returncode == 0 and value:
            return Path(value)
    except (OSError, subprocess.SubprocessError):
        pass
    return Path.home() / "Documents"


def remove_legacy_startup(platform_name: Literal["macos", "windows"], zshrc: Path, powershell_cmd: str) -> list[tuple[Path, Path]]:
    removed: list[tuple[Path, Path]] = []
    if platform_name == "macos":
        validate = ["zsh", "-f", "-n"] if shutil.which("zsh") else None
        backup = remove_legacy_block(zshrc, MAC_STARTUP_START, MAC_STARTUP_END, validate)
        if backup:
            removed.append((zshrc, backup))
        return removed

    docs = windows_documents_dir(powershell_cmd)
    for profile_path in [
        docs / "PowerShell" / "Microsoft.PowerShell_profile.ps1",
        docs / "WindowsPowerShell" / "Microsoft.PowerShell_profile.ps1",
    ]:
        backup = remove_legacy_block(profile_path, WIN_STARTUP_START, WIN_STARTUP_END, None)
        if backup:
            removed.append((profile_path, backup))
    return removed


def configure_macos(args: argparse.Namespace, config: Path, hooks_dir: Path, finished_audio: Path, waiting_audio: Path) -> tuple[Path, Path]:
    player_cmd = expand_path(args.player_cmd or "/usr/bin/afplay")
    if not os.access(player_cmd, os.X_OK):
        raise RuntimeError(f"audio player command is not executable: {player_cmd}")
    notify_script = hooks_dir / "speak_notify.sh"
    waiting_script = hooks_dir / "speak_waiting.sh"
    notify = MAC_NOTIFY_TEMPLATE.replace("__PLAYER_CMD__", shlex.quote(str(player_cmd)))
    notify = notify.replace("__FINISHED_AUDIO__", shlex.quote(str(finished_audio)))
    notify = notify.replace("__WAITING_AUDIO__", shlex.quote(str(waiting_audio)))
    waiting = MAC_WAITING_TEMPLATE.replace("__PLAYER_CMD__", shlex.quote(str(player_cmd)))
    waiting = waiting.replace("__WAITING_AUDIO__", shlex.quote(str(waiting_audio)))
    write_executable(notify_script, notify)
    write_executable(waiting_script, waiting)
    return notify_script, waiting_script


def configure_windows(args: argparse.Namespace, config: Path, hooks_dir: Path, finished_audio: Path, waiting_audio: Path) -> tuple[Path, Path, Path]:
    powershell_cmd = args.powershell_cmd or "powershell.exe"
    play_script = hooks_dir / "pira_play_audio.ps1"
    notify_script = hooks_dir / "speak_notify.ps1"
    waiting_script = hooks_dir / "speak_waiting.ps1"
    play_script.write_text(WIN_PLAY_TEMPLATE, encoding="utf-8")
    notify = WIN_NOTIFY_TEMPLATE.replace("__FINISHED_AUDIO__", ps_single_quoted(str(finished_audio)))
    notify = notify.replace("__WAITING_AUDIO__", ps_single_quoted(str(waiting_audio)))
    notify = notify.replace("__POWERSHELL_CMD__", ps_single_quoted(powershell_cmd))
    waiting = WIN_WAITING_TEMPLATE.replace("__WAITING_AUDIO__", ps_single_quoted(str(waiting_audio)))
    waiting = waiting.replace("__POWERSHELL_CMD__", ps_single_quoted(powershell_cmd))
    notify_script.write_text(notify, encoding="utf-8")
    waiting_script.write_text(waiting, encoding="utf-8")
    return notify_script, waiting_script, play_script


def detect_platform(value: str) -> Literal["macos", "windows"]:
    if value != "auto":
        return value  # type: ignore[return-value]
    system = platform_module.system().lower()
    if system == "darwin":
        return "macos"
    if system == "windows":
        return "windows"
    raise RuntimeError("Audio setup is supported only on macOS and Windows")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Install optional Codex audio notifications for PIRA.")
    parser.add_argument("--platform", choices=["auto", "macos", "windows"], default="auto")
    parser.add_argument("--config", required=True, help="Path to Codex config.toml.")
    parser.add_argument("--audio-dir", default="~/agent/PIRA_Voice/Samantha", help="Directory containing complete_msg.m4a and waiting_msg.m4a.")
    parser.add_argument("--finished-audio", default="", help="Completion audio file. Default: AUDIO_DIR/complete_msg.m4a.")
    parser.add_argument("--waiting-audio", default="", help="Waiting/approval audio file. Default: AUDIO_DIR/waiting_msg.m4a.")
    parser.add_argument("--player-cmd", default="", help="macOS audio player command. Default: /usr/bin/afplay.")
    parser.add_argument("--powershell-cmd", default="powershell.exe", help="Windows PowerShell command. Default: powershell.exe.")
    parser.add_argument("--zshrc", default="~/.zshrc", help="macOS zsh config to clean up legacy PIRA startup wrappers.")
    parser.add_argument("--force", action="store_true", help="Replace an existing top-level notify entry after backing up config.")
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        platform_name = detect_platform(args.platform)
        config = expand_path(args.config)
        audio_dir = expand_path(args.audio_dir)
        finished_audio = expand_path(args.finished_audio) if args.finished_audio else audio_dir / "complete_msg.m4a"
        waiting_audio = expand_path(args.waiting_audio) if args.waiting_audio else audio_dir / "waiting_msg.m4a"
        for audio in [finished_audio, waiting_audio]:
            if not audio.is_file():
                raise RuntimeError(f"Audio file is missing: {audio}")
            if not os.access(audio, os.R_OK):
                raise RuntimeError(f"Audio file is not readable: {audio}")

        config.parent.mkdir(parents=True, exist_ok=True)
        if not config.exists():
            config.touch()
        original = config.read_text(encoding="utf-8")
        unmanaged = remove_managed_blocks(original)
        if has_top_level_notify(unmanaged) and not args.force:
            raise RuntimeError("Refusing to replace an existing top-level notify entry without --force. Inspect it first, then rerun with --force if replacement is acceptable.")

        backup = None
        if config.stat().st_size > 0:
            backup = backup_path(config)
            shutil.copy2(config, backup)

        hooks_dir = config.parent / "hooks"
        hooks_dir.mkdir(parents=True, exist_ok=True)
        if platform_name == "macos":
            notify_script, waiting_script = configure_macos(args, config, hooks_dir, finished_audio, waiting_audio)
            notify_line = f'notify = ["/bin/bash", {toml_basic_string(str(notify_script))}]'
            waiting_command = "/bin/bash " + shlex.quote(str(waiting_script))
            play_script = None
        else:
            notify_script, waiting_script, play_script = configure_windows(args, config, hooks_dir, finished_audio, waiting_audio)
            notify_line = "notify = [" + toml_basic_string(args.powershell_cmd) + ', "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", ' + toml_basic_string(str(notify_script)) + "]"
            waiting_command = f'{args.powershell_cmd} -NoProfile -ExecutionPolicy Bypass -File "{waiting_script}"'

        new_text = remove_top_level_notify(unmanaged)
        new_text = insert_top_level_notify(new_text, notify_line, "macOS" if platform_name == "macos" else "Windows")
        new_text = ensure_hooks_feature(new_text)
        new_text = add_permission_hook(new_text, waiting_command)
        config.write_text(new_text, encoding="utf-8")

        removed = remove_legacy_startup(platform_name, expand_path(args.zshrc), args.powershell_cmd)

        print(f"Codex audio notification mode installed for {'macOS' if platform_name == 'macos' else 'Windows'}.")
        print(f"Config: {config}")
        print(f"Audio directory: {audio_dir}")
        print(f"Finished audio: {finished_audio}")
        print(f"Waiting audio: {waiting_audio}")
        print(f"Notify script: {notify_script}")
        print(f"Waiting hook: {waiting_script}")
        if platform_name == "macos":
            print(f"Audio player: {expand_path(args.player_cmd or '/usr/bin/afplay')}")
        elif play_script:
            print(f"Audio helper: {play_script}")
        if backup:
            print(f"Config backup: {backup}")
        for target, target_backup in removed:
            print(f"Removed legacy startup wrapper from: {target}")
            print(f"Startup wrapper backup: {target_backup}")
        print("Restart Codex to load the new notification settings.")
        return 0
    except (RuntimeError, OSError, subprocess.CalledProcessError) as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
