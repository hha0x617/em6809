# OS‑9 / NitrOS‑9 連携ガイド（日本語）

> **⚠️ 現状: ブート未対応**
> em6809 での OS‑9 / NitrOS‑9 のブートは**長期的な目標**として
> 位置付けていますが、**現時点ではまだ実現できていません**。
> 例えば OS‑9 のデバイスツリーから利用可能なディスクイメージ
> / マスストレージコントローラ表面、カーネルの協調マルチタス
> クに必要なハードウェア割込配線、検証済のブートスクリプト
> /モジュールロード経路、といった主要部分が未整備です。
>
> 以下の手順は、これらが揃った**完成形のワークフロー想定**を
> 記述したものです。チュートリアルではなくロードマップとして
> 読んでください。現時点では本書のコマンドそのままでは
> 実 NitrOS‑9 カーネルは起動しません。

> **商標について:** *OS‑9* は Microware Systems Corporation の
> 商標、*NitrOS‑9* は独立したオープンソースプロジェクトです。
> 本ドキュメント中の言及はすべて説明目的で、本プロジェクトは
> いずれとも提携・推奨・スポンサー関係を持ちません。詳細は
> リポジトリトップの README「商標について」節を参照してください。

NitrOS‑9/OS‑9 をエミュレータ上でブート/検証するための典型手順とコマンド例を示します。実際のモジュール構成・エントリは配布物/環境に合わせて調整してください。

## ブート実現に必要なタスク（Roadmap）

下表は、実 NitrOS‑9 カーネルを em6809 上で起動させるために必要と判断
している作業の整理です。本ドキュメントの後段「ブート手順（例）」は、
これらが揃った時点で機能する想定のワークフローです。

### A. ハードウェア（チップ・エミュレーション層）

| ID | 項目 | 現状 | 備考 |
|---|---|---|---|
| A‑1 | 実チップ準拠ディスクコントローラ | ✗ | 現在の `BlockDev` は em6809 独自レジスタ配置。OS‑9 RBF ドライバが期待する **WD1773 / WD1793 FDC** や **WD33C93 SCSI** の選択 + 実装、もしくは BlockDev 配置を target にする RBF ドライバの新規実装が必要 |
| A‑2 | OS‑9 期待レートの RTC ティック | △ | `TimerDev` 既存。50 / 60 Hz 設定 + IRQ vector 配線 + OS‑9 `D.Tick` への接続を要検証 |
| A‑3 | CPU 命令完全性 | △ | LBSR / MUL / CWAI / SYNC / SEX / ABX 等を NitrOS‑9 が活用。LEAS/LEAU swap / ABX / TST mem / SBC borrow‑in 等は修正済。残存バグは smoke test で洗い出し |
| A‑4 | SWI / SWI2 / SWI3 push order | ✓ (PR #17) | MC6809 spec 準拠を修正済。NitrOS‑9 ABI での再検証推奨 |

### B. OS‑9 / NitrOS‑9 アーティファクト

| ID | 項目 | 現状 | 備考 |
|---|---|---|---|
| B‑1 | em6809 hardware を target した OS‑9 配布 | ✗ | OS‑9 / NitrOS‑9 は機種固有 (Coco / Dragon / MM/1 等)。em6809 のメモリマップ + 周辺デバイス選択に合わせて **init / krn / krnp2 / デバイス記述子 (`/Term`, `/D0`) / デバイスドライバ (SCF, RBF) / ブートトラックレイアウト** をビルド or port する作業が必要 |
| B‑2 | OS‑9 module フォーマット対応 loader | ✗ | OS‑9 module は CRC / name pointer / type/lang/sys revs を含む固有ヘッダ。現状の loader は raw / S‑record 限定。**module‑aware loader** または boot stub で kernel entry を module ヘッダ経由で取得する仕組みが必要 |

### C. メモリマップ / ブートストラップ

| ID | 項目 | 現状 | 備考 |
|---|---|---|---|
| C‑1 | DAT / MC6829 MMU の OS‑9 L2 互換性 | △ | `Mc6829` 実装あり。NitrOS‑9 のページ操作シーケンス (page in/out, fork での map 複製) が正しく走るか突き合わせ要 |
| C‑2 | リセットベクタ + boot stub | △ | `$FFFE/$FFFF` から init module entry へ、`D.ModDir` 初期化、システムスタック設定 |
| C‑3 | 割込ベクタ配線 | △ | IRQ → console RDRF / disk DRDY / RTC tick の優先順位と disambiguate、FIRQ / NMI 用途整理 |

### D. 統合 / 検証

| ID | 項目 | 現状 | 備考 |
|---|---|---|---|
| D‑1 | boot smoke test | ✗ | kernel が boot disk から init を読める / shell プロセスが `$ ` プロンプト到達 / `dir` でディレクトリリスト表示 |
| D‑2 | プロセス切替検証 | ✗ | `procs` で複数プロセス表示、multi‑process 時の DAT 切替が安定 |
| D‑3 | パフォーマンス | ✓ (probably) | NetBSD boot で 44 MIPS / 256 MHz 実測 (Memory). 実用域 |

### 規模感

| ブロック | 規模 |
|---|---|
| A‑1 (FDC or RBF driver) | 中〜大 (1〜2 週間相当) |
| A‑2 / A‑3 / A‑4 | 各 小〜中 (各数日) |
| B‑1 (NitrOS‑9 port) | **大** (数週間以上、OS‑9 内部知識前提) |
| B‑2 (module loader) | 小〜中 |
| C‑1 (DAT 互換性) | 中 (バグ次第) |
| D (検証) | 中 (反復作業) |

**ボトルネックは B‑1**。NitrOS‑9 を em6809 hardware に target するには
OS‑9 ソースコード (NitrOS‑9 は MIT 系で公開) の理解と asm 修正が必要で、
ここが投入時間の最も読めない部分。

## 準備

- カーネルやドライバの S‑Record（例: `nitros9_kernel.s19`, `drivers.s19`）
- デバイスマップの雛形: [`em6809-core/docs/ja/os9_device_map_templates.md`](https://github.com/hha0x617/em6809-core/blob/main/docs/ja/os9_device_map_templates.md)
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

- デバイスマップの作例: [`em6809-core/docs/ja/os9_device_map_templates.md`](https://github.com/hha0x617/em6809-core/blob/main/docs/ja/os9_device_map_templates.md)
- ブートスクリプト雛形: `samples/bootscripts/README.md`, `samples/bootscripts/*.boot`
- 命令カバレッジ: [`em6809-core/docs/ja/instruction_status.md`](https://github.com/hha0x617/em6809-core/blob/main/docs/ja/instruction_status.md)

