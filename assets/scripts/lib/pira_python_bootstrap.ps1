# Shared Python 3 discovery/bootstrap helpers for PIRA PowerShell wrappers.

function Test-PiraPythonCandidate {
    param(
        [Parameter(Mandatory = $true)][string]$File,
        [string[]]$PrefixArgs = @()
    )
    $testArgs = @($PrefixArgs + @("-c", "import sys; raise SystemExit(0 if sys.version_info[0] == 3 else 1)"))
    try {
        & $File @testArgs *> $null
        return ($LASTEXITCODE -eq 0)
    } catch {
        return $false
    }
}

function Find-PiraPython3 {
    $candidates = @(
        @{ File = "py"; PrefixArgs = @("-3") },
        @{ File = "python3"; PrefixArgs = @() },
        @{ File = "python"; PrefixArgs = @() }
    )
    foreach ($candidate in $candidates) {
        if (-not (Get-Command $candidate.File -ErrorAction SilentlyContinue)) { continue }
        if (Test-PiraPythonCandidate -File $candidate.File -PrefixArgs $candidate.PrefixArgs) {
            return @{ File = $candidate.File; PrefixArgs = $candidate.PrefixArgs }
        }
    }
    return $null
}

function Test-PiraAssumeYes {
    param([string[]]$Args = @())
    if ($env:PIRA_SETUP_ASSUME_YES -eq "1") { return $true }
    return @($Args) -contains "--yes"
}

function Install-PiraPythonHint {
    param([string[]]$Args = @())
    $winget = Get-Command winget -ErrorAction SilentlyContinue
    if (-not $winget) {
        Write-Error "Python 3 was not found and winget is unavailable. Install Python 3 from https://www.python.org/downloads/ or Microsoft Store, then rerun this setup wrapper."
    }

    Write-Host "Python 3 was not found. It can be installed with winget:"
    Write-Host "  winget install --id Python.Python.3 --source winget"
    $install = $false
    if (Test-PiraAssumeYes -Args $Args) {
        $install = $true
    } else {
        $answer = Read-Host "Install Python 3 now with winget? [y/N]"
        $install = @("y", "yes") -contains $answer.Trim().ToLowerInvariant()
    }
    if (-not $install) {
        Write-Error "Python 3 is required."
    }

    & winget install --id Python.Python.3 --source winget --accept-package-agreements --accept-source-agreements
    if ($LASTEXITCODE -ne 0) {
        Write-Error "winget failed to install Python 3. Install Python manually, then rerun this setup wrapper."
    }
}

function Require-PiraPython3 {
    $python = Find-PiraPython3
    if (-not $python) {
        Write-Error "Python 3 is required. Run assets/scripts/setup_pira.ps1 first, or install Python 3 and retry."
    }
    return $python
}

function Bootstrap-PiraPython3 {
    param([string[]]$Args = @())
    $python = Find-PiraPython3
    if (-not $python) {
        Install-PiraPythonHint -Args $Args
        $python = Find-PiraPython3
    }
    if (-not $python) {
        Write-Error "Python 3 is still unavailable; cannot continue."
    }
    return $python
}
