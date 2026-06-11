<# Thin Windows wrapper for the shared PIRA Codex audio setup helper. #>

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$ConfigPath,

    [string]$AudioDir = "$HOME\agent\PIRA_Voice\Samantha",

    [string]$FinishedAudio = "",

    [string]$WaitingAudio = "",

    [string]$PowerShellCmd = "powershell.exe",

    [switch]$Force
)

$ErrorActionPreference = "Stop"
$SetupPy = Join-Path $PSScriptRoot "setup_codex_audio_mode.py"
. (Join-Path $PSScriptRoot "lib\pira_python_bootstrap.ps1")

$python = Require-PiraPython3
$args = @(
    "--platform", "windows",
    "--config", $ConfigPath,
    "--audio-dir", $AudioDir,
    "--powershell-cmd", $PowerShellCmd
)
if (-not [string]::IsNullOrWhiteSpace($FinishedAudio)) { $args += @("--finished-audio", $FinishedAudio) }
if (-not [string]::IsNullOrWhiteSpace($WaitingAudio)) { $args += @("--waiting-audio", $WaitingAudio) }
if ($Force) { $args += "--force" }

$pythonFile = $python.File
$pythonArgs = @($python.PrefixArgs + @($SetupPy) + $args)
& $pythonFile @pythonArgs
exit $LASTEXITCODE
