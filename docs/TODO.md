# TODOs / Future Work

This file tracks planned enhancements for the GUI and tooling. Please keep items concise and update status as work progresses.

## Short‑term

- Memory dump pane (GUI):
  - [done] Add a dedicated memory view next to the listing pane
  - [done] Features: start address + length, Hex + ASCII columns, scrolling, copy selection, optional highlight of the current address/PC
  - [done] Nice‑to‑have: follow cursor/PC toggle; jump to address box

- Register and flag editing (GUI):
  - [done] Make register cells editable (A, B, D, X, Y, U, S, DP, PC)
  - [done] CC flags (E F H I N Z V C) as toggle buttons with validation
  - [done] Apply/Cancel actions; input validation (radix, width) before committing to CPU state and snapshot

- [done] Enable editing of memory values in the memory dump pane

- Non‑Windows platforms (Linux, macOS):
  - [done] Raw terminal for `--console-stdin` (disable ICANON/ECHO via termios; VMIN=1/VTIME=0)
  - [done] Automatic Japanese font discovery (common Linux/macOS paths, prefer `EM6809_GUI_FONT` if set)
  - Documentation updates (console_io.md)
  - [todo] Strengthen font discovery (include macOS English‑name Hiragino and `/Library/Fonts`; consider CoreText‑based enumeration later)
  - [known-issue] macOS: `IMKCFRunLoopWakeUpReliable` warning on TextEdit (no functional impact)
    - [plan] Upgrade eframe/winit and switch to official window/IME APIs; disable IME for numeric fields on macOS
    - [workaround] Prefer DragValue for numeric inputs to avoid IME path (user keyboard mode switch is not a long‑term solution)

## Notes

- Keep the viewer responsive: avoid heavy recomputation on each keystroke (apply on Enter/Apply)
- Reuse existing disassembler formatting for consistency

---

# TODO（日本語）

GUI とツールの今後の拡張を管理します。簡潔に記載し、進捗に応じて更新してください。

## 直近

- メモリダンプ表示ペイン（GUI）
  - [done] 右ペインの隣にメモリビューを追加
  - [done] 機能: 開始アドレス/長さ、HEX + ASCII、スクロール、選択のコピー、（任意）現在アドレス/PC のハイライト
  - [done] あると良い: カーソル/PC 追従のトグル、アドレスジャンプ欄

- レジスタ/フラグの変更（GUI）
  - [done] レジスタ（A, B, D, X, Y, U, S, DP, PC）を編集可能に
  - [done] CC フラグ（E F H I N Z V C）をトグルで変更、検証あり
  - [done] 適用/キャンセル、入力検証（基数/桁幅）後に CPU 状態とスナップショットへ反映

- [done] メモリダンプペインでメモリの値を編集可能にする

- Windows 以外の環境への対応（Linux, macOS）
  - [done] `--console-stdin` で端末を raw 化（termios で ICANON/ECHO を無効化、VMIN=1/VTIME=0）
  - [done] 日本語フォント自動検出（Linux/macOS の代表パスを追加、`EM6809_GUI_FONT` を優先）
  - 付随するドキュメント更新（console_io.md）
  - [todo] フォント検出の強化（macOS の英語名 Hiragino フォントや `/Library/Fonts` 配下も候補に追加。将来的に CoreText による動的列挙の検討）
  - [known-issue] macOS: TextEdit 編集時に `IMKCFRunLoopWakeUpReliable` 警告がコンソールに出力される（機能影響なし）
    - [plan] eframe/winit を将来バージョンへ更新し、ウィンドウハンドル/IME 設定を正式 API で扱えるようになってから、macOS では数値欄に対して IME を無効化
    - [workaround] 短期は DragValue への置換で IME を経由しない数値入力にする（ユーザー側の英字(ABC) 切替は恒久策としては非推奨）

## メモ

- 応答性重視: 入力中に重い再計算は避け、Enter/適用時に反映
- 逆アセンブルの表記は既存のスタイルに合わせる
