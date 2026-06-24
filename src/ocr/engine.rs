use anyhow::{anyhow, Result};
use image::{ImageBuffer, Luma};
use std::process::Command;
use tempfile::NamedTempFile;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use super::setup::{find_tesseract_executable, find_tessdata_dir};

/// Windows flag to prevent console window from appearing
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

/// Represents a line of OCR text with confidence score
#[derive(Debug, Clone)]
pub struct OcrLine {
    pub text: String,
    pub words: Vec<OcrWord>,
    pub confidence: f32,
}

/// Represents a single word from OCR with confidence score
#[derive(Debug, Clone)]
pub struct OcrWord {
    pub text: String,
    pub confidence: f32,
}

/// Runs Tesseract on a preprocessed grayscale image.
/// Returns structured output with lines and confidence scores.
pub fn recognize_image(img: &ImageBuffer<Luma<u8>, Vec<u8>>) -> Result<Vec<OcrLine>> {
    let tesseract_exe = find_tesseract_executable()?;
    let tessdata_dir = find_tessdata_dir()?;

    // Save image to temporary file
    let temp_input = NamedTempFile::with_suffix(".png")?;
    img.save(temp_input.path())?;

    // Create a unique output base path for Tesseract
    // Tesseract will append .tsv to this path
    let temp_dir = std::env::temp_dir();
    let output_base = temp_dir
        .join(format!("tesseract_out_{}", std::process::id()))
        .to_string_lossy()
        .to_string();

    // Run Tesseract with TSV output for structured data
    // Use -c tessedit_create_tsv=1 instead of "tsv" config file
    let mut cmd = Command::new(&tesseract_exe);
    cmd.arg(temp_input.path())
        .arg(&output_base)
        .arg("--tessdata-dir")
        .arg(&tessdata_dir)
        .arg("-l")
        .arg("eng")
        .arg("--psm")
        .arg("6") // Assume single uniform block of text
        .arg("-c")
        .arg("tessedit_create_tsv=1");

    // Prevent console window from appearing on Windows
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let output = cmd.output()?;

    // Check for errors (log stderr even on success for debugging)
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stderr.is_empty() {
        crate::log(&format!("Tesseract stderr: {}", stderr.trim()));
    }

    if !output.status.success() {
        return Err(anyhow!("Tesseract failed with exit code {:?}: {}", output.status.code(), stderr));
    }

    // Read TSV output
    let tsv_path = format!("{}.tsv", output_base);
    let tsv_content = match std::fs::read_to_string(&tsv_path) {
        Ok(content) => content,
        Err(e) => {
            // Log debugging info
            crate::log(&format!("Tesseract output base: {}", output_base));
            crate::log(&format!("Expected TSV path: {}", tsv_path));
            crate::log(&format!("Tesseract stdout: {}", String::from_utf8_lossy(&output.stdout).trim()));
            return Err(anyhow!("Failed to read Tesseract output at {}: {}", tsv_path, e));
        }
    };

    // Clean up output file
    let _ = std::fs::remove_file(&tsv_path);

    // Parse TSV output
    parse_tsv_output(&tsv_content)
}

/// Parses Tesseract TSV output into structured OcrLine data
fn parse_tsv_output(tsv: &str) -> Result<Vec<OcrLine>> {
    let mut lines: Vec<OcrLine> = Vec::new();
    let mut current_line_num: i32 = -1;
    let mut current_words: Vec<OcrWord> = Vec::new();
    let mut current_conf_sum: f32 = 0.0;
    let mut current_word_count: usize = 0;

    for line in tsv.lines().skip(1) {
        // Skip header
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() < 12 {
            continue;
        }

        // TSV fields: level, page_num, block_num, par_num, line_num, word_num,
        //             left, top, width, height, conf, text
        let level: i32 = fields[0].parse().unwrap_or(-1);
        let line_num: i32 = fields[4].parse().unwrap_or(-1);
        let conf: f32 = fields[10].parse().unwrap_or(-1.0);
        let text = fields[11].trim();

        // Level 5 = word
        if level != 5 {
            continue;
        }

        // Skip empty text
        if text.is_empty() {
            continue;
        }

        // Check if we've moved to a new line
        if line_num != current_line_num && current_line_num >= 0 {
            // Save previous line
            if !current_words.is_empty() {
                let avg_conf = if current_word_count > 0 {
                    current_conf_sum / current_word_count as f32
                } else {
                    0.0
                };
                let line_text = current_words
                    .iter()
                    .map(|w| w.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");
                lines.push(OcrLine {
                    text: line_text,
                    words: current_words,
                    confidence: avg_conf,
                });
            }
            current_words = Vec::new();
            current_conf_sum = 0.0;
            current_word_count = 0;
        }

        current_line_num = line_num;

        // Add word
        if conf >= 0.0 {
            current_words.push(OcrWord {
                text: text.to_string(),
                confidence: conf,
            });
            current_conf_sum += conf;
            current_word_count += 1;
        }
    }

    // Don't forget the last line
    if !current_words.is_empty() {
        let avg_conf = if current_word_count > 0 {
            current_conf_sum / current_word_count as f32
        } else {
            0.0
        };
        let line_text = current_words
            .iter()
            .map(|w| w.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        lines.push(OcrLine {
            text: line_text,
            words: current_words,
            confidence: avg_conf,
        });
    }

    Ok(lines)
}

/// Runs Tesseract on a cropped score-row image.
///
/// Uses --psm 6 (block of text) for proper word segmentation when multiple
/// numbers are present in the cropped region. No character whitelist is used;
/// the crop itself limits noise, and downstream regex filtering handles the rest.
pub fn recognize_image_line(img: &ImageBuffer<Luma<u8>, Vec<u8>>) -> Result<Vec<OcrLine>> {
    let tesseract_exe = find_tesseract_executable()?;
    let tessdata_dir = find_tessdata_dir()?;

    // Save image to temporary file
    let temp_input = NamedTempFile::with_suffix(".png")?;
    img.save(temp_input.path())?;

    let temp_dir = std::env::temp_dir();
    let output_base = temp_dir
        .join(format!("tesseract_line_{}", std::process::id()))
        .to_string_lossy()
        .to_string();

    let mut cmd = Command::new(&tesseract_exe);
    cmd.arg(temp_input.path())
        .arg(&output_base)
        .arg("--tessdata-dir")
        .arg(&tessdata_dir)
        .arg("-l")
        .arg("eng")
        .arg("--psm")
        .arg("6") // Block of text — better word segmentation for multiple numbers
        .arg("-c")
        .arg("tessedit_create_tsv=1");

    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let output = cmd.output()?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stderr.is_empty() {
        crate::log(&format!("Tesseract stderr: {}", stderr.trim()));
    }

    if !output.status.success() {
        return Err(anyhow!("Tesseract failed with exit code {:?}: {}", output.status.code(), stderr));
    }

    let tsv_path = format!("{}.tsv", output_base);
    let tsv_content = match std::fs::read_to_string(&tsv_path) {
        Ok(content) => content,
        Err(e) => {
            crate::log(&format!("Expected TSV path: {}", tsv_path));
            return Err(anyhow!("Failed to read Tesseract output at {}: {}", tsv_path, e));
        }
    };

    let _ = std::fs::remove_file(&tsv_path);

    parse_tsv_output(&tsv_content)
}

/// OCRs a pre-binarized crop as a single isolated integer.
///
/// Distinct from `recognize_image_line`: uses page-segmentation mode 7 ("a
/// single text line") and a character whitelist, suited to the isolated stage
/// total and bonus badge (which never overlap anything).
///
/// `whitelist` is passed to Tesseract's `tessedit_char_whitelist` ("0123456789,"
/// for the total, "0123456789+" for the bonus). When `anchor_plus` is true, only
/// the digits after the **last** "+" are taken — the bonus badge always renders
/// a "+" immediately before its number, so whatever the crown icon reads as
/// lands before that "+". If no "+" survived (the crown/"+" was misread as a
/// digit), the bonus is rejected to `None` rather than trusted. Otherwise the
/// longest run of digits/commas is used.
///
/// Returns `Ok(None)` when nothing parseable is found, or when the value has an
/// obviously-wrong digit count for its kind (> 7 digits for a total, > 6 for a
/// bonus) — a cheap over-detection guard so the worst garbage never reaches the
/// checksum. A failed total/bonus simply disables the checksum tier downstream
/// rather than crashing.
pub fn recognize_single_number(
    img: &ImageBuffer<Luma<u8>, Vec<u8>>,
    whitelist: &str,
    anchor_plus: bool,
) -> Result<Option<u32>> {
    let tesseract_exe = find_tesseract_executable()?;
    let tessdata_dir = find_tessdata_dir()?;

    let temp_input = NamedTempFile::with_suffix(".png")?;
    img.save(temp_input.path())?;

    let temp_dir = std::env::temp_dir();
    let output_base = temp_dir
        .join(format!("tesseract_num_{}", std::process::id()))
        .to_string_lossy()
        .to_string();

    let mut cmd = Command::new(&tesseract_exe);
    cmd.arg(temp_input.path())
        .arg(&output_base)
        .arg("--tessdata-dir")
        .arg(&tessdata_dir)
        .arg("-l")
        .arg("eng")
        .arg("--psm")
        .arg("7") // Single text line
        .arg("-c")
        .arg(format!("tessedit_char_whitelist={}", whitelist))
        .arg("-c")
        .arg("tessedit_create_tsv=1");

    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let output = cmd.output()?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stderr.is_empty() {
        crate::log(&format!("Tesseract stderr: {}", stderr.trim()));
    }

    if !output.status.success() {
        return Err(anyhow!("Tesseract failed with exit code {:?}: {}", output.status.code(), stderr));
    }

    let tsv_path = format!("{}.tsv", output_base);
    let tsv_content = match std::fs::read_to_string(&tsv_path) {
        Ok(content) => content,
        Err(e) => {
            crate::log(&format!("Expected TSV path: {}", tsv_path));
            return Err(anyhow!("Failed to read Tesseract output at {}: {}", tsv_path, e));
        }
    };

    let _ = std::fs::remove_file(&tsv_path);

    let lines = parse_tsv_output(&tsv_content)?;
    let raw: String = lines
        .iter()
        .map(|l| l.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");

    Ok(parse_single_number(&raw, anchor_plus))
}

/// Extracts a single integer from raw OCR text for `recognize_single_number`.
///
/// Pure helper (unit-testable without Tesseract). See `recognize_single_number`
/// for the `anchor_plus` semantics and the digit-count over-detection guard.
fn parse_single_number(raw: &str, anchor_plus: bool) -> Option<u32> {
    // For the bonus, keep only what follows the last "+" (crown noise lands
    // before it). The badge ALWAYS renders a literal "+", so if none survived,
    // the crown/"+" glyph was misread as a digit (e.g. "+65,575" -> "465575");
    // the read cannot be trusted, so return None rather than keep the bogus
    // prepended digit. The bonus is only a checksum cross-check, so discarding
    // it is safe — it never corrupts a stored score, only disables corroboration.
    let segment: &str = if anchor_plus {
        match raw.rfind('+') {
            Some(idx) => &raw[idx + 1..],
            None => return None,
        }
    } else {
        raw
    };

    // Take the longest contiguous run of digits (commas/periods inside a number
    // do not break the run but are not kept), so a stray noise group can't be
    // glued onto the real number.
    let mut digits = longest_digit_run(segment);
    if digits.is_empty() {
        return None;
    }

    // The stage total renders "X,XXX,XXXPt"; the faint "Pt" suffix can OCR as a
    // spurious trailing digit (e.g. "3,122,1936" for 3,122,193). At the tuned
    // total_threshold the real digits are correct and the leak is always at the
    // END, so an 8-digit total is recovered as its first 7 digits rather than
    // discarded. The exact-checksum step in reconcile_stage is the backstop if
    // this guess is ever wrong. The bonus (anchor_plus) has no such suffix.
    let max_digits = if anchor_plus { 6 } else { 7 };
    if !anchor_plus && digits.len() == max_digits + 1 {
        digits.truncate(max_digits);
    }
    if digits.len() > max_digits {
        return None;
    }

    digits.parse::<u32>().ok()
}

/// Returns the longest run of digits in `s`, treating `,`/`.` as in-number
/// separators that neither extend nor break a run (they are simply skipped).
fn longest_digit_run(s: &str) -> String {
    let mut best = String::new();
    let mut cur = String::new();
    for c in s.chars() {
        if c.is_ascii_digit() {
            cur.push(c);
        } else if c == ',' || c == '.' {
            // Thousands separator inside a number: don't break the run.
        } else {
            if cur.len() > best.len() {
                best = std::mem::take(&mut cur);
            } else {
                cur.clear();
            }
        }
    }
    if cur.len() > best.len() {
        best = cur;
    }
    best
}

/// Simple OCR that just returns raw text (for debugging)
pub fn recognize_image_simple(img: &ImageBuffer<Luma<u8>, Vec<u8>>) -> Result<String> {
    let tesseract_exe = find_tesseract_executable()?;
    let tessdata_dir = find_tessdata_dir()?;

    // Save image to temporary file
    let temp_input = NamedTempFile::with_suffix(".png")?;
    img.save(temp_input.path())?;

    // Run Tesseract to stdout
    let mut cmd = Command::new(&tesseract_exe);
    cmd.arg(temp_input.path())
        .arg("stdout")
        .arg("--tessdata-dir")
        .arg(&tessdata_dir)
        .arg("-l")
        .arg("eng")
        .arg("--psm")
        .arg("6");

    // Prevent console window from appearing on Windows
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let output = cmd.output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Tesseract failed: {}", stderr));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[cfg(test)]
mod tests {
    use super::{longest_digit_run, parse_single_number};

    #[test]
    fn test_total_parsing() {
        // Clean total with commas.
        assert_eq!(parse_single_number("2,744,700", false), Some(2744700));
        // Trailing unit text stripped (whitelist would normally remove it, but
        // be robust anyway).
        assert_eq!(parse_single_number("3,322,171 Pt", false), Some(3322171));
        // 8-digit total = correct 7 digits + a trailing "Pt"-leak digit; recover
        // the first 7 (real cases: "3,122,1936" -> 3122193, "3,322,1716").
        assert_eq!(parse_single_number("31221936", false), Some(3122193));
        assert_eq!(parse_single_number("3,322,1716", false), Some(3322171));
        // 9+ digits is genuine garbage → rejected.
        assert_eq!(parse_single_number("332217106", false), None);
    }

    #[test]
    fn test_bonus_parsing_anchor_plus() {
        // Crown noise lands before the "+"; take the digits after it.
        assert_eq!(parse_single_number("5+81571", true), Some(81571));
        assert_eq!(parse_single_number("+265506", true), Some(265506));
        // Multiple "+" → digits after the LAST one.
        assert_eq!(parse_single_number("4+2+265506", true), Some(265506));
        // Over-detected 8-digit bonus (the 102842 blue-min-150 failure) → rejected.
        assert_eq!(parse_single_number("+23545335", true), None);
        // No "+" anchor: the badge always renders a "+", so its absence means the
        // crown/"+" was misread as a digit (e.g. "+65,575" -> "465575"). Reject
        // rather than keep the bogus prepended digit.
        assert_eq!(parse_single_number("465575", true), None);
        assert_eq!(parse_single_number("234533", true), None);
        // A correctly-anchored bonus is unchanged.
        assert_eq!(parse_single_number("+65,575", true), Some(65575));
        // The change is anchor-only: a total (anchor_plus = false) keeps its value.
        assert_eq!(parse_single_number("465575", false), Some(465575));
    }

    #[test]
    fn test_parse_single_number_empty() {
        assert_eq!(parse_single_number("", false), None);
        assert_eq!(parse_single_number("Pt", false), None);
        assert_eq!(parse_single_number("+", true), None);
    }

    #[test]
    fn test_longest_digit_run() {
        assert_eq!(longest_digit_run("2,744,700"), "2744700");
        assert_eq!(longest_digit_run("12 345678"), "345678");
        assert_eq!(longest_digit_run("abc"), "");
    }
}
