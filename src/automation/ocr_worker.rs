//! OCR worker thread for processing screenshots.
//!
//! Runs in a separate thread, receiving screenshot paths from the work queue
//! and processing them with OCR. Results are written to a CSV file.

use std::path::PathBuf;
use std::sync::mpsc::Receiver;

use crate::automation::config::OcrRegions;
use crate::automation::csv_writer::{append_to_csv, append_to_raw_csv};
use crate::automation::queue::OcrWorkItem;
use crate::ocr::{ocr_screenshot, Recovery};

/// Worst recovery outcome across the three stages (Flagged > Repaired > Ok).
fn worst_recovery(flags: &[Recovery; 3]) -> Recovery {
    if flags.contains(&Recovery::Flagged) {
        Recovery::Flagged
    } else if flags.contains(&Recovery::Repaired) {
        Recovery::Repaired
    } else {
        Recovery::Ok
    }
}

/// Runs the OCR worker loop.
///
/// Processes items from the queue until the channel is closed (sender dropped).
/// Each screenshot is loaded, processed with OCR, and results appended to CSV.
///
/// This function blocks until the channel closes, so it should be run in a
/// dedicated thread.
pub fn run_ocr_worker(
    receiver: Receiver<OcrWorkItem>,
    csv_path: PathBuf,
    regions: OcrRegions,
) {
    crate::log("OCR worker started");

    loop {
        match receiver.recv() {
            Ok(work_item) => {
                crate::log(&format!(
                    "OCR worker: processing iteration {} ({})",
                    work_item.iteration,
                    crate::paths::relative_display(&work_item.screenshot_path)
                ));

                // Load screenshot from disk
                let img = match image::open(&work_item.screenshot_path) {
                    Ok(img) => img.to_rgba8(),
                    Err(e) => {
                        crate::log(&format!(
                            "OCR worker: failed to load {}: {}",
                            crate::paths::relative_display(&work_item.screenshot_path),
                            e
                        ));
                        continue; // Skip this item, continue with next
                    }
                };

                // Run OCR
                let readout = match ocr_screenshot(&img, &regions) {
                    Ok(readout) => readout,
                    Err(e) => {
                        crate::log(&format!(
                            "OCR worker: OCR failed for iteration {}: {}",
                            work_item.iteration, e
                        ));
                        continue; // Skip this item, continue with next
                    }
                };
                let scores = readout.scores;

                // Log the extracted scores
                crate::log(&format!(
                    "OCR complete for iteration {}: Stage1={:?}, Stage2={:?}, Stage3={:?}",
                    work_item.iteration, scores[0], scores[1], scores[2]
                ));

                // Overlap-reconstruction outcome (worst of the three stages).
                let recovery = worst_recovery(&readout.flags);
                let recovery_str = match recovery {
                    Recovery::Ok => "ok",
                    Recovery::Repaired => "repaired",
                    Recovery::Flagged => "flagged",
                };
                match recovery {
                    Recovery::Flagged => crate::log(&format!(
                        "OCR worker: iteration {} FLAGGED for review — {} (scores={:?}, flags={:?})",
                        work_item.iteration,
                        crate::paths::relative_display(&work_item.screenshot_path),
                        scores,
                        readout.flags
                    )),
                    Recovery::Repaired => crate::log(&format!(
                        "OCR worker: iteration {} scores repaired (flags={:?})",
                        work_item.iteration, readout.flags
                    )),
                    Recovery::Ok => {}
                }

                // Feed the live distribution buffer (read by the GUI to render the
                // in-run box plot). Done before the CSV write so the live view does
                // not depend on disk success. Flagged rows are kept but excluded
                // from live stats until verified.
                crate::automation::runner::record_live_score(
                    scores,
                    matches!(recovery, Recovery::Flagged),
                );

                // Append to CSV
                if let Err(e) = append_to_csv(&csv_path, &work_item, &scores, recovery_str) {
                    crate::log(&format!(
                        "OCR worker: failed to write CSV for iteration {}: {}",
                        work_item.iteration, e
                    ));
                    // Continue anyway - the screenshot is saved for manual retry
                }

                // Append to raw CSV (just scores, no header)
                let raw_csv_path = csv_path.with_file_name("rehearsal_data.csv");
                if let Err(e) = append_to_raw_csv(&raw_csv_path, &scores) {
                    crate::log(&format!(
                        "OCR worker: failed to write raw CSV for iteration {}: {}",
                        work_item.iteration, e
                    ));
                }
            }
            Err(_) => {
                // Channel closed, sender was dropped
                crate::log("OCR worker: channel closed, exiting");
                break;
            }
        }
    }

    crate::log("OCR worker finished");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::csv_writer::init_csv;
    use crate::automation::queue::create_work_queue;
    use std::thread;
    use tempfile::tempdir;

    #[test]
    fn test_worker_exits_when_channel_closes() {
        use crate::automation::config::RelativeRect;

        let dir = tempdir().unwrap();
        let csv_path = dir.path().join("test.csv");
        init_csv(&csv_path).unwrap();

        let (sender, receiver) = create_work_queue();

        let score_regions = [
            RelativeRect { x: 0.0, y: 0.165, width: 1.0, height: 0.035 },
            RelativeRect { x: 0.0, y: 0.418, width: 1.0, height: 0.035 },
            RelativeRect { x: 0.0, y: 0.670, width: 1.0, height: 0.035 },
        ];
        let regions = OcrRegions {
            score: score_regions,
            total: score_regions,
            bonus: score_regions,
        };

        // Spawn worker
        let csv_path_clone = csv_path.clone();
        let handle = thread::spawn(move || {
            run_ocr_worker(receiver, csv_path_clone, regions);
        });

        // Drop sender to close channel
        drop(sender);

        // Worker should exit
        handle.join().expect("Worker thread panicked");
    }
}
