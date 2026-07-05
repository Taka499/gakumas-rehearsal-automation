pub mod setup;
pub mod preprocess;
pub mod engine;
pub mod extract;
pub mod reconcile;

pub use setup::ensure_tesseract;
pub use preprocess::threshold_bright_pixels;
pub use engine::{recognize_image, OcrLine, OcrWord};
pub use extract::extract_scores;
pub use reconcile::Recovery;

use anyhow::Result;
use image::{ImageBuffer, Rgba};

use crate::automation::config::OcrRegions;
use preprocess::{blue_mask, crop_region};
use engine::{recognize_image_line, recognize_single_number};
use extract::extract_single_stage;
use reconcile::{reconcile_stage, reconstruct_from_digits};

/// Per-stage OCR readout: the nine per-character scores plus the isolated
/// stage total and bonus badge that drive checksum reconstruction.
///
/// `scores` holds the (post-reconciliation, once M4 wires it) per-character
/// values. `totals`/`bonuses` are `None` when that isolated number failed to
/// OCR or looked like over-detected garbage. `flags` records each stage's
/// reconstruction confidence (default `Recovery::Ok` until M3/M4 fill it).
#[derive(Clone, Copy, Debug)]
pub struct StageReadout {
    pub scores: [[u32; 3]; 3],
    pub totals: [Option<u32>; 3],
    pub bonuses: [Option<u32>; 3],
    pub flags: [Recovery; 3],
}

/// Alternate luminance thresholds for the multi-threshold total retry, tried in
/// order when the primary `total_threshold` read can't be reconciled. Spread
/// around the 210 default: lower cutoffs sharpen a "3" that 210 reads as "5";
/// higher ones drop a faint comma/Pt pixel that 210 reads as an extra digit.
const TOTAL_ALT_THRESHOLDS: &[u8] = &[180, 220, 190, 200, 230, 170, 240];

/// Alternate luminance thresholds for the score-row retry, tried in order when
/// the primary `ocr_threshold` read can't be reconciled. Ordered by distance
/// from the 190 default. Field evidence (session 20260704_052007): at exactly
/// 190 Tesseract misreads a trailing "1"/"0" as "2" at the 721x1281 window
/// size; every neighbouring cutoff reads it correctly.
const SCORE_ALT_THRESHOLDS: &[u8] = &[200, 180, 210, 170, 220];

/// High-level function: screenshot → per-stage readout using per-stage cropping.
///
/// For each of the 3 stages, crops and OCRs the score row, the isolated stage
/// total (white text, luminance threshold), and the bonus badge (light-blue
/// text, blue-selective mask). The preprocessing thresholds are read from the
/// global config (`ocr_threshold`, `total_threshold`, `bonus_blue_min`,
/// `bonus_br_margin`). The total/bonus feed the checksum reconstruction (M3/M4);
/// a failed total/bonus reads as `None` and simply disables the checksum tier.
pub fn ocr_screenshot(
    img: &ImageBuffer<Rgba<u8>, Vec<u8>>,
    regions: &OcrRegions,
) -> Result<StageReadout> {
    let config = crate::automation::config::get_config();
    let threshold = config.ocr_threshold;
    let total_threshold = config.total_threshold;
    let bonus_blue_min = config.bonus_blue_min;
    let bonus_br_margin = config.bonus_br_margin;

    let mut readout = StageReadout {
        scores: [[0u32; 3]; 3],
        totals: [None; 3],
        bonuses: [None; 3],
        flags: [Recovery::Ok; 3],
    };

    for stage_idx in 0..3 {
        // Score row.
        let score_crop = crop_region(img, &regions.score[stage_idx]);
        let score_bin = threshold_bright_pixels(&score_crop, threshold);
        let lines = recognize_image_line(&score_bin)?;
        readout.scores[stage_idx] = extract_single_stage(&lines)?;

        // Stage total: white text, same luminance threshold style as score rows.
        let total_crop = crop_region(img, &regions.total[stage_idx]);
        let total_bin = threshold_bright_pixels(&total_crop, total_threshold);
        readout.totals[stage_idx] = recognize_single_number(&total_bin, "0123456789,", false)?;

        // Bonus badge: light-blue text, blue-selective mask, "+"-anchored parse.
        let bonus_crop = crop_region(img, &regions.bonus[stage_idx]);
        let bonus_bin = blue_mask(&bonus_crop, bonus_blue_min, bonus_br_margin);
        readout.bonuses[stage_idx] = recognize_single_number(&bonus_bin, "0123456789+", true)?;

        // Reconstruct overlapping-million corruption via the total/bonus checksum.
        let raw = readout.scores[stage_idx];
        let bonus = readout.bonuses[stage_idx];
        let (mut reconciled, mut flag) = reconcile_stage(raw, readout.totals[stage_idx], bonus);

        // Score-row multi-threshold retry. The score-row binarization is
        // threshold-sensitive the same way the total's is: at one knife-edge
        // cutoff a trailing "1"/"0" reads as "2" (session 20260704_052007).
        // Re-OCR the score crop at alternate thresholds and adopt the first
        // re-read that reconciles without a flag against the already-read
        // total/bonus (still no guessing — only an exact checksum is
        // accepted). Runs only on flagged stages, stops at first success.
        if flag == Recovery::Flagged {
            for &alt in SCORE_ALT_THRESHOLDS {
                if alt == threshold {
                    continue;
                }
                let alt_bin = threshold_bright_pixels(&score_crop, alt);
                let alt_lines = recognize_image_line(&alt_bin)?;
                let alt_raw = match extract_single_stage(&alt_lines) {
                    Ok(s) => s,
                    Err(_) => continue, // unparseable at this cutoff — try next
                };
                if alt_raw == raw {
                    continue; // same read would reconcile (and flag) identically
                }
                let (r2, f2) = reconcile_stage(alt_raw, readout.totals[stage_idx], bonus);
                if f2 != Recovery::Flagged {
                    crate::log(&format!(
                        "OCR stage {}: score retry t{} {:?} -> {:?}",
                        stage_idx + 1, alt, raw, r2
                    ));
                    reconciled = r2;
                    flag = f2;
                    break;
                }
            }
        }

        // Multi-threshold total retry. The isolated total OCR is threshold-
        // sensitive in opposite directions: at one cutoff a "3" reads as "5", at
        // another a thousands comma leaks an extra digit — so no single
        // `total_threshold` reads every total correctly. When the primary read
        // can't be reconciled, re-OCR the total crop at alternate thresholds and
        // adopt the first reading that yields a non-flagged *exact-checksum*
        // recovery (still no guessing — only an exact total is accepted). Cheap:
        // it runs only on the ~7% of stages the primary read flags, and stops at
        // the first success.
        if flag == Recovery::Flagged {
            for &alt in TOTAL_ALT_THRESHOLDS {
                if alt == total_threshold {
                    continue;
                }
                let alt_bin = threshold_bright_pixels(&total_crop, alt);
                let alt_total = recognize_single_number(&alt_bin, "0123456789,", false)?;
                if alt_total.is_none() || alt_total == readout.totals[stage_idx] {
                    continue;
                }
                let (r2, f2) = reconcile_stage(raw, alt_total, bonus);
                if f2 != Recovery::Flagged {
                    crate::log(&format!(
                        "OCR stage {}: total retry t{} {:?} -> {:?} (total {:?}->{:?})",
                        stage_idx + 1, alt, raw, r2, readout.totals[stage_idx], alt_total
                    ));
                    reconciled = r2;
                    flag = f2;
                    readout.totals[stage_idx] = alt_total;
                    break;
                }
            }
        }

        // Fallback for cases the comma-based tokenizer can't recover (chiefly two
        // adjacent collisions, which scramble the comma grouping): reconstruct
        // straight from the score row's raw digit stream, guided by the checksum.
        if flag == Recovery::Flagged {
            let digit_stream: String = lines
                .iter()
                .flat_map(|l| l.text.chars())
                .filter(|c| c.is_ascii_digit())
                .collect();
            if let Some((rebuilt, rebuilt_flag)) = reconstruct_from_digits(
                &digit_stream,
                readout.totals[stage_idx],
                readout.bonuses[stage_idx],
            ) {
                if rebuilt_flag != Recovery::Flagged {
                    crate::log(&format!(
                        "OCR stage {}: digit-stream reconstruction {:?} -> {:?}",
                        stage_idx + 1, raw, rebuilt
                    ));
                    reconciled = rebuilt;
                    flag = rebuilt_flag;
                }
            }
        }

        readout.scores[stage_idx] = reconciled;
        readout.flags[stage_idx] = flag;

        let total = readout.totals[stage_idx];
        let bonus = readout.bonuses[stage_idx];
        match flag {
            Recovery::Ok => crate::log(&format!(
                "OCR stage {}: scores={:?} total={:?} bonus={:?} (ok)",
                stage_idx + 1, reconciled, total, bonus
            )),
            Recovery::Repaired => crate::log(&format!(
                "OCR stage {}: REPAIRED {:?} -> {:?} (total={:?} bonus={:?})",
                stage_idx + 1, raw, reconciled, total, bonus
            )),
            Recovery::Flagged => crate::log(&format!(
                "OCR stage {}: FLAGGED for review, raw={:?} kept={:?} (total={:?} bonus={:?})",
                stage_idx + 1, raw, reconciled, total, bonus
            )),
        }
    }

    Ok(readout)
}

#[cfg(test)]
mod e2e_tests {
    //! End-to-end acceptance for the overlap-score recovery (M2/M4).
    //!
    //! These run the real Tesseract pipeline on the checked-in sample PNGs
    //! (stored via Git LFS under `tests/fixtures/overlap_samples/`), so they are
    //! `#[ignore]`d (Tesseract isn't present in a bare unit-test run, the admin
    //! manifest blocks `cargo test` unless built with `GAKUMAS_NO_MANIFEST=1`,
    //! and the fixtures require `git lfs pull`). Run explicitly with:
    //!
    //!     GAKUMAS_NO_MANIFEST=1 cargo test ocr_overlap_recovery_e2e -- --ignored --nocapture
    use super::*;
    use crate::ocr::reconcile::Recovery;

    #[test]
    #[ignore = "requires embedded Tesseract + LFS fixtures; run with --ignored"]
    fn ocr_overlap_recovery_e2e() {
        crate::automation::config::init_config();
        crate::ocr::ensure_tesseract().expect("extract embedded tesseract");
        let config = crate::automation::config::get_config();

        // (path, expected stage-2 scores, expected stage-2 recovery)
        let cases: [(&str, [u32; 3], Recovery); 6] = [
            ("tests/fixtures/overlap_samples/003_20260618_101738.png", [1327533, 1151661, 0], Recovery::Repaired),
            ("tests/fixtures/overlap_samples/005_20260618_101804.png", [1083349, 1062741, 0], Recovery::Repaired),
            ("tests/fixtures/overlap_samples/gakumas_20260618_102842.png", [1172665, 1161196, 1093518], Recovery::Repaired),
            ("tests/fixtures/overlap_samples/gakumas_20260618_102623.png", [912127, 1171024, 1004816], Recovery::Ok),
            // Two adjacent collisions (three >= 1M scores): 1,314,245 / 1,206,537 / 1,103,897.
            ("tests/fixtures/overlap_samples/iter009_two_collisions.png", [1314245, 1206537, 1103897], Recovery::Repaired),
            // Single collision; 8-digit total (Pt leak) must recover: 1,240,513 / 1,178,565 / 455,013.
            ("tests/fixtures/overlap_samples/iter018_single_collision.png", [1240513, 1178565, 455013], Recovery::Repaired),
        ];

        let mut failures = Vec::new();
        for (path, want_scores, want_rec) in cases {
            let img = image::open(path)
                .unwrap_or_else(|e| panic!("open {path}: {e}"))
                .to_rgba8();
            let r = ocr_screenshot(&img, &config.ocr_regions())
                .unwrap_or_else(|e| panic!("ocr {path}: {e}"));
            println!(
                "{path}\n  stage2 scores={:?} total={:?} bonus={:?} flag={:?}",
                r.scores[1], r.totals[1], r.bonuses[1], r.flags[1]
            );
            if r.scores[1] != want_scores || r.flags[1] != want_rec {
                failures.push(format!(
                    "{path}: got scores={:?} flag={:?}, want scores={:?} flag={:?}",
                    r.scores[1], r.flags[1], want_scores, want_rec
                ));
            }
        }
        assert!(failures.is_empty(), "stage-2 mismatches:\n{}", failures.join("\n"));
    }

    /// Acceptance for the score-row multi-threshold retry (see
    /// docs/EXECPLAN_SCORE_ROW_THRESHOLD_RETRY.md). Field screenshots from
    /// session 20260704_052007: at ocr_threshold 190 Tesseract misreads the
    /// second score's trailing digit ("276,981" -> "276,982"); the retry
    /// re-reads at alternate thresholds and adopts the checksum-exact read.
    /// Both cases must come back Ok (zero-edit reconciliation of the re-read),
    /// not Repaired/Flagged.
    #[test]
    #[ignore = "requires embedded Tesseract + LFS fixtures; run with --ignored"]
    fn ocr_score_row_retry_e2e() {
        crate::automation::config::init_config();
        crate::ocr::ensure_tesseract().expect("extract embedded tesseract");
        let config = crate::automation::config::get_config();

        // (path, expected stage-1 scores, expected stage-1 recovery)
        let cases: [(&str, [u32; 3], Recovery); 2] = [
            // Was FLAGGED (units-edit tie kept the wrong value 568600/276982).
            ("tests/fixtures/score_retry_samples/107_20260704_053916.png", [568601, 276981, 0], Recovery::Ok),
            // Field run silently mis-repaired this one to 601170: its total OCR'd
            // as 1,231,935 that night and the solver bent the score to match.
            // The pixels read 510,531 / 601,171 with total 1,231,936 (verified by
            // eye and by re-OCR of the saved PNG); the retry now reads it clean.
            ("tests/fixtures/score_retry_samples/045_20260704_052733.png", [510531, 601171, 0], Recovery::Ok),
        ];

        let mut failures = Vec::new();
        for (path, want_scores, want_rec) in cases {
            let img = image::open(path)
                .unwrap_or_else(|e| panic!("open {path}: {e}"))
                .to_rgba8();
            let r = ocr_screenshot(&img, &config.ocr_regions())
                .unwrap_or_else(|e| panic!("ocr {path}: {e}"));
            println!(
                "{path}\n  stage1 scores={:?} total={:?} bonus={:?} flag={:?}",
                r.scores[0], r.totals[0], r.bonuses[0], r.flags[0]
            );
            if r.scores[0] != want_scores || r.flags[0] != want_rec {
                failures.push(format!(
                    "{path}: got scores={:?} flag={:?}, want scores={:?} flag={:?}",
                    r.scores[0], r.flags[0], want_scores, want_rec
                ));
            }
        }
        assert!(failures.is_empty(), "stage-1 mismatches:\n{}", failures.join("\n"));
    }

    /// Full-session field replay, gated on GAKUMAS_REPLAY_SESSION pointing at a
    /// session folder (results.csv + screenshots/). Ground truth is the CSV,
    /// whose flagged rows were hand-corrected in review. Every row whose re-run
    /// flag is NOT Flagged must match the CSV exactly; rows the pipeline still
    /// flags are allowed to differ (flagged means "a human must look", and the
    /// CSV value came from that human). Tesseract-bound: ~10-15 min for 500 rows.
    #[test]
    #[ignore = "field replay; set GAKUMAS_REPLAY_SESSION and run with --ignored --nocapture"]
    fn ocr_session_replay() {
        let Ok(session) = std::env::var("GAKUMAS_REPLAY_SESSION") else {
            println!("GAKUMAS_REPLAY_SESSION not set; skipping");
            return;
        };
        crate::automation::config::init_config();
        crate::ocr::ensure_tesseract().expect("extract embedded tesseract");
        let config = crate::automation::config::get_config();
        let regions = config.ocr_regions();

        let csv = std::fs::read_to_string(format!("{session}/results.csv"))
            .expect("read results.csv");
        let mut mismatches: Vec<String> = Vec::new();
        let mut rows = 0u32;
        let mut still_flagged = 0u32;
        for line in csv.lines().skip(1) {
            let f: Vec<&str> = line.split(',').collect();
            if f.len() < 12 {
                continue;
            }
            let iteration = f[0];
            let screenshot = f[2];
            let want: Vec<u32> = f[3..12].iter().map(|s| s.parse().unwrap_or(0)).collect();
            let img = match image::open(screenshot) {
                Ok(i) => i.to_rgba8(),
                Err(e) => {
                    println!("iter {iteration}: cannot open {screenshot}: {e}");
                    continue;
                }
            };
            let r = ocr_screenshot(&img, &regions)
                .unwrap_or_else(|e| panic!("ocr iter {iteration}: {e}"));
            rows += 1;
            let got: Vec<u32> = r.scores.iter().flatten().copied().collect();
            let flagged = r.flags.contains(&Recovery::Flagged);
            if flagged {
                still_flagged += 1;
                println!("iter {iteration}: still flagged (got {:?}, csv {:?})", got, want);
            } else if got != want {
                mismatches.push(format!(
                    "iter {iteration}: got {:?} (flags {:?}), csv {:?}",
                    got, r.flags, want
                ));
            }
        }
        println!(
            "replay: {rows} rows, {} non-flagged mismatches, {still_flagged} still flagged",
            mismatches.len()
        );
        assert!(
            mismatches.is_empty(),
            "non-flagged mismatches:\n{}",
            mismatches.join("\n")
        );
    }
}
