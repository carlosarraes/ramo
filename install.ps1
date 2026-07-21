param(
    [string]$Version = "latest",
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA "Programs\pdiff"),
    [ValidateSet("", "x64", "arm64")]
    [string]$Architecture = "",
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"
$Repository = "carlosarraes/pdiff"

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
$Archive = "pdiff-${Target}.zip"
$DownloadUrl = if ($Version -eq "latest") {
    "https://github.com/${Repository}/releases/latest/download/${Archive}"
} else {
    "https://github.com/${Repository}/releases/download/${Version}/${Archive}"
}
$Destination = Join-Path $InstallDir "pdiff.exe"

Write-Output "Installing pdiff for ${Target}..."
if ($DryRun) {
    Write-Output "Download: ${DownloadUrl}"
    Write-Output "Install: ${Destination}"
    return
}

$Temporary = Join-Path ([System.IO.Path]::GetTempPath()) ("pdiff-install-" + [guid]::NewGuid())
try {
    New-Item -ItemType Directory -Path $Temporary | Out-Null
    $Zip = Join-Path $Temporary $Archive
    Invoke-WebRequest -Uri $DownloadUrl -OutFile $Zip
    Expand-Archive -Path $Zip -DestinationPath $Temporary
    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    Move-Item -Force -Path (Join-Path $Temporary "pdiff.exe") -Destination $Destination
} finally {
    if (Test-Path -LiteralPath $Temporary) {
        Remove-Item -LiteralPath $Temporary -Recurse -Force
    }
}

Write-Output "Installed pdiff to ${Destination}"
$UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
if (($UserPath -split ";") -notcontains $InstallDir) {
    Write-Output "Add to your user PATH: ${InstallDir}"
}
