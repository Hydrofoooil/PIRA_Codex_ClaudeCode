<# Bootstrap wrapper for PIRA setup on Windows. #>

[CmdletBinding()]
param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$SetupArgs
)

$ErrorActionPreference = "Stop"
$SetupPy = Join-Path $PSScriptRoot "setup_pira.py"
. (Join-Path $PSScriptRoot "lib\pira_python_bootstrap.ps1")

$python = Bootstrap-PiraPython3 -Args $SetupArgs
$pythonFile = $python.File
$pythonArgs = @($python.PrefixArgs + @($SetupPy) + $SetupArgs)
& $pythonFile @pythonArgs
exit $LASTEXITCODE
