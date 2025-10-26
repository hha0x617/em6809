# Instruction Implementation Status (MC6809)

This document summarizes the current coverage in `src/cpu.rs` and highlights areas recently implemented.

## Implemented Highlights
- Conditional branches (short): BRA, BNE, BEQ, BRN, BHI, BLS, BCC/BHS, BCS/BLO, BVC, BVS, BPL, BMI, BGE, BLT, BGT, BLE
- Conditional branches (long, 0x10): LBcc (all via `cond_eval`)
- Y/CPY/LDY/STY (page 2) variants:
  - LDY: immediate/direct/indexed/extended (0x10 8E/9E/AE/BE)
  - STY: direct/indexed/extended (0x10 9F/AF/BF)
  - CMPY: immediate/direct/indexed/extended (0x10 8C/9C/AC/BC)
- U/CMPS (page 3) variants:
  - CMPU: immediate/direct/indexed/extended (0x11 83/93/A3/B3)
  - CMPS: immediate/direct/indexed/extended (0x11 8C/9C/AC/BC)
- S-register ops (page 2):
  - LDS: immediate/direct/indexed/extended (0x10 CE/DE/EE/FE)
  - STS: direct/indexed/extended (0x10 DF/EF/FF)
- Many core ALU/load/store/indexed addressing modes are present for A/B/X/Y/U and extended/direct/indexed forms.

## Known Gaps / Behavior Notes
- Any 0x10/0x11 sub-opcode not listed above will fall back to `UNIMPL-PFX` (trace-visible). These can be implemented on demand.
- The default `UNIMPL` path for unknown base opcodes remains as a safety fallback.
- Trace strings are concise; if you need mnemonic-rich traces for specific opcodes, open an issue.

## Quick Trace Verification
- Use `samples/traces/trace_all.asm` to exercise the implemented groups.
- Build: `lwasm -9 --format=srec -o trace_all.s19 trace_all.asm`
- Run: `mc6809emu trace_all.s19 --format srec --trace --step 800`

If you find an opcode landing in UNIMPL paths, please share the opcode and addressing form; we will prioritize adding it.
## 運用向け詳細（カバレッジ・表記・検証）

### 概要

- 本エミュレータは MC6809 の主要命令・アドレッシングモードをカバー。分岐（短/長）、A/B/D レジスタ演算、X/Y/U/S のロード/ストア、メモリシフト/クリア/ジャンプ、各種コントロール命令を実装。
- 逆アセンブラはページ 0x00/0x10/0x11 を網羅し、被演算子（即値/ダイレクト/インデックス/拡張）と間接表記、分岐オフセット→ターゲットの可視化を統一形式で出力。

### 逆アセンブルの表記仕様

- 即値: `#${nn}`（例: `#${10}`）
- ダイレクト/拡張: `${addr}`（例: `${E000}`）
- インデックス: `±off,REG`（例: `-4,X` / `8,U`）
- 間接: 上記被演算子を角括弧で包む（例: `[10,X]`、`[${E040}]`）
- 分岐: `Bcc/LBcc offset -> $${target}` として相対→絶対を併記

### 命令カバレッジ（サマリ）

- 分岐
  - 短分岐 `Bcc` 全種（`BRA`, `BRN`, `BHI`, `BLS`, `BCC`, `BCS`, `BNE`, `BEQ`, `BVC`, `BVS`, `BPL`, `BMI`）
  - 長分岐 `LBcc` 全種（ページ 0x10/0x11）
  - `BSR`/`LBSR`（サンプルは LBSR に置換済みでオーバーフロー防止）
- 比較・ロード/ストア
  - `CMPA/B`, `SUBA/B`, `ADCA/B`, `ADDA/B`, `SBCA/B`, `ANDA/B`, `ORA/B`, `EORA/B`, `BITA/B`
  - `LDA/B/D`, `STA/B/D`, `LDX/U`, `STX/U`, `LDY/S`, `STY/S`, `CPX`, `CPY`, `CMPU`, `CMPS`, `LDS`, `STS`
  - アドレッシング: 即値/ダイレクト/インデックス/拡張（各命令の規定モードに準拠）
- メモリ操作・シフト/ローテート/テスト
  - `ASL/ASR/LSL/LSR`, `ROL/ROR`, `TST`, `CLR`, `COM`
  - 対象: ダイレクト/インデックス/拡張、累積器版（A/B）該当種
- 転送・合成・制御
  - `JMP`（D/X/Y/U/S/拡張/インデックス/間接対応），`JSR`（各モード）
  - `LEAX/Y/U/S`, `PSHS/U`, `PULS/U`, `EXG`, `TFR`
  - `SYNC`, `CWAI`, `MUL`, `DAA`, `SEX`, `ABX`, `ANDCC`, `ORCC`, `SWI/SWI2/SWI3`

### カバレッジ行列表（運用確認用）

累積器演算（A/B）系とロード/ストアは、各アドレッシングでの実装状況を以下に示します（✓=実装、—=対象外）。

| グループ | 命令セット | Imm | Direct | Indexed | Extended |
|---|---|:--:|:--:|:--:|:--:|
| 8-bit 演算A/B | `ADDA/B`, `ADCA/B`, `SUBA/B`, `SBCA/B` | ✓ | ✓ | ✓ | ✓ |
| 論理/ビット | `ANDA/B`, `ORA/B`, `EORA/B`, `BITA/B` | ✓ | ✓ | ✓ | ✓ |
| 比較A/B | `CMPA/B` | ✓ | ✓ | ✓ | ✓ |
| ロード | `LDA/B`, `LDD` | ✓ | ✓ | ✓ | ✓ |
| ストア | `STA/B`, `STD` | — | ✓ | ✓ | ✓ |
| インデクス | `LDX/U`, `STX/U` | ✓/— | ✓ | ✓ | ✓ |
| インデクス | `LDY/S`, `STY/S` | ✓/— | ✓ | ✓ | ✓ |
| 比較 | `CPX`, `CPY`, `CMPU`, `CMPS` | ✓ | ✓ | ✓ | ✓ |
| スタック | `LDS`, `STS` | ✓/— | ✓ | ✓ | ✓ |

メモリ操作と制御系の実装状況（対象外は —）。

| グループ | 命令セット | Direct | Indexed | Extended |
|---|---|:--:|:--:|:--:|
| メモリ操作 | `TST`, `CLR`, `COM` | ✓ | ✓ | ✓ |
| シフト/ローテート | `ASL/LSL`, `ASR/LSR`, `ROL`, `ROR` | ✓ | ✓ | ✓ |
| ジャンプ/サブルーチン | `JMP`, `JSR` | ✓ | ✓ | ✓ |

分岐/ページ関連の実装状況。

| 項目 | 内容 | 状態 |
|---|---|---|
| 短分岐 | `Bcc` 全種 | ✓ |
| 長分岐 | `LBcc` 全種（0x10/0x11） | ✓ |
| サブルーチン | `BSR`/`LBSR` | ✓ |
| 割込み | `SWI`, `SWI2`, `SWI3` | ✓ |

注記

- `Imm` は即値、`Indexed` は全バリアント（5-bit/8-bit/16-bit 変位、間接を含む）を網羅する方針。
- `LDX/U` と `LDY/S` の Imm 列は、対応命令のみ ✓（一部は Immediate を持たないため `✓/—` と表記）。
- サイクル数は命令主体で整備済み。メモリ待ちなどプラットフォーム依存の遅延は未モデル化。

### ページ 0x10/0x11 の補足

- 長分岐 `LBcc` は全実装。`SWI2`/`SWI3`（0x10/0x11 ページ）をサポート。
- 逆アセンブラはページ接頭辞を認識し、統一オペランド表記で出力。

### フラグと副作用（実装ポリシ）

- N/Z/V/C: 公式定義に準拠。`DAA/MUL/CWAI` 等は仕様通り。
- 比較系（`CMP*`, `CP*`）は減算相当のフラグ生成でデータ変更なし。
- `ANDCC/ORCC` は CCR をビットマスクで更新。`CWAI` は CCR と待機に関する状態を処理。

### 既知の仕様・表示

- 分岐は「相対オフセット」と「絶対アドレス」を併記し、トレースでは解決後の PC を表示。
- インデックス間接は角括弧表記。`D` を使うオフセットなど複合形式も統一。

### 想定ギャップ（要確認）

- 一部の命令で未使用/予約パターン（Illegal/Unimplemented）が残る可能性
- サイクル数の厳密性は「命令主体」で整備済み。メモリウェイト/周辺待ちのモデル化は未実装
- 間接 + 変位の境界条件（ページ切替、ベクタ領域）要追加テスト

### 検証手順（運用）

- トレースサンプル（`samples/traces/trace_all*.s19`）を `--run --trace` で実行し、マーカー箇所でフラグ/レジスタを点検
  - 例: `target/release/mc6809emu --run --srec samples/traces/trace_all.s19 --trace`
- カバレッジ確認（静的）
  - 逆アセンブラで対象 ROM 範囲を走査: `... --disasm 0xE000:0xFFFF`
  - 分岐ターゲットの妥当性は `--trace` と併用しイベント順に検証
- 実行開始位置の確認
  - `--pc` 指定が最優先。未指定時はリセットベクタ→SREC entry（無効時）→最初のロード領域先頭

### 変更履歴への反映（メンテ規約）

- 新規命令の実装時は本ファイルの「命令カバレッジ」と「フラグ・副作用」節を更新
- 逆アセンブラの表記変更は README の「表記仕様」と本ファイルの「表記仕様」を同時更新
- サンプル/トレースに命令が追加された場合は、README のサンプルコマンドと本ファイルの検証手順も更新
