# 使い方（GUI モード）

em6809 は GUI モードに統一されています。起動時に必ずウィンドウが開き、レジスタ/リスティング/メモリビューから操作できます（初期状態は一時停止）。

## 基本

- ビルド: `cargo build --release`
- 実行（S‑Record/BIN をロードして GUI を開く）:
  - `target/release/em6809 path/to/rom.s19 --format srec`
- PC の明示指定:
  - `target/release/em6809 path/to/rom.s19 --format srec --pc 0xE000`
- ヘルプ表示: `target/release/em6809 --help`

PC 初期化の優先順位:

1. `--pc` 指定
2. リセットベクタ
3. S‑Record のエントリ（ベクタが無効な場合）
4. 最初にロードされたメモリレンジの先頭

## 主なオプション（GUI 前提）

- 入力フォーマット/配置:
  - `--format bin|srec` / `--base 0xADDR` / `--pc 0xADDR`
- Console I/O（概要。詳細は console_io.md）:
  - `--console 0xADDR`（例: `0xFF00`）
  - `--console-stdin`, `--console-in <TEXT>`, `--console-out <FILE>`
  - `--console-echo`, `--console-flush-every <N>`, `--console-tee-stderr`
- MMU / プリセット:
  - `--mmu`, `--mmu-phys`, `--mmu-reg-base`, `--mmu-task`, `--mmu-idmap`
  - `--mmu-map P=F`, `--mmu-map-file PATH`, `--preset NAME`, `--preset-list`
  - `--mmu-mode sys|user`, `--protect-vectors`, `--prot`, `--attr`
- ブートスクリプト/テンプレート:
  - `--boot-script PATH`, `--emit-boot-template NAME`
- その他:
  - `--cycles`（サイクル数出力）

WSL2 + WSLg（Wayland）のセットアップ手順・既知の注意点は `docs/troubleshooting.md` の「WSL2 / Linux の GUI について」を参照してください。

入出力関連（概要。詳細は console_io.md）

- `--console-stdin`: 標準入力を接続
- `--console-echo {ON|OFF}`: ローカルエコー制御
- `--console-in <TEXT>`: エスケープ可の文字列を送信（`\r`, `\n`, `\t`, `\\`）
- `--console-out <FILE>`: 出力ミラー
- `--console-tee-stderr`: `stderr` にもティー
- `--console-flush-every {off|byte|<N>}`: フラッシュ間隔

## 簡単なレシピ

- Hello World（GUI 表示・初期状態は一時停止）:
  - `target/release/em6809 samples/hello_world.s19 --format srec`
- 標準入力とエコー ON（Console 接続）:
  - `... --console 0xFF00 --console-stdin --console-echo`

詳細は console_io.md（I/O 制御）、tracing.md（リスティング/逆アセンブルの考え方）、os9_guide.md（OS‑9 連携）を参照してください。

