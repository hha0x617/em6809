# 使用方法（CLI 全体）

このドキュメントは、mc6809emu の主要な CLI オプションと、よく使うコマンド例を一覧でまとめたものです。詳細な入出力制御やトレース、OS‑9 連携の深掘りは別ドキュメントを参照してください。

## 基本

- ビルド: `cargo build --release`
- 実行（S‑Record をロードして Ctrl+C まで走行）:
  - `target/release/mc6809emu --run --srec path/to/rom.s19`
- PC の明示指定:
  - `target/release/mc6809emu --run --srec path/to/rom.s19 --pc 0xE000`
- ヘルプ表示: `target/release/mc6809emu --help`

PC 初期化の優先順位:

1. `--pc` 指定
2. リセットベクタ
3. S‑Record のエントリ（ベクタが無効な場合）
4. 最初にロードされたメモリレンジの先頭

## 代表オプション

- `--run`: 連続実行（Ctrl+C で終了）
- `--srec <FILE>`: S‑Record をロード（複数指定可）
- `--pc <ADDR>`: 開始アドレスを強制指定
- `--trace`: 実行トレースを出力（命令/レジスタ/分岐先）
- `--step`: ステップ実行モード
- `--disasm <RANGE>`: 逆アセンブル（例: `0xE000:0xE100`）
- `--hex <RANGE>`: 16 進ダンプ（例: `0xE000:0xE040`）

WSL2 + WSLg（Wayland）で GUI（`--features gui` + `--dump-regs`）を使う場合の設定例は `docs/troubleshooting.md` の「WSL2 / Linux の GUI について」を参照してください。

入出力関連（概要。詳細は console_io.md）

- `--console-stdin`: 標準入力を接続
- `--console-echo {ON|OFF}`: ローカルエコー制御
- `--console-in <TEXT>`: エスケープ可の文字列を送信（`\r`, `\n`, `\t`, `\\`）
- `--console-out <FILE>`: 出力ミラー
- `--console-tee-stderr`: `stderr` にもティー
- `--console-flush-every {off|byte|<N>}`: フラッシュ間隔

## 簡単なレシピ

- Hello World 実行:
  - `target/release/mc6809emu --run --srec samples/hello_world.s19`
- 逆アセンブルのみ:
  - `target/release/mc6809emu --disasm 0xE000:0xE100`
- トレースを見ながら実行:
  - `target/release/mc6809emu --run --srec path/to/rom.s19 --trace`
- 標準入力とエコー ON:
  - `... --console-stdin --console-echo ON`

より詳細な I/O 制御は console_io.md、デバッグ/トレースは tracing.md、OS‑9 連携は os9_guide.md を参照してください。

