# em6809

[![Build and Test](https://github.com/hha0x617/em6809/actions/workflows/build.yml/badge.svg)](https://github.com/hha0x617/em6809/actions/workflows/build.yml)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](LICENSE-APACHE)

*このアプリケーションは vibe coding ワークフローの一環として AI コーディングツールの支援を受けて生成されました。コミット `d85c2eb` までは **Codex CLI**、その次のコミット `aa1759e` 以降は **Claude Code** の支援を受けて開発が継続されています。*

## 概要

em6809 は MC6809 のソフトウェアエミュレータです。GUI を中心に、詳細なコンソール I/O、分岐先の可視化を含むトレース/逆アセンブルを備えています。OS‑9 / NitrOS‑9 のブートは長期的な目標として位置付けていますが、**現時点ではまだ実現できておらず**、カーネル起動までに解決が必要な課題が複数残っています。現状と残タスクは `docs/ja/os9_guide.md` を参照してください。

## インストール

- Rust（stable）を用意してビルド:
  - `cargo build --release`

## クイックスタート（GUIのみ）

- 実行して GUI を開く（初期状態は一時停止）:
  - `target/release/em6809 samples/hello/hello.s19 --format srec --console 0xFF00`
- ヘルプ表示:
  - `target/release/em6809 --help`
- PC を指定して開始:
  - `target/release/em6809 samples/hello/hello.s19 --format srec --console 0xFF00 --pc 0xE000`

## 基本ユースケース

- 標準入力/エコー: `... samples/echo/echo.s19 --console 0xFF00 --console-stdin --console-echo`
  （逆アセンブル/メモリ/トレースは GUI ウィンドウ内で操作します）

## ドキュメント

- 使用方法: `docs/ja/usage.md`
- Console I/O 詳細: `docs/ja/console_io.md`
- メモリダンプ編集（GUI）: `docs/ja/memory_editing.md`
- トレース/逆アセンブル: `docs/ja/tracing.md`
- OS‑9/NitrOS‑9 ガイド: `docs/ja/os9_guide.md`
- トラブルシューティング: `docs/ja/troubleshooting.md`
- 命令カバレッジ: `docs/ja/instruction_status.md`
 - GUI の環境変数（フォント/言語）: `docs/ja/usage.md`（「環境変数」節）

## リンク

- リポジトリ: https://github.com/hha0x617/em6809
- Issue: https://github.com/hha0x617/em6809/issues
- リリース: https://github.com/hha0x617/em6809/releases
- ドキュメント索引: `docs/README.md`
- 英語ドキュメント: `docs/en/`
- 日本語ドキュメント: `docs/ja/`
- サンプル（トレース/ブートスクリプト）: `samples/`

## 参考文献

- Motorola 6809（概要）: https://en.wikipedia.org/wiki/Motorola_6809
- MC6809 資料アーカイブ（Bitsavers）: http://bitsavers.org/components/motorola/6809/
- S‑Record 形式: https://en.wikipedia.org/wiki/SREC_(file_format)
- NitrOS‑9 プロジェクト: https://sourceforge.net/projects/nitros9/
- 6809 コミュニティリソース: http://www.6809.org.uk/
- LWTOOLS: https://www.lwtools.ca/

## 既知の問題

### Linux + Wayland (Ubuntu Desktop / Mutter コンポジタ)

GUI は [eframe / egui](https://github.com/emilk/egui) の multi-viewport
を使い、Breakpoints / Call Stack / Console を別トップレベルウィンドウ
として描画しています。Wayland 環境 (特に Ubuntu Desktop 24.04 LTS の
Mutter — macOS ホスト上の Parallels VM で確認) では、**Console
ウィンドウがメインウィンドウの背面に完全に隠れた瞬間にアプリケーションが
フリーズする** 不具合があります。例えば、メインウィンドウのメニューを
クリックした時や、メインウィンドウを Console に重なる位置までドラッグ
した時に発生します。

数秒後にウィンドウマネージャが *「em6809 が応答していません」* ダイアログ
を表示します。`Ctrl+C` でも終了できないことがあり、確実に終了するには
メインウィンドウの `[X]` ボタンを使う必要があります。

eframe 0.29 / 0.30 / 0.31 / 0.32 で検証しましたが、いずれのリリース版
でも解消できていません。これは egui で in-app ウィンドウではなく無理に
複数のトップレベルウィンドウを組み合わせて使用するアプリケーション設計
との相性問題のように思えます。本リポジトリは部分的な緩和策 (parent
context への 50 ms
heartbeat repaint、33 ms 周期の Console repaint ticker、Linux での
Console viewport の `show_viewport_immediate` 化) を含んでおり、起動時
に Console が背面に作られて発生する deadlock は回避できますが、ユーザ
操作で後から Console が背面化した場合の freeze は解消できていません。

対処法:

* Linux は **X11** で起動する (GNOME ログイン画面で「Ubuntu on Xorg」を選択)。
* または Console ウィンドウを必ず一部でも見える位置に配置する — メイン
  ウィンドウの真後ろに置かない。
* または Windows / macOS でビルド・実行する。これらの環境では multi-
  viewport は問題なく動作します。

### プロジェクトの方向性

当面の間、本リポジトリでのマルチプラットフォーム対応開発は**停止**します。
アクティブな開発は、Windows に特化した新プロジェクト **emfe** (プラグ
イン式エミュレータフレームワーク) に移行します。emfe では本 `em6809` を
コアバックエンドとして**プラグイン**で利用しつつ、フロントエンド UI を
Windows ネイティブのスタック (WinUI 3 / WPF) で再構築するため、Wayland
multi-viewport 系の問題を構造的に回避できます。

本リポジトリは引き続き Windows / macOS / Linux (ネイティブ X11) で
ビルド・実行可能で、emfe がロードする MC6809 コアの正規 Rust 実装と
して維持されます。CPU コア・デバッガバックエンド・プラットフォーム非
依存部分の Pull Request は歓迎しますが、eframe フロントエンド固有の新規
GUI 機能の追加は本リポジトリでは予定していません。

## 貢献とポリシー

- 開発/運用ルール: `AGENTS.md`
- 行動規範/セキュリティ: `CODE_OF_CONDUCT.md` / `SECURITY.md`（存在する場合）

## 商標について

*OS‑9* は Microware Systems Corporation の商標です。*NitrOS‑9* は
OS‑9 ファミリーに属する独立したオープンソースオペレーティング
システムです。本プロジェクト (em6809) は Microware Systems
Corporation、RadiSys Corporation、NitrOS‑9 プロジェクトのいずれ
とも提携、推奨、スポンサー関係を持ちません。本ドキュメントに
おける OS‑9 / NitrOS‑9 への言及はすべて説明目的です — em6809 は
将来 OS‑9 ファミリーのカーネルをホストすることを**目指している**
CPU エミュレータであり、互換性・認証・推奨を主張するもの
ではありません。

その他、本ドキュメント中に登場する製品名・商標・登録商標は、
それぞれの所有者に帰属します。

## ライセンス

MIT OR Apache‑2.0（デュアルライセンス）。詳細は `LICENSE-MIT` と `LICENSE-APACHE` を参照してください。

### 最近の GUI レジスタビューアの拡張

- `.lst` が無い場合のフォールバック（プログラムスパンの HEX 逆アセンブルを自動生成）。
- 「Hex view」切替と「Follow PC」ハイライト/自動スクロール。
- 自己書換えに対するイベント連動の再逆アセンブル（Full span / PC window）。
- 右クリックメニュー（リスティング行）: ここまで実行 / PC を現在のアドレスに変更 / キャンセル。
- リスティング行のホバー時に行全体を薄くハイライト。コンテキスト操作後は各ペインを即時更新。
