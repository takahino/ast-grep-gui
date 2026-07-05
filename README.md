# ast-grep GUI

[日本語版はこちら](./README.jp.md)

A desktop GUI frontend for [ast-grep](https://ast-grep.github.io/) built with Rust and `egui`.
It is designed to make structural code search easier for users who prefer a visual workflow over the CLI.

![ast-grep GUI main window](./assets/screenshots/main-window.png)

## Highlights

- **Search targets:** local directory, or **remote Git / SVN URL** (no prior clone/checkout; fetched into a local cache, then searched)
- **Open hits in the default app:** open a matched file with the OS file association from the file list, code view, or table view
- AST-based code search powered by `ast-grep-core`
- Batch rewrite (like `--rewrite`): preview, diff, then write back files in `AST` mode
- Search modes for `AST`, `Token`, plain text, regex, and **YAML Rule** (embedded ast-grep rules—no external `sg` CLI)
- Auto language detection by file extension for mixed-language repositories
- Code view, table view (with double-click preview popup), **Summary** view (aggregates type-hint variations: receiver type, call arity, and per-argument types—plus a method column when the pattern exposes one), and **batch report** view (run multiple patterns with per-job settings, then review an aggregated report)
- **Type hints (primarily for C/C++):** best-effort syntax-based inference for metavariable captures—one column per single metavariable (`$NAME`), and for multi-node captures (`$$$ARGS`, etc.) a count column (`ARGS#arity`) plus one column per captured node (`ARGS#0`, `ARGS#1`, …). **C and C++** are the main targets (legacy MFC/Win32 modernization); other languages get lighter inference only. **C++** can follow `#include` into headers on disk; **Advanced settings** (AST-related modes) let you add semicolon-separated **include directories** (compiler `-I` equivalent) and an **include diagnostic** panel. **Type-hint assist rules** (YAML, C++ only) let you register method return types, macros, constants, fields, binary operators, and more.
- Pattern help, presets, snippet-based pattern assist, and **pattern input history** (up to 30 entries)
- Optional **incremental search** that automatically reruns after you stop typing for a short delay
- Built-in **regex visualizer** to inspect and test regular expressions interactively
- Plain-text search options for **case-insensitive** and **whole-word** matching
- Export results to `TXT`, `JSON`, `Markdown`, `HTML`, and `Excel (.xlsx)` (including multi-job batch reports after a batch run)
- UI language switching between Japanese and English (auto-detected from OS locale)
- Configurable **max hit count** to cap large result sets (default: 100,000)
- Auto text encoding detection with `chardetng`, plus manual `UTF-8`, `UTF-16 LE`, `UTF-16 BE`, `Shift_JIS`, `EUC-JP`, `JIS`, `GBK`, `Big5`, `EUC-KR`, and `Latin1` family overrides
- Built-in terminal panel for PowerShell commands and `sg run`-style searches
- **In-document find** (**Ctrl+F**) in the code view, table view, and file preview popup: literal substring search (with optional case sensitivity), previous/next match, scroll sync, and **highlighting** of all hits (the active hit is emphasized)
- **Single-exe `--batch` mode** for large-scale batch search from a one-pattern-per-line file
- **GUI CLI builder** to compose CLI commands, copy them, or run the same settings inside the GUI

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
- Kotlin
- Scala
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

Batch mode uses the same executable with `--batch` (no separate binary).

## Command-Line Batch Search

Run many AST patterns without opening the GUI. Provide **one pattern per line** in a text file.

### Pattern file example (`patterns.txt`)

```text
# Comments and blank lines are ignored
fn $NAME($$$ARGS)
$VAR.unwrap()
console.log($$$ARGS)
```

### Basic command

```powershell
cargo run -- --batch `
  --patterns patterns.txt `
  --dir C:\path\to\repo `
  --lang rust `
  --view table `
  --format json `
  --output result.json
```

### Output views

| View | Description |
|------|-------------|
| `code` | Code-view style (per file, with surrounding context) |
| `table` | Table view (full rows with type-hint columns) |
| `summary` | Summary (type-hint variation aggregation) |

Combine views with `--view code,summary`. For Excel (`xlsx`), **one workbook** contains separate sheets per view.

```powershell
cargo run -- --batch `
  --patterns patterns.txt `
  --dir C:\path\to\repo `
  --lang cpp `
  --view code,summary `
  --format xlsx `
  --output report.xlsx
```

### Main options

| Option | Description |
|--------|-------------|
| `--patterns` | One-pattern-per-line input file |
| `--dir` | Local search root directory |
| `--git-url` | Git remote URL to fetch before search (alternative to `--dir`) |
| `--svn-url` | SVN remote URL to fetch before search (alternative to `--dir`) |
| `--ref` | Git branch / tag / commit (with `--git-url`) |
| `--revision` | SVN revision (with `--svn-url`; empty = HEAD) |
| `--subdir` | Subdirectory within the fetched tree |
| `--refresh-cache` | Re-fetch remote content on the next run |
| `--lang` | Language (`auto`, `rust`, `cpp`, …) |
| `--view` | `code` / `table` / `summary` (comma-separated) |
| `--format` | `text` / `json` / `markdown` / `html` / `xlsx` |
| `--output` | Output path (required in batch mode) |
| `--context` | Context lines (default: 2) |
| `--filter` | File name filter (`;`-separated globs) |
| `--skip-dirs` | Directory names to skip (`;`-separated) |
| `--max-hits` | Max hits per pattern (0 = unlimited) |
| `--include-dirs` | C++ include paths for type hints |
| `--no-type-hints` | Disable type-hint inference |

### Remote Git / SVN (batch)

```powershell
cargo run -- --batch `
  --patterns patterns.txt `
  --git-url https://github.com/example/repo.git `
  --ref main `
  --subdir src `
  --lang rust `
  --view table `
  --format json `
  --output result.json
```

```powershell
cargo run -- --batch `
  --patterns patterns.txt `
  --svn-url https://svn.example.com/repo/trunk `
  --revision 12345 `
  --lang auto `
  --view table `
  --format json `
  --output result.json
```

Remote fetch uses embedded Rust libraries (`gix` for Git, `svn` + WebDAV for SVN)—not the `git` or `svn` CLI. Fetched trees are cached under `%LOCALAPPDATA%\ast-grep-gui\vcs-cache` (Windows).

## GUI CLI Builder

Open **⌨ CLI builder** in the toolbar **Batch jobs** section to compose the same CLI from the GUI.

1. Choose a **pattern file** and optional **output file** (required for `xlsx`)
2. Select **output views** (code / table / summary) and **format**
3. Review the **command preview**, then **Copy** or **Run with these settings**
4. Runs execute as in-GUI batch search; review results in **Batch report**
5. If an output file was set, export runs automatically when the batch finishes

Search directory, language, and advanced settings come from the main toolbar. When **Git URL** or **SVN URL** is selected as the search target, the remote URL, ref/revision, and subdirectory fields are included in the CLI preview.

## Usage

1. Choose a **search target**: **Local**, **Git URL**, or **SVN URL**.
2. For **Local**, pick a directory. For **Git/SVN**, enter the remote URL and optionally a ref/revision and subdirectory; use **Refresh** to force a re-fetch on the next search.
3. Choose a search mode (`AST`, `Token`, text, regex, or **YAML Rule**).
4. In `AST` mode, choose a language or use `Auto`. In **YAML Rule** mode, configure rules (see below).
5. Enter an AST pattern, token sequence, plain text, regex, or YAML rule input.
6. Adjust context lines, file filter, encoding, skip directories, mode-specific options, and (in AST-related modes) **Advanced settings**—including **C++ include directories** and the **include diagnostic** panel for type-hint resolution—as needed.
7. Run the search and inspect the results in code view, table view, or **Summary** view.
8. Use **Open** on a hit to launch the file with the OS default application, or export/copy results as needed.

### Remote Git / SVN search

- **Git URL** supports HTTPS, SSH (`git@…`), `git://`, and `file://` via `gix` (no `git` executable required).
- **SVN URL** supports `svn://` and HTTP(S) via Rust libraries (no `svn` executable required).
- Only the **tracked tree** at the requested ref/revision is searched (no untracked files).
- The resolved cache path is shown in the toolbar after a successful fetch.
- **Note:** `svn+ssh://` is not enabled in the current dependency set (Rust toolchain / crate compatibility).

### In-document find (Ctrl+F)

With the code panel, table view, or file preview focused, press **Ctrl+F** to open the find bar. Type a substring, optionally toggle **case sensitivity**, and use **↑ / ↓** (or the equivalent buttons) to jump between matches. Matches are **highlighted** in the source (code view and preview); the table view scrolls to each matching row.

### YAML Rule search

Select **YAML Rule** in the toolbar to run [ast-grep YAML rules](https://ast-grep.github.io/) without the external `sg` CLI (rules are loaded via the embedded `ast-grep-config` engine).

1. Optionally set **`sgconfig.yml`** path (leave empty to auto-detect by walking up from the search root).
2. Optionally pick a **single rule YAML file**, or enter **rule text** directly in the toolbar (inline text takes priority over the file).
3. Optionally filter by **rule id** regex (empty = all rules).
4. Run the search. Scanned file extensions are derived from each rule’s `language` and `languageGlobs`.

Separate multiple inline rules with `---`. Example rule text:

```yaml
id: find-unwrap
language: Rust
rule:
  pattern: $EXPR.unwrap()
```

### AST Pattern Tips

- Use meta variables such as `$VAR`, `$$$ARGS`, and `$_`
- When a pattern includes metavariables that capture code, the app computes type hints (syntax-based, best-effort): single metavariables (`$RECV`, `$VAR`, …) get one column each; multi-node metavariables (`$$$ARGS`, …) get a `NAME#arity` column (number of captured nodes, e.g. call arity) and `NAME#0`, `NAME#1`, … for each captured node’s inferred type. Anonymous `$$$` / `$$$_` are not listed as columns. For **C++**, set **include directories** in Advanced settings if types exist only in headers outside the current file’s directory (e.g. `#include <vector>`). Right-click a type-hint cell in the **table** or **summary** view and choose **Add type-hint rule…** to open the settings window with fields prefilled from the hit—both unresolved cells (`?`, including parenthesized expressions such as `(1 + 2)` for `$RECV`) and inferred **class.method** labels such as `CTime.Format`. For **methods** rules, **arity** and argument types are prefilled from the same row when available.
- Open the built-in help popup for examples and presets
- Use the pattern assist dialog to generate candidate patterns from a code snippet

Example patterns:

```text
fn $NAME($$$ARGS)
$VAR.unwrap()
console.log($$$ARGS)
```

### Type hints overview

Type-hint columns appear in the table, summary, and structured exports when a pattern captures code via metavariables. **The inference engine is built primarily for C and C++** (especially legacy MFC/Win32 codebases); other languages receive lighter, local-scope inference only.

| Scope | Languages | What is inferred |
|-------|-----------|------------------|
| **Primary** | **C, C++** | Method chains, headers via `#include`, inheritance, out-of-class definitions, typedef/using aliases, free functions, extern globals, macros, receiver expressions, and more (see below) |
| **Secondary** | **Java** | Local variables, enhanced `for`, method chains |
| **Basic** | Rust, Go, Python, TS/JS, Kotlin, Scala, C# | `self`/`let`/receiver types, block-local variables, limited chains |

**C uses the same C++ inference path** (C sources are parsed with the C++ parser for hints).

#### C/C++ automatic inference (built-in)

When type hints are enabled, the app resolves types from the current translation unit and recursively from `#include`d headers (depth limit: 8; per-header size limit: 512 KB). Header files are read with the same **automatic encoding detection** as search sources (UTF-8, Shift_JIS, UTF-16, etc.). Relative include directories in Advanced settings are resolved from the **search root** (semicolon-separated, compiler `-I` equivalent).

Built-in resolution includes:

- **Inheritance:** walk base classes for fields and method return types (assist YAML `methods`/`fields` rules also apply to base classes)
- **Out-of-class definitions:** e.g. `CWnd* CMyApp::GetMainWnd()` in `.cpp` files
- **typedef / using:** one-level alias expansion (pointer typedefs, anonymous `typedef struct { … } NAME`, multiple declarators)
- **Free-function prototypes** and **extern global variables** (e.g. `AfxGetApp()`, `theApp`)
- **Method chains** starting from free functions (e.g. `AfxGetApp()->GetMainWnd()->…`)
- **Receiver expressions:** parenthesized dereference `(*p).m`, casts `((T*)ptr)->m`, subscript bases `arr[i].m`
- **Macro auto-analysis** (4 patterns): cast `#define M(x) ((TYPE)(x))`, deref alias `#define theApp (*AfxGetApp())`, simple identifier alias, and forwarding `#define GETAPP() AfxGetApp()`. Other macros should be registered in assist YAML under `macros`.

Open the **include diagnostic** collapsible panel under Advanced settings to see unresolved `#include` paths, read errors, and hint-column statistics after a search.

#### Limitations

- This is **syntax-based best-effort inference**, not a compiler: template element types, multi-level namespaces, and `using namespace` are unsupported or limited.
- Namespace traversal is **one level**; inheritance and include recursion are capped at **depth 8**.
- Only the four macro patterns above are auto-parsed; transparent macros such as `#define CHECK(x) (x)` are not.
- Function-pointer typedefs are out of scope (they cannot be receivers).
- A known edge case: when the depth limit is hit, a negative cache may prevent a shallower retry within the same search job (rare in practice).

### Type-hint assist rules (C++ only / YAML)

When type hints are enabled in AST-related modes, open **Type-hint assist…** under **Advanced settings** to edit **C++-only** assist rules in the GUI. The same window supports **Load** / **Save** YAML files. Rules persist across app restarts (the last saved YAML path is remembered when you save manually). In the editor, enter **`params` one type per line** (e.g. `LPCTSTR` on its own line); YAML files still use a `params` array. These rules take **priority** over built-in source/header inference.

| Category | Example use |
|----------|-------------|
| `methods` | `CString.GetLength` → `int` (overload resolution via `arity` / `params`) |
| `functions` | `MAKEINTRESOURCE(int)` → `LPCTSTR` |
| `macros` | `_T("abc")` → `LPCTSTR` |
| `constants` | `IDC_OK` → `int`, `WM_USER` → `UINT` |
| `fields` | `CWnd.m_hWnd` → `HWND` |
| `binary_ops` | `"abc" + CString` → `CString`, `LPCTSTR + CString` → `CString` |

Example config (`type-hint-config.yaml`):

```yaml
version: 1
cpp:
  methods:
    - class: CString
      method: GetLength
      arity: 0
      returns: int
    - class: CTime
      method: Format
      params: [LPCTSTR]
      returns: CString
  functions:
    - name: MAKEINTRESOURCE
      params: [int]
      returns: LPCTSTR
  macros:
    - name: _T
      arity: 1
      returns: LPCTSTR
  constants:
    - name: IDC_OK
      type: int
    - name: WM_USER
      type: UINT
  fields:
    - class: CWnd
      field: m_hWnd
      type: HWND
  binary_ops:
    - op: "+"
      lhs: StringLiteral
      rhs: CString
      returns: CString
    - op: "+"
      lhs: LPCTSTR
      rhs: CString
      returns: CString
```

- YAML rules take **priority** over source/header inference (the built-in logic still applies as a fallback).
- Numeric binary expressions such as `(nSel + 1) * 100` are inferred as `int` when local variable types are known (no rule required in many cases).
- Expressions like `"abc" + x` can be pointer arithmetic in C++; use `binary_ops` explicitly, or rely on resolution only when one side’s type is known.
- **Re-run the search** after changing rules so type-hint columns update.

## Search Modes

- `AST`: structural search using ast-grep patterns
- `Token`: searches space-separated tokens in order, allowing flexible whitespace between them
- `Text`: plain substring search with optional case-insensitive and whole-word matching
- `Regex`: regular-expression search
- `YAML Rule`: run ast-grep YAML rules from `sgconfig.yml`, a rule file, or inline rule text (no external CLI)

## Export Formats

- `TXT`
- `JSON`
- `Markdown`
- `HTML`
- `Excel (.xlsx)`
- Copy to clipboard

When the pattern includes metavariables used for type hints, `JSON`, `Markdown`, `HTML`, and `Excel` exports include the same hint columns as the table view (including `NAME#arity` and `NAME#i` for `$$$NAME` captures). These structured exports also embed a **snapshot of the search conditions** used for the run (pattern, language, include paths, YAML rule settings, etc.).

## Packaging and Release

- `build.rs` embeds `assets/icon.ico` into Windows builds when available
- `.cargo/config.toml` enables static CRT linking for `x86_64-pc-windows-msvc`
- `.github/workflows/release.yml` builds and publishes `ast-grep-gui.exe` when a `v*` tag is pushed

## Project Structure

```text
src/lib.rs               Shared library for GUI and CLI
src/main.rs              GUI entry point
src/cli_runner.rs          `--batch` CLI batch runner
src/cli_config.rs        Shared CLI / GUI builder settings
src/app.rs               App state and main UI flow
src/search.rs            Background search engine
src/search_target.rs     Search target types (local / Git / SVN remote)
src/remote_fetch.rs      Remote fetch orchestration (async + sync for CLI)
src/vcs_cache.rs         Remote fetch cache paths and markers
src/git_remote.rs        Git clone via gix (no git CLI)
src/svn_remote.rs        SVN export via svn crate / WebDAV (no svn CLI)
src/ast_pattern.rs       Pattern compilation strategies (contextual call support)
src/receiver_hint.rs     Best-effort metavariable type hints (primarily C/C++; lighter support for other languages)
src/type_hint_config.rs  Type-hint assist rule YAML schema, lookup, and draft generation (C++ only)
src/yaml_rule.rs         Embedded ast-grep YAML rule loader and matcher
src/lang.rs              Language definitions and presets
src/pattern_assist.rs    Snippet-to-pattern suggestions
src/export.rs            Exporters
src/file_encoding.rs     Text encoding detection and reading
src/i18n.rs              UI language (Japanese / English)
src/regex_visualizer.rs  Regex tokenizer for the visualizer feature
src/help_html.rs         Opens embedded HTML help in the OS browser
src/terminal.rs          Built-in terminal state
src/sg_command.rs        Parses `sg run`-style terminal commands
src/ui/cli_builder_panel.rs  CLI builder popup
src/ui/type_hint_config_panel.rs  Type-hint assist settings window
src/ui/cpp_include_diagnostic.rs  C++ include-path diagnostic panel (Advanced settings)
src/ui/                  GUI panels and popups
assets/help/             Embedded pattern help HTML pages
```

## Notes

- The app currently targets Windows-focused distribution.
- Column offsets for highlighted matches are byte-based, so multibyte text can still have edge cases.
- Search settings, remote URL/ref/subdir, C++ include paths (Advanced), **type-hint assist rules** (YAML content and last saved path), and pattern history are persisted between launches.
- Remote fetch cache lives under `%LOCALAPPDATA%\ast-grep-gui\vcs-cache` on Windows.

## Recent updates (excerpt)

User-facing changes in v0.3.0 and recent development (see `git log` for the full history):

- **YAML Rule search:** run ast-grep YAML rules from the GUI without the external `sg` CLI; supports `sgconfig.yml` auto-detect, rule files, inline rule text (`---` separated), and rule-id regex filters.
- **C/C++ type-hint engine (major):** inheritance traversal, out-of-class definitions, typedef/using expansion, free functions, extern globals, method chains, receiver expressions, and four macro auto-parse patterns; C integrated into the C++ path; header encoding auto-detect (Shift_JIS, etc.); relative include dirs resolved from search root.
- **Type-hint assist rules (C++ only):** YAML/GUI editor for methods, macros, constants, fields, and binary ops; right-click from table/summary cells to prefill rules (including base-class lookup for `methods`/`fields`).
- **C++ include diagnostic panel:** unresolved `#include` paths and hint stats under Advanced settings.
- **Remote Git/SVN search:** search by remote URL without a local clone/checkout; optional ref/revision and subdirectory; local cache with refresh.
- **CLI batch mode & GUI CLI builder:** one-pattern-per-line batch search and in-GUI command composition.
- **Summary view:** aggregates inferred receiver types, arity, and per-argument types (and a method column when the pattern exposes one).
- **Export search-condition snapshot:** JSON/Markdown/HTML/Excel exports record the settings used for the run.
- **In-document find (Ctrl+F):** search within code, table, or preview with hit highlighting.
