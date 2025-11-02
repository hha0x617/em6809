# OS‑9 / NitrOS‑9 連携ガイド

NitrOS‑9/OS‑9 をエミュレータ上でブート/検証するための典型手順とコマンド例を示します。実際のモジュール構成・エントリは配布物/環境に合わせて調整してください。

## 準備

- カーネルやドライバの S‑Record（例: `nitros9_kernel.s19`, `drivers.s19`）
- デバイスマップの雛形: `docs/os9_device_map_templates.md`
- ブートスクリプトの雛形: `samples/bootscripts/*.boot`

## ブート手順（例）

1. S‑Record をロードし、標準入力を接続
   - `target/release/em6809 path/to/nitros9_kernel.s19 --format srec --console-stdin --console-echo`
2. 追加モジュールがある場合は順次ロード
   - 必要に応じて追加モジュールを読み込みます（追加入力や GUI 操作で対応）。
3. 開始位置を制御（必要なら PC 指定）
   - `... --pc 0xE000`
4. 初期トレースで入出力/分岐を確認

## デバッグのポイント

- 逆アセンブルで先頭 256B を確認: `... --disasm 0xE000:0xE100`
- 入力が反映されない場合: `--console-stdin` とデバイス初期化の順序を確認
- 出力が詰まる場合: `--console-flush-every 128` でバッファリングを調整

## 参考資料

- デバイスマップの作例: `docs/os9_device_map_templates.md`
- ブートスクリプト雛形: `samples/bootscripts/README.md`, `samples/bootscripts/*.boot`
- 命令カバレッジ: `docs/instruction_status.md`

