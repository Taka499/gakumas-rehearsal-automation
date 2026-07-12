<!--
Format (per docs/EXECPLAN_CHANGELOG_AND_JP_NOTES.md):
- One "## vX.Y.Z — YYYY-MM-DD" section per release, newest first.
- First line under the heading: one-line Japanese summary. It doubles as the
  first paragraph of the GitHub release body (= the in-app update hover hint),
  so it must stand alone and read naturally.
- Then Japanese bullets (user-facing).
- Then a "### English" subsection (for maintainers/agents). The in-app
  更新履歴 window hides everything from "### English" to the next "## ".
- No Unreleased section: entries are written during the release procedure.
-->

# 更新履歴 / Changelog

## v0.10.0 — 2026-07-13

アプリ内からのフィードバック送信と、更新履歴の表示機能を追加

- ヘッダーの「フィードバック」ボタンから、ご意見や不具合の報告をアプリ内から直接送信できるようになりました（バグ報告にはセッションログを添付できます）
- ヘッダーの「更新履歴」ボタンで、これまでのバージョンの変更内容をいつでも確認できるようになりました
- 「結果の確認・修正」画面は、要確認（flagged）の行だけを最初に表示するようになりました

### English
- In-app feedback form: header フィードバック button opens a floating form (message + bug/request/other; bug reveals a session-log picker, newest preselected). POSTs to the tia.run Worker, which creates labeled issues in the private `tia-tools/feedback` repo; log travels inline, tail-truncated to ~60KB; rate-limited 5/day.
- In-app 更新履歴 window: the bilingual repo CHANGELOG.md is embedded into the binary via `include_str!` and rendered Japanese-only in a scrollable window.
- Review window now opens with the flagged-only status filter (repaired rows start hidden).

## v0.9.1 — 2026-07-08

固定ダウンロードリンク、署名付き自動アップデート、匿名利用統計を追加

- 最新版はいつでも https://rehearsal-automation.tia.run/download から入手できます
- アプリ内アップデートは暗号署名で検証されるようになり、改ざんされた更新が配布されることはありません
- ダウンロード数・更新チェック数を匿名で集計するようにしました（日付・バージョン・国のみ。IPアドレスや個人を特定できる情報は保存しません）

### English
- Permanent download URL: https://rehearsal-automation.tia.run/download (302 to the latest zip).
- Updates are minisign-signed; the updater verifies against the public key embedded in the binary before installing (ADR-0013).
- Anonymous usage metrics on the dist Worker: day/event/version/country + daily-rotating salted IP hash, no persistent identifiers (ADR-0012).

## v0.9.0 — 2026-07-07

ワンクリック自動アップデートと、実行中スコア分布のライブ表示を追加

- 起動時に新バージョンを自動チェックし、ヘッダーの通知からワンクリックで更新できるようになりました（`config.json` のキャリブレーションは更新で消えません）
- 実行中に9つのスコア分布（箱ひげ図と統計表）をリアルタイム表示するサイドパネルを追加
- 箱ひげ図やレビュー画面の切り抜き画像を右クリックでクリップボードにコピーできるようになりました
- 未確認（flagged）の行は、確認が済むまで最終グラフ・統計から除外されるようになりました
- OCRのしきい値ぎりぎりの読み取りミスを自動で再試行して修正するようになりました

### English
- One-click auto-update: launch-time check (domain manifest first, GitHub fallback), staged download → sha256 verify → rename-swap install (ADR-0011). First release on the identity-separated dist channel `tia-tools/releases`.
- Live nine-box score-distribution side panel (box plot + Avg/Med/Max/Min/Q1/Q3 table) updating per iteration; seeded from the newest session at launch.
- Right-click-to-copy for GUI images (box plot, review stage crops) with a toast.
- Flagged rows excluded from final charts/stats until verified.
- Score-row multi-threshold OCR retry for knife-edge trailing-digit misreads.
- (The published v0.9.0 release body omitted the live box plot; this entry is the complete record.)

## v0.6.1 — 2026-06-27

バックグラウンドでもスクリーンショットが即座に撮影されるようになりました

- ツールのウィンドウが背面にあっても Ctrl+Shift+S のスクリーンショットが即時に実行されます（Ctrl+Shift+Q の中止も同様）
- 画面上部に利用できるショートカット一覧のヒントを追加

### English
- Ctrl+Shift+S / Ctrl+Shift+Q hotkeys fire immediately while the tool window is unfocused (previously deferred until refocus).
- Shortcut hint line added under the heading.

## v0.6.0 — 2026-06-27

OCR結果をアプリ内で確認・修正できるレビュー画面を追加

- 「結果の確認・修正」ウィンドウで、終了したどの実行結果でもスコアを一覧・編集できます
- 各行の 📷 ボタンで、ステージごとのスクリーンショット切り抜きを編集欄のすぐ下に表示
- 状態別フィルタ（flagged / repaired / ok / manual）と Ctrl+F のスコア検索
- 修正は `results.csv` / `rehearsal_data.csv` に保存され、手動編集として記録されます（`recovery=manual`）
- ステージ合計で確認できる正しいスコアが、ボーナス表示の読み取りミスだけで誤って要確認になることがなくなりました

### English
- OCR result review/edit window: editable score table, inline per-stage crops, per-status filters + live search, saves marked `recovery=manual`.
- Total-confirmed scores are no longer falsely flagged by a misread bonus badge.

## v0.5.1 — 2026-06-22

100万点以上のスコアが重なって表示される場合の復元を強化

- 左のスコアが「0」で終わる場合に重なりが「4」と誤読されるケース、先頭の「1」が二重に読まれるケースも自動復元されるようになりました
- これらは従来も誤った値になることはなく「要確認」扱いでしたが、今後は自動修復されます

### English
- Overlap recovery now reconstructs the "substituted" (0+1 glyph OCRs as 4) and "duplicated" (leading 1 read twice) corruption modes via the total checksum; previously flagged-but-unrecovered.

## v0.5.0 — 2026-06-18

100万点以上のスコアが重なって表示されても正しく読み取れるようになりました

- 隣り合うスコアがどちらも100万点以上のとき、ゲーム画面では右の数字の先頭「1」が左の数字に重なって描画されます。画面上のステージ合計とボーナスを検算に使って本来のスコアを復元します
- 復元できない場合は誤った値を保存せず「要確認（flagged）」として記録し、スクリーンショットと共にログに残します
- `results.csv` に `recovery` 列（`ok` / `repaired` / `flagged`）を追加
- 実際のリハーサル100回で誤検出ゼロを確認済み

### English
- Overlapping-million score recovery: reconstructs colliding ≥1,000,000 adjacent scores using the on-screen `total = c1+c2+c3+bonus` checksum; unverifiable reads are flagged, never silently stored. New `recovery` CSV column.

## v0.4.0 — 2026-06-15

「追加実行」と実行回数のプリセットボタンを追加

- 一連の実行が終わった後、同じフォルダに続けて追加の実行ができるようになりました（回数の連番・CSV・グラフはすべて一つのシリーズとして継続されます）
- 実行回数を 100 / 200 / 500 / 1000 のボタンでワンタップ設定できるようになりました

### English
- 追加実行 (extend): run more iterations into the same finished session folder — numbering continues, CSVs append, charts regenerate over the whole series. Never shown alongside 続行 (resume).
- Preset run-count buttons (100/200/500/1000) under the count inputs.

## v0.3.3 — 2026-06-05

100万点以上のスコアでOCRが失敗する問題を修正

- スコアが7桁（100万点以上）になると隣のスコアと繋がって読み取られ、そのステージ全体のスコアが失われることがありました。数字の区切りを桁区切りパターンで再分割して正しく読み取るようにしました

### English
- 7-digit scores glued to their left neighbor by Tesseract overflowed u32 and dropped the stage; the extractor now re-tokenizes lines with a thousands-group regex (`\d{1,3}(,\d{3})*`), subsuming the old leading-garbage stripping.

## v0.3.2 — 2026-03-21

各実行が約2.4秒速くなり、ボタン検出の誤判定も減りました

- クリックの確認待ちを検出ループに統合し、1回あたり約2.4秒短縮
- ボタン検出は3回連続で一致した場合のみ確定するようになり、誤検出が減少
- ログに絶対パスを出力しないようにしました（プライバシー改善）

### English
- Non-blocking click retry folded into detection polling (~2.4s/iteration saved); 3-consecutive-match confirmation for button detection; logs use relative paths.

## v0.3.1 — 2026-02-07

一番左のスコアが0になることがある問題を修正

- OCRが先頭に混入させるゴミ文字を除去し、さらに切り抜き範囲を調整して区切り線を除外することで、左端のスコアが約1.5%の確率で0になる問題を解消しました

### English
- Leftmost-score dropout (~1.5%) fixed two ways: strip leading non-digit garbage before regex matching, and tighten crop regions to exclude the horizontal divider line (44px → 28px at 720x1280).

## v0.3.0 — 2026-02-07

OCRの精度が約60%から99.2%に向上しました

- スコア行ごとに個別に切り抜いて認識する方式に変更し、ボーナスや合計値などの誤読み取りを排除
- 切り抜き位置は `config.json` の `score_regions` で調整可能（通常は変更不要）

### English
- Per-stage crop→threshold→OCR→extract pipeline (accuracy ~60% → 99.2% over 1000 iterations); `--psm 6` block mode; configurable `score_regions` with sensible defaults.

## v0.2.2 — 2026-02-03

外部ツールで処理しやすい `rehearsal_data.csv` を追加

- 9つのスコアだけをカンマ区切り（ヘッダーなし）で出力する `rehearsal_data.csv` を `results.csv` と併せて生成するようになりました

### English
- Added `rehearsal_data.csv` (9 scores per line, no header) alongside `results.csv`.

## v0.2.1 — 2026-01-30

スキップボタンの検出精度を改善

- 明るさのしきい値の既定値を 95 から 94 に調整しました

### English
- Default `brightness_threshold` 95 → 94 for Skip button detection.

## v0.2.0 — 2026-01-15

完了時の情報表示を充実させ、どの解像度でも動作するようになりました

- 完了時に出力フォルダ名・生成ファイル一覧を表示
- 自動実行中は「⚠ 実行中はマウスを動かさないでください」と警告
- ゲームをどの解像度（1080p、720pなど）で実行してもページ検出が動作するようになりました

### English
- Completion feedback (folder name, generated-files summary), mouse-movement warning during automation, and resolution-independent detection (captured regions resized to reference dimensions before histogram comparison).

## v0.1.0 — 2026-01-12

初回リリース

- Ctrl+Shift+S でゲーム画面のスクリーンショットを撮影（タイトルバー・枠なしのクライアント領域のみ）
- システムトレイに常駐、トレイメニューから終了
- Windows 10 バージョン1803以降が必要

### English
- Initial release: client-area screenshot via Windows Graphics Capture API, Ctrl+Shift+S hotkey, system tray app. Requires Windows 10 1803+.
