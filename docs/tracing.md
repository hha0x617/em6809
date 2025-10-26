# トレース/デバッグと逆アセンブル

実行中の命令・レジスタ・分岐先の可視化、ステップ実行、逆アセンブルと 16 進ダンプの使い方をまとめます。

## 実行トレース

- 実行と同時にトレースを表示:
  - `target/release/mc6809emu --run --srec path/to/rom.s19 --trace`
- ステップ実行で 1 命令ずつ確認:
  - `target/release/mc6809emu --srec path/to/rom.s19 --step`

分岐は「相対オフセット」と「絶対アドレス」を併記し、レジスタ/CCR の変化を追跡できます。

## 逆アセンブル

- メモリ範囲を指定して逆アセンブル:
  - `target/release/mc6809emu --disasm 0xE000:0xE100`
- 実行開始周辺だけ確認（例: 0xE000〜0xE050）:
  - `... --disasm 0xE000:0xE050`

表記仕様（抜粋）

- 即値: `#${nn}`、ダイレクト/拡張: `${addr}`、インデックス: `±off,REG`、間接: `[...]`
- 分岐: `Bcc/LBcc offset -> $${target}` として表示

## 16 進ダンプ

- メモリ範囲を 16 進で表示:
  - `target/release/mc6809emu --hex 0xE000:0xE040`

## サンプル ROM と検証

- トレースサンプル（マーカー付き）:
  - `target/release/mc6809emu --run --srec samples/traces/trace_all.s19 --trace`
- START から始めるバリアント:
  - `target/release/mc6809emu --run --srec samples/traces/trace_all_startfirst.s19 --trace`

命令カバレッジやフラグの扱いは `docs/instruction_status.md` を参照してください。

