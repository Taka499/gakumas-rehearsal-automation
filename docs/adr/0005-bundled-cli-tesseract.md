---
status: accepted
---

# OCR shells out to a bundled Tesseract exe embedded as a zip, extracted next to the exe

OCR invokes a bundled `tesseract.exe` via `std::process::Command` rather than linking libtesseract, and the ~30 MB Tesseract zip (56 DLLs, ~72 MB uncompressed) is embedded with `include_bytes!` and extracted to `<exe_dir>/tesseract/` on first run. Rejected alternatives: C bindings (`tesseract`/`leptonica-sys` — harder builds, no simpler setup for users) and extraction to `%LOCALAPPDATA%` (less portable, needs cleanup; the app runs as admin so writing next to the exe is available). Consequence: the release binary carries the 30 MB payload and the OCR engine is built around subprocess temp-file I/O with `CREATE_NO_WINDOW`.

Source: `docs/EXECPLAN_PHASE2_OCR.md` and `docs/EXECPLAN_TESSERACT_BUNDLE.md` Decision Logs; verified standalone (Bundle M1) and end-to-end (Bundle M5, 38.15 MB binary).
