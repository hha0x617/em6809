# 使用方法（GUI・日本語）

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

WSL2 + WSLg（Wayland）のセットアップ手順・既知の注意点は `docs/ja/troubleshooting.md` の「WSL2 / Linux の GUI について」を参照してください。

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

### ブロックデバイスを使ったサンプル

- lwasm (lwtools) でアセンブル:
  - `lwasm -9 --format=srec -o samples/block_device_read.s19 samples/block_device_read.asm`
  - `lwasm -9 --format=srec -o samples/block_device_write.s19 samples/block_device_write.asm`
- 読み出しデモ (付属イメージを使用):

```bash
cargo run -- samples/block_device_read.s19 \
  --block 0xFF20 \
  --block-file samples/block_device_image.bin \
  --console 0xFF00 \
  --console-echo \
  --run
```

ゲストは `$FF20` のブロックコントローラに 1 セクタ読み込み（LBA 0）を要求し、戻ってきたテキストを長さバイトに従ってコンソールへストリームします。`samples/block_device_image.bin` は 512 バイト以上あり、先頭 1 バイトにメッセージ長、続く領域に ASCII テキスト（512 バイトを満たすためにパディングあり）が入っているので、そのまま書き換えるだけで表示内容を差し替えられます。

- 書き込みデモ (任意のファイルへ書き込み、読み戻し表示):

```bash
BLOCK_IMG=/tmp/block_write_demo.bin
cargo run -- samples/block_device_write.s19 \
  --block 0xFF20 \
  --block-file ${BLOCK_IMG} \
  --console 0xFF00 \
  --console-echo \
  --run
hexdump -C -n 64 ${BLOCK_IMG}
```

LBA 1 へ長さ付きペイロードをストリームし、BUSY ビットが落ちるまで待機したのちに READ を実行して内容を確認します。`hexdump` で `${BLOCK_IMG}` の先頭を確認したり、読み出しサンプルに同じファイルを渡して表示内容を検証できます。

詳細は console_io.md（I/O 制御）、tracing.md（リスティング/逆アセンブルの考え方）、memory_editing.md（メモリ編集）、os9_guide.md（OS‑9 連携）を参照してください。

## GPIO / Timer の CLI オプション

- `--gpio-bits N`: GPIO LED の本数（デフォルト 8、最大 32）。GPIO は 8bit ごとの 4 バイトスロットで構成されます: `+0` OUT(書き込み)、`+1` IN/READ、`+2` DIR(書き込み)、`+3` 制御(スロット0のみ: bit0=1 で入力ラッチクリア)。指定ビット数を超えるビットはマスクされ常に 0 になり、必要スロット外のアクセスは無視されます。
- `--gpio-on-color 0xRRGGBB`: LED ON 時の色（例: `0xFFD000`）
- `--timer` 系のオプションは英語版 usage.md と同等で、ベースアドレス/リロード値/IRQ/FIRQ/start などを指定できます。

## 環境変数（Linux の CSD ヘッダー）

- `EM6809_CSD`: Linux で `1` または `true` を設定すると、ウィンドウのクライアントサイド装飾（CSD）モードを有効化します（アイコン＋タイトル、Help→About、最小化/最大化/閉じるのカスタムヘッダー）。
  - 例: `EM6809_CSD=1 cargo run --release -- samples/echo/echo.s19 --format srec`
  - 備考: Linux のネイティブ Settings ダイアログは、Wayland/X11 の標準装飾（SSD）経由では最小化/最大化ボタンの無効化ができないため、常に CSD を使用します。winit/eframe 0.27 では `ViewportCommand::EnableButtons` が Mutter/GNOME などに無視されるため、Close だけを確実に残す手段が CSD しかありません。GTK/Qt のようなネイティブツールキットとは異なる制限がある点に注意してください。
  - 将来的には winit/eframe 側で Wayland/X11 の SSD ボタン制御がサポートされた際に再検討します（例: `xdg_toplevel` の `set_minimize/maximize` 相当 API が公開された場合に、CSD なしで Close のみを残せるか確認する）。

## その他の環境変数

- `EM6809_FALLBACK_SETTINGS`: `1` または `true` を指定すると、Preferences/Settings で in-app 版ダイアログを使用します（デフォルトはネイティブウィンドウ）。CLI の `--fallback-settings` オプションでも同じ効果があります。

## 環境変数（GUI レジスタビューア）

`--features gui` でビルドした場合、以下の環境変数を利用できます。詳細は `docs/en/regview_integration.md` を参照してください。

- `EM6809_GUI_FONT`: 優先して使うフォントファイル（`.ttf`/`.otf`/`.ttc`）。日本語表示の文字化け回避に有効です。
- `EM6809_LANG`: リスティング右クリックメニューの言語を強制します。`ja`（日本語）/`en`（英語）。未設定時は `LC_ALL`/`LC_MESSAGES`/`LANG`（または Windows の OS 言語）から自動判定します。

追加（Linux の About ダイアログ アイコン）:

- `EM6809_ABOUT_ICON`: Linux で About ダイアログに表示する PNG 画像の絶対パス。
  - 指定時は 32×32 にリサイズして表示します。
  - 未指定時は `assets/app_icon_32.png` → `assets/icon32.png` → `assets/icon.png` の順で探索します。
  - いずれも見つからない場合は、角丸の背景に小文字の "e" を描画するベクタのフォールバックを表示します。

## GUI メニュー

- Help → About em6809…: バージョン・ライセンス表示。
  - Windows/macOS: プラットフォームのウィンドウ/アプリのアイコンを使用します。
  - Linux: 上記の `EM6809_ABOUT_ICON`/assets から PNG を読み込み（無い場合はベクタ フォールバック）。

## レジスタビューアの機能拡張（最近の追加）

- リスティングの自動切替/フォールバック
  - 読み込んだイメージと同名の `.lst` があればそれを表示、無い場合はプログラムスパンの HEX 逆アセンブルを自動生成して表示します。
  - 「Hex view」トグルで `.lst` と HEX 逆アセンブルを切替可能です。
- フォロー/ハイライト
  - 「Follow PC」有効時は PC を自動追従し、現在行をハイライトします。
- 自己書換えへイベント連動で対応
  - メモリ書込みを検知して再逆アセンブルを実行（タイマではなくイベント駆動）。
  - Span Mode: Full span（PC を含むスパン全体を再生成）/ PC window（変更アドレス近傍のみ再生成、ウィンドウ長は可変）。
- コンテキストメニュー（右クリック）
  - 「ここまで実行」: 選択行のアドレス到達で自動停止（Run to here）。
  - 「PCの値を現在のアドレスに変更」: 即時に PC を変更（Set PC）。
  - 「キャンセル」: メニューを閉じる。
  - メニュー表記はロケールに応じて日本語/英語に自動切替（`EM6809_LANG` で強制指定も可能）。
  - 「ここまで実行」「PCの値を現在のアドレスに変更」実行後は、レジスタとリスティングの各ペインが即時に最新状態へ更新されます。
- 行ホバーのハイライト
  - 右ペインの行上にマウスカーソルを置くと、その行の背景が薄くハイライトされ、狙いやすくなります。
