[CmdletBinding()]
param(
    [ValidateRange(1, 10)] [int] $Iterations = 3,
    [string[]] $Fixture = @(),
    [string] $OutputDirectory,
    [ValidateSet('debug', 'release', 'performance')]
    [string] $BuildProfile = 'release',
    [switch] $Live,
    [switch] $SkipBuild
)

$ErrorActionPreference = 'Stop'
& (Join-Path $PSScriptRoot 'run-alpha.ps1') @PSBoundParameters
