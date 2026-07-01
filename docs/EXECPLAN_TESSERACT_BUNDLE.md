# Bundle Tesseract and Organize Release Folder Structure

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds.

This document must be maintained in accordance with `docs/PLANS.md`.


## Purpose / Big Picture

Currently, users must manually install Tesseract OCR before using the automation feature. This creates friction and potential setup failures. After this change, users will download a single folder containing everything needed to run the application. On first run, the embedded Tesseract files will be extracted automatically to a `tesseract/` subdirectory next to the executable.

Additionally, the release folder will have an organized structure with dedicated subdirectories for logs, screenshots, and reference images (assets), making the application self-contained and portable.

The user-visible outcome is: download the release folder, run the exe, and automation works immediately without any manual Tesseract installation.


## Progress

- [x] (2026-01-15) Milestone 1: Prepare Tesseract portable package for embedding
  - Created scripts/prepare-tesseract.ps1 to package from existing installation
  - Generated resources/tesseract.zip (30.29 MB) containing:
    - tesseract.exe + 56 DLLs + tessdata/eng.traineddata
  - Verified package works standalone (no system DLL dependencies)
- [x] (2026-01-15) Milestone 2: Implement embedded extraction logic
  - Rewrote src/ocr/setup.rs with `include_bytes!` embedding
  - Implemented `extract_embedded_tesseract()` with backslash path handling
  - Added `ensure_tesseract()` call in main() at startup
  - Removed legacy download/fallback logic
- [x] (2026-01-15) Milestone 3: Reorganize output paths (logs, screenshots, assets)
  - Created src/paths.rs with centralized path resolution via OnceLock
  - Updated 8 files to use centralized paths:
    - main.rs: log file, reference image saving
    - capture/screenshot.rs: screenshot directory
    - automation/runner.rs: screenshot and CSV paths
    - automation/config.rs: config loading, default reference paths
    - automation/detection.rs: reference image loading
    - calibration/wizard.rs: config saving
    - ocr/setup.rs: tesseract directory
  - Default reference paths now use assets/ prefix
- [x] (2026-01-15) Milestone 4: Update build process and create release structure
  - Created scripts/package-release.ps1
  - Updated .gitignore to exclude release/
- [x] (2026-01-15) Milestone 5: End-to-end validation
  - Built release executable: 38.15 MB (includes embedded Tesseract)
  - Created release package with proper folder structure
  - Verified compilation succeeds with all path changes


## Surprises & Discoveries

- Observation: Tesseract 5.5.0 requires 56 DLLs (72 MB uncompressed, 30 MB zipped)
  Evidence: The UB-Mannheim build includes ICU libraries (30+ MB), graphics libraries (cairo, pango, harfbuzz), and compression libraries. All are needed for tesseract.exe to run standalone.

- Observation: ZIP created with .NET has Windows-style paths (backslashes)
  Evidence: When extracting with Unix unzip, warning appears about backslashes. Need to ensure Rust zip extraction handles this correctly.


## Decision Log

- Decision: Extract Tesseract to exe directory instead of %LOCALAPPDATA%
  Rationale: Simpler, fully portable, no cleanup needed, matches existing pattern where config.json is next to exe. App runs as admin so write permission is available.
  Date/Author: 2026-01-15

- Decision: Skip code signing due to cost
  Rationale: User preference. Windows SmartScreen will show "Unknown publisher" warning once, which users can dismiss with "Run anyway".
  Date/Author: 2026-01-15

- Decision: Release as folder instead of single exe
  Rationale: Allows including organized subdirectories (logs/, screenshots/, assets/), makes the package self-contained and intuitive.
  Date/Author: 2026-01-15


## Outcomes & Retrospective

### Outcomes
- Release package now self-contained at 38.15 MB
- Tesseract extracts automatically on first run to `tesseract/` directory
- All output organized into subdirectories: `logs/`, `screenshots/`, `assets/`
- No manual Tesseract installation required by end users

### Files Changed
- **New**: `src/paths.rs` - centralized path resolution
- **New**: `scripts/package-release.ps1` - release packaging script
- **Modified**: `src/main.rs` - added paths module, ensure_tesseract call
- **Modified**: `src/ocr/setup.rs` - complete rewrite for embedded extraction
- **Modified**: `src/capture/screenshot.rs` - use screenshots directory
- **Modified**: `src/automation/runner.rs` - use centralized paths
- **Modified**: `src/automation/config.rs` - use centralized paths, assets/ prefix
- **Modified**: `src/automation/detection.rs` - use centralized paths
- **Modified**: `src/calibration/wizard.rs` - use centralized paths
- **Modified**: `.gitignore` - exclude release/

### Release Package Structure
```
gakumas-rehearsal-automation/
├── gakumas-rehearsal-automation.exe (38.15 MB, includes embedded Tesseract)
├── config.json
├── logs/               (created on startup)
├── screenshots/        (created on startup)
├── resources/
│   └── template/
│       └── rehearsal/  (reference images, copied from repo)
└── tesseract/          (extracted on first run, ~72 MB uncompressed)
```

### Retrospective
- Implementation went smoothly following the detailed plan
- The backslash path handling in zip extraction was correctly anticipated
- Centralizing paths in a single module significantly improved maintainability
- Build time increased due to larger binary, but acceptable tradeoff for user experience


## Context and Orientation

The gakumas-rehearsal-automation application is a Windows system tray tool that automates game rehearsals by capturing screenshots and extracting scores via OCR. It requires Tesseract OCR to function.

Key files and their current behavior:

    src/ocr/setup.rs
    - get_tesseract_dir(): Returns %LOCALAPPDATA%\gakumas-rehearsal-automation\tesseract
    - ensure_tesseract(): Tries to download or find system Tesseract installation
    - find_tesseract_executable(): Searches multiple locations for tesseract.exe
    - find_tessdata_dir(): Searches for eng.traineddata

    src/main.rs
    - Line 60: Opens "gakumas_screenshot.log" in current directory
    - log() function: Writes to this log file

    src/capture/screenshot.rs
    - Line 218-219: Saves screenshots to current_dir() with "gakumas_*.png" pattern

    src/automation/state.rs
    - Line 274: Saves automation screenshots as "{iteration}_{timestamp}.png"

    src/automation/config.rs
    - Lines 119, 123, 142: Default reference image paths are "start_button_ref.png", etc.
    - load_config(): Loads config.json from exe directory

    src/automation/detection.rs
    - load_reference_histogram(): Loads reference images from exe directory

Current release structure (implicit):

    (wherever user puts it)/
    ├── gakumas-rehearsal-automation.exe
    ├── config.json
    ├── gakumas_screenshot.log
    ├── gakumas_*.png (screenshots scattered)
    ├── *_button_ref.png (reference images scattered)
    └── (Tesseract must be installed separately)

Target release structure:

    gakumas-rehearsal-automation/
    ├── gakumas-rehearsal-automation.exe
    ├── config.json
    ├── tesseract/
    │   ├── tesseract.exe
    │   ├── *.dll (leptonica, etc.)
    │   └── tessdata/
    │       └── eng.traineddata
    ├── logs/
    │   └── gakumas_screenshot.log
    ├── screenshots/
    │   └── (captured images go here)
    └── assets/
        ├── start_button_ref.png
        ├── skip_button_ref.png
        └── end_button_ref.png


## Plan of Work

The work is divided into five milestones. Each milestone produces a testable intermediate state.

### Milestone 1: Prepare Tesseract Portable Package

Before embedding, we need a minimal Tesseract package. The UB-Mannheim Tesseract release includes many unnecessary files. We will create a minimal package containing only the files required for OCR.

Required files from Tesseract installation:
- tesseract.exe (main executable)
- Required DLLs: leptonica-1.84.1.dll, libarchive-13.dll, libbz2.dll, libcrypto-3-x64.dll, libcurl.dll, libiconv.dll, libjpeg-62.dll, liblz4.dll, liblzma.dll, libpng16.dll, libssl-3-x64.dll, libzstd.dll, tiff.dll, zlib.dll (or similar set depending on version)
- tessdata/eng.traineddata

The package should be zipped and placed at `resources/tesseract.zip` for embedding.

### Milestone 2: Implement Embedded Extraction

Modify `src/ocr/setup.rs` to:
1. Embed the tesseract.zip file using `include_bytes!`
2. On first run, extract to `<exe_dir>/tesseract/`
3. Return the extracted path for OCR operations

The extraction should be idempotent: if `tesseract/tesseract.exe` already exists, skip extraction.

### Milestone 3: Reorganize Output Paths

Create a new module `src/paths.rs` that centralizes all path resolution:
- `get_exe_dir()`: Directory containing the executable
- `get_logs_dir()`: `<exe_dir>/logs/`
- `get_screenshots_dir()`: `<exe_dir>/screenshots/`
- `get_assets_dir()`: `<exe_dir>/assets/`
- `get_tesseract_dir()`: `<exe_dir>/tesseract/`

Update all file I/O to use these centralized paths:
- `src/main.rs`: Log file location
- `src/capture/screenshot.rs`: Screenshot save location
- `src/automation/state.rs`: Automation screenshot location
- `src/automation/config.rs`: Reference image paths
- `src/automation/detection.rs`: Reference image loading

### Milestone 4: Update Build Process

Create a build/release process that:
1. Compiles the release binary
2. Creates the release folder structure
3. Copies config.json
4. Creates empty subdirectories (logs/, screenshots/, assets/)

This could be a PowerShell script `scripts/package-release.ps1` or integrated into `build.rs`.

### Milestone 5: End-to-End Validation

Test the complete flow:
1. Start with a fresh release folder (no prior extraction)
2. Run the exe
3. Verify Tesseract extraction happens automatically
4. Verify logs go to logs/
5. Verify screenshots go to screenshots/
6. Run calibration, verify reference images go to assets/
7. Run automation, verify everything works


## Concrete Steps

### Milestone 1 Steps

1. Download Tesseract 5.5.0 from UB-Mannheim: https://github.com/UB-Mannheim/tesseract/releases

2. Install or extract to identify required files. Run tesseract.exe and note any missing DLL errors, or use a tool like Dependencies to identify required DLLs.

3. Create minimal package:

       mkdir resources
       # Copy tesseract.exe and required DLLs to resources/tesseract/
       # Copy tessdata/eng.traineddata to resources/tesseract/tessdata/
       # Zip the tesseract folder

4. Verify the package works standalone:

       cd resources
       tesseract/tesseract.exe --version
       # Should print version without errors

5. Commit resources/tesseract.zip to the repository (or document where to obtain it if too large for git).

### Milestone 2 Steps

1. Add the `zip` crate to Cargo.toml if not present (it is already present per ROADMAP).

2. Modify `src/ocr/setup.rs`:

   Replace the current `get_tesseract_dir()` to return exe directory:

       pub fn get_tesseract_dir() -> PathBuf {
           std::env::current_exe()
               .ok()
               .and_then(|p| p.parent().map(|p| p.to_path_buf()))
               .unwrap_or_else(|| PathBuf::from("."))
               .join("tesseract")
       }

   Add embedded zip constant (at top of file):

       const TESSERACT_ZIP: &[u8] = include_bytes!("../../resources/tesseract.zip");

   Replace `ensure_tesseract()` to extract from embedded zip:

       pub fn ensure_tesseract() -> Result<TesseractPaths> {
           let tesseract_dir = get_tesseract_dir();
           let executable = tesseract_dir.join("tesseract.exe");
           let tessdata_dir = tesseract_dir.join("tessdata");
           let eng_traineddata = tessdata_dir.join("eng.traineddata");

           if executable.exists() && eng_traineddata.exists() {
               log(&format!("Tesseract found at: {}", tesseract_dir.display()));
               return Ok(TesseractPaths {
                   executable,
                   tessdata: tessdata_dir,
               });
           }

           log("Extracting embedded Tesseract...");
           extract_embedded_tesseract(&tesseract_dir)?;

           Ok(TesseractPaths {
               executable,
               tessdata: tessdata_dir,
           })
       }

   Add extraction function:

       fn extract_embedded_tesseract(target_dir: &Path) -> Result<()> {
           use std::io::Cursor;
           use zip::ZipArchive;

           let cursor = Cursor::new(TESSERACT_ZIP);
           let mut archive = ZipArchive::new(cursor)?;

           for i in 0..archive.len() {
               let mut file = archive.by_index(i)?;
               let outpath = target_dir.join(file.name());

               if file.name().ends_with('/') {
                   fs::create_dir_all(&outpath)?;
               } else {
                   if let Some(parent) = outpath.parent() {
                       fs::create_dir_all(parent)?;
                   }
                   let mut outfile = fs::File::create(&outpath)?;
                   std::io::copy(&mut file, &mut outfile)?;
               }
           }

           log(&format!("Tesseract extracted to: {}", target_dir.display()));
           Ok(())
       }

3. Remove or simplify the download functions since they are no longer needed.

4. Update `find_tesseract_executable()` and `find_tessdata_dir()` to only check the local directory (no system path fallback needed).

### Milestone 3 Steps

1. Create `src/paths.rs`:

       use std::path::PathBuf;
       use std::sync::OnceLock;

       static EXE_DIR: OnceLock<PathBuf> = OnceLock::new();

       /// Returns the directory containing the executable.
       pub fn get_exe_dir() -> &'static PathBuf {
           EXE_DIR.get_or_init(|| {
               std::env::current_exe()
                   .ok()
                   .and_then(|p| p.parent().map(|p| p.to_path_buf()))
                   .unwrap_or_else(|| PathBuf::from("."))
           })
       }

       pub fn get_logs_dir() -> PathBuf {
           get_exe_dir().join("logs")
       }

       pub fn get_screenshots_dir() -> PathBuf {
           get_exe_dir().join("screenshots")
       }

       pub fn get_assets_dir() -> PathBuf {
           get_exe_dir().join("assets")
       }

       pub fn get_tesseract_dir() -> PathBuf {
           get_exe_dir().join("tesseract")
       }

       /// Ensures all output directories exist. Call at startup.
       pub fn ensure_directories() -> std::io::Result<()> {
           std::fs::create_dir_all(get_logs_dir())?;
           std::fs::create_dir_all(get_screenshots_dir())?;
           std::fs::create_dir_all(get_assets_dir())?;
           Ok(())
       }

2. Add module to `src/main.rs`:

       mod paths;

   Call `paths::ensure_directories()` early in main().

3. Update log file path in `src/main.rs`:

   Change line 60 from:
       .open("gakumas_screenshot.log")
   To:
       .open(crate::paths::get_logs_dir().join("gakumas_screenshot.log"))

4. Update `src/capture/screenshot.rs` line 219:

   Change from:
       let path = std::env::current_dir()?.join(&filename);
   To:
       let path = crate::paths::get_screenshots_dir().join(&filename);

5. Update `src/automation/state.rs` line 274:

   Change from:
       let path = exe_dir.join(&filename);
   To:
       let path = crate::paths::get_screenshots_dir().join(&filename);

6. Update `src/automation/config.rs` default reference paths:

   Change default functions to return paths under assets/:
       fn default_start_button_reference() -> String {
           "assets/start_button_ref.png".to_string()
       }
   (Similarly for skip and end button references)

7. Update `src/automation/detection.rs` to load from assets directory:

   The `load_reference_histogram()` function should resolve paths relative to exe_dir. If the config path doesn't start with "assets/", prepend it for backward compatibility, or simply update to use `paths::get_assets_dir()`.

8. Update reference image saving in `src/main.rs` (the `save_*_button_reference()` calls) to save to assets/ directory.

9. Update `src/ocr/setup.rs` to use `crate::paths::get_tesseract_dir()` instead of its own function.

### Milestone 4 Steps

1. Create `scripts/package-release.ps1`:

       # Package release folder
       param(
           [string]$OutputDir = "release"
       )

       $ErrorActionPreference = "Stop"

       # Build release
       Write-Host "Building release..."
       cargo build --release

       # Create output structure
       $releaseDir = "$OutputDir/gakumas-rehearsal-automation"
       if (Test-Path $releaseDir) {
           Remove-Item -Recurse -Force $releaseDir
       }

       New-Item -ItemType Directory -Path $releaseDir | Out-Null
       New-Item -ItemType Directory -Path "$releaseDir/logs" | Out-Null
       New-Item -ItemType Directory -Path "$releaseDir/screenshots" | Out-Null
       New-Item -ItemType Directory -Path "$releaseDir/assets" | Out-Null

       # Copy files
       Copy-Item "target/release/gakumas-rehearsal-automation.exe" $releaseDir
       Copy-Item "config.json" $releaseDir -ErrorAction SilentlyContinue

       # Note: tesseract/ will be created on first run from embedded zip

       Write-Host "Release package created at: $releaseDir"

2. Optionally update `.gitignore` to ignore `release/` directory.

### Milestone 5 Steps

1. Run the package script:

       powershell -ExecutionPolicy Bypass -File scripts/package-release.ps1

2. Navigate to release folder and delete any tesseract/ directory if it exists from previous tests.

3. Run the exe:

       cd release/gakumas-rehearsal-automation
       ./gakumas-rehearsal-automation.exe

4. Verify in the log output that Tesseract extraction occurs.

5. Check that tesseract/ directory was created with tesseract.exe and tessdata/eng.traineddata.

6. Check that logs/gakumas_screenshot.log exists.

7. Use the Screenshot hotkey (Ctrl+Shift+S) and verify the image appears in screenshots/.

8. Use the tray menu to capture reference images and verify they appear in assets/.

9. Run a full automation cycle and verify:
   - OCR works (using extracted Tesseract)
   - Screenshots saved to screenshots/
   - Results CSV saved appropriately


## Validation and Acceptance

The feature is complete when:

1. A fresh release folder (with only exe and config.json) can be run, and on first launch:
   - tesseract/ directory is created with working Tesseract
   - logs/ directory is created with log file
   - No errors about missing Tesseract

2. Manual screenshot capture saves to screenshots/ directory.

3. Calibration reference images save to assets/ directory.

4. Automation runs successfully using the extracted Tesseract.

5. The entire release folder can be moved to a different location and still works (portability).

Test command sequence:

    # From repository root
    powershell -ExecutionPolicy Bypass -File scripts/package-release.ps1
    cd release/gakumas-rehearsal-automation
    # Ensure tesseract/ does not exist
    Remove-Item -Recurse -Force tesseract -ErrorAction SilentlyContinue
    # Run
    ./gakumas-rehearsal-automation.exe
    # After startup, check:
    # - tesseract/tesseract.exe exists
    # - logs/gakumas_screenshot.log exists
    # Use Ctrl+Shift+S to take screenshot, check screenshots/ folder


## Idempotence and Recovery

The extraction logic is idempotent: if tesseract/tesseract.exe already exists, extraction is skipped.

If extraction fails partway (e.g., disk full), user can delete the tesseract/ folder and restart the application to retry.

Directory creation in `ensure_directories()` uses `create_dir_all()` which is idempotent.


## Artifacts and Notes

Expected executable size increase: approximately 30-40 MB (Tesseract + DLLs + traineddata).

The `include_bytes!` macro embeds the zip at compile time, so the resources/tesseract.zip must exist before building.

If tesseract.zip is too large for git (>100MB), consider:
- Using Git LFS
- Documenting manual download steps
- Hosting the zip externally and downloading during build


## Interfaces and Dependencies

Dependencies (already in Cargo.toml):
- `zip` crate for extraction
- `dirs` crate (can be removed if no longer using %LOCALAPPDATA%)

New module:

    // src/paths.rs
    pub fn get_exe_dir() -> &'static PathBuf
    pub fn get_logs_dir() -> PathBuf
    pub fn get_screenshots_dir() -> PathBuf
    pub fn get_assets_dir() -> PathBuf
    pub fn get_tesseract_dir() -> PathBuf
    pub fn ensure_directories() -> std::io::Result<()>

Modified functions in src/ocr/setup.rs:

    pub fn ensure_tesseract() -> Result<TesseractPaths>
    // Now extracts from embedded zip if not present

    fn extract_embedded_tesseract(target_dir: &Path) -> Result<()>
    // New function to handle zip extraction
