# 使い方（GUI モード）

em6809 は GUI モードに統一されています。起動時に必ずウィンドウが開き、レジスタ/リスティング/メモリビューから操作できます（初期状態は一時停止）。

## プラットフォーム前提条件

本エミュレータはネイティブ GUI（eframe/winit）で動作します。各 OS で以下を用意してください。

- Windows 10/11
  - Rust（rustup）: https://rustup.rs/
  - Visual Studio Build Tools（C++）: https://visualstudio.microsoft.com/visual-cpp-build-tools/
  - 日本語表示用フォント（任意）: `EM6809_GUI_FONT` で TTF/OTF/TTC のパスを指定可能。
- macOS 12+
  - Rust（rustup）: `curl https://sh.rustup.rs -sSf | sh`
  - Xcode Command Line Tools: `xcode-select --install`
  - 日本語フォント（任意）: `EM6809_GUI_FONT` を設定可能。
- Linux / WSL2 + WSLg
  - Rust（rustup）
  - ビルド/実行に必要なパッケージ（Debian/Ubuntu 例）:
    - `sudo apt update && sudo apt install -y build-essential pkg-config libxkbcommon0 libxkbcommon-dev libwayland-client0 libwayland-cursor0 libwayland-dev libx11-xcb1 libxcb1 libxcb-render0 libxcb-shape0 libxcb-xfixes0 libegl1 libgles2`
  - WSL2: Windows 11 の WSLg で Wayland/EGL がそのまま利用できます（他の X/Wayland サーバでも可）。

注意
- Windows ではイベントループをバックグラウンドスレッドで作成します。Unix 系の一部環境では GUI がメインスレッドを要求するため、実行環境によってはヘッドレスへフォールバックすることがあります。

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
  - `--console-echo`, `--console-flush-every <N>`, `--console-flush-newline`, `--console-tee-stderr`
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
- `--console-echo`: ローカルエコーを有効化
- `--console-in <TEXT>`: エスケープ可の文字列を送信（`\r`, `\n`, `\t`, `\\`）
- `--console-out <FILE>`: 出力ミラー
- `--console-tee-stderr`: `stderr` にもティー
- `--console-flush-every {off|byte|<N>}`: フラッシュ間隔（既定 1 = 毎バイト、最大 128）
- `--console-flush-newline`: 改行検出時にもフラッシュ

## 簡単なレシピ

- Hello World（GUI 表示・初期状態は一時停止）:
  - `target/release/em6809 samples/hello_world.s19 --format srec`
- 標準入力とエコー ON（Console 接続）:
  - `... --console 0xFF00 --console-stdin --console-echo`

詳細は console_io.md（I/O 制御）、tracing.md（リスティング/逆アセンブルの考え方）、memory_editing.md（メモリ編集）、os9_guide.md（OS‑9 連携）を参照してください。

## 環境変数（Linux の CSD ヘッダー）

- `EM6809_CSD`: Linux で `1` または `true` を設定すると、ウィンドウのクライアントサイド装飾（CSD）モードを有効化します（アイコン＋タイトル、Help→About、最小化/最大化/閉じるのカスタムヘッダー）。
  - 例: `EM6809_CSD=1 cargo run --release -- samples/echo/echo.s19 --format srec`
