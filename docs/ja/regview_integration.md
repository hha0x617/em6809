# レジスタビューア連携（GUI・日本語）

命令単位でレジスタを表示するライブウィンドウを提供します。Pause/Resume
ボタンで実行を一時停止できます。本ドキュメントは
`docs/en/regview_integration.md` の翻訳版です。`docs/en/` の英語版を
canonical とし、本ファイルはそれと並行して更新されます。

## Cargo の設定

`Cargo.toml` に以下を追加します:

```
[features]
# `--features gui` で GUI レジスタビューアをビルド
gui = ["eframe"]

[dependencies]
eframe = { version = "0.27", optional = true }
```

GUI を有効にしてビルド・実行:

- `cargo run --release --features gui -- path/to/rom.s19 --format srec --dump-regs`

## ビューアの組み込み

1) ビューアを取り込む（例: `src/main.rs` の冒頭）:

```rust
mod regview; // 先頭で
```

2) `--dump-regs` 指定時、CPU ループ開始前にウィンドウを起動:

```rust
#[cfg(feature = "gui")]
let viewer = regview::Viewer::spawn("MC6809 Registers");
#[cfg(not(feature = "gui"))]
let viewer = regview::Viewer::spawn(""); // GUI 無効時は no-op
```

3) 1 命令実行ごとにスナップショットを送り、Pause/Step を尊重します:

```rust
// CPU の現在状態からスナップショットを構築
let snap = regview::RegSnapshot {
    a: cpu.a,
    b: cpu.b,
    d: cpu.d(),           // あるいは ((cpu.a as u16) << 8) | (cpu.b as u16)
    x: cpu.x,
    y: cpu.y,
    u: cpu.u,
    s: cpu.s,
    dp: cpu.dp,
    pc: cpu.pc,
    cc: cpu.cc,           // ビット配置: E F H I N Z V C
};
viewer.push(snap);

// Pause 中で step が積まれていなければ待機。step があれば 1 回だけ進む
while viewer.should_pause() {
    std::thread::sleep(std::time::Duration::from_millis(10));
}
```

このスニペットは「1 命令実行する」呼び出し（例: `cpu.step()`）の直後に
入れてください。これでステップごとにウィンドウが更新されます。

## 振る舞い

- ビューアは `eframe` / `egui` を介して別ネイティブウィンドウとして動作します。
- Pause/Resume ボタンは命令境界で実行をトグルします。
- `--features gui` が無いビルドでは API は no-op となり、エミュレータは通常通り動作します。

## リスティングソースと Hex ビュー

- ロードしたイメージと同じディレクトリに `.lst` ファイルがあれば、それを右ペインに表示します。
- `.lst` が見つからない場合は、プログラムスパンの hex 形式逆アセンブルを自動生成してフォールバックとして表示します。
- 「Hex view」トグルで `.lst` と生成された hex 逆アセンブルをいつでも切り替えられます（`.lst` が存在する場合も切替可能）。
- hex バイト列の桁はそろえてあるため、ニーモニックが一貫した位置に並びます。

## Follow PC とハイライト

- 「Follow PC」は現在の PC を中心に表示し、対応する行をハイライトします。
- ステップ実行中・連続実行中ともにハイライトはリアルタイムで更新されます。
- リスティングはマウスカーソル下の行を薄くハイライト（hover）し、視認性とクリック精度を向上させます。

## 自己書換えコードへの対応（イベント駆動の再逆アセンブル）

- エミュレータはメモリバスを軽量な書込みトラッカーでラップしています。
- コード領域への書込みを検知した場合:
  - **Full span** モード: PC を含むスパン全体を 1 度再逆アセンブルし、リスティングを更新します。
  - **PC window** モード: 変更されたアドレス周辺の小さなウィンドウだけを再逆アセンブルし、対応行を in-place で差し替えます。
- 周期的なフルリフレッシュを避けることで、大きなプログラムでもスケールします。

## Span モードとウィンドウサイズ

- Span Mode は再逆アセンブルのコスト・レイテンシのトレードオフを制御します:
  - **Full span**: 単純で正確。コード変更時に PC スパン全体を再逆アセンブルします。
  - **PC window**: 高速。変更アドレス周辺のローリングウィンドウ（バイト数指定可）だけを再逆アセンブルします。
- ウィンドウサイズは可変（既定 256 バイト）。大きいほど命令境界のずれに強く、小さいほど高速です。

## コンテキストメニュー（リスティング行で右クリック）

- 行の先頭が 4 桁 hex アドレスで始まる場合、右クリックでコンテキストメニューが開きます:
  - **ここまで実行（Run to here）**: 実行を再開し、PC が該当アドレスに到達した時点で自動停止します。
  - **PC の値を現在のアドレスに変更（Set PC to this address）**: 即座に PC を該当アドレスに変更します。
  - **キャンセル（Cancel）**: メニューを閉じます。
- ラベル表記はロケールに応じて日本語/英語が切り替わります（後述「環境変数」参照）。
- 「ここまで実行」「PC を現在のアドレスに変更」を実行すると、レジスタペインとリスティングペインの両方が即座にリフレッシュされ、追加のステップなしで新しい PC とハイライトが反映されます。

## 環境変数

GUI ビューアの一部の挙動は環境変数で調整できます:

- `EM6809_GUI_FONT`: フォントファイル（`.ttf` / `.otf` / `.ttc`）の絶対パス。
  - 設定されかつ読み取り可能であれば、そのフォントを proportional / monospace の両ファミリの先頭にロードし、CJK（日本語）グリフが正しく描画されるようにします。
  - 未設定時は、適切なシステムフォント（Windows: Meiryo / Yu Gothic、macOS: Hiragino、Linux: Noto CJK）を自動検出して使用します。
- `EM6809_LANG`: リスティングペインのコンテキストメニューラベルのロケールを強制します。
  - `ja` → 日本語ラベル（「ここまで実行」「PCの値を現在のアドレスに変更」「キャンセル」）。
  - `en` → 英語ラベル（"Run to here"、"Set PC to this address"、"Cancel"）。
  - 未設定時は `LC_ALL` / `LC_MESSAGES` / `LANG` または OS 言語（Windows）から自動検出し、判別不能なら英語にフォールバックします。

About ダイアログのアイコン（Linux のみ）:

- `EM6809_ABOUT_ICON`: Linux で About ダイアログに表示する PNG 画像の絶対パス。
  - 設定されかつ読み取り可能であれば、PNG をロードし 32×32 にリサイズして表示します。
  - 未設定時は `assets/app_icon_32.png` → `assets/icon32.png` → `assets/icon.png` の順に探索します。
  - PNG が見つからない場合は、角丸の背景に小文字の "e" を描画するベクタフォールバックを表示します。
