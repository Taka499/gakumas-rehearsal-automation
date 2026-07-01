# Package release folder for gakumas-rehearsal-automation
# Creates a portable release folder with proper directory structure.
#
# Usage: powershell -ExecutionPolicy Bypass -File scripts/package-release.ps1
#        powershell -ExecutionPolicy Bypass -File scripts/package-release.ps1 -OutputDir "dist"

param(
    [string]$OutputDir = "release"
)

$ErrorActionPreference = "Stop"

Write-Host "=== Gakumas Screenshot Release Packager ===" -ForegroundColor Cyan
Write-Host ""

# Guard: a running instance locks the output exe and would fail the build/copy
# only after a full compile. Abort early with a clear message instead.
$running = Get-Process gakumas-rehearsal-automation -ErrorAction SilentlyContinue
if ($running) {
    Write-Host "ERROR: gakumas-rehearsal-automation.exe is running (PID $($running.Id))." -ForegroundColor Red
    Write-Host "Close it (tray -> 終了) before packaging — it locks the binary." -ForegroundColor Red
    exit 1
}

# Build release
Write-Host "Building release binary..." -ForegroundColor Yellow
cargo build --release
if ($LASTEXITCODE -ne 0) {
    Write-Host "Build failed!" -ForegroundColor Red
    exit 1
}
Write-Host "Build successful." -ForegroundColor Green
Write-Host ""

# Create output structure
$releaseDir = Join-Path $OutputDir "gakumas-rehearsal-automation"
Write-Host "Creating release folder: $releaseDir" -ForegroundColor Yellow

if (Test-Path $releaseDir) {
    Write-Host "Removing existing release folder..." -ForegroundColor Gray
    Remove-Item -Recurse -Force $releaseDir
}

# Create directories
New-Item -ItemType Directory -Path $releaseDir -Force | Out-Null
New-Item -ItemType Directory -Path (Join-Path $releaseDir "logs") -Force | Out-Null
New-Item -ItemType Directory -Path (Join-Path $releaseDir "screenshots") -Force | Out-Null
New-Item -ItemType Directory -Path (Join-Path $releaseDir "resources/template/rehearsal") -Force | Out-Null
New-Item -ItemType Directory -Path (Join-Path $releaseDir "resources/guide") -Force | Out-Null

Write-Host "Created directory structure:" -ForegroundColor Green
Write-Host "  $releaseDir/"
Write-Host "  $releaseDir/logs/"
Write-Host "  $releaseDir/screenshots/"
Write-Host "  $releaseDir/resources/template/rehearsal/"
Write-Host "  $releaseDir/resources/guide/"
Write-Host ""

# Copy executable
$exePath = "target/release/gakumas-rehearsal-automation.exe"
if (-not (Test-Path $exePath)) {
    Write-Host "Error: Executable not found at $exePath" -ForegroundColor Red
    exit 1
}
Copy-Item $exePath $releaseDir
Write-Host "Copied gakumas-rehearsal-automation.exe" -ForegroundColor Green

# Copy config.json if exists
if (Test-Path "config.json") {
    Copy-Item "config.json" $releaseDir
    Write-Host "Copied config.json" -ForegroundColor Green
} else {
    Write-Host "No config.json found (will use defaults)" -ForegroundColor Gray
}

# Copy template files
$templateSrc = "resources/template"
$resourcesDst = Join-Path $releaseDir "resources"
if (Test-Path $templateSrc) {
    Copy-Item -Path $templateSrc -Destination $resourcesDst -Recurse -Force
    Write-Host "Copied template folder to resources/" -ForegroundColor Green
} else {
    Write-Host "No template folder found" -ForegroundColor Gray
}

# Copy guide images
$guideSrc = "resources/guide"
if (Test-Path $guideSrc) {
    Copy-Item -Path $guideSrc -Destination $resourcesDst -Recurse -Force
    Write-Host "Copied guide folder to resources/" -ForegroundColor Green
} else {
    Write-Host "No guide folder found" -ForegroundColor Gray
}

Write-Host ""
Write-Host "=== Release Package Complete ===" -ForegroundColor Cyan
Write-Host "Location: $releaseDir" -ForegroundColor White
Write-Host ""
Write-Host "Contents:" -ForegroundColor Yellow
Get-ChildItem $releaseDir -Recurse | ForEach-Object {
    $relativePath = $_.FullName.Replace((Resolve-Path $releaseDir).Path, "").TrimStart("\")
    if ($_.PSIsContainer) {
        Write-Host "  [DIR]  $relativePath/" -ForegroundColor Blue
    } else {
        $size = "{0:N2} MB" -f ($_.Length / 1MB)
        Write-Host "  [FILE] $relativePath ($size)" -ForegroundColor White
    }
}

Write-Host ""
Write-Host "Note: Tesseract will be extracted automatically on first run." -ForegroundColor Gray
Write-Host "The embedded Tesseract adds ~30-40 MB to the executable size." -ForegroundColor Gray
