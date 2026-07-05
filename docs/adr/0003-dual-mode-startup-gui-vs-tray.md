---
status: accepted
---

# Dual-mode startup (GUI vs legacy tray) with a message-only hotkey window

The app has two separate startup paths — egui GUI by default, the legacy Win32 tray app when `developer_mode=true` — plus a background thread owning a message-only window for `RegisterHotKey`, because eframe's event loop cannot coexist with the existing Windows message loop (attempting it hit an `RPC_E_CHANGED_MODE` COM conflict) and global hotkeys did not fire in GUI mode without their own message pump. The rejected alternative was integrating the two loops on one thread.

Source: `docs/EXECPLAN_PHASE5_GUI.md` (Surprises & Discoveries, Decision Log); evidence: observed COM-init panic and hotkey failure during implementation.
