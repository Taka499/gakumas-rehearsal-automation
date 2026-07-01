# Gakumas Rehearsal Automation - Feature Roadmap

## Document Information

| Field | Value |
|-------|-------|
| Version | 1.8 |
| Created | 2026-01-12 |
| Status | Phase 5 In Progress |
| Target | Complete automation of rehearsal → screenshot → OCR → statistics pipeline |

---

## Table of Contents

1. [Project Overview](#1-project-overview)
2. [Current System Architecture](#2-current-system-architecture)
3. [Target Feature Description](#3-target-feature-description)
4. [Technical Specifications](#4-technical-specifications)
5. [Phase 1: UI Automation Foundation](#5-phase-1-ui-automation-foundation)
6. [Phase 2: OCR Integration](#6-phase-2-ocr-integration)
7. [Phase 3: Automation Loop](#7-phase-3-automation-loop)
8. [Phase 4: Statistics & Visualization](#8-phase-4-statistics--visualization)
9. [Phase 5: User Interface](#9-phase-5-user-interface)
10. [Dependencies & Environment Setup](#10-dependencies--environment-setup)
11. [File Structure](#11-file-structure)
12. [Appendix](#appendix)

---

## 1. Project Overview

### 1.1 What is Gakumas?

Gakumas (学園アイドルマスター / Gakuen iDOLM@STER) is a game that includes a "Contest Mode" with rehearsal stages. Each rehearsal:
- Contains **3 stages**
- Each stage has **3 characters** performing
- Each character receives a **score** (integer, comma-formatted, e.g., "12,345")

Players often want to run multiple rehearsals to gather statistical data on score distributions.

### 1.2 Project Goal

Automate the repetitive process of:
1. Starting a rehearsal
2. Waiting for it to complete
3. Capturing the result screen
4. Extracting scores via OCR
5. Repeating N times
6. Generating statistical analysis and charts

### 1.3 Current State (MVP)

The project currently provides a **manual screenshot tool**:
- Runs as a Windows system tray application
- Captures the game window on hotkey press (Ctrl+Shift+S)
- Uses Windows Graphics Capture API for high-quality screenshots
- Saves PNG files with timestamps

---

## 2. Current System Architecture

### 2.1 Technology Stack

| Component | Technology |
|-----------|------------|
| Language | Rust (Edition 2024) |
| Platform | Windows only |
| Graphics API | Windows Graphics Capture (WGC) |
| GPU Interface | Direct3D 11 |
| Image Processing | `image` crate |
| Build | Cargo with LTO optimization |

### 2.2 Source Code Structure

The application is organized into modules:

```
gakumas-rehearsal-automation/
├── src/
│   ├── main.rs              # Application shell, tray, message loop, hotkeys
│   ├── capture/
│   │   ├── mod.rs           # Module exports
│   │   ├── window.rs        # Window discovery (find_gakumas_window)
│   │   ├── screenshot.rs    # WGC capture pipeline (full window, cursor excluded)
│   │   └── region.rs        # Region capture (partial window, cursor excluded)
│   ├── automation/
│   │   ├── mod.rs           # Module exports
│   │   ├── config.rs        # Configuration loading (config.json)
│   │   ├── csv_writer.rs    # CSV output for results
│   │   ├── detection.rs     # Page detection (histogram), loading detection (brightness)
│   │   ├── input.rs         # Mouse input simulation (click_at_relative)
│   │   ├── ocr_worker.rs    # Background OCR processing thread
│   │   ├── queue.rs         # Thread-safe job queue
│   │   ├── runner.rs        # Main automation loop
│   │   └── state.rs         # AutomationState enum
│   ├── calibration/
│   │   ├── mod.rs           # Module exports
│   │   ├── coords.rs        # Coordinate conversion utilities
│   │   ├── preview.rs       # Preview window rendering
│   │   ├── state.rs         # CalibrationState, CalibrationItems
│   │   └── wizard.rs        # 9-step calibration wizard
│   └── ocr/
│       ├── mod.rs           # Module exports, high-level ocr_screenshot()
│       ├── setup.rs         # Tesseract path detection
│       ├── preprocess.rs    # Color threshold preprocessing
│       ├── engine.rs        # Tesseract CLI wrapper, TSV parsing
│       └── extract.rs       # Pattern matching, score extraction
├── config.json              # Automation configuration (button positions, thresholds)
├── build.rs                 # Embeds Windows manifest
├── gakumas-rehearsal-automation.exe.manifest  # UAC elevation (requireAdministrator)
├── gakumas-rehearsal-automation.rc    # Resource file for manifest
├── Cargo.toml               # Dependencies and build config
├── CLAUDE.md                # Development guidance
└── docs/
    ├── PLANS.md             # ExecPlan authoring guidelines
    ├── EXECPLAN_PHASE1_REMAINING.md  # Phase 1 implementation plan
    ├── EXECPLAN_PHASE2_OCR.md        # Phase 2 implementation plan (complete)
    ├── EXECPLAN_PHASE3_AUTOMATION.md # Phase 3 implementation plan (complete)
    └── ROADMAP_AUTOMATION.md  # This document
```

### 2.3 Core Components in main.rs

#### 2.3.1 Entry Point and Initialization

```rust
fn main() -> Result<()> {
    // Initialize Windows Runtime (required for WGC)
    RoInitialize(RO_INIT_MULTITHREADED)?;

    // Create hidden window for message handling
    let hwnd = create_message_window()?;

    // Add system tray icon
    add_tray_icon(hwnd)?;

    // Register global hotkey: Ctrl+Shift+S
    RegisterHotKey(hwnd, HOTKEY_ID, MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT, 0x53)?;

    // Enter Windows message loop
    // ... message loop code ...
}
```

#### 2.3.2 Window Discovery

The `find_gakumas_window()` function locates the game window:

1. Calls `EnumWindows` to iterate all windows
2. For each visible window with a title:
   - Gets the process ID via `GetWindowThreadProcessId`
   - Opens the process with `OpenProcess`
   - Queries the executable name with `QueryFullProcessImageNameW`
   - Checks if the name contains "gakumas" (case-insensitive)
3. Returns the window handle (HWND) when found

#### 2.3.3 Screen Capture Pipeline

The `capture_gakumas()` function performs screenshot capture:

```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│  Find Window    │────▶│  Get Client     │────▶│  Create D3D11   │
│  (HWND)         │     │  Area Info      │     │  Device         │
└─────────────────┘     └─────────────────┘     └─────────────────┘
                                                         │
                                                         ▼
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│  Save PNG       │◀────│  Copy to        │◀────│  Create Capture │
│  File           │     │  Staging Texture│     │  Session        │
└─────────────────┘     └─────────────────┘     └─────────────────┘
```

Key steps:
1. **Get client area**: Excludes title bar and window borders
2. **Create D3D11 device**: Hardware-accelerated graphics
3. **Create capture item**: Via `IGraphicsCaptureItemInterop::CreateForWindow`
4. **Frame pool**: Receives captured frames asynchronously
5. **Staging texture**: GPU → CPU memory transfer
6. **Pixel conversion**: BGRA → RGBA format
7. **Crop**: Extract only the client area portion

#### 2.3.4 System Tray

The application runs invisibly with a system tray icon:
- Right-click shows context menu with "Exit" option
- Double-click triggers a log message (extensible for future features)
- Tooltip shows "Gakumas Screenshot (Ctrl+Shift+S)"

#### 2.3.5 Logging

All operations log to both console and `gakumas_screenshot.log`:

```rust
fn log(msg: &str) {
    let timestamp = Local::now().format("%H:%M:%S%.3f");
    let line = format!("[{}] {}\n", timestamp, msg);
    print!("{}", line);
    // Also appends to log file
}
```

### 2.4 Windows API Patterns

#### Using the `windows` Crate

The project uses Microsoft's official `windows` crate (v0.58). Key patterns:

```rust
// Importing Windows APIs
use windows::Win32::UI::WindowsAndMessaging::*;

// Wide string literals for Windows APIs
let class_name = w!("MyClassName");

// Calling unsafe Windows APIs
unsafe {
    RegisterHotKey(hwnd, HOTKEY_ID, MOD_CONTROL | MOD_SHIFT, 0x53)?;
}

// Error handling with anyhow
let hwnd = find_window().context("Failed to find window")?;
```

#### Window Procedure Pattern

Windows GUI applications use a callback for message handling:

```rust
unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_HOTKEY => { /* handle hotkey */ }
        WM_DESTROY => { PostQuitMessage(0); }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}
```

---

## 3. Target Feature Description

### 3.1 Automation Workflow

```
┌──────────────────────────────────────────────────────────────────┐
│                    USER STARTS AUTOMATION                         │
│                    (at rehearsal page)                            │
└──────────────────────────────────────────────────────────────────┘
                                │
                                ▼
┌──────────────────────────────────────────────────────────────────┐
│  LOOP (n iterations)                                              │
│  ┌────────────────────────────────────────────────────────────┐  │
│  │ 1. Click "開始する" (Start) button                          │  │
│  │ 2. Wait for loading to complete                            │  │
│  │    - Detect: loading dots disappear above スキップ button   │  │
│  │    - Detect: button brightness returns to normal           │  │
│  │ 3. Click "スキップ" (Skip) button                           │  │
│  │ 4. Capture result screen                                   │  │
│  │ 5. OCR extract 9 scores (3 stages × 3 characters)          │  │
│  │ 6. Store scores in memory                                  │  │
│  └────────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────────┘
                                │
                                ▼
┌──────────────────────────────────────────────────────────────────┐
│  POST-PROCESSING                                                  │
│  - Calculate statistics (mean, mode, min, max) per character     │
│  - Generate bar chart (average scores)                           │
│  - Generate box plot (score distribution)                        │
│  - Save charts as PNG files                                      │
│  - Export raw data as CSV                                        │
└──────────────────────────────────────────────────────────────────┘
```

### 3.2 Data Model

```
RunResult {
    timestamp: DateTime,
    iteration: u32,
    stages: [StageResult; 3]
}

StageResult {
    stage_number: u8,        // 1, 2, or 3
    scores: [CharacterScore; 3]
}

CharacterScore {
    character_index: u8,     // 0, 1, or 2 (position in stage)
    score: u32               // e.g., 12345
}
```

### 3.3 Configuration Parameters

| Parameter | Type | Description | Example |
|-----------|------|-------------|---------|
| iterations | u32 | Number of rehearsals to run | 100 |
| output_dir | PathBuf | Where to save results | `./results/` |
| capture_delay_ms | u64 | Wait after skip before capture | 500 |
| loading_timeout_ms | u64 | Max wait for loading | 30000 |

These should be configurable at runtime (command-line args or config file), with a future GUI for editing.

### 3.4 UI Coordinate System

The game window may resize based on monitor, but **proportions remain fixed**.

All button/score positions should be stored as **relative coordinates** (0.0 to 1.0):

```rust
struct RelativeRect {
    x: f32,      // 0.0 = left edge, 1.0 = right edge
    y: f32,      // 0.0 = top edge, 1.0 = bottom edge
    width: f32,
    height: f32,
}

// Convert to absolute pixels
fn to_absolute(rel: &RelativeRect, client_width: u32, client_height: u32) -> Rect {
    Rect {
        x: (rel.x * client_width as f32) as i32,
        y: (rel.y * client_height as f32) as i32,
        width: (rel.width * client_width as f32) as u32,
        height: (rel.height * client_height as f32) as u32,
    }
}
```

---

## 4. Technical Specifications

### 4.1 Score Format

- Scores are **integers** displayed with **comma separators**
- Example: `12,345` or `1,234,567`
- OCR must handle comma removal: `"12,345"` → `12345`

### 4.2 Button States

#### "開始する" (Start) Button
- Appears on the rehearsal preparation page
- Always clickable when visible (no loading state)

#### "スキップ" (Skip) Button
- **Loading state**:
  - Three animated dots (`...`) appear above the button
  - Button appears dimmed (lower brightness)
- **Ready state**:
  - Dots disappear
  - Button returns to full brightness

### 4.3 Result Screen Layout

The result screen displays scores in a grid:

```
┌─────────────────────────────────────────┐
│            STAGE 1                      │
│  ┌─────────┐ ┌─────────┐ ┌─────────┐   │
│  │ Char 1  │ │ Char 2  │ │ Char 3  │   │
│  │ 12,345  │ │ 23,456  │ │ 34,567  │   │
│  └─────────┘ └─────────┘ └─────────┘   │
├─────────────────────────────────────────┤
│            STAGE 2                      │
│  ┌─────────┐ ┌─────────┐ ┌─────────┐   │
│  │ Char 1  │ │ Char 2  │ │ Char 3  │   │
│  │ 45,678  │ │ 56,789  │ │ 67,890  │   │
│  └─────────┘ └─────────┘ └─────────┘   │
├─────────────────────────────────────────┤
│            STAGE 3                      │
│  ┌─────────┐ ┌─────────┐ ┌─────────┐   │
│  │ Char 1  │ │ Char 2  │ │ Char 3  │   │
│  │ 78,901  │ │ 89,012  │ │ 90,123  │   │
│  └─────────┘ └─────────┘ └─────────┘   │
└─────────────────────────────────────────┘
```

Each score region needs a defined ROI (Region of Interest) for OCR.

---

## 5. Phase 1: UI Automation Foundation

### 5.1 Objective

Enable the application to:
1. Send mouse clicks to specific screen coordinates
2. Detect button positions using relative coordinates
3. Detect loading state by analyzing screen regions

### 5.2 Experimental Findings (2026-01-13)

We tested two approaches for simulating mouse clicks on the game window:

#### 5.2.1 Method Comparison

| Method | API | Cursor Moves? | Works? | Requirements |
|--------|-----|---------------|--------|--------------|
| PostMessage | `PostMessageW` + `WM_LBUTTONDOWN/UP` | No | Yes* | Window foreground + cursor inside window |
| SendInput | `SendInput` + `MOUSEINPUT` | Yes | Yes | Window foreground |

*PostMessage only works when both conditions are met.

#### 5.2.2 Key Findings

1. **UIPI (User Interface Privilege Isolation)**: If the game runs elevated (as Administrator) and our tool runs as normal user, `PostMessage` returns "Access is denied" (0x80070005). Both processes must run at the same privilege level.

2. **Game ignores unfocused input**: Even when `PostMessage` succeeds (`Ok(())`), the game does not respond unless it has focus. This indicates the game validates window focus state before processing input.

3. **SendInput is more reliable**: `SendInput` simulates hardware-level input that the game's input layer (likely DirectInput/RawInput) processes correctly.

4. **SetForegroundWindow required**: Before sending any input, we must call `SetForegroundWindow` to ensure the game window is active.

#### 5.2.3 Chosen Approach

**SendInput with SetForegroundWindow** is the confirmed working method.

Trade-offs:
- User cannot use the PC during automation (cursor will move, window will be focused)
- Must run as Administrator if the game runs as Administrator
- Reliable across different game input implementations

#### 5.2.4 Process Name Matching Fix

During testing, discovered that substring matching (`contains("gakumas")`) incorrectly matched `gakumas-rehearsal-automation.exe`. Fixed to use exact match:

```rust
// Before (buggy)
if process_name_lower.contains("gakumas") { ... }

// After (correct)
if process_name_lower == "gakumas.exe" { ... }
```

### 5.3 Mouse Input Simulation

#### 5.3.1 Windows API: SendInput

The `SendInput` function sends simulated input events:

```rust
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_MOUSE, MOUSEINPUT,
    MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_MOVE
};
```

#### 5.2.2 Implementation Steps

1. **Add required Windows features** to Cargo.toml:
   ```toml
   "Win32_UI_Input_KeyboardAndMouse"  # Already included
   ```

2. **Create click function**:
   ```rust
   fn click_at(x: i32, y: i32) -> Result<()> {
       // 1. Convert screen coordinates to normalized coordinates (0-65535)
       // 2. Create INPUT structure for mouse move
       // 3. Create INPUT structures for mouse down/up
       // 4. Call SendInput with all three events
   }
   ```

3. **Coordinate translation**:
   - Game client coordinates → Screen coordinates
   - Use `ClientToScreen` API (already imported in current code)

#### 5.3.3 Click Implementation Pattern (Verified Working)

```rust
fn click_at_screen(hwnd: HWND, screen_x: i32, screen_y: i32) -> Result<()> {
    // IMPORTANT: Bring window to foreground first
    unsafe {
        SetForegroundWindow(hwnd);
    }
    std::thread::sleep(Duration::from_millis(100));

    // Get screen dimensions for normalization
    let screen_width = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let screen_height = unsafe { GetSystemMetrics(SM_CYSCREEN) };

    // Normalize to 0-65535 range (required by MOUSEEVENTF_ABSOLUTE)
    let norm_x = ((screen_x as i64 * 65535) / screen_width as i64) as i32;
    let norm_y = ((screen_y as i64 * 65535) / screen_height as i64) as i32;

    unsafe {
        // Move to position
        let move_input = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: norm_x,
                    dy: norm_y,
                    dwFlags: MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE,
                    ..Default::default()
                },
            },
        };
        SendInput(&[move_input], std::mem::size_of::<INPUT>() as i32);

        std::thread::sleep(Duration::from_millis(50));

        // Mouse down with absolute position
        let down_input = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: norm_x,
                    dy: norm_y,
                    dwFlags: MOUSEEVENTF_LEFTDOWN | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_MOVE,
                    ..Default::default()
                },
            },
        };
        SendInput(&[down_input], std::mem::size_of::<INPUT>() as i32);

        std::thread::sleep(Duration::from_millis(50));

        // Mouse up with absolute position
        let up_input = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: norm_x,
                    dy: norm_y,
                    dwFlags: MOUSEEVENTF_LEFTUP | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_MOVE,
                    ..Default::default()
                },
            },
        };
        SendInput(&[up_input], std::mem::size_of::<INPUT>() as i32);
    }

    Ok(())
}
```

### 5.4 Relative Coordinate System

#### 5.4.1 Configuration Structure

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
struct ButtonConfig {
    /// Relative X position (0.0 - 1.0)
    x: f32,
    /// Relative Y position (0.0 - 1.0)
    y: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct AutomationConfig {
    start_button: ButtonConfig,   // "開始する" button
    skip_button: ButtonConfig,    // "スキップ" button
    score_regions: [[ScoreRegion; 3]; 3],  // 3 stages × 3 characters
}
```

#### 5.4.2 Coordinate Calibration

A calibration utility should be created to help users define button positions:
1. Take a reference screenshot
2. User clicks on button locations
3. Convert click positions to relative coordinates
4. Save to configuration file

### 5.5 Loading State Detection

#### 5.5.1 Detection Strategy

Two complementary methods:

**Method A: Brightness Detection**
- Capture a small region around the skip button
- Calculate average brightness
- Compare against threshold:
  - Below threshold = dimmed (loading)
  - Above threshold = ready

**Method B: Dot Pattern Detection**
- Capture region above the skip button
- Look for animated dot pattern
- Dots present = loading, dots absent = ready

#### 5.5.2 Brightness Calculation

```rust
fn calculate_average_brightness(img: &ImageBuffer<Rgba<u8>, Vec<u8>>) -> f32 {
    let mut total: u64 = 0;
    let pixel_count = img.width() * img.height();

    for pixel in img.pixels() {
        // Convert to grayscale luminance
        let luminance = (0.299 * pixel[0] as f32
                       + 0.587 * pixel[1] as f32
                       + 0.114 * pixel[2] as f32) as u64;
        total += luminance;
    }

    total as f32 / pixel_count as f32
}
```

#### 5.5.3 Polling Loop

```rust
fn wait_for_loading_complete(
    hwnd: HWND,
    skip_button_region: &RelativeRect,
    timeout_ms: u64,
    brightness_threshold: f32,
) -> Result<()> {
    let start = Instant::now();

    loop {
        if start.elapsed().as_millis() > timeout_ms as u128 {
            return Err(anyhow!("Loading timeout"));
        }

        // Capture region around skip button
        let region_img = capture_region(hwnd, skip_button_region)?;
        let brightness = calculate_average_brightness(&region_img);

        if brightness > brightness_threshold {
            // Button is no longer dimmed
            return Ok(());
        }

        std::thread::sleep(Duration::from_millis(100));
    }
}
```

### 5.6 Deliverables for Phase 1

| Deliverable | Description |
|-------------|-------------|
| `click_at()` function | Send mouse click to screen coordinates |
| `click_at_relative()` function | Click using relative coordinates |
| `capture_region()` function | Capture a sub-region of the game window |
| `wait_for_loading()` function | Poll until loading completes |
| `AutomationConfig` struct | Configuration for button positions |
| Calibration utility | Helper to determine button coordinates |

---

## 6. Phase 2: OCR Integration

**Status: COMPLETE** (2026-01-14)

### 6.1 Objective

Extract score values from captured screenshots using Tesseract OCR.

### 6.1.1 Actual Implementation

The implementation follows the gakumas-tools approach (full-image pattern matching) instead of the region-based approach originally planned:

1. **Full-image preprocessing**: Apply color threshold to entire screenshot (keep pixels where R, G, B all > threshold)
2. **Tesseract CLI**: Use `tesseract` command via `std::process::Command` with TSV output
3. **Pattern matching**: Find score lines using regex `^((\d+[,.])*\d+|[—\-]+)$`
4. **Confidence filtering**: Only accept lines with 60%+ average word confidence

Key differences from original plan:
- No region-based extraction (simpler, more robust)
- CLI-based Tesseract (no C bindings, easier setup)
- TSV output parsing for word-level confidence
- `ocr_threshold` config field (default 190) instead of fixed value

### 6.2 Tesseract Setup

#### 6.2.1 Windows Installation

1. **Download Tesseract for Windows**:
   - URL: https://github.com/UB-Mannheim/tesseract/wiki
   - Install to: `C:\Program Files\Tesseract-OCR\`

2. **Install Japanese language data**:
   - During installation, select "Japanese" language pack
   - Or manually download `jpn.traineddata` to `tessdata` folder

3. **Add to PATH**:
   - Add `C:\Program Files\Tesseract-OCR` to system PATH
   - Or specify path in code

#### 6.2.2 Verify Installation

```powershell
tesseract --version
tesseract --list-langs  # Should show "jpn" if Japanese is installed
```

### 6.3 Rust Tesseract Bindings

#### 6.3.1 Crate Selection

Use `rusty-tesseract` crate:
- Provides safe Rust wrapper around Tesseract
- Supports image input from various formats

```toml
[dependencies]
rusty-tesseract = "1.1"
```

#### 6.3.2 Basic Usage Pattern

```rust
use rusty_tesseract::{Args, Image};

fn ocr_score(img: &ImageBuffer<Rgba<u8>, Vec<u8>>) -> Result<u32> {
    // Save to temporary file (rusty-tesseract requires file path)
    let temp_path = std::env::temp_dir().join("ocr_temp.png");
    img.save(&temp_path)?;

    // Configure Tesseract
    let args = Args {
        lang: "jpn".to_string(),  // Japanese language
        config_variables: HashMap::from([
            ("tessedit_char_whitelist".to_string(), "0123456789,".to_string()),
        ]),
        ..Default::default()
    };

    // Perform OCR
    let image = Image::from_path(&temp_path)?;
    let output = rusty_tesseract::image_to_string(&image, &args)?;

    // Parse result
    parse_score(&output)
}

fn parse_score(text: &str) -> Result<u32> {
    // Remove commas and whitespace
    let cleaned: String = text.chars()
        .filter(|c| c.is_ascii_digit())
        .collect();

    cleaned.parse::<u32>()
        .context("Failed to parse score as integer")
}
```

### 6.4 Score Region Configuration

#### 6.4.1 Region Definition

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
struct ScoreRegion {
    /// Relative position and size within game window
    x: f32,
    y: f32,
    width: f32,
    height: f32,

    /// Metadata
    stage: u8,           // 1, 2, or 3
    character_index: u8, // 0, 1, or 2
}
```

#### 6.4.2 Region Extraction

```rust
fn extract_score_region(
    full_image: &ImageBuffer<Rgba<u8>, Vec<u8>>,
    region: &ScoreRegion,
) -> ImageBuffer<Rgba<u8>, Vec<u8>> {
    let img_width = full_image.width();
    let img_height = full_image.height();

    let x = (region.x * img_width as f32) as u32;
    let y = (region.y * img_height as f32) as u32;
    let w = (region.width * img_width as f32) as u32;
    let h = (region.height * img_height as f32) as u32;

    image::imageops::crop_imm(full_image, x, y, w, h).to_image()
}
```

### 6.5 OCR Accuracy Improvements

#### 6.5.1 Image Preprocessing

Before OCR, apply preprocessing to improve accuracy:

```rust
fn preprocess_for_ocr(img: &ImageBuffer<Rgba<u8>, Vec<u8>>) -> ImageBuffer<Luma<u8>, Vec<u8>> {
    // Convert to grayscale
    let gray = image::imageops::grayscale(img);

    // Apply threshold to create high-contrast binary image
    let threshold = 128u8;
    ImageBuffer::from_fn(gray.width(), gray.height(), |x, y| {
        let pixel = gray.get_pixel(x, y);
        if pixel[0] > threshold {
            Luma([255u8])
        } else {
            Luma([0u8])
        }
    })
}
```

#### 6.5.2 Tesseract Configuration

Optimize Tesseract for numeric recognition:

```rust
let args = Args {
    lang: "eng".to_string(),  // English may work better for numbers
    config_variables: HashMap::from([
        // Only recognize digits and comma
        ("tessedit_char_whitelist".to_string(), "0123456789,".to_string()),
        // Single line mode
        ("tessedit_pageseg_mode".to_string(), "7".to_string()),
    ]),
    ..Default::default()
};
```

Page segmentation modes:
- `6` = Assume single uniform block of text
- `7` = Treat image as single text line
- `8` = Treat image as single word

### 6.6 Batch OCR Processing

```rust
struct RunResult {
    timestamp: DateTime<Local>,
    iteration: u32,
    scores: [[u32; 3]; 3],  // [stage][character]
}

fn ocr_all_scores(
    full_image: &ImageBuffer<Rgba<u8>, Vec<u8>>,
    regions: &[[ScoreRegion; 3]; 3],
) -> Result<[[u32; 3]; 3]> {
    let mut scores = [[0u32; 3]; 3];

    for (stage_idx, stage_regions) in regions.iter().enumerate() {
        for (char_idx, region) in stage_regions.iter().enumerate() {
            let cropped = extract_score_region(full_image, region);
            let preprocessed = preprocess_for_ocr(&cropped);
            scores[stage_idx][char_idx] = ocr_score(&preprocessed)?;
        }
    }

    Ok(scores)
}
```

### 6.7 Deliverables for Phase 2

| Deliverable | Status | Description |
|-------------|--------|-------------|
| `src/ocr/mod.rs` | ✅ | Module exports, `ocr_screenshot()` entry point |
| `src/ocr/setup.rs` | ✅ | Tesseract path detection |
| `src/ocr/preprocess.rs` | ✅ | Color threshold preprocessing (`threshold_bright_pixels`) |
| `src/ocr/engine.rs` | ✅ | Tesseract CLI wrapper, TSV parsing (`recognize_image`) |
| `src/ocr/extract.rs` | ✅ | Pattern matching, score extraction (`extract_scores`) |
| Tray menu "Test OCR" | ✅ | Manual testing via right-click menu |
| `ocr_threshold` config | ✅ | Configurable threshold in config.json (default 190) |

---

## 7. Phase 3: Automation Loop

**Status: COMPLETE** (2026-01-15)

### 7.1 Objective

Combine all components into a cohesive automation loop that runs N iterations.

### 7.1.1 Actual Implementation

The automation loop was implemented with the following features:

1. **State machine pattern**: Manages automation flow through well-defined states
2. **Histogram comparison**: Detects page state by comparing screen regions against reference images (Start, Skip, End buttons)
3. **Brightness threshold**: Detects when Skip button becomes enabled (loading complete)
4. **Background OCR processing**: OCR runs in a separate thread to avoid blocking automation
5. **CSV output**: Results written to timestamped CSV files
6. **Abort mechanism**: Ctrl+Shift+Escape to stop automation at any time
7. **9-step calibration wizard**: Calibrates all 3 buttons (position + detection region each)
8. **Cursor exclusion**: Screenshots exclude the mouse cursor via `SetIsCursorCaptureEnabled(false)`

Key flow:
```
Start Page Detection → Click Start → Wait for Loading →
Click Skip → Wait for Result → Capture & OCR → Click End → Loop
```

### 7.2 State Machine Design

```
                    ┌─────────────────┐
                    │      IDLE       │
                    └────────┬────────┘
                             │ start_automation()
                             ▼
                    ┌─────────────────┐
         ┌─────────│   CLICK_START   │
         │         └────────┬────────┘
         │                  │ click successful
         │                  ▼
         │         ┌─────────────────┐
         │         │  WAIT_LOADING   │──────────┐
         │         └────────┬────────┘          │ timeout
         │                  │ loading complete  │
         │                  ▼                   ▼
         │         ┌─────────────────┐   ┌───────────┐
         │         │   CLICK_SKIP    │   │   ERROR   │
         │         └────────┬────────┘   └───────────┘
         │                  │ click successful
         │                  ▼
         │         ┌─────────────────┐
         │         │ CAPTURE_RESULT  │
         │         └────────┬────────┘
         │                  │ scores extracted
         │                  ▼
         │         ┌─────────────────┐
         │         │  CHECK_LOOP     │
         │         └────────┬────────┘
         │                  │
         │     ┌────────────┴────────────┐
         │     │ iteration < n           │ iteration >= n
         │     ▼                         ▼
         └─────┘                ┌─────────────────┐
                                │   COMPLETE      │
                                └─────────────────┘
```

### 7.3 Implementation Structure

```rust
enum AutomationState {
    Idle,
    ClickingStart,
    WaitingForLoading,
    ClickingSkip,
    CapturingResult,
    CheckingLoop,
    Complete,
    Error(String),
}

struct AutomationContext {
    state: AutomationState,
    config: AutomationConfig,
    current_iteration: u32,
    max_iterations: u32,
    results: Vec<RunResult>,
    hwnd: HWND,
}

impl AutomationContext {
    fn run(&mut self) -> Result<Vec<RunResult>> {
        while !matches!(self.state, AutomationState::Complete | AutomationState::Error(_)) {
            self.step()?;
        }

        match &self.state {
            AutomationState::Complete => Ok(std::mem::take(&mut self.results)),
            AutomationState::Error(msg) => Err(anyhow!("{}", msg)),
            _ => unreachable!(),
        }
    }

    fn step(&mut self) -> Result<()> {
        match self.state {
            AutomationState::Idle => {
                log(&format!("Starting automation: {} iterations", self.max_iterations));
                self.state = AutomationState::ClickingStart;
            }

            AutomationState::ClickingStart => {
                log(&format!("Iteration {}/{}", self.current_iteration + 1, self.max_iterations));
                self.click_start_button()?;
                self.state = AutomationState::WaitingForLoading;
            }

            AutomationState::WaitingForLoading => {
                self.wait_for_loading()?;
                self.state = AutomationState::ClickingSkip;
            }

            AutomationState::ClickingSkip => {
                self.click_skip_button()?;
                std::thread::sleep(Duration::from_millis(self.config.capture_delay_ms));
                self.state = AutomationState::CapturingResult;
            }

            AutomationState::CapturingResult => {
                let result = self.capture_and_ocr()?;
                self.results.push(result);
                self.state = AutomationState::CheckingLoop;
            }

            AutomationState::CheckingLoop => {
                self.current_iteration += 1;
                if self.current_iteration >= self.max_iterations {
                    self.state = AutomationState::Complete;
                } else {
                    self.state = AutomationState::ClickingStart;
                }
            }

            _ => {}
        }
        Ok(())
    }
}
```

### 7.4 Error Handling and Recovery

#### 7.4.1 Timeout Recovery

```rust
fn wait_for_loading(&self) -> Result<()> {
    match wait_for_loading_complete(
        self.hwnd,
        &self.config.skip_button_region,
        self.config.loading_timeout_ms,
        self.config.brightness_threshold,
    ) {
        Ok(()) => Ok(()),
        Err(e) => {
            log(&format!("Loading timeout, attempting recovery: {}", e));
            // Could implement recovery: click somewhere safe, try again
            Err(e)
        }
    }
}
```

#### 7.4.2 OCR Validation

```rust
fn validate_score(score: u32) -> Result<u32> {
    // Scores should be within reasonable range
    const MIN_SCORE: u32 = 0;
    const MAX_SCORE: u32 = 10_000_000;  // Adjust based on game

    if score >= MIN_SCORE && score <= MAX_SCORE {
        Ok(score)
    } else {
        Err(anyhow!("Score {} is outside valid range", score))
    }
}
```

### 7.5 Abort Mechanism

```rust
static ABORT_REQUESTED: AtomicBool = AtomicBool::new(false);

// In window_proc, add hotkey for abort (e.g., Escape)
WM_HOTKEY if wparam.0 as i32 == ABORT_HOTKEY_ID => {
    ABORT_REQUESTED.store(true, Ordering::SeqCst);
    log("Abort requested by user");
}

// In automation loop
fn step(&mut self) -> Result<()> {
    if ABORT_REQUESTED.load(Ordering::SeqCst) {
        self.state = AutomationState::Error("Aborted by user".to_string());
        return Ok(());
    }
    // ... rest of step logic
}
```

### 7.6 Progress Reporting

```rust
fn report_progress(&self) {
    let percent = (self.current_iteration as f32 / self.max_iterations as f32) * 100.0;
    log(&format!(
        "Progress: {}/{} ({:.1}%)",
        self.current_iteration,
        self.max_iterations,
        percent
    ));

    // Update tray icon tooltip
    update_tray_tooltip(&format!(
        "Gakumas Automation: {}/{} ({:.0}%)",
        self.current_iteration,
        self.max_iterations,
        percent
    ));
}
```

### 7.7 Deliverables for Phase 3

| Deliverable | Status | Description |
|-------------|--------|-------------|
| `src/automation/state.rs` | ✅ | `AutomationState` enum with all states |
| `src/automation/runner.rs` | ✅ | Main automation loop, state transitions |
| `src/automation/queue.rs` | ✅ | Thread-safe queue for OCR jobs |
| `src/automation/ocr_worker.rs` | ✅ | Background OCR processing thread |
| `src/automation/csv_writer.rs` | ✅ | CSV output for results |
| `src/automation/detection.rs` | ✅ | `wait_for_start_page`, `wait_for_loading`, `wait_for_result` |
| `src/calibration/` module | ✅ | 9-step calibration wizard |
| Abort mechanism (Ctrl+Shift+Esc) | ✅ | Hotkey to stop automation |
| Tray menu "Start Automation" | ✅ | Menu item to begin automation loop |
| Cursor exclusion | ✅ | `SetIsCursorCaptureEnabled(false)` in all capture functions |
| Reference image capture | ✅ | Menu items for Start, Skip, End button references |

---

## 8. Phase 4: Statistics & Visualization

### 8.1 Objective

Process collected scores to calculate statistics and generate visual charts.

### 8.2 Data Structure for Analysis

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
struct CharacterStats {
    character_index: u8,
    stage: u8,
    scores: Vec<u32>,
    mean: f64,
    mode: u32,
    min: u32,
    max: u32,
    std_dev: f64,
    median: f64,
    quartile_1: f64,
    quartile_3: f64,
}

struct AnalysisResult {
    total_iterations: u32,
    timestamp: DateTime<Local>,
    character_stats: [[CharacterStats; 3]; 3],  // [stage][character]
}
```

### 8.3 Statistics Calculation

```rust
fn calculate_stats(scores: &[u32]) -> CharacterStats {
    let n = scores.len() as f64;

    // Mean
    let sum: u64 = scores.iter().map(|&s| s as u64).sum();
    let mean = sum as f64 / n;

    // Sort for median, quartiles, min, max
    let mut sorted = scores.to_vec();
    sorted.sort();

    let min = sorted[0];
    let max = sorted[sorted.len() - 1];

    // Median
    let median = if sorted.len() % 2 == 0 {
        (sorted[sorted.len() / 2 - 1] + sorted[sorted.len() / 2]) as f64 / 2.0
    } else {
        sorted[sorted.len() / 2] as f64
    };

    // Quartiles (using linear interpolation)
    let q1 = percentile(&sorted, 25.0);
    let q3 = percentile(&sorted, 75.0);

    // Mode (most frequent value)
    let mode = calculate_mode(scores);

    // Standard deviation
    let variance: f64 = scores.iter()
        .map(|&s| (s as f64 - mean).powi(2))
        .sum::<f64>() / n;
    let std_dev = variance.sqrt();

    CharacterStats {
        scores: scores.to_vec(),
        mean,
        mode,
        min,
        max,
        std_dev,
        median,
        quartile_1: q1,
        quartile_3: q3,
        ..Default::default()
    }
}

fn percentile(sorted: &[u32], p: f64) -> f64 {
    let idx = (p / 100.0) * (sorted.len() - 1) as f64;
    let lower = sorted[idx.floor() as usize] as f64;
    let upper = sorted[idx.ceil() as usize] as f64;
    let frac = idx.fract();
    lower + (upper - lower) * frac
}

fn calculate_mode(values: &[u32]) -> u32 {
    let mut counts = HashMap::new();
    for &v in values {
        *counts.entry(v).or_insert(0) += 1;
    }
    counts.into_iter()
        .max_by_key(|&(_, count)| count)
        .map(|(value, _)| value)
        .unwrap_or(0)
}
```

### 8.4 Chart Generation with Plotters

#### 8.4.1 Crate Setup

```toml
[dependencies]
plotters = "0.3"
```

#### 8.4.2 Bar Chart (Average Scores)

```rust
use plotters::prelude::*;

fn generate_bar_chart(
    stats: &[[CharacterStats; 3]; 3],
    output_path: &Path,
) -> Result<()> {
    let root = BitMapBackend::new(output_path, (800, 600)).into_drawing_area();
    root.fill(&WHITE)?;

    // Find max score for Y axis
    let max_score = stats.iter()
        .flat_map(|stage| stage.iter())
        .map(|s| s.mean)
        .fold(0.0f64, f64::max);

    let mut chart = ChartBuilder::on(&root)
        .caption("Average Scores by Character", ("sans-serif", 30))
        .margin(20)
        .x_label_area_size(40)
        .y_label_area_size(60)
        .build_cartesian_2d(
            0..9,  // 9 character slots
            0.0..max_score * 1.1,
        )?;

    chart.configure_mesh()
        .x_labels(9)
        .x_label_formatter(&|x| {
            let stage = x / 3 + 1;
            let char_idx = x % 3 + 1;
            format!("S{}C{}", stage, char_idx)
        })
        .y_desc("Average Score")
        .draw()?;

    // Draw bars
    let colors = [RED, GREEN, BLUE];
    for stage in 0..3 {
        for char_idx in 0..3 {
            let idx = stage * 3 + char_idx;
            let mean = stats[stage][char_idx].mean;

            chart.draw_series(std::iter::once(
                Rectangle::new(
                    [(idx, 0.0), (idx + 1, mean)],
                    colors[stage].filled(),
                )
            ))?;
        }
    }

    root.present()?;
    Ok(())
}
```

#### 8.4.3 Box Plot (Score Distribution)

```rust
fn generate_box_plot(
    stats: &[[CharacterStats; 3]; 3],
    output_path: &Path,
) -> Result<()> {
    let root = BitMapBackend::new(output_path, (800, 600)).into_drawing_area();
    root.fill(&WHITE)?;

    let max_score = stats.iter()
        .flat_map(|stage| stage.iter())
        .map(|s| s.max as f64)
        .fold(0.0f64, f64::max);

    let mut chart = ChartBuilder::on(&root)
        .caption("Score Distribution by Character", ("sans-serif", 30))
        .margin(20)
        .x_label_area_size(40)
        .y_label_area_size(60)
        .build_cartesian_2d(
            0..9,
            0.0..max_score * 1.1,
        )?;

    chart.configure_mesh()
        .x_labels(9)
        .x_label_formatter(&|x| format!("S{}C{}", x / 3 + 1, x % 3 + 1))
        .y_desc("Score")
        .draw()?;

    // Draw box plots
    for stage in 0..3 {
        for char_idx in 0..3 {
            let idx = stage * 3 + char_idx;
            let s = &stats[stage][char_idx];

            let box_x = idx as f64 + 0.2;
            let box_width = 0.6;

            // Box (Q1 to Q3)
            chart.draw_series(std::iter::once(
                Rectangle::new(
                    [(box_x as i32, s.quartile_1), ((box_x + box_width) as i32, s.quartile_3)],
                    BLUE.stroke_width(2),
                )
            ))?;

            // Median line
            chart.draw_series(std::iter::once(
                PathElement::new(
                    vec![
                        (box_x as i32, s.median),
                        ((box_x + box_width) as i32, s.median),
                    ],
                    RED.stroke_width(2),
                )
            ))?;

            // Whiskers (min to Q1, Q3 to max)
            let center_x = (box_x + box_width / 2.0) as i32;
            chart.draw_series(std::iter::once(
                PathElement::new(
                    vec![(center_x, s.min as f64), (center_x, s.quartile_1)],
                    BLACK.stroke_width(1),
                )
            ))?;
            chart.draw_series(std::iter::once(
                PathElement::new(
                    vec![(center_x, s.quartile_3), (center_x, s.max as f64)],
                    BLACK.stroke_width(1),
                )
            ))?;
        }
    }

    root.present()?;
    Ok(())
}
```

### 8.5 Data Export

#### 8.5.1 CSV Export

```rust
fn export_to_csv(results: &[RunResult], path: &Path) -> Result<()> {
    let mut wtr = csv::Writer::from_path(path)?;

    // Header
    wtr.write_record(&[
        "iteration", "timestamp",
        "s1c1", "s1c2", "s1c3",
        "s2c1", "s2c2", "s2c3",
        "s3c1", "s3c2", "s3c3",
    ])?;

    // Data rows
    for result in results {
        wtr.write_record(&[
            result.iteration.to_string(),
            result.timestamp.to_rfc3339(),
            result.scores[0][0].to_string(),
            result.scores[0][1].to_string(),
            result.scores[0][2].to_string(),
            result.scores[1][0].to_string(),
            result.scores[1][1].to_string(),
            result.scores[1][2].to_string(),
            result.scores[2][0].to_string(),
            result.scores[2][1].to_string(),
            result.scores[2][2].to_string(),
        ])?;
    }

    wtr.flush()?;
    Ok(())
}
```

#### 8.5.2 JSON Export

```rust
fn export_to_json(analysis: &AnalysisResult, path: &Path) -> Result<()> {
    let json = serde_json::to_string_pretty(analysis)?;
    std::fs::write(path, json)?;
    Ok(())
}
```

### 8.6 Future Enhancement: Character Icons

The roadmap mentions that a future GUI will display character icons next to charts. To support this:

1. **Capture character icons** during calibration phase
2. **Store as reference images** in a `icons/` directory
3. **Composite onto charts** using `image` crate's overlay functions

```rust
fn add_character_icon_to_chart(
    chart_img: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
    icon_path: &Path,
    position: (u32, u32),
) -> Result<()> {
    let icon = image::open(icon_path)?.to_rgba8();
    image::imageops::overlay(chart_img, &icon, position.0 as i64, position.1 as i64);
    Ok(())
}
```

### 8.7 Deliverables for Phase 4

| Deliverable | Description |
|-------------|-------------|
| `CharacterStats` struct | Per-character statistics |
| `calculate_stats()` function | Mean, mode, min, max, std_dev, quartiles |
| `generate_bar_chart()` function | Bar chart PNG generation |
| `generate_box_plot()` function | Box plot PNG generation |
| `export_to_csv()` function | Raw data CSV export |
| `export_to_json()` function | Analysis results JSON export |

---

## 9. Phase 5: User Interface

### 9.1 Objective

Provide user-friendly interfaces for configuration and monitoring.

### 9.2 Command-Line Interface (Immediate)

#### 9.2.1 Argument Parsing

```rust
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "gakumas-rehearsal-automation")]
#[command(about = "Gakumas rehearsal automation tool")]
struct Cli {
    /// Number of iterations to run
    #[arg(short = 'n', long, default_value = "10")]
    iterations: u32,

    /// Output directory for results
    #[arg(short, long, default_value = "./results")]
    output: PathBuf,

    /// Configuration file path
    #[arg(short, long, default_value = "./config.json")]
    config: PathBuf,

    /// Run calibration wizard
    #[arg(long)]
    calibrate: bool,

    /// Screenshot only (no automation)
    #[arg(long)]
    screenshot_only: bool,
}
```

#### 9.2.2 Interactive Calibration

```rust
fn run_calibration_wizard() -> Result<AutomationConfig> {
    println!("=== Gakumas Calibration Wizard ===");
    println!();
    println!("This wizard will help you configure button and score positions.");
    println!("Make sure the game is running and visible.");
    println!();

    // Step 1: Find game window
    let hwnd = find_gakumas_window()?;
    println!("Found game window!");

    // Step 2: Capture reference screenshot
    println!("Press Enter to capture reference screenshot...");
    wait_for_enter();
    let reference = capture_gakumas()?;
    println!("Screenshot saved: {}", reference.display());

    // Step 3: Get button positions
    println!();
    println!("Open the screenshot and note the pixel coordinates of:");
    println!("  1. Center of '開始する' (Start) button");
    println!("  2. Center of 'スキップ' (Skip) button");
    println!();

    let start_button = prompt_coordinates("Start button")?;
    let skip_button = prompt_coordinates("Skip button")?;

    // Step 4: Get score regions
    println!();
    println!("Now we need the score regions (9 total).");
    let mut score_regions = [[ScoreRegion::default(); 3]; 3];
    for stage in 0..3 {
        for char_idx in 0..3 {
            let region = prompt_region(&format!("Stage {} Character {}", stage + 1, char_idx + 1))?;
            score_regions[stage][char_idx] = region;
        }
    }

    // Step 5: Save configuration
    let config = AutomationConfig {
        start_button,
        skip_button,
        score_regions,
        ..Default::default()
    };

    Ok(config)
}
```

### 9.3 Configuration File Format

```json
{
  "start_button": {
    "x": 0.5,
    "y": 0.85
  },
  "skip_button": {
    "x": 0.9,
    "y": 0.95
  },
  "score_regions": [
    [
      {"x": 0.15, "y": 0.20, "width": 0.10, "height": 0.05, "stage": 1, "character_index": 0},
      {"x": 0.45, "y": 0.20, "width": 0.10, "height": 0.05, "stage": 1, "character_index": 1},
      {"x": 0.75, "y": 0.20, "width": 0.10, "height": 0.05, "stage": 1, "character_index": 2}
    ],
    [
      {"x": 0.15, "y": 0.45, "width": 0.10, "height": 0.05, "stage": 2, "character_index": 0},
      {"x": 0.45, "y": 0.45, "width": 0.10, "height": 0.05, "stage": 2, "character_index": 1},
      {"x": 0.75, "y": 0.45, "width": 0.10, "height": 0.05, "stage": 2, "character_index": 2}
    ],
    [
      {"x": 0.15, "y": 0.70, "width": 0.10, "height": 0.05, "stage": 3, "character_index": 0},
      {"x": 0.45, "y": 0.70, "width": 0.10, "height": 0.05, "stage": 3, "character_index": 1},
      {"x": 0.75, "y": 0.70, "width": 0.10, "height": 0.05, "stage": 3, "character_index": 2}
    ]
  ],
  "loading_timeout_ms": 30000,
  "capture_delay_ms": 500,
  "brightness_threshold": 200.0
}
```

### 9.4 Future GUI (Planned)

A future phase will add a graphical user interface using a Rust GUI framework.

#### 9.4.1 Framework Options

| Framework | Pros | Cons |
|-----------|------|------|
| `egui` | Immediate mode, simple, cross-platform | Not native look |
| `iced` | Elm-inspired, clean API | Larger binary |
| `tauri` | Web technologies, mature ecosystem | Separate frontend skills |

#### 9.4.2 GUI Features (Implemented)

- **Real-time progress visualization**: Progress bar, elapsed time, current state description
- **Iteration counter input**: User-configurable number of iterations (1-9999)
- **Start/Stop buttons**: Visual controls for automation
- **Guide images**: Embedded screenshots showing required game state
- **Mouse warning**: Orange warning "⚠ 実行中はマウスを動かさないでください" during automation
- **Completion summary**: Shows generated files (results.csv, statistics.json, charts/)
- **Output path display**: Status shows folder name (e.g., "完了 (100回) → 20260116_143022")
- **Open Folder button**: Quick access to session output folder
- **Generate Charts button**: Manual chart regeneration
- **System tray integration**: Runs in background with tray icon and menu

#### 9.4.3 GUI Features (Future)

- Visual calibration (click on game window to set coordinates)
- Chart display with character icons
- Settings editor
- History of past runs

### 9.5 Deliverables for Phase 5

| Deliverable | Status | Description |
|-------------|--------|-------------|
| egui-based GUI window | ✅ | Main application window with eframe |
| Guide images display | ✅ | Embedded step-by-step instruction images |
| Iteration input | ✅ | DragValue control for iteration count |
| Start/Stop controls | ✅ | Button controls for automation |
| Progress visualization | ✅ | Progress bar, elapsed time, state description |
| Mouse movement warning | ✅ | Orange warning text during automation |
| Completion feedback | ✅ | Shows output folder name and generated files |
| Open Folder action | ✅ | Opens session folder in Windows Explorer |
| Generate Charts action | ✅ | Manual chart generation button |
| System tray integration | ✅ | Tray icon with menu (show window, exit) |
| Global hotkeys | ✅ | Ctrl+Shift+S (screenshot), Ctrl+Shift+Q (abort) |

---

## 10. Dependencies & Environment Setup

### 10.1 Complete Cargo.toml

```toml
[package]
name = "gakumas-rehearsal-automation"
version = "0.2.0"
edition = "2024"

[dependencies]
# Windows APIs
windows = { version = "0.58", features = [
    "Graphics_Capture",
    "Graphics_DirectX",
    "Graphics_DirectX_Direct3D11",
    "Graphics_Imaging",
    "Foundation",
    "Win32_Foundation",
    "Win32_Graphics_Direct3D",
    "Win32_Graphics_Direct3D11",
    "Win32_Graphics_Dxgi",
    "Win32_Graphics_Dxgi_Common",
    "Win32_Graphics_Gdi",
    "Win32_System_LibraryLoader",
    "Win32_System_Threading",
    "Win32_System_WinRT",
    "Win32_System_WinRT_Direct3D11",
    "Win32_System_WinRT_Graphics_Capture",
    "Win32_UI_Input_KeyboardAndMouse",
    "Win32_UI_Shell",
    "Win32_UI_WindowsAndMessaging",
]}

# Image processing
image = "0.25"

# Date/time
chrono = "0.4"

# Error handling
anyhow = "1.0"

# OCR dependencies (Phase 2)
reqwest = { version = "0.12", features = ["blocking"] }  # For potential auto-download
dirs = "5.0"           # Standard directories
regex = "1.10"         # Score pattern matching
zip = "2.2"            # Archive handling
tempfile = "3.14"      # Temporary files for Tesseract

# Chart generation
plotters = "0.3"

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# CSV export
csv = "1.1"

# CLI argument parsing
clap = { version = "4.0", features = ["derive"] }

[profile.release]
lto = true
strip = true
```

### 10.2 System Requirements

| Requirement | Details |
|-------------|---------|
| Operating System | Windows 10 version 1903+ or Windows 11 |
| Rust Toolchain | 1.85+ (Edition 2024 support) |
| Tesseract OCR | 5.0+ with Japanese language pack |
| Visual Studio Build Tools | For linking Windows libraries |

### 10.3 Development Environment Setup

#### 10.3.1 Install Rust

```powershell
# Download and run rustup-init.exe from https://rustup.rs/
rustup default stable
rustup update
```

#### 10.3.2 Install Visual Studio Build Tools

Required for compiling Windows applications:
1. Download from https://visualstudio.microsoft.com/visual-cpp-build-tools/
2. Select "Desktop development with C++"

#### 10.3.3 Install Tesseract

```powershell
# Option 1: Download installer from UB-Mannheim
# https://github.com/UB-Mannheim/tesseract/wiki

# Option 2: Using Chocolatey
choco install tesseract

# Add to PATH
$env:Path += ";C:\Program Files\Tesseract-OCR"

# Verify installation
tesseract --version
tesseract --list-langs
```

#### 10.3.4 Install Japanese Language Data

If not included in installer:
```powershell
# Download jpn.traineddata from:
# https://github.com/tesseract-ocr/tessdata/blob/main/jpn.traineddata

# Place in Tesseract tessdata folder:
# C:\Program Files\Tesseract-OCR\tessdata\jpn.traineddata
```

### 10.4 Build and Run

```powershell
# Development build
cargo build

# Release build (optimized)
cargo build --release

# Run with default settings
.\target\release\gakumas-rehearsal-automation.exe

# Run with automation
.\target\release\gakumas-rehearsal-automation.exe -n 100 -o ./results

# Run calibration
.\target\release\gakumas-rehearsal-automation.exe --calibrate
```

---

## 11. File Structure

### 11.1 Current Structure

```
gakumas-rehearsal-automation/
├── src/
│   └── main.rs
├── Cargo.toml
├── Cargo.lock
├── CLAUDE.md
└── docs/
    └── ROADMAP_AUTOMATION.md
```

### 11.2 Current Structure (After Phase 3)

```
gakumas-rehearsal-automation/
├── src/
│   ├── main.rs              # Entry point, tray app, message loop, hotkeys
│   ├── capture/
│   │   ├── mod.rs           # Module exports
│   │   ├── window.rs        # Window discovery (find_gakumas_window)
│   │   ├── screenshot.rs    # WGC capture (full window, cursor excluded)
│   │   └── region.rs        # Region capture (partial window, cursor excluded)
│   ├── automation/
│   │   ├── mod.rs           # Module exports
│   │   ├── config.rs        # Configuration loading (config.json)
│   │   ├── csv_writer.rs    # CSV output for results
│   │   ├── detection.rs     # Page detection (histogram), loading detection (brightness)
│   │   ├── input.rs         # Mouse input simulation (SendInput)
│   │   ├── ocr_worker.rs    # Background OCR processing thread
│   │   ├── queue.rs         # Thread-safe job queue
│   │   ├── runner.rs        # Main automation loop
│   │   └── state.rs         # AutomationState enum
│   ├── calibration/
│   │   ├── mod.rs           # Module exports
│   │   ├── coords.rs        # Coordinate conversion utilities
│   │   ├── preview.rs       # Preview window rendering
│   │   ├── state.rs         # CalibrationState, CalibrationItems
│   │   └── wizard.rs        # 9-step calibration wizard
│   └── ocr/
│       ├── mod.rs           # ocr_screenshot() entry point
│       ├── setup.rs         # Tesseract path detection
│       ├── preprocess.rs    # Color threshold preprocessing
│       ├── engine.rs        # Tesseract CLI wrapper
│       └── extract.rs       # Pattern matching, score extraction
├── config.json              # Automation configuration (button positions, thresholds)
├── Cargo.toml
├── Cargo.lock
├── build.rs                 # Embeds Windows manifest
├── gakumas-rehearsal-automation.exe.manifest  # UAC elevation (requireAdministrator)
├── gakumas-rehearsal-automation.rc    # Resource file for manifest
├── CLAUDE.md                # Development guidance
└── docs/
    ├── PLANS.md             # ExecPlan authoring guidelines
    ├── EXECPLAN_PHASE1_REMAINING.md
    ├── EXECPLAN_PHASE2_OCR.md
    ├── EXECPLAN_PHASE3_AUTOMATION.md
    └── ROADMAP_AUTOMATION.md  # This document
```

### 11.3 Target Structure (After All Phases)

```
gakumas-rehearsal-automation/
├── src/
│   ├── ...                  # (current structure)
│   └── analysis/            # Phase 4 - PLANNED
│       ├── mod.rs
│       ├── statistics.rs    # Stats calculation
│       └── charts.rs        # Chart generation with plotters
└── ...
```

---

## Appendix

### A. Glossary

| Term | Definition |
|------|------------|
| Client Area | The drawable portion of a window, excluding title bar and borders |
| HWND | Handle to a Window - Windows' identifier for window objects |
| ROI | Region of Interest - a subset of an image for focused processing |
| WGC | Windows Graphics Capture - modern API for screen/window capture |
| D3D11 | Direct3D 11 - graphics API used for GPU-accelerated capture |
| OCR | Optical Character Recognition - converting images of text to actual text |
| Staging Texture | GPU memory buffer that can be read by CPU |

### B. Windows API Quick Reference

| API | Purpose |
|-----|---------|
| `EnumWindows` | Iterate all top-level windows |
| `GetWindowRect` | Get window position and size |
| `GetClientRect` | Get client area dimensions |
| `ClientToScreen` | Convert client coordinates to screen coordinates |
| `SendInput` | Simulate mouse/keyboard input (hardware-level) |
| `PostMessageW` | Post message to window queue (may be ignored by games) |
| `SetForegroundWindow` | Bring window to foreground (required before SendInput) |
| `GetSystemMetrics` | Get screen dimensions (SM_CXSCREEN, SM_CYSCREEN) |
| `RegisterHotKey` | Register global keyboard shortcut |
| `Shell_NotifyIconW` | Create/manage system tray icon |

### C. Tesseract Page Segmentation Modes

| Mode | Description |
|------|-------------|
| 0 | Orientation and script detection only |
| 1 | Automatic page segmentation with OSD |
| 3 | Fully automatic page segmentation (default) |
| 6 | Assume single uniform block of text |
| 7 | Treat image as single text line |
| 8 | Treat image as single word |
| 10 | Treat image as single character |

### D. Useful Resources

- [windows-rs crate documentation](https://docs.rs/windows/latest/windows/)
- [Tesseract documentation](https://tesseract-ocr.github.io/)
- [Plotters crate documentation](https://docs.rs/plotters/latest/plotters/)
- [Rust async book](https://rust-lang.github.io/async-book/)

### E. Known Issues and Workarounds

| Issue | Workaround |
|-------|------------|
| EnumWindows returns FALSE on early stop | Don't use `?` operator; FALSE is expected when stopping enumeration |
| Tesseract slow on first call | Pre-initialize Tesseract at startup |
| SendInput blocked by UAC | Application now requires admin by default via embedded manifest |
| Japanese characters in paths | Use wide string APIs (W suffix) consistently |
| PostMessage returns "Access is denied" | UIPI blocking - run at same privilege level as target |
| Game ignores PostMessage clicks | Game requires foreground focus; use SendInput with SetForegroundWindow instead |
| Process name substring match | Use exact match (`== "gakumas.exe"`) to avoid matching `gakumas-rehearsal-automation.exe` |
| Config not loading | Config is loaded from exe directory, not CWD; copy config.json next to the exe |
| Brightness unreliable for State 1→2 | Use histogram comparison against reference images instead of brightness for page detection |
| Cursor appears in screenshots | Fixed: Use `SetIsCursorCaptureEnabled(false)` on capture session |

---

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-01-12 | Initial roadmap document |
| 1.1 | 2026-01-13 | Added experimental findings for mouse input methods |
| 1.2 | 2026-01-13 | Refactored into modules; added admin manifest for UAC elevation |
| 1.3 | 2026-01-13 | Phase 1 complete: config system, relative coords, region capture, brightness detection |
| 1.4 | 2026-01-14 | Phase 2 complete: OCR module using full-image pattern matching with Tesseract CLI |
| 1.5 | 2026-01-15 | Phase 3 complete: automation loop with state machine, histogram-based page detection, 9-step calibration, cursor exclusion, background OCR, CSV output |
| 1.6 | 2026-01-15 | Phase 4 complete: statistics calculation, per-character charts (box plot + histogram), statistics table, JSON export |
| 1.7 | 2026-01-15 | Phase 4 enhancements: combined box plot, orange styling, configurable chart_config.json, output folder |
| 1.8 | 2026-01-16 | Phase 5 GUI improvements: completion summary with generated files, output path in status, mouse movement warning |
