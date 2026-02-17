$ErrorActionPreference = "Stop"

# Determine Architecture
$arch = $env:PROCESSOR_ARCHITECTURE
if ($arch -eq "AMD64") {
    $asset_suffix = "windows-x64.zip"
} else {
    Write-Host "Unsupported architecture: $arch" -ForegroundColor Red
    exit 1
}

Write-Host "Detecting latest version..." -ForegroundColor Cyan
try {
    $latestRelease = Invoke-RestMethod -Uri "https://api.github.com/repos/letmutex/gitas/releases/latest"
    $tagName = $latestRelease.tag_name
} catch {
    Write-Host "Error: Could not find latest release version." -ForegroundColor Red
    exit 1
}

if ([string]::IsNullOrEmpty($tagName)) {
    Write-Host "Error: Could not determine version tag." -ForegroundColor Red
    exit 1
}

Write-Host "Latest version: $tagName" -ForegroundColor Green

# Construct download URL
$downloadUrl = "https://github.com/letmutex/gitas/releases/download/$tagName/gitas-$tagName-$asset_suffix"
$installDir = "$env:LOCALAPPDATA\gitas"
$binName = "gitas.exe"
$zipPath = "$env:TEMP\gitas.zip"

# Create install directory
if (!(Test-Path -Path $installDir)) {
    New-Item -ItemType Directory -Force -Path $installDir | Out-Null
}

Write-Host "Downloading $downloadUrl..." -ForegroundColor Cyan
Invoke-WebRequest -Uri $downloadUrl -OutFile $zipPath

Write-Host "Extracting..." -ForegroundColor Cyan
Expand-Archive -Path $zipPath -DestinationPath $installDir -Force

# Move binary to top level of install dir if nested (archive structure might vary)
# Assuming archive structure:/gitas.exe
# If archive is just the exe, Expand-Archive might behave differently? 
# The build script packs it directly or in a folder?
# Let's assume standard zip structure where gitas.exe is at root or inside a folder.
# We'll find gitas.exe recursively and move it to $installDir

$exePath = Get-ChildItem -Path $installDir -Recurse -Filter "gitas.exe" | Select-Object -First 1
if ($exePath) {
    Move-Item -Path $exePath.FullName -Destination "$installDir\$binName" -Force
} else {
    Write-Host "Error: gitas.exe not found in archive." -ForegroundColor Red
    exit 1
}

# Cleanup
Remove-Item -Path $zipPath -Force

# Add to PATH
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath -notlike "*$installDir*") {
    Write-Host "Adding $installDir to User PATH..." -ForegroundColor Cyan
    [Environment]::SetEnvironmentVariable("Path", "$userPath;$installDir", "User")
    $env:Path += ";$installDir"
}

Write-Host "Successfully installed gitas $tagName to $installDir" -ForegroundColor Green
Write-Host "You may need to restart your terminal for PATH changes to take effect." -ForegroundColor Yellow
