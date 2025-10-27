# TODOs / Future Work

This file tracks planned enhancements for the GUI and tooling. Please keep items concise and update status as work progresses.

## Short‑term

- Memory dump pane (GUI):
  - Add a dedicated memory view next to the listing pane
  - Features: start address + length, Hex + ASCII columns, scrolling, copy selection, optional highlight of the current address/PC
  - Nice‑to‑have: follow cursor/PC toggle; jump to address box

- Register and flag editing (GUI):
  - Make register cells editable (A, B, D, X, Y, U, S, DP, PC)
  - CC flags (E F H I N Z V C) as toggle buttons with validation
  - Apply/Cancel actions; input validation (radix, width) before committing to CPU state and snapshot

## Notes

- Keep the viewer responsive: avoid heavy recomputation on each keystroke (apply on Enter/Apply)
- Reuse existing disassembler formatting for consistency

---

# TODO（日本語）

GUI とツールの今後の拡張を管理します。簡潔に記載し、進捗に応じて更新してください。

## 直近

- メモリダンプ表示ペイン（GUI）
  - 右ペインの隣にメモリビューを追加
  - 機能: 開始アドレス/長さ、HEX + ASCII、スクロール、選択のコピー、（任意）現在アドレス/PC のハイライト
  - あると良い: カーソル/PC 追従のトグル、アドレスジャンプ欄

- レジスタ/フラグの変更（GUI）
  - レジスタ（A, B, D, X, Y, U, S, DP, PC）を編集可能に
  - CC フラグ（E F H I N Z V C）をトグルで変更、検証あり
  - 適用/キャンセル、入力検証（基数/桁幅）後に CPU 状態とスナップショットへ反映

- Windows 以外の環境への対応（Linux, macOS）
  - ビルド/実行確認、依存パッケージ（フォント探索、GUIバックエンド）の差異吸収
  - 日本語フォント自動検出パスの整備、`EM6809_GUI_FONT` の利用ドキュメント補強

## メモ

- 応答性重視: 入力中に重い再計算は避け、Enter/適用時に反映
- 逆アセンブルの表記は既存のスタイルに合わせる
