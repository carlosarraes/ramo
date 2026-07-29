param(
    [string]$Version = "latest",
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA "Programs\ramo"),
    [ValidateSet("", "x64", "arm64")]
    [string]$Architecture = "",
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"
$Repository = "carlosarraes/ramo"

if ($Architecture -eq "") {
    $Architecture = switch ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture) {
        "X64" { "x64" }
        "Arm64" { "arm64" }
        default { throw "Unsupported Windows architecture: $_" }
    }
}

$Target = switch ($Architecture) {
    "x64" { "x86_64-pc-windows-msvc" }
    "arm64" { "aarch64-pc-windows-msvc" }
}
$Archive = "ramo-${Target}.zip"
$DownloadUrl = if ($Version -eq "latest") {
    "https://github.com/${Repository}/releases/latest/download/${Archive}"
} else {
    "https://github.com/${Repository}/releases/download/${Version}/${Archive}"
}
$Destination = Join-Path $InstallDir "ramo.exe"
$ServerDestination = Join-Path $InstallDir "ramo-server.exe"

Write-Output "Installing ramo for ${Target}..."
if ($DryRun) {
    Write-Output "Download: ${DownloadUrl}"
    Write-Output "Install: ${Destination}"
    Write-Output "Install: ${ServerDestination}"
    return
}

$Temporary = Join-Path ([System.IO.Path]::GetTempPath()) ("ramo-install-" + [guid]::NewGuid())
try {
    New-Item -ItemType Directory -Path $Temporary | Out-Null
    $Zip = Join-Path $Temporary $Archive
    Invoke-WebRequest -Uri $DownloadUrl -OutFile $Zip
    Expand-Archive -Path $Zip -DestinationPath $Temporary
    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    if (-not (Test-Path -LiteralPath (Join-Path $Temporary "ramo.exe")) -or
        -not (Test-Path -LiteralPath (Join-Path $Temporary "ramo-server.exe"))) {
        throw "The Ramo release archive is missing ramo.exe or ramo-server.exe."
    }
    Move-Item -Force -Path (Join-Path $Temporary "ramo.exe") -Destination $Destination
    Move-Item -Force -Path (Join-Path $Temporary "ramo-server.exe") -Destination $ServerDestination
} finally {
    if (Test-Path -LiteralPath $Temporary) {
        Remove-Item -LiteralPath $Temporary -Recurse -Force
    }
}

Write-Output "Installed ramo and ramo-server to ${InstallDir}"
Write-Output "Run 'ramo server setup' to enable private mobile AI analysis."
$UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
if (($UserPath -split ";") -notcontains $InstallDir) {
    Write-Output "Add to your user PATH: ${InstallDir}"
}
