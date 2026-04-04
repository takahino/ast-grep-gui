# ast-grep GUI

[English README](./README.md)

Rust と `egui` で作られた [ast-grep](https://ast-grep.github.io/) 向けのデスクトップ GUI フロントエンドです。
CLI に慣れていないユーザーでも、構造検索を視覚的に使いやすくすることを目的にしています。

## 主な機能

- `ast-grep-core` を使った AST ベースの構造検索
- `AST`、`ast-grepそのまま`、文字列検索、正規表現検索の 4 モード
- 拡張子ベースの自動言語判定で混在リポジトリにも対応
- コードビューと表ビューの 2 つの結果表示
- パターンヘルプ、プリセット、スニペットからのパターン支援
- `TXT`、`JSON`、`Markdown`、`HTML`、`Excel (.xlsx)` へのエクスポート
- 日本語 / 英語の UI 切り替え
- `UTF-8` と `Shift_JIS (CP932)` の文字コード対応
- PowerShell コマンドや `sg` 風検索を使える内蔵ターミナル

## 対応言語

- Rust
- Java
- Python
- JavaScript
- TypeScript
- Go
- C
- C++
- C#
- `Auto` モードでは拡張子から言語を自動判定します

## 動作要件

- Rust stable toolchain
- 主なターゲット環境は Windows
- このリポジトリのリリースビルド対象は `x86_64-pc-windows-msvc`

## ローカル実行

```powershell
cargo run
```

最適化付きで実行する場合:

```powershell
cargo run --release
```

Windows 向けリリースバイナリを明示的にビルドする場合:

```powershell
cargo build --release --target x86_64-pc-windows-msvc
```

## 使い方

1. 検索対象ディレクトリを選びます。
2. 検索モードを選びます。
3. AST モードでは対象言語を選ぶか `Auto` を使います。
4. パターン、文字列、または正規表現を入力します。
5. 必要に応じてコンテキスト行数、ファイルフィルタ、文字コード、スキップディレクトリを調整します。
6. 検索を実行し、コードビューまたは表ビューで結果を確認します。
7. 必要なら結果をコピーまたはエクスポートします。

### AST パターンのヒント

- `$VAR`、`$$$ARGS`、`$_` などのメタ変数を使えます
- 内蔵ヘルプから例やプリセットを参照できます
- パターン支援ダイアログでコード片から候補パターンを生成できます

例:

```text
fn $NAME($$$ARGS)
$VAR.unwrap()
console.log($$$ARGS)
```

## 検索モード

- `AST`: ast-grep の構造検索
- `ast-grepそのまま`: AST 検索結果をコードパネルで CLI 風表示
- `文字列`: 大文字小文字を区別する通常の部分一致検索
- `正規表現`: 正規表現による検索

## エクスポート形式

- `TXT`
- `JSON`
- `Markdown`
- `HTML`
- `Excel (.xlsx)`
- クリップボードコピー

## 配布とリリース

- `build.rs` は `assets/icon.ico` が存在すれば Windows ビルドに埋め込みます
- `.cargo/config.toml` では `x86_64-pc-windows-msvc` 向けに CRT 静的リンクを有効化しています
- `.github/workflows/release.yml` は `v*` タグ push 時に `ast-grep-gui.exe` をビルドして GitHub Release に添付します

## ディレクトリ概要

```text
src/main.rs              アプリ起動処理
src/app.rs               アプリ状態と UI 全体制御
src/search.rs            バックグラウンド検索エンジン
src/lang.rs              言語定義とプリセット
src/pattern_assist.rs    スニペットからのパターン候補生成
src/export.rs            各種エクスポート処理
src/terminal.rs          内蔵ターミナル
src/ui/                  GUI パネルとポップアップ
assets/help/             埋め込みパターンヘルプ HTML
```

## 補足

- 現状は Windows 向け配布を主眼にしています。
- マッチ位置の列オフセットはバイト単位のため、マルチバイト文字ではハイライトにずれが出る場合があります。
- 検索設定やパターン履歴はアプリ再起動後も保持されます。
