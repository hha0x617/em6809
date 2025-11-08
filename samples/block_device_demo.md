# Block Device Sample (MC6809)

This sample exercises the emulatorの 512 バイト/セクタのブロックストレージデバイス。

## ファイル

| ファイル | 役割 |
| --- | --- |
| `block_device_read.asm` | ブロックデバイスから LBA 0 を読み出し、受信した文字列をコンソールへストリームするサンプル。 |
| `block_device_write.asm` | LBA 1 へ長さ付きペイロードを書き込み、その後に読み出し結果をコンソールへ表示するサンプル。 |
| `block_device_image.bin` | 読み出しサンプル用の 512 バイトイメージ。先頭 1 バイトが長さ、続く領域に ASCII テキストを格納しています。 |

## ビルド方法

他のサンプル同様に [lwtools](http://www.lwtools.ca/) の `lwasm` を利用してください。

```bash
# 読み出しデモ
lwasm -9 --format=srec -o block_device_read.s19 block_device_read.asm
# 書き込みデモ
lwasm -9 --format=srec -o block_device_write.s19 block_device_write.asm
```

## 実行例 (読み出し)

```bash
cargo run -- samples/block_device_read.s19 \
  --block 0xFF20 \
  --block-file samples/block_device_image.bin \
  --console 0xFF00 \
  --console-echo \
  --run
```

処理の流れ:

1. ゲストが `$FF20` のブロックデバイスレジスタへ 1 セクタ読み込み (LBA0) を要求します。
2. `STATUS` レジスタの `DRDY` ビットが立つまでポーリングします。
3. `DATA` ポートの 1 バイト目を長さとして受け取り、その回数だけ文字を読み出してコンソールへ送信します。
4. バッキングファイル (`block_device_image.bin`) の先頭セクタには、先頭 1 バイトに長さ、続く領域に ASCII メッセージが入っているため、即座にコンソールにテキストが現れます。

### カスタマイズ

- `block_device_image.bin` を編集して別のセクタ内容を用意できます (512 バイト単位を維持してください)。
- アセンブリの `BLOCK_LBA` 定義を変更すれば、別の LBA を読むようにできます。
- IRQ/FIRQ を有効にすると、`STATUS` の `DRDY` ビットで割り込みを受けてストレージ完了を通知するサンプルに拡張できます。

## 実行例 (書き込み)

書き込みサンプルでは、LBA 1 をターゲットに 512 バイトのセクタを丸ごと更新します。任意の書き込み先ファイル (存在しない場合は自動作成) を `--block-file` で指定してください。

```bash
BLOCK_IMG=/tmp/block_write_demo.bin
cargo run -- samples/block_device_write.s19 \
  --block 0xFF20 \
  --block-file ${BLOCK_IMG} \
  --console 0xFF00 \
  --console-echo \
  --run
```

コンソールに「Writing payload...」「Write complete, reading back:」と順に表示され、その後読み戻したペイロードが出力されます。書き込まれたセクタは、例えば以下のように確認できます (先頭バイトが長さ、以降が ASCII):

```bash
hexdump -C -n 64 ${BLOCK_IMG}
```

同じファイルを読み出しデモ (`block_device_read.s19`) に渡すことで、書き込み結果を GUI から再確認することも可能です。
