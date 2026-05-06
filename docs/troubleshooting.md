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

### ZINK 初期化失敗時の workaround（GUI が起動しない / 起動直後に異常終了）

WSLg 上で OpenGL コンテキストの初期化に失敗するケースがあります。Windows 側の GPU ドライバ更新、スリープ復帰、Mesa パッケージ更新などのタイミングで顕在化しやすく、em6809 固有の問題ではなく WSLg 全体に影響します。

- **症状（stderr に出力される警告/エラー）**
  ```
  libEGL warning: failed to get driver name for fd -1
  libEGL warning: MESA-LOADER: failed to retrieve device information
  MESA: error: ZINK: failed to choose pdev
  libEGL warning: egl: failed to create dri2 screen
  Io error: Broken pipe (os error 32)
  [regview] GUI error: winit EventLoopError: Exit Failure: 1 — running headless
  ```
  もしくは起動はするものの数秒で異常終了する。

- **原因の概略**: WSL2 の `/dev/dri/` は通常存在せず、WSLg は ZINK (Vulkan ベースの OpenGL 実装) → D3D12 経由でホストの GPU にブリッジしている。このパイプラインのどこかで初期化に失敗すると、Mesa がデバイスを見つけられず GL コンテキスト作成が失敗する。

- **ワークアラウンド: ZINK を経由せず llvmpipe（CPU レンダリング）に切替える**
  ```bash
  export LIBGL_ALWAYS_SOFTWARE=1
  export GALLIUM_DRIVER=llvmpipe
  cargo run --release
  ```

- **永続化（推奨）**: `~/.bashrc` に追記すれば新しい shell でも自動で適用される
  ```bash
  cat >> ~/.bashrc <<'EOF'
  export LIBGL_ALWAYS_SOFTWARE=1
  export GALLIUM_DRIVER=llvmpipe
  EOF
  source ~/.bashrc
  ```

- **トレードオフ**: GPU 加速が無効になる。ただし em6809 は egui ベースで描画負荷が小さく、体感差はほぼないと報告されている。

- **一時的に GPU 加速を試したい場合**: `unset LIBGL_ALWAYS_SOFTWARE GALLIUM_DRIVER` で外して再実行。

- **補助的な復旧手順**（上記でも改善しない場合）
  - PowerShell から `wsl --shutdown` → 数秒待ってから WSL terminal を開き直す。
  - Mesa の再インストール: `sudo apt install --reinstall -y libgl1 libegl1 libglx-mesa0 libgl1-mesa-dri`
  - `glxinfo -B` で `direct rendering: Yes` と llvmpipe が表示されれば OK（`mesa-utils` 必須）。
