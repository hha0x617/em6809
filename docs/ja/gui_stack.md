# GUI スタックメモ (eframe / egui)

このページは em6809 のデスクトップ GUI が eframe / egui のどの feature
構成で動いているか、そして直近の依存バンプ時に下した技術判断を残すための
コントリビュータ向けドキュメントです。エンドユーザは読む必要はありません。

## 現行バージョン

| Crate | バージョン | 備考 |
|-------|-----------|------|
| `eframe` | `0.34.x` | `default-features = false`。明示的に `default_fonts`, `glow`, `wayland`, `x11` を有効化。 |
| `egui_extras` | `0.34.x` | デフォルト feature のまま。 |
| `winit` | `0.30.x` | eframe 経由 + 一部 raw API のため直接依存。 |

`Cargo.toml` 上で `eframe` と `egui_extras` は同じ minor に揃える。
片方だけバンプするとリンク段階で落ちる。

## なぜ `default-features = false` + `glow` なのか

`eframe 0.34` のデフォルト feature は `wgpu` を有効化し、推移的に
`wgpu-hal 29` を引っ張ってくる。`wgpu-hal 29` は **`windows = 0.62`** に
依存する。一方、別の推移的依存である `gpu-allocator 0.28` (2026-05 時点で
最新の stable リリース) は依然として **`windows = 0.61`** に pin されており、
`windows_core::Param` トレイトが 2 バージョン間で unify せず、Windows
ビルドが `wgpu-hal` 内で **`E0277` エラー 10 件** で compile failure する。

現状の解は eframe から `wgpu` feature を外し、代わりに `glow` (`egui_glow`
経由の OpenGL バックエンド) を使うこと。em6809 は 0.29 から 0.33 までも
ずっと glow で描画してきたので、エンドユーザ視点での挙動変化はない。

`gpu-allocator` が `windows = 0.62` 以降に追従したリリースを出した時点で
`wgpu` feature の復帰を再検討する。`cargo tree -i windows` で 2 バージョン
共存が解消したかを確認できる。

## 先送りした deprecation: `Panel::show(ctx, …)` → `App::ui` 移行

eframe 0.34 で `Panel::show(ctx, …)` が deprecated になり、新しい正規ルート
として `App::ui(&mut Ui, &mut Frame)` トレイトメソッド + `Panel::show_inside(ui, …)`
が導入された。旧 `App::update(&mut Context, &mut Frame)` は今でも eframe の
ネイティブループから毎フレーム呼ばれており、エラーにはならず警告のみ出す。

em6809 の 1 フレーム分のロジックはすべて `App::update` の中にある。`update`
の本体は **約 850 行** あり、`Context` を複数のヘルパと 3 つの viewport
closure (Breakpoints / Call Stack / Console) を経由して引き回している。
各 viewport closure は `Context::show_viewport_immediate / _deferred` から
`Context` のみを受け取る (`Ui` は来ない) ため、忠実に `show_inside` 化する
には viewport entry point ごとに `egui::Area::new("root").show(ctx, |ui| { … show_inside(ui, …) })`
のようなラッパーを噛ませる必要がある。

0.34 バンプ (PR #36) では構造的移行を **明示的に先送り** することにした:

1. ~850 行の `update` + 3 つの viewport closure を一気に移行する diff は、
   依存バンプ PR としては大きすぎてレビュー性を損なう。
2. 内部実装上 deprecated な `show()` は `show_inside()` の薄いラッパー
   なので、機能的差異は今のところ存在しない。
3. アクティブな GUI 開発は **emfe** (Windows 特化のプラグイン式エミュレータ
   フレームワーク。README "Project direction" 節参照) に移行している。
   em6809 の eframe フロントエンドは保守モードであり、ここで大規模 UI
   リファクタを行う費用対効果は低い。

具体的には、バンプ PR 内で簡単な deprecation (`Rounding` → `CornerRadius`、
`Margin::same(f32)` → `Margin::same(i8)`、`Ui::close_menu` → `Ui::close`
などの単純 rename) はすべて修正済み。残った `Panel::show(ctx, …)` 警告は
該当箇所を保有する **9 関数** に `#[allow(deprecated)]` を貼って suppress
した:

| ファイル | 関数 | 役割 |
|----------|------|------|
| `src/regview.rs` | `App::update` (`RegApp`) | トップレベルのメニューバー + レイアウト |
| `src/regview/gui/regs_ui.rs` | `render_regs_panel` | 中央のレジスタペイン |
| `src/regview/gui/list_ui.rs` | `render_listing_panel` | 右側のリスティングペイン |
| `src/regview/gui/mem_ui.rs` | `render_memory_panel` | 右側のメモリペイン |
| `src/regview/gui/console_ui.rs` | `render_console` | Console viewport の中身 |
| `src/regview/gui/console_ui.rs` | `show_console_viewport` | viewport 配線 |
| `src/regview/gui/breakpoints_ui.rs` | `show_breakpoints_viewport` | viewport 配線 |
| `src/regview/gui/call_stack_ui.rs` | `show_call_stack_viewport` | viewport 配線 |
| `src/settings_native.rs` | `SettingsNativeApp::draw` | Settings ウィンドウ |

各 attribute には本ドキュメントを指す短いコメントを添えてあるので、
先送り中の移行が grep で発見可能。

### `#[allow(deprecated)]` を解除するトリガ

以下のいずれかが成立した時点で移行を実施する:

- egui が `Panel::show(ctx, …)` を **削除する** (warning ではなく compile
  error 化する) リリースを announce したとき。その時点で次の依存バンプ
  と同時に移行が必須になる。
- emfe が安定し、em6809 の GUI フロントエンドに本質的な改善を入れる
  作業に戻ってきたとき。単独でやるより同時にやる方が安い。

移行手順 (実施時に詳細化する想定の概略):

1. `RegApp::update` のシグネチャを
   `fn ui(&mut self, ui: &mut egui::Ui, frame: &mut Frame)` に変更。
2. body 冒頭で `let ctx = ui.ctx().clone();` を取得し、既存の `ctx.input(…)` /
   `ctx.send_viewport_cmd(…)` をそのまま維持。
3. 各 `Panel::show(ctx, |inner| { … })` を `Panel::show_inside(ui, |inner| { … })`
   に置換。`regs_ui::render_regs_panel` / `list_ui::render_listing_panel` /
   `mem_ui::render_memory_panel` ヘルパのシグネチャも `(&mut RegApp, &Context)`
   から `(&mut RegApp, &mut Ui)` に変える。
4. Breakpoints / Call Stack / Console viewport closure については、本体を
   `egui::Area::new("root").show(ctx, |ui| { … })` でラップして
   `CentralPanel::default().show(ctx, …)` を `show_inside(ui, …)` に置換する。
   または「viewport ルートでは `show(ctx, …)` が引き続き自然」と判断して
   その箇所だけ `#[allow(deprecated)]` を残す選択もあり (egui 側がこの用途に
   ついて deprecation を緩める可能性がある)。
5. deprecated トレイトメソッド `update` を空 stub として実装し、何らかの
   呼び出し経路が残っていても満たされるようにする。
6. 上記 9 関数の `#[allow(deprecated)]` をすべて削除し、
   `cargo check --all-targets` で warning 表面が真に空であることを確認。

## Linux Wayland マルチビューポートのフリーズ

依存スタックとは独立した古い問題として、Linux/Wayland (具体的には
Ubuntu 24.04 の Mutter、Parallels VM 上で確認) で Console viewport が
メインウィンドウの背面に完全に隠れたタイミングで em6809 がデッドロック
する症状がある — 症状とワークアラウンドはトップレベル README の
「既知の問題」節に記載済み。0.29 から 0.34 へのバンプはこのシナリオに
対しても検証済みで、**本問題は解消しない**。egui のマルチビューポート
モデルと Wayland のサーフェイスライフサイクルとの設計上の相性問題と
判断しており、上流で修正される類のバグではないと位置づけている。
