# トラブルシューティング

よくある詰まりや見落としに対する対処法をまとめます。

## 出力が途切れる/遅い

- `--console-flush-every 128` など、フラッシュ間隔を増やしてバッファリングを強化
- 大量ログは `--console-out <FILE>` でファイルにミラー

## 入力が反映されない

- `--console-stdin` を付ける
- ローカルエコーが必要なら `--console-echo`
- OS‑9/デバイス初期化の順番を確認（必要に応じてトレース機能を使用）

## 開始アドレスが意図通りでない

- 最優先は `--pc`。未指定時は「リセットベクタ > SREC entry（無効時）> 最初のロードレンジ先頭」

## 文字化け/文字入力の問題

- 端末の文字コードを UTF‑8 に設定
- Windows: キーバインドや IME の干渉に注意（Ctrl+C は終了）

## macOS の GUI について

- Terminal.app や iTerm2 からの実行を推奨します（GUI はメインスレッド上で起動されます）。
- 日本語ラベルや出力が豆腐になる場合は、`EM6809_GUI_FONT` に日本語フォントの絶対パスを指定してください（例: `/System/Library/Fonts/Hiragino Sans W6.ttc`）。未設定時は自動検出を試みます。
- HiDPI（Retina）環境では eframe の既定スケーリングが適用されます。表示倍率が気になる場合は macOS のシステム設定で調整してください。
- まれにウィンドウが開かない/すぐ閉じる場合は、標準出力・標準エラーのログを確認し、`cargo clean && cargo run --release` で再ビルドを試してください。

## WSL2 / Linux の GUI について

WSL2 + WSLg（Wayland）の環境で GUI を表示するための準備と手順まとめです。

- 依存パッケージ（Ubuntu/Debian 系）
  - Wayland + GL 最低限: `libxkbcommon0 libwayland-client0 libwayland-cursor0 libwayland-egl1 libegl1-mesa libgles2`
  - X11（XWayland）フォールバック用: `libx11-6 libxi6 libxcursor1 libxrandr2 libxinerama1`
  - 動作確認ツール（任意）: `mesa-utils`（`glxinfo -B` で GL 確認）
  - 一括インストール例:
    - `sudo apt update && sudo apt install -y libxkbcommon0 libwayland-client0 libwayland-cursor0 libwayland-egl1 libegl1-mesa libgles2 libx11-6 libxi6 libxcursor1 libxrandr2 libxinerama1 mesa-utils`

- ビルド（GUI 有効）
  - `cargo build --release --features gui`

- 実行時の環境変数（WSLg/Wayland 推奨）
  - `export XDG_RUNTIME_DIR=/mnt/wslg/runtime-dir`（WSLg で Wayland ソケットを参照できるようにする）
  - `export WINIT_UNIX_BACKEND=wayland`（winit を Wayland バックエンドで動かす）
  - `export WGPU_BACKEND=gl`（wgpu のバックエンドに OpenGL を使用）
  - 実行例:
    - `XDG_RUNTIME_DIR=/mnt/wslg/runtime-dir WINIT_UNIX_BACKEND=wayland WGPU_BACKEND=gl cargo run --release --features gui -- <image> --paused --dump-regs`

- X11（XWayland）フォールバック（Wayland が不調な場合）
  - `export WINIT_UNIX_BACKEND=x11`
  - （必要なら）`export DISPLAY=:0`（WSLg なら既定で設定されていることが多い）
  - 実行例: `WINIT_UNIX_BACKEND=x11 WGPU_BACKEND=gl cargo run --release --features gui -- <image> --paused --dump-regs`

- 旧来の WSL（WSLg なし）での代替（参考）
  - Windows 側に X サーバ（VcXsrv/X410 など）を起動し、WSL で `DISPLAY` を設定:
    - `export DISPLAY=$(awk '/nameserver/{print $2; exit}' /etc/resolv.conf):0`
    - `export WINIT_UNIX_BACKEND=x11`
    - 必要に応じて `export LIBGL_ALWAYS_INDIRECT=1`

- 動作確認（目安）
  - `echo $WAYLAND_DISPLAY` が `wayland-0` など非空であること
  - `echo $DISPLAY` が設定されていること（XWayland フォールバック）
  - `glxinfo -B` で OpenGL 実装が表示されること（`mesa-utils` に含まれる）

- 既知の注意点
  - Wayland 環境で `XDG_RUNTIME_DIR` が未設定だと GUI 起動に失敗する場合があります（WSLg では通常 `/mnt/wslg/runtime-dir`）。
  - バックエンド切替（`WINIT_UNIX_BACKEND=wayland|x11`）と `WGPU_BACKEND=gl|vulkan` の組み合わせで改善することがあります。
  - マルチビューポートの挙動を調べる際は `RUST_LOG=egui_winit=debug` を付けて実行すると、`ViewportCommand::InnerSize ignored by winit` などの詳細ログを確認できます（Linux でのネイティブ Settings の描画不具合調査に有効）。
