# Console I/O 詳細ガイド

エミュレータのコンソール入出力を制御するオプションと使用例をまとめます。Windows / Linux / macOS いずれの環境でも、`--console-stdin` 指定時は標準入力を raw（逐次）モードに設定し、押下ごとにゲストへ転送します。UTF‑8 出力は内部でバッファリングされ、改行または設定に応じてフラッシュされます。

## 主なオプション

- `--console-stdin`: 標準入力を接続（ユーザーキーボード入力をゲストに送る）
- `--console-echo`: ローカルエコーを有効化（既定 OFF）
- `--console-in <TEXT>`: 文字列を即時送信（エスケープ可: `\r`, `\n`, `\t`, `\\`）
- `--console-out <FILE>`: コンソール出力をファイルへミラー
- `--console-tee-stderr`: 出力を `stderr` にもミラー
- `--console-flush-every {off|byte|<N>}`: フラッシュ間隔（既定: 1 = 毎バイト、最大 128）
- `--console-flush-newline`: 改行検出時にもフラッシュ（追従出力向け）
- `--console-rx-watermark <N>`: 受信バッファに N バイト以上たまったときに IRQ をアサートするウォーターマーク。0 なら 1 バイト受信で即 IRQ。
- `--console-irq-hold <N>`: IRQ をアサートしてから N クロック保持するホールド時間。短いパルスを取りこぼしたくない場合に非 0 を指定。0 の場合、イベント後すぐに IRQ を落とす。

## 実用レシピ

- 1 回だけコマンドを送る:
  - `... --console-in "HELP\\r\\n"`
- 逐次入力（エコーあり）で操作する:
  - `... --console-stdin --console-echo`
- ログをファイル保存しながら画面にも表示:
  - `... --console-out session.log --console-tee-stderr`
- 出力が詰まる場合はバッファリングを増やす:
  - `... --console-flush-every 128`
- タイトなタイミング検証で毎バイトフラッシュ:
  - `... --console-flush-every byte`

## トラブルシューティング

- 入力が反映されない: `--console-stdin` と `--console-echo` を確認
- 文字化け: 実ホストの端末の文字コード/フォント設定を確認（UTF‑8 推奨）
- キー割り込み: Ctrl+C はエミュレータを終了します
