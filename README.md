# gakumas-rehearsal-automation

学園アイドルマスター (gakumas.exe) のクライアント領域をキャプチャするスクリーンショットツール。リハーサル自動化機能付き。

## 機能

- **ホットキー**: `Ctrl+Shift+S` でスクリーンショットを撮影
- **自動化**: `Ctrl+Shift+A` でリハーサル自動周回、`Ctrl+Shift+Q` で中止
- **OCR内蔵**: Tesseract OCRが内蔵されており、初回起動時に自動展開
- **クライアント領域のみ**: タイトルバーやウィンドウ枠を除いたゲーム画面のみをキャプチャ
- **システムトレイ**: トレイアイコンから右クリックで各種操作可能
- **Windows Graphics Capture**: 高性能なキャプチャAPI使用（Windows 10 1803以降対応）

## 使い方

1. `gakumas-rehearsal-automation.exe` を起動（管理者権限が必要）
2. システムトレイにアイコンが表示される
3. ゲームを起動した状態で `Ctrl+Shift+S` を押す
4. `screenshots/` フォルダに `gakumas_YYYYMMDD_HHMMSS.png` として保存される
5. 終了するにはトレイアイコンを右クリック → Exit

## フォルダ構成

```
gakumas-rehearsal-automation/
├── gakumas-rehearsal-automation.exe  # 実行ファイル（Tesseract内蔵）
├── config.json             # 設定ファイル
├── logs/                   # ログファイル
├── screenshots/            # スクリーンショット保存先
├── resources/
│   └── template/
│       └── rehearsal/      # リハーサル用リファレンス画像
└── tesseract/              # 初回起動時に自動展開
```

## ビルド方法

```powershell
cargo build --release
```

リリースパッケージの作成:
```powershell
powershell -ExecutionPolicy Bypass -File scripts/package-release.ps1
```

出力: `release/gakumas-rehearsal-automation/`

## 動作要件

- Windows 10 バージョン 1803 以降
- 管理者権限（SendInput使用のため）
- gakumas.exe が起動していること

## ログ

動作ログは `logs/gakumas_screenshot.log` に出力されます。

---

# English

Screenshot tool for capturing the client area of Gakuen iDOLM@STER (gakumas.exe). Includes rehearsal automation features.

## Features

- **Hotkey**: Press `Ctrl+Shift+S` to take a screenshot
- **Automation**: Press `Ctrl+Shift+A` to start rehearsal automation, `Ctrl+Shift+Q` to abort
- **Built-in OCR**: Tesseract OCR is embedded and auto-extracts on first run
- **Client area only**: Captures only the game screen, excluding title bar and window borders
- **System tray**: Access various functions via right-click menu on tray icon
- **Windows Graphics Capture**: Uses high-performance capture API (Windows 10 1803+)

## Usage

1. Run `gakumas-rehearsal-automation.exe` (requires administrator privileges)
2. An icon appears in the system tray
3. With the game running, press `Ctrl+Shift+S`
4. Screenshot is saved as `gakumas_YYYYMMDD_HHMMSS.png` in the `screenshots/` folder
5. To exit, right-click the tray icon → Exit

## Folder Structure

```
gakumas-rehearsal-automation/
├── gakumas-rehearsal-automation.exe  # Executable (Tesseract embedded)
├── config.json             # Configuration file
├── logs/                   # Log files
├── screenshots/            # Screenshot output
├── resources/
│   └── template/
│       └── rehearsal/      # Rehearsal reference images
└── tesseract/              # Auto-extracted on first run
```

## Build

```powershell
cargo build --release
```

Create release package:
```powershell
powershell -ExecutionPolicy Bypass -File scripts/package-release.ps1
```

Output: `release/gakumas-rehearsal-automation/`

## Requirements

- Windows 10 version 1803 or later
- Administrator privileges (required for SendInput)
- gakumas.exe must be running

## Logs

Operation logs are written to `logs/gakumas_screenshot.log`.
