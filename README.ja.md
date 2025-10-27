このアプリケーションは、vibe codingワークフローの一環として、AIツール: Codex CLI の支援を受けて生成されました。

# em6809

## 概要

em6809 は MC6809 のソフトウェアエミュレータです。実行用 CLI、詳細なコンソール I/O、分岐先の可視化を含むトレース/逆アセンブルを備え、OS‑9/NitrOS‑9 のブート検証にも活用できます。

## インストール

- Rust（stable）を用意してビルド:
  - `cargo build --release`

## Quick Start

- Hello World を実行:
  - `target/release/em6809 samples/hello/hello.s19 --run --format srec --console 0xFF00`
- ヘルプ表示:
  - `target/release/em6809 --help`
- PC を指定して開始:
  - `target/release/em6809 samples/hello/hello.s19 --run --format srec --console 0xFF00 --pc 0xE000`

## 基本ユースケース

- トレースしながら実行: `... samples/hello/hello.s19 --run --format srec --console 0xFF00 --trace`
- 逆アセンブル: `... --disasm 0xE000:0xE100`
- 16 進ダンプ: `... --hex 0xE000:0xE040`
- 標準入力/エコー: `... samples/echo/echo.s19 --run --console-stdin --console-flush-every 1 --console 0xFF00`

## ドキュメント

- 使用方法（CLI 全体）: `docs/ja/usage.md`
- Console I/O 詳細: `docs/ja/console_io.md`
- トレース/逆アセンブル: `docs/ja/tracing.md`
- OS‑9/NitrOS‑9 ガイド: `docs/ja/os9_guide.md`
- トラブルシューティング: `docs/ja/troubleshooting.md`
- 命令カバレッジ: `docs/ja/instruction_status.md`

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
