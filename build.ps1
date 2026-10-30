#requires -Version 7.0
[CmdletBinding()]
param([switch]$SkipAndroid)

$ErrorActionPreference = 'Stop'
function Invoke-Checked([string]$Command, [string[]]$CommandArguments) {
    & $Command @CommandArguments
    if ($LASTEXITCODE -ne 0) { throw "$Command failed (exit $LASTEXITCODE)" }
}

Push-Location $PSScriptRoot
try {
    Invoke-Checked cargo @('test', '--locked', '--workspace')
    Invoke-Checked cargo @('test', '--locked', '--release')
    Invoke-Checked npm @('ci', '--prefix', 'js')
    Invoke-Checked npm @('run', 'build', '--prefix', 'js')
    Invoke-Checked npm @('test', '--prefix', 'js')
    Invoke-Checked npm @('run', 'typecheck', '--prefix', 'js')
    if (-not $SkipAndroid) { & (Join-Path $PSScriptRoot 'android/build.ps1') }
}
finally { Pop-Location }
