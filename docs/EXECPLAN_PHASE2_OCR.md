# Phase 2: OCR Integration using Full-Image Pattern Matching

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds. This document must be maintained in accordance with `docs/PLANS.md`.


## Purpose / Big Picture

After this change, the application can extract all 9 rehearsal scores from a screenshot without requiring manual region calibration. Given a screenshot of the rehearsal result page, the OCR module:

1. Preprocesses the image using color thresholding (keeping only bright pixels where score text appears)
2. Runs Tesseract OCR on the entire preprocessed image
3. Pattern-matches the OCR output to find lines containing exactly 3 numbers
4. Returns the scores as a `[[u32; 3]; 3]` array (3 stages × 3 breakdown scores)

This approach is resolution-independent and does not require any score region calibration. Users only need to calibrate button positions for automation (Phase 3), not for OCR.

This is inspired by the [gakumas-tools](https://github.com/surisuririsu/gakumas-tools) project which uses the same approach successfully in JavaScript/Tesseract.js.


## Progress

- [x] Milestone 1: Tesseract runtime setup and verification
- [x] Milestone 2: Color threshold preprocessing
- [x] Milestone 3: Full-image OCR with Tesseract
- [x] Milestone 4: Pattern matching and score extraction
- [x] Milestone 5: Integration and testing with sample images
- [x] Milestone 6: Tray menu integration ("Test OCR" option)

**Status: COMPLETE** (2026-01-14)


## Surprises & Discoveries

- The Rust 2024 edition requires the `embed-resource` crate's Windows manifest to be handled carefully. Tests require elevation due to the admin manifest.
- Used CLI-based Tesseract approach (via `std::process::Command`) rather than C bindings for simpler setup and cross-platform compatibility.
- Tesseract auto-download is partially implemented; for now, manual installation of Tesseract is required. The code detects system-installed Tesseract in PATH or Program Files.


## Decision Log

- Decision: Use full-image OCR with pattern matching instead of region-based OCR
  Rationale: The gakumas-tools project demonstrated this approach works well. It eliminates the need for calibrating 12 score regions, making the tool much easier to use. It's also resolution-independent and more robust to UI layout changes.
  Date/Author: 2026-01-14 / Design revision

- Decision: Use color thresholding (R,G,B all > threshold) instead of grayscale
  Rationale: Score text in the game is displayed in bright white/cream color (RGB ~240-255). Checking that all three channels exceed a threshold more reliably isolates the text than grayscale luminance conversion.
  Date/Author: 2026-01-14 / Adopted from gakumas-tools

- Decision: Keep Tesseract auto-download approach from original plan
  Rationale: Users shouldn't need to manually install Tesseract. Auto-download to AppData keeps the experience seamless.
  Date/Author: 2026-01-13 / Retained from original plan

- Decision: Remove stage_total_regions from calibration
  Rationale: The full-image approach doesn't need explicit region definitions. Stage totals can optionally be extracted by pattern matching if needed, or omitted entirely since individual scores are sufficient.
  Date/Author: 2026-01-14 / Simplification

- Decision: Use English model (eng) instead of Japanese (jpn) for OCR
  Rationale: We only need to recognize digits and commas. The English model handles these well and is smaller (~15MB vs ~50MB for Japanese).
  Date/Author: 2026-01-14 / Optimization


## Outcomes & Retrospective

### What was delivered

- Full OCR module (`src/ocr/`) with 5 submodules:
  - `setup.rs` - Tesseract path detection (auto-download scaffolding for future)
  - `preprocess.rs` - Color threshold preprocessing
  - `engine.rs` - Tesseract CLI wrapper with TSV output parsing
  - `extract.rs` - Pattern matching and score extraction
  - `mod.rs` - Public API and `ocr_screenshot()` convenience function
- "Test OCR" tray menu option for manual verification
- `ocr_threshold` config field (default: 190)
- Successfully extracts 9 scores from rehearsal result screenshots

### What worked well

- The gakumas-tools approach (full-image + pattern matching) worked excellently
- Color thresholding effectively isolates score text from complex backgrounds
- CLI-based Tesseract integration is simpler than C bindings
- TSV output parsing provides reliable word-level confidence scores

### What could be improved

- Tesseract auto-download not fully implemented (requires manual installation)
- Unit tests cannot run due to admin manifest requirement
- Could add more robust error messages for common OCR failures

### Lessons learned

- Pattern matching on OCR output is more robust than region-based cropping
- Confidence filtering (>60%) effectively removes noise
- The regex `^((\d+[,.])*\d+|[—\-]+)$` reliably matches score patterns


## Context and Orientation

The gakumas-rehearsal-automation application is a Windows system tray tool that captures screenshots of the game "Gakuen iDOLM@STER". Phase 1 (complete) provides:

- Window discovery via process name matching (`gakumas.exe`)
- Screenshot capture via Windows Graphics Capture API
- Mouse click simulation for automation
- Brightness-based loading state detection
- Calibration tool for button positions

Key existing files:

    src/main.rs                    - Entry point, tray menu, hotkey handling
    src/capture/screenshot.rs      - Full window capture returning ImageBuffer
    src/capture/region.rs          - Partial region capture (less relevant now)
    src/automation/config.rs       - Configuration types and loading
    config.json                    - Runtime configuration

The result screen displays scores in a consistent visual style:
- Three stages, each showing 3 breakdown scores
- Scores are displayed in bright white/cream text (~RGB 240-255)
- Background, portraits, and other UI elements are darker

Sample test images available:

    sample_rehearsal_result_page.png   - Contains scores to extract

Terms used in this document:

- Color threshold: Keeping only pixels where R, G, and B values all exceed a minimum brightness
- PSM (Page Segmentation Mode): Tesseract setting controlling how it interprets image layout
- Pattern matching: Using regex to identify lines containing score-like text patterns
- tessdata: Tesseract's trained language model files


## Plan of Work

### Milestone 1: Tesseract Runtime Setup

Create infrastructure to ensure Tesseract is available at runtime. On first run, download Tesseract and the English trained data to a local directory. This avoids requiring users to install Tesseract manually.

The setup module will:
1. Check if Tesseract exists in `%LOCALAPPDATA%\gakumas-rehearsal-automation\tesseract\`
2. If not found, download `tesseract.exe` and required DLLs from a GitHub release
3. Download `eng.traineddata` from the official tessdata repository
4. Return the path to `tesseract.exe` for use by the OCR engine

Fallback: If download fails, check for system-wide Tesseract installation (in PATH or Program Files).


### Milestone 2: Color Threshold Preprocessing

Implement the preprocessing pipeline that converts the screenshot to a binary image optimized for OCR.

The algorithm (from gakumas-tools):
- For each pixel, if R > threshold AND G > threshold AND B > threshold: output black (text)
- Otherwise: output white (background)

This isolates the bright score text from the darker background elements.

The threshold value:
- For screenshots (clean, consistent colors): 190
- For video frames (compression artifacts): 160 (not currently needed)

The preprocessing function will work on the full image, not cropped regions.


### Milestone 3: Full-Image OCR with Tesseract

Create a wrapper around Tesseract that:
1. Accepts a preprocessed image
2. Saves it to a temporary file (Tesseract CLI requires file input)
3. Runs Tesseract with appropriate settings
4. Parses the structured output (hOCR or TSV format) to get lines with confidence scores
5. Returns the raw OCR result including text blocks, lines, words, and confidence

Tesseract settings:
- Language: `eng` (English - sufficient for digits)
- PSM: `3` (Fully automatic page segmentation) or `6` (Single uniform block)
- No character whitelist (let Tesseract see everything, filter in post-processing)


### Milestone 4: Pattern Matching and Score Extraction

Implement the score extraction logic that filters OCR output to find score lines.

The algorithm (from gakumas-tools):
1. Iterate through all detected lines
2. Skip lines with confidence < 60%
3. For each line, filter words to only those matching the pattern: `^((\d+[,.])?(\d+)|[—-]+)$`
4. If exactly 3 words remain after filtering, this is a score line
5. Parse each word by removing non-digit characters and converting to integer
6. Stop after finding 3 valid score lines (one per stage)

The regex pattern matches:
- Plain numbers: `12345`
- Numbers with comma separators: `12,345` or `1,234,567`
- Numbers with period separators: `12.345` (some locales)
- Dashes: `--` or `—` (indicating zero or missing score)


### Milestone 5: Integration and Testing

Create an end-to-end test using the sample images:

1. Load `sample_rehearsal_result_page.png`
2. Run the full pipeline: preprocess → OCR → pattern match → extract scores
3. Verify extracted scores match expected values
4. Test edge cases: different thresholds, image sizes

Expected scores from `sample_rehearsal_result_page.png` (to be verified during implementation):

    Stage 1: [50339, 50796, 70859]
    Stage 2: [64997, 168009, 128450]
    Stage 3: [122130, 105901, 96776]


### Milestone 6: Tray Menu Integration

Add a "Test OCR" option to the system tray menu that:
1. Captures the current game window
2. Runs the OCR pipeline
3. Displays results in a message box or console output
4. Helps users verify OCR is working before running automation

This provides immediate feedback without needing to run the full automation loop.


## Concrete Steps

All commands run from repository root: `C:\Work\GitRepos\gakumas-rehearsal-automation`


### Step 1: Add dependencies

Edit `Cargo.toml` to add OCR-related dependencies:

    # For Tesseract OCR
    tesseract = "0.15"        # Rust bindings for Tesseract (uses system Tesseract or bundled)

    # Alternative: rusty-tesseract for CLI-based approach
    # rusty-tesseract = "1.1"

    # For downloading Tesseract on first run
    reqwest = { version = "0.12", features = ["blocking"] }

    # For finding AppData directory
    dirs = "5.0"

    # For pattern matching
    regex = "1.10"

Note: We'll evaluate both `tesseract` (C bindings) and `rusty-tesseract` (CLI wrapper) crates during implementation. The CLI approach may be simpler for auto-download scenarios.

Verify:

    cargo build --release

Expected: Build succeeds with new dependencies downloaded.


### Step 2: Create OCR module structure

Create these new files:

    src/ocr/mod.rs           - Module exports
    src/ocr/setup.rs         - Tesseract download and path management
    src/ocr/preprocess.rs    - Color threshold preprocessing
    src/ocr/engine.rs        - Tesseract wrapper
    src/ocr/extract.rs       - Pattern matching and score extraction

Add `mod ocr;` to `src/main.rs`.

Verify:

    cargo build --release

Expected: Build succeeds with empty module stubs.


### Step 3: Implement Tesseract setup

Implement `src/ocr/setup.rs` with:

    pub fn ensure_tesseract() -> Result<TesseractPaths>
    pub fn get_tesseract_dir() -> PathBuf

The function downloads Tesseract if not present and returns paths to the executable and tessdata.

Test manually by running with a temporary test in main:

    // Temporary test code
    let paths = ocr::setup::ensure_tesseract()?;
    println!("Tesseract: {:?}", paths);

Expected output (first run):

    Tesseract not found locally, downloading...
    Downloading tesseract.exe...
    Downloading eng.traineddata...
    Tesseract ready at: C:\Users\<user>\AppData\Local\gakumas-rehearsal-automation\tesseract

Expected output (subsequent runs):

    Tesseract found at: C:\Users\<user>\AppData\Local\gakumas-rehearsal-automation\tesseract


### Step 4: Implement color threshold preprocessing

Implement `src/ocr/preprocess.rs`:

    pub fn threshold_bright_pixels(
        img: &ImageBuffer<Rgba<u8>, Vec<u8>>,
        threshold: u8,
    ) -> ImageBuffer<Luma<u8>, Vec<u8>>

Test by saving preprocessed image:

    let img = image::open("sample_rehearsal_result_page.png")?.to_rgba8();
    let preprocessed = threshold_bright_pixels(&img, 190);
    preprocessed.save("test_preprocessed.png")?;

Expected: `test_preprocessed.png` shows white background with black text where scores appear. All background elements (characters, UI chrome) should be erased.


### Step 5: Implement Tesseract wrapper

Implement `src/ocr/engine.rs`:

    pub struct OcrLine {
        pub text: String,
        pub words: Vec<OcrWord>,
        pub confidence: f32,
    }

    pub struct OcrWord {
        pub text: String,
        pub confidence: f32,
    }

    pub fn recognize_image(img: &ImageBuffer<Luma<u8>, Vec<u8>>) -> Result<Vec<OcrLine>>

Test with preprocessed image:

    let lines = recognize_image(&preprocessed)?;
    for line in &lines {
        println!("[{:.0}%] {}", line.confidence, line.text);
    }

Expected: Output includes lines with score-like text among other detected text.


### Step 6: Implement pattern matching and extraction

Implement `src/ocr/extract.rs`:

    pub fn extract_scores(lines: &[OcrLine]) -> Result<[[u32; 3]; 3]>

The function filters lines to find exactly 3 that contain 3 number-like words each.

Test:

    let scores = extract_scores(&lines)?;
    println!("Stage 1: {:?}", scores[0]);
    println!("Stage 2: {:?}", scores[1]);
    println!("Stage 3: {:?}", scores[2]);

Expected output (approximate, verify with actual image):

    Stage 1: [50339, 50796, 70859]
    Stage 2: [64997, 168009, 128450]
    Stage 3: [122130, 105901, 96776]


### Step 7: Create integration test

Create `src/ocr/tests.rs` with:

    #[test]
    fn test_ocr_sample_image() {
        let img = image::open("sample_rehearsal_result_page.png")
            .unwrap()
            .to_rgba8();

        let preprocessed = threshold_bright_pixels(&img, 190);
        let lines = recognize_image(&preprocessed).unwrap();
        let scores = extract_scores(&lines).unwrap();

        // Verify known scores (update with actual values)
        assert_eq!(scores[0][0], 50339);
        assert_eq!(scores[0][1], 50796);
        assert_eq!(scores[0][2], 70859);
        // ... etc
    }

Run:

    cargo test ocr::tests::test_ocr_sample_image -- --nocapture

Expected: Test passes with correct score extraction.


### Step 8: Add tray menu option

In `src/main.rs`, add a "Test OCR" menu item that:
1. Finds the game window
2. Captures a screenshot
3. Runs OCR pipeline
4. Shows results in a message box

Test by right-clicking tray icon and selecting "Test OCR" while game is showing result screen.

Expected: Message box displays extracted scores or an error message if OCR fails.


## Validation and Acceptance

The OCR module is complete when:

1. **Tesseract auto-setup works**: Running on a clean system without Tesseract installed, the application downloads and configures Tesseract automatically on first use.

2. **Preprocessing correctly isolates text**: The `threshold_bright_pixels()` function produces an image where only score text is visible (black on white), with all background elements removed.

3. **Score extraction succeeds**: Given `sample_rehearsal_result_page.png`, the full pipeline extracts the correct 9 scores:
   - All 9 values are non-zero integers
   - Values match what's visible in the image (manual verification)

4. **No region calibration required**: The OCR works without any `score_regions` or `stage_total_regions` in `config.json`. Only button positions are needed (for Phase 3 automation).

5. **Tray menu integration works**: Selecting "Test OCR" from the tray menu while the game shows a result screen displays the extracted scores.


## Idempotence and Recovery

- `ensure_tesseract()` is idempotent: if Tesseract is already downloaded, it returns immediately
- OCR failures return `Err` with descriptive messages, not panics
- Temporary files created during OCR are cleaned up after use
- If Tesseract download fails, error message includes manual installation instructions
- The preprocessing threshold can be adjusted via config if needed for different display settings


## Artifacts and Notes

### Tesseract Download Sources

Tesseract executable and DLLs:

    Primary: UB-Mannheim releases
    https://github.com/UB-Mannheim/tesseract/releases

English trained data:

    https://github.com/tesseract-ocr/tessdata/raw/main/eng.traineddata
    (Approximately 15MB)

### Preprocessing Visualization

Before preprocessing (original):
- Full-color game screenshot with characters, UI, background

After preprocessing (threshold=190):
- White background
- Black text where scores appear
- All other elements erased

### Pattern Matching Regex

    ^((\d+[,.])?(\d+)|[—-]+)$

Matches:
- `12345` → captures as number
- `12,345` → captures as number with comma
- `1,234,567` → captures as number with multiple commas
- `--` → captures as dash (zero/missing)

Does NOT match:
- `ステージ` (Japanese text)
- `Pt` (unit suffix)
- `195,601pt` (has non-number suffix - needs word boundary)

### Config Changes

The OCR module adds one optional config field:

    {
      "ocr_threshold": 190
    }

Default is 190 if not specified. Lower values (e.g., 160) may be needed for lower-quality captures.

The `score_regions` and `stage_total_regions` fields from the calibration tool ExecPlan are NO LONGER NEEDED and can be removed from the config schema.


## Interfaces and Dependencies

### New Dependencies in Cargo.toml

    # Option A: Tesseract C bindings
    tesseract = "0.15"
    leptonica-sys = "0.4"   # Required by tesseract crate

    # Option B: CLI wrapper (evaluate during implementation)
    # rusty-tesseract = "1.1"

    # Common dependencies
    reqwest = { version = "0.12", features = ["blocking"] }
    dirs = "5.0"
    regex = "1.10"


### New Module: src/ocr/mod.rs

    pub mod setup;
    pub mod preprocess;
    pub mod engine;
    pub mod extract;

    pub use setup::ensure_tesseract;
    pub use preprocess::threshold_bright_pixels;
    pub use engine::{recognize_image, OcrLine, OcrWord};
    pub use extract::extract_scores;

    /// High-level function: screenshot → scores
    pub fn ocr_screenshot(img: &ImageBuffer<Rgba<u8>, Vec<u8>>) -> Result<[[u32; 3]; 3]> {
        let preprocessed = threshold_bright_pixels(img, 190);
        let lines = recognize_image(&preprocessed)?;
        extract_scores(&lines)
    }


### Key Types and Functions

In `src/ocr/setup.rs`:

    pub struct TesseractPaths {
        pub executable: PathBuf,
        pub tessdata: PathBuf,
    }

    /// Ensures Tesseract is installed. Downloads if necessary.
    pub fn ensure_tesseract() -> Result<TesseractPaths>;


In `src/ocr/preprocess.rs`:

    /// Converts image to binary by keeping only bright pixels.
    /// Pixels where R > threshold AND G > threshold AND B > threshold become black.
    /// All other pixels become white.
    pub fn threshold_bright_pixels(
        img: &ImageBuffer<Rgba<u8>, Vec<u8>>,
        threshold: u8,
    ) -> ImageBuffer<Luma<u8>, Vec<u8>>;


In `src/ocr/engine.rs`:

    pub struct OcrLine {
        pub text: String,
        pub words: Vec<OcrWord>,
        pub confidence: f32,
    }

    pub struct OcrWord {
        pub text: String,
        pub confidence: f32,
    }

    /// Runs Tesseract on a preprocessed grayscale image.
    /// Returns structured output with lines and confidence scores.
    pub fn recognize_image(img: &ImageBuffer<Luma<u8>, Vec<u8>>) -> Result<Vec<OcrLine>>;


In `src/ocr/extract.rs`:

    /// Extracts 9 scores from OCR output using pattern matching.
    /// Returns [[u32; 3]; 3] representing [stage][character] scores.
    pub fn extract_scores(lines: &[OcrLine]) -> Result<[[u32; 3]; 3]>;

    /// Parses a single score string, removing commas and whitespace.
    pub fn parse_score(text: &str) -> Result<u32>;


### Updated Config (optional field)

In `src/automation/config.rs`, add:

    #[serde(default = "default_ocr_threshold")]
    pub ocr_threshold: u8,

    fn default_ocr_threshold() -> u8 { 190 }


---

## Revision History

- 2026-01-13: Initial ExecPlan created (region-based approach)
- 2026-01-14: Major revision - switched to full-image + pattern matching approach
  - Removed dependency on calibrated score regions
  - Adopted preprocessing and extraction algorithm from gakumas-tools
  - Simplified calibration requirements (no score regions needed)
  - Updated all milestones and concrete steps accordingly
