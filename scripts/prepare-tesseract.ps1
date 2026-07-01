# prepare-tesseract.ps1
# Creates a minimal Tesseract package for embedding from an existing installation
#
# Usage:
#   powershell -ExecutionPolicy Bypass -File scripts/prepare-tesseract.ps1
#
# The script will try to:
# 1. Find existing Tesseract installation
# 2. Copy required files
# 3. Create tesseract.zip for embedding

param(
    [string]$TesseractPath = "",  # Optional: explicit path to Tesseract installation
    [string]$OutputDir = "resources"
)

$ErrorActionPreference = "Stop"

# Paths to check for existing Tesseract installation
$searchPaths = @(
    $TesseractPath,
    "C:\Program Files\Tesseract-OCR",
    "C:\Program Files (x86)\Tesseract-OCR",
    "$env:LOCALAPPDATA\gakumas-rehearsal-automation\tesseract",
    "$env:LOCALAPPDATA\Tesseract-OCR"
)

# Find Tesseract installation
$tesseractDir = $null
foreach ($path in $searchPaths) {
    if ($path -and (Test-Path "$path\tesseract.exe")) {
        $tesseractDir = $path
        break
    }
}

# Also check PATH
if (-not $tesseractDir) {
    $tesseractInPath = Get-Command tesseract -ErrorAction SilentlyContinue
    if ($tesseractInPath) {
        $tesseractDir = Split-Path $tesseractInPath.Source -Parent
    }
}

if (-not $tesseractDir) {
    Write-Host "ERROR: Tesseract installation not found!" -ForegroundColor Red
    Write-Host ""
    Write-Host "Please install Tesseract OCR first:"
    Write-Host "  1. Download from: https://github.com/UB-Mannheim/tesseract/releases"
    Write-Host "  2. Run the installer"
    Write-Host "  3. Run this script again"
    Write-Host ""
    Write-Host "Or specify the path explicitly:"
    Write-Host "  .\scripts\prepare-tesseract.ps1 -TesseractPath 'C:\path\to\tesseract'"
    exit 1
}

Write-Host "Found Tesseract at: $tesseractDir"

# Verify tesseract.exe works
Write-Host "Verifying Tesseract installation..."
$versionOutput = & "$tesseractDir\tesseract.exe" --version 2>&1
if ($LASTEXITCODE -ne 0) {
    Write-Host "WARNING: tesseract --version returned non-zero exit code" -ForegroundColor Yellow
}
Write-Host $versionOutput[0]

# Create temp package directory
$tempDir = Join-Path $env:TEMP "tesseract-package"
$packageDir = Join-Path $tempDir "tesseract"

if (Test-Path $tempDir) {
    Remove-Item -Recurse -Force $tempDir
}
New-Item -ItemType Directory -Path $packageDir | Out-Null
New-Item -ItemType Directory -Path "$packageDir\tessdata" | Out-Null

# Copy tesseract.exe
Write-Host "Copying tesseract.exe..."
Copy-Item "$tesseractDir\tesseract.exe" $packageDir

# Copy all DLLs
Write-Host "Copying DLLs..."
$dlls = Get-ChildItem -Path $tesseractDir -Filter "*.dll"
$dllCount = 0
foreach ($dll in $dlls) {
    Copy-Item $dll.FullName $packageDir
    $dllCount++
}
Write-Host "  Copied $dllCount DLL files"

# Copy eng.traineddata
Write-Host "Copying tessdata..."
$tessdataDir = Join-Path $tesseractDir "tessdata"
$engTraineddata = Join-Path $tessdataDir "eng.traineddata"

if (Test-Path $engTraineddata) {
    Copy-Item $engTraineddata "$packageDir\tessdata\"
    Write-Host "  Copied eng.traineddata"
} else {
    Write-Host "ERROR: eng.traineddata not found at $engTraineddata" -ForegroundColor Red
    Write-Host "Please ensure English language data is installed."
    exit 1
}

# Verify the package works standalone
Write-Host ""
Write-Host "Verifying package..."
$testOutput = & "$packageDir\tesseract.exe" --version 2>&1
if ($LASTEXITCODE -eq 0 -or $testOutput -match "tesseract") {
    Write-Host "Package verification: OK" -ForegroundColor Green
} else {
    Write-Host "WARNING: Package verification may have issues" -ForegroundColor Yellow
    Write-Host $testOutput
}

# Calculate package size before zipping
$packageSize = (Get-ChildItem -Recurse $packageDir | Measure-Object -Property Length -Sum).Sum / 1MB
Write-Host "Package size (uncompressed): $([math]::Round($packageSize, 2)) MB"

# Ensure output directory exists
if (-not (Test-Path $OutputDir)) {
    New-Item -ItemType Directory -Path $OutputDir | Out-Null
}

# Create ZIP file
$outputZip = Join-Path $OutputDir "tesseract.zip"
Write-Host ""
Write-Host "Creating ZIP archive..."

if (Test-Path $outputZip) {
    Remove-Item $outputZip
}

# Use .NET compression
Add-Type -AssemblyName System.IO.Compression.FileSystem
[System.IO.Compression.ZipFile]::CreateFromDirectory($packageDir, $outputZip)

# Report results
$zipSize = (Get-Item $outputZip).Length / 1MB

Write-Host ""
Write-Host "========================================" -ForegroundColor Green
Write-Host "SUCCESS: Tesseract package created!" -ForegroundColor Green
Write-Host "========================================" -ForegroundColor Green
Write-Host ""
Write-Host "Output: $((Resolve-Path $outputZip).Path)"
Write-Host "Size:   $([math]::Round($zipSize, 2)) MB"
Write-Host ""

# List package contents
Write-Host "Package contents:"
Write-Host "  tesseract/"
Get-ChildItem $packageDir | ForEach-Object {
    $size = if ($_.PSIsContainer) { "" } else { " ($([math]::Round($_.Length/1KB, 0)) KB)" }
    Write-Host "    $($_.Name)$size"
}
Write-Host "    tessdata/"
Get-ChildItem "$packageDir\tessdata" | ForEach-Object {
    Write-Host "      $($_.Name) ($([math]::Round($_.Length/1MB, 1)) MB)"
}

# Cleanup
Remove-Item -Recurse -Force $tempDir

Write-Host ""
Write-Host "Next steps:"
Write-Host "  1. The tesseract.zip is ready in the 'resources' folder"
Write-Host "  2. Proceed with Milestone 2 to embed it into the binary"
