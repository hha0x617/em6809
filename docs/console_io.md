# Console I/O 詳細ガイド

エミュレータのコンソール入出力を制御するオプションと使用例をまとめます。Windows では標準入力が raw モードに設定され、UTF‑8 出力は内部でバッファリングされます。

## 主なオプション

- `--console-stdin`: 標準入力を接続（ユーザーキーボード入力をゲストに送る）
- `--console-echo {ON|OFF}`: ローカルエコー（既定 OFF）
- `--console-in <TEXT>`: 文字列を即時送信（エスケープ可: `\r`, `\n`, `\t`, `\\`）
- `--console-out <FILE>`: コンソール出力をファイルへミラー
- `--console-tee-stderr`: 出力を `stderr` にもミラー
- `--console-flush-every {off|byte|<N>}`: フラッシュ間隔（既定: 改行ごと）

## 実用レシピ

- 1 回だけコマンドを送る:
  - `... --console-in "HELP\\r\\n"`
- 逐次入力（エコーあり）で操作する:
  - `... --console-stdin --console-echo ON`
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

