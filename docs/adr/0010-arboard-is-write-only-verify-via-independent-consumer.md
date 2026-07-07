---
status: accepted
---

# arboard is write-only; clipboard verification reads use an independent consumer

The GUI copies images to the Windows clipboard via `arboard::set_image` (`src/gui/clipboard.rs`), but arboard's own read path is broken in practice: `get_image` prefers arboard's custom "PNG" clipboard format and `GetClipboardData` fails on it ("failed to read clipboard PNG data") even on a healthy interactive desktop where every real paste target (Paint, chat apps) reads the written image fine. So arboard is used write-only, and any read-back verification goes through an independent consumer — the `clipboard_roundtrip` test shells out to WinForms `Clipboard.GetImage` (`powershell -STA`) and pixel-compares a saved PNG. Do not "simplify" the test back to `arboard::get_image`, and do not build features that read images through arboard. Rejected alternatives: in-process read-back (broken as above) and write-only assertion (worthless — `set_image` returns `Ok` even in clipboard-less contexts such as agent shells, where no clipboard write actually lands).

Source: docs/EXECPLAN_IMAGE_COPY_TO_CLIPBOARD.md (Surprises & Discoveries, Decision Log). Evidence: identical read failure reproduced on the user's interactive desktop and the agent shell (2026-07-07) while in-app paste worked; WinForms round-trip then passed pixel-exact on the same desktop.
