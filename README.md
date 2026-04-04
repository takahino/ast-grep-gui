# ast-grep GUI

[日本語版はこちら](./README.jp.md)

A desktop GUI frontend for [ast-grep](https://ast-grep.github.io/) built with Rust and `egui`.
It is designed to make structural code search easier for users who prefer a visual workflow over the CLI.

![ast-grep GUI main window](./assets/screenshots/main-window.png)

## Highlights

- AST-based code search powered by `ast-grep-core`
- Search modes for `AST`, `ast-grep (raw)`, plain text, and regex
- Auto language detection by file extension for mixed-language repositories
- Code view and table view for browsing results
- Pattern help, presets, and snippet-based pattern assist
- Export results to `TXT`, `JSON`, `Markdown`, `HTML`, and `Excel (.xlsx)`
- UI language switching between Japanese and English
- Auto text encoding detection with `chardetng`, plus manual `UTF-8`, `UTF-16 LE`, `UTF-16 BE`, `Shift_JIS`, `EUC-JP`, `JIS`, `GBK`, `Big5`, `EUC-KR`, and `Latin1` family overrides
- Built-in terminal panel for PowerShell commands and `sg`-style searches

## Supported Languages

- Rust
- Java
- Python
- JavaScript
- TypeScript
- Go
- C
- C++
- C#
- `Auto` mode detects the parser from each file extension

## Requirements

- Rust stable toolchain
- Windows is the primary target environment
- For release builds in this repository, the configured target is `x86_64-pc-windows-msvc`

## Run Locally

```powershell
cargo run
```

For an optimized build:

```powershell
cargo run --release
```

To build the Windows release binary explicitly:

```powershell
cargo build --release --target x86_64-pc-windows-msvc
```

## Usage

1. Select a directory to search.
2. Choose a search mode.
3. In AST mode, choose a language or use `Auto`.
4. Enter a pattern, text, or regex.
5. Adjust context lines, file filter, encoding, and skip directories as needed.
6. Run the search and inspect the results in code view or table view.
7. Export or copy the results if needed.

### AST Pattern Tips

- Use meta variables such as `$VAR`, `$$$ARGS`, and `$_`
- Open the built-in help popup for examples and presets
- Use the pattern assist dialog to generate candidate patterns from a code snippet

Example patterns:

```text
fn $NAME($$$ARGS)
$VAR.unwrap()
console.log($$$ARGS)
```

## Search Modes

- `AST`: structural search using ast-grep patterns
- `ast-grep (raw)`: same AST search, but the code panel shows CLI-style output
- `Text`: case-sensitive plain substring search
- `Regex`: regular-expression search

## Export Formats

- `TXT`
- `JSON`
- `Markdown`
- `HTML`
- `Excel (.xlsx)`
- Copy to clipboard

## Packaging and Release

- `build.rs` embeds `assets/icon.ico` into Windows builds when available
- `.cargo/config.toml` enables static CRT linking for `x86_64-pc-windows-msvc`
- `.github/workflows/release.yml` builds and publishes `ast-grep-gui.exe` when a `v*` tag is pushed

## Project Structure

```text
src/main.rs              Application entry point
src/app.rs               App state and main UI flow
src/search.rs            Background search engine
src/lang.rs              Language definitions and presets
src/pattern_assist.rs    Snippet-to-pattern suggestions
src/export.rs            Exporters
src/terminal.rs          Built-in terminal panel
src/ui/                  GUI panels and popups
assets/help/             Embedded pattern help pages
```

## Notes

- The app currently targets Windows-focused distribution.
- Column offsets for highlighted matches are byte-based, so multibyte text can still have edge cases.
- Search settings and pattern history are persisted between launches.
