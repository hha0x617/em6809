# em6809

[![Build and Test](https://github.com/hha0x617/em6809/actions/workflows/build.yml/badge.svg)](https://github.com/hha0x617/em6809/actions/workflows/build.yml)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](LICENSE-APACHE)

*このアプリケーションは、vibe coding ワークフローの一環として、AI ツール Codex CLI の支援を受けて生成されました。*

## 概要

em6809 は MC6809 のソフトウェアエミュレータです。GUI を中心に、詳細なコンソール I/O、分岐先の可視化を含むトレース/逆アセンブルを備え、OS‑9/NitrOS‑9 のブート検証にも活用できます。

## インストール

- Rust（stable）を用意してビルド:
  - `cargo build --release`

## クイックスタート（GUIのみ）

- 実行して GUI を開く（初期状態は一時停止）:
  - `target/release/em6809 samples/hello/hello.s19 --format srec --console 0xFF00`
- ヘルプ表示:
  - `target/release/em6809 --help`
- PC を指定して開始:
  - `target/release/em6809 samples/hello/hello.s19 --format srec --console 0xFF00 --pc 0xE000`

## 基本ユースケース

- 標準入力/エコー: `... samples/echo/echo.s19 --console 0xFF00 --console-stdin --console-echo`
  （逆アセンブル/メモリ/トレースは GUI ウィンドウ内で操作します）

## ドキュメント

- 使用方法: `docs/ja/usage.md`
- Console I/O 詳細: `docs/ja/console_io.md`
- メモリダンプ編集（GUI）: `docs/ja/memory_editing.md`
- トレース/逆アセンブル: `docs/ja/tracing.md`
- OS‑9/NitrOS‑9 ガイド: `docs/ja/os9_guide.md`
- トラブルシューティング: `docs/ja/troubleshooting.md`
- 命令カバレッジ: `docs/ja/instruction_status.md`
 - GUI の環境変数（フォント/言語）: `docs/ja/usage.md`（「環境変数」節）

## リンク

- リポジトリ: https://github.com/hhagiwara/mc6809emu
- Issue: https://github.com/hhagiwara/mc6809emu/issues
- リリース: https://github.com/hhagiwara/mc6809emu/releases
- ドキュメント索引: `docs/README.md`
- 英語ドキュメント: `docs/en/`
- 日本語ドキュメント: `docs/ja/`
- サンプル（トレース/ブートスクリプト）: `samples/`

## 参考文献

- Motorola 6809（概要）: https://en.wikipedia.org/wiki/Motorola_6809
- MC6809 資料アーカイブ（Bitsavers）: http://bitsavers.org/components/motorola/6809/
- S‑Record 形式: https://en.wikipedia.org/wiki/SREC_(file_format)
- NitrOS‑9 プロジェクト: https://sourceforge.net/projects/nitros9/
- 6809 コミュニティリソース: http://www.6809.org.uk/
- LWTOOLS: https://www.lwtools.ca/

## 貢献とポリシー

- 開発/運用ルール: `AGENTS.md`
- 行動規範/セキュリティ: `CODE_OF_CONDUCT.md` / `SECURITY.md`（存在する場合）

## ライセンス

MIT OR Apache‑2.0（デュアルライセンス）。詳細は `LICENSE-MIT` と `LICENSE-APACHE` を参照してください。

### 最近の GUI レジスタビューアの拡張

- `.lst` が無い場合のフォールバック（プログラムスパンの HEX 逆アセンブルを自動生成）。
- 「Hex view」切替と「Follow PC」ハイライト/自動スクロール。
- 自己書換えに対するイベント連動の再逆アセンブル（Full span / PC window）。
- 右クリックメニュー（リスティング行）: ここまで実行 / PC を現在のアドレスに変更 / キャンセル。
- リスティング行のホバー時に行全体を薄くハイライト。コンテキスト操作後は各ペインを即時更新。
