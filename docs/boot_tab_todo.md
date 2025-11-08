# Boot 用設定タブ（GUI）— TODO / 設計メモ

本ドキュメントは、Settings ダイアログに「Boot（ブートスクリプト）」タブを追加するための背景、課題、解決策案、要相談事項を整理したものです。

## 現時点で未実装の主な理由

- 設計上の分離: Settings はデバイス/表示などの「ランタイム設定」を扱う方針で、ブートスクリプトは「起動時シーケンスの自動化」。性質が異なり、混在を避けていた。
- 現状は CLI 主導: `--boot-script` / `--emit-boot-template` による CLI 入口のみで、GUI のコマンド系（`src/regview.rs`）にロード/適用/停止などの経路がない。i18n キーも未定義。
- DSL の拡張途上: `docs/dsl_actions_future.md` に将来の拡張案があり、GUI を先に固定化すると後方互換や UX 改修コストが増える懸念。
- 実行モデルの複雑さ: ブートスクリプトは `BootSequencer::on_pre_step/on_post_step`（`src/bootscript.rs`）でステップ実行にフック。途中適用/差し替え/停止には安全な一時停止、状態の引き継ぎやリセットの扱いが必要（Settings の Apply/Revert/Cancel との整合も課題）。
- エラー処理と UX: 解析エラーの表示、ファイル入出力、テンプレート適用、ログ連携など、ひと通りの GUI ワークフローを整える実装コストが今回の Settings 改修スコープ外だった。

## 解決策（提案）

1) タブとデータモデルの追加
- Settings に「Boot」タブを追加（`tab-boot`）。
- `SettingsState` に以下を追加（例）:
  - `boot_enable: bool`（セッションで有効）
  - `boot_script_path: String`（読み込み用）
  - `boot_editor_text: Option<String>`（インライン編集/プレビュー用・段階導入）
  - `boot_preset_name: String`（テンプレート名の選択肢）
  - `boot_log_triggers: bool`（トリガー発火ログの有無）

2) コマンド/ワーカー連携
- `regview::Command` に以下を追加:
  - `BootApplyFile { path: PathBuf }`
  - `BootApplyText { text: String }`
  - `BootEmitTemplate { name: String }`（エディタに流し込む）
  - `BootControlStart | BootControlPause | BootControlStop | BootReload`（任意）
- `main.rs` のワーカーループで `parse_boot_script` → `BootSequencer::new` を生成・保持し、既存の on_pre_step/on_post_step に統合。Stop 時は `None` に戻す。
- 適用時は一時停止→差し替え→再開の順で安全に更新（失敗時は UI にエラー表示し、既存状態は維持）。

3) UI/UX
- まずは「ファイルから読み込み + 適用/停止 + ログON/OFF」を実装（Phase A）。
- 次に「テンプレートを選択してエディタへ展開→編集→適用」（Phase B）。
- 最後に「起動時自動適用（永続化）」などのプリファレンス連携（Phase C）。
- Settings フッターの Apply/Revert/Cancel と整合:
  - Boot は「Apply で初めて sequencer を差し替え」・「Revert は UI の未適用変更を破棄のみ」・「Cancel は閉じるのみ」。
  - 実機能の適用/停止ボタンは Boot パネル内に独立して持たせる（即時操作の性格）。

4) i18n / ドキュメント
- `locales/en/ja` に Boot タブ/各種ボタン・ラベルのキーを追加。
- `docs/usage.md` に GUI からの適用手順・テンプレート一覧を追記。
- 既存サンプル（`samples/bootscripts/*.boot`）へ誘導するヘルプ本文を UI に表示。

5) ロギング/可視化
- 既存のログ出力へ「トリガー発火」をトレース（例: `on_pc 0xE000: con_ctrl 0x11`）。
- 任意で GUI 側に簡易ログパネル（将来のタスク）。

## TODO（実装タスク）

- [ ] UI: Settings に「Boot」タブ追加（自動サイズ/横幅制限は既存に準拠）。
- [ ] 状態: `SettingsState` に Boot 関連フィールドを追加、Open/Revert/Apply のベースライン連携。
- [ ] コマンド: `regview::Command` に Boot 系を追加、GUI→ワーカーへの送出を実装。
- [ ] ワーカー: `main.rs` の各ループに Boot 制御を追加（生成/差し替え/停止）。
- [ ] 解析/適用: `parse_boot_script` のエラーを UI で可視化（赤字/トースト等）。
- [ ] ファイルダイアログ: rfd 統合（最近ディレクトリの保持は任意）。
- [ ] テンプレート: `emit_boot_template(NAME)` を UI から利用（プリセットの一覧も UI 化）。
- [ ] i18n: `locales/en/ja` へキー追加（タブ名、読み込み、適用、停止、テンプレート、ログ等）。
- [ ] ドキュメント: `docs/usage.md` に GUI 操作手順を追記、サンプルへのリンクを追加。
- [ ] 手動検証: OS‑9 サンプルで発火ログ/挙動を確認（Pause→Apply→Resume の分岐も確認）。
- [ ] 将来: インラインエディタ（シンタックス強調は任意）・起動時自動適用（永続化）。

## 要相談事項（設計ポリシー）

- 永続化の範囲: 「前回の Boot 設定を起動時に自動適用」するか（プロファイル/プロジェクト単位の構想があるか）。
- インライン編集の是非: 当面はファイル読み込みのみか、簡易エディタも併設するか（誤適用防止のため Apply のみ反映に統一するか）。
- 実行中の差し替え: 適用時に自動で一時停止→差し替え→再開まで行うか、ユーザー操作を前提にするか。
- リセット/巻き戻し: 新規スクリプト適用時にリセット（PC/メモリ初期化）を提供するか。GUI に「Reset」動作を追加するか。 
- テンプレートの露出範囲: どのテンプレート名を UI へ出すか（`os9l2-boot`/`os9l2-io`/`os9l2-full` など）。
- ログの粒度: トリガー発火の詳細（PC/Step/アクション）をどの程度出すか。GUI パネル化の要否。
- 競合回避: Settings の Apply/Revert/Cancel と Boot タブ内の「即時適用/停止」操作の優先順位・表記統一。
- スレッド安全性: ワーカーとの同期方式（既存のコマンドキュー準拠で良いか、排他制御の改善が必要か）。

## フェーズ分割（実装順）

- Phase A: Boot タブ（読み込み/適用/停止/ログトグル）、テンプレートの流し込み、エラー表示。
- Phase B: インラインエディタ、テンプレート選択、保存/再読込、UI トースト/ガイド強化。
- Phase C: 設定の永続化（自動適用）、「Reset」動作の導入、ログパネル（任意）。

## 参照

- CLI 入口: `src/main.rs`（`--boot-script` / `--emit-boot-template`）
- 解析/実行: `src/bootscript.rs`（`parse_boot_script` / `BootSequencer`）
- GUI: `src/regview.rs`（Settings ダイアログ、コマンドキュー）
- サンプル: `samples/bootscripts/*.boot`
- 将来拡張: `docs/dsl_actions_future.md`

