# 設定とブートスクリプトの相互作用（日本語）

em6809 ではデバイス挙動を 2 つのレイヤで形成できます:

- **設定 (Configuration)** — `config.toml` + コマンドライン引数。
  起動時に **1 度だけ**適用される静的セットアップ。
- **ブートスクリプト (Boot script)** — `--boot-script PATH`
  (`.boot` ファイル)。実行中に PC / ステップトリガで適用される
  **動的な runtime tweak**。

両者は同じレジスタ (MMU mode、コンソール制御、ブロックデバイスの
IRQ 配線等) をターゲットにできるため、本ドキュメントでは **実行
順序**、**重なり領域**、両者の食い違いから生じる**失敗モード**を
整理します。

各レイヤの詳細は以下を参照:

- 設定: `docs/settings_persistence.md`
- ブートスクリプト (DSL とサンプル):
  `samples/bootscripts/README.md`、`samples/bootscripts/*.boot`、
  および計画メモ `docs/boot_tab_todo.md`

なお本ファイルは `docs/en/config_and_boot_script.md` (canonical)
の日本語版です。仕様変更時は英語版が先に更新されます。

## 起動シーケンス

順序は固定で、`src/main.rs` の 484〜1585 行付近 (セットアップ) と
4400〜5500 行付近 (ランループ) で見ることができます:

```
Phase 1 — 静的セットアップ (決定的)
─ CLI パース
─ prefs::load()                       APPDATA / XDG / Library から config.toml
─ Effective = resolve(CLI > Persisted > Defaults)
─ Effective を使って Bus + デバイスをアタッチ:
    console (base / stdin / echo / firq / rx_wm / irq_hold / …)
    block   (base / file / irq / firq / irq_hold)
    timer   (base / reload / irq / firq)
    gpio    (base / bits)
    mmu     (regs_base / phys_bytes / mode / map / attr / prot)
─ イメージロード (S-Record / BIN)

Phase 2 — ブートスクリプト準備 (オプション)
─ if --boot-script: parse → BootSequencer::new()
    (この時点ではトリガーリストが構築されるだけ、実行はされない)

Phase 3 — ランループ (動的、毎ステップ)
─ loop:
    boot_seq.on_pre_step(bus, mmu_regs_base, pc, cpu)
        ↓ PC / ステップトリガを発火、Action を実行
    cpu.step()
    boot_seq.on_post_step(bus, mmu_regs_base)
```

設定はブートスクリプトより**必ず先に**適用されます — レースや
順序の曖昧さはありません。両者が干渉するのは、ブートスクリプトの
`Action` が、設定で初期化済みのレジスタに対して **runtime 書き込み**
を行うときです。

## 干渉マトリクス

各ブートスクリプト Action と、それが上書きしうる設定項目:

| Boot-script Action               | 書込先                          | 対応する設定項目                                 | 重なり |
| -------------------------------- | ------------------------------- | ------------------------------------------------ | ------ |
| `mode system` / `mode user`      | `mmu_regs_base + 0x11`          | `--mmu-mode`                                     | ✓ runtime で上書き |
| `prot 0xNN`                      | `mmu_regs_base + 0x12`          | `--protect-vectors`、`--prot`                    | ✓ |
| `mmu_map page frame`             | `mmu_regs_base + page`          | `--mmu-map`、`--mmu-map-file`、`--preset`        | ✓ (意図的な段階的マップ展開で常用) |
| `attr page val`                  | `mmu_regs_base + 0x18 + page`   | `--attr`                                         | ✓ |
| `con_ctrl V`                     | console 制御レジスタ            | `[console]` / `--console-*`                      | ✓ |
| `con_rx_wm N`                    | console RX watermark            | `--console-rx-watermark`                         | ✓ |
| `con_irq_hold N`                 | console IRQ hold                | `--console-irq-hold`                             | ✓ |
| `con_firq on / off`              | console IRQ ルーティング        | `[console].firq`                                 | ✓ |
| `blk_irq on / off`               | block デバイス IRQ 有効化       | `[block].irq_enable`、`--block-irq`              | ✓ |
| `blk_firq on / off`              | block デバイス FIRQ ルーティング | `[block].firq`、`--block-firq`                   | ✓ |
| `blk_irq_hold N`                 | block デバイス IRQ hold         | `[block].irq_hold`                               | ✓ |
| `irq_mask on / off`              | CPU `CC.I` フラグ               | (CC は設定項目ではない)                          | runtime のみ |
| `firq_mask on / off`             | CPU `CC.F` フラグ               | (CC は設定項目ではない)                          | runtime のみ |

重なるケースの意味論はすべて **last write wins** (最後の書き込み
が勝つ): 設定値は Phase 1 から有効で、対応する boot-script Action
が Phase 3 で発火した時点で上書きされます。

ほとんどの場合これは意図的です — 設定で妥当な静的ベースを敷き、
boot script が適切なタイミング (例: カーネルのマップが揃ったあとで
`mode system` から `mode user` へ切替) で調整するというパターン。
下記の落とし穴はあくまで「boot script が設定を上書きすることに気付
いていない」ケースを指します。

## 既知の落とし穴

### A. 暗黙の上書き (メンタルモデルとの乖離)

ユーザが Settings 経由で値を永続化 (例: `console.firq = false`)
し、それが run の正本だと思っている状態で、`con_firq on` を含む
boot script を読ませると、IRQ ルーティングが実行中に黙って反転し
ます。設定ファイル自体は無変更ですが、runtime の挙動が異なる。

*現状の緩和策:* なし。デバッグ時は両レイヤを読む必要がある。

*将来的な改善案:* (まだ未実装の) Settings GUI の Boot タブで、
「この boot script が変更するフィールド:」を視覚化すれば関係が
見えるようになる。

### B. Silent no-op (無効化されたデバイスを boot script が叩く)

`console.enable = false` (または対応する CLI フラグ未指定) で console
がアタッチされていない状況で、`con_ctrl 0x11` を含む boot script
が走ると、`bootscript.rs::apply_action` の
`bus.as_any_mut().downcast_mut::<IoBus<…>>()` は `Some` を返します
(IoBus 自体はある)。続く `with_console_mut(...)` 呼出は console
が存在しない場合 no-op となり、**警告ログは一切出ません**。

外見上の症状は「boot script は走っているはずなのに何も変わらない」。

*現状の緩和策:* なし。boot script がターゲットとするデバイスが
すべて設定で有効化されていることを事前に確認する。

*将来的な改善案:* console / block 系の Action が、対応するデバイス
が無効な状態で初めて呼ばれた時に 1 度だけ warning ログを出す。

### C. MMU ベースアドレスの不整合 (メモリ破壊リスク)

`Action::Mode` / `Prot` / `Map` / `Attr` はすべて
`bus.write8(mmu_regs_base + offset, value)` を**無条件に**実行
します。MMU が**有効でない** (`mmu_regs_base = 0` あるいは MMU
I/O に対応しない値) 場合、この書き込みは **プレーン RAM の
`offset` に着弾**します。

たとえば `mmu_regs_base = 0` の状態で `prot 0xFF` を実行すると、
`0xFF` がアドレス `0x0012` (OS‑9 のダイレクトページ領域)に書き込
まれます。

*現状の緩和策:* runtime には無し。パーサも設定とのクロスチェック
は行わない。

*将来的な改善案:* `BootSequencer::on_pre_step` 冒頭で「MMU 系
Action があるが `mmu_regs_base` が妥当でない」状況を検出し、
warning を出すか Action をスキップする。

## 作成ガイドライン

`.boot` ファイルを書くときは、設定をその script の **動作前提
契約** として扱ってください:

1. boot script が触れるデバイスがすべて、実際の run で使う設定
   (CLI 引数 + `config.toml`) で有効化されていることを確認する。
   関連するベースアドレスは `--preset-list` や
   `--emit-boot-template` で確認できる。
2. MMU 系 script では `--mmu` を指定 (あるいは
   `mmu.enable = true` で永続化) し、script の想定 `regs_base` が
   実際の `--mmu-reg-base` と一致することを確認する。
3. 設定項目と重なる Action は **一時的な書き換え**として扱う —
   別の boot script (または boot script なし) で次回起動すると、
   設定の値に戻ります。
4. 長いシーケンスでは、`.boot` ファイルの先頭にコメントブロックで
   「想定する設定」を明示しておく。暗黙の依存関係を文書化する最も
   安価な方法。

## 関連ドキュメント

- `docs/settings_persistence.md` — 設定モデル、ファイル形式、
  CLI > saved > defaults の優先順位。
- `docs/boot_tab_todo.md` — 計画中の GUI Boot タブ。
  boot-script DSL の Action 一覧もここにある。
- `samples/bootscripts/README.md` — 動作する boot-script サンプル群。
- `src/bootscript.rs` — `BootSequencer` および `Action` の定義。
- `src/main.rs` — 実際の起動シーケンスとランループフック。
