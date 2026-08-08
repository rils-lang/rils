[CmdletBinding()]
param(
    [switch]$SkipInstall
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$repositoryRoot = Split-Path -Parent $PSScriptRoot
$extensionRoot = Join-Path $repositoryRoot "editors\vscode-rils"
$manifestPath = Join-Path $extensionRoot "package.json"
$distDirectory = Join-Path $extensionRoot "dist"

if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
    throw "VS Code extension manifest not found: $manifestPath"
}
if (-not (Get-Command npm -ErrorAction SilentlyContinue)) {
    throw "npm was not found on PATH. Install Node.js and try again."
}
$vsceCommand = Get-Command vsce.cmd -ErrorAction SilentlyContinue
if (-not $vsceCommand) {
    $vsceCommand = Get-Command vsce -ErrorAction SilentlyContinue
}
if (-not $vsceCommand) {
    throw "vsce was not found on PATH. Install it with 'npm install --global @vscode/vsce' and try again."
}

$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
$packageName = "$($manifest.name)-$($manifest.version).vsix"
$outputPath = Join-Path $distDirectory $packageName

function Invoke-CheckedCommand {
    param(
        [Parameter(Mandatory)]
        [string]$Command,

        [Parameter(ValueFromRemainingArguments)]
        [string[]]$Arguments
    )

    & $Command @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "Command failed with exit code ${LASTEXITCODE}: $Command $($Arguments -join ' ')"
    }
}

Push-Location $extensionRoot
try {
    if (-not $SkipInstall) {
        Invoke-CheckedCommand npm ci
    }

    Invoke-CheckedCommand npm run check
    New-Item -ItemType Directory -Path $distDirectory -Force | Out-Null
    Invoke-CheckedCommand $vsceCommand.Source package --out $outputPath
}
finally {
    Pop-Location
}

Write-Host "VS Code extension package created: $outputPath"
