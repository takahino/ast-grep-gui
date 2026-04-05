use std::collections::HashSet;
use std::path::Path;

use ast_grep_language::SupportLang;

use crate::i18n::{Tr, UiLanguage};

/// GUIで選択可能な言語（検索対象のプログラミング言語）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum SupportedLanguage {
    /// ファイル拡張子から言語を推定し、複数言語混在のプロジェクトにも対応
    Auto,
    Rust,
    Java,
    Python,
    JavaScript,
    TypeScript,
    Go,
    Cpp,
    C,
    CSharp,
    Kotlin,
    Scala,
}

impl SupportedLanguage {
    /// 固定言語のみ（パターン検証・拡張子マップ用）
    pub fn all() -> &'static [SupportedLanguage] {
        &[
            Self::Rust,
            Self::Java,
            Self::Python,
            Self::JavaScript,
            Self::TypeScript,
            Self::Go,
            Self::Cpp,
            Self::C,
            Self::CSharp,
            Self::Kotlin,
            Self::Scala,
        ]
    }

    /// ツールバー用: 先頭に Auto を含む
    pub fn all_with_auto() -> &'static [SupportedLanguage] {
        &[
            Self::Auto,
            Self::Rust,
            Self::Java,
            Self::Python,
            Self::JavaScript,
            Self::TypeScript,
            Self::Go,
            Self::Cpp,
            Self::C,
            Self::CSharp,
            Self::Kotlin,
            Self::Scala,
        ]
    }

    /// ツールバー・コンボ用（UI 言語に合わせた「自動」表記）
    pub fn combo_label(self, ui: UiLanguage) -> String {
        match (self, ui) {
            (Self::Auto, UiLanguage::Japanese) => "自動（拡張子）".to_string(),
            (Self::Auto, UiLanguage::English) => "Auto (extension)".to_string(),
            _ => self.display_name().to_string(),
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Rust => "Rust",
            Self::Java => "Java",
            Self::Python => "Python",
            Self::JavaScript => "JavaScript",
            Self::TypeScript => "TypeScript",
            Self::Go => "Go",
            Self::C => "C",
            Self::Cpp => "C++",
            Self::CSharp => "C#",
            Self::Kotlin => "Kotlin",
            Self::Scala => "Scala",
        }
    }

    pub fn extensions(&self) -> &'static [&'static str] {
        match self {
            Self::Auto => &[],
            Self::Rust => &["rs"],
            Self::Java => &["java"],
            Self::Python => &["py", "pyi"],
            Self::JavaScript => &["js", "jsx", "mjs", "cjs"],
            Self::TypeScript => &["ts", "tsx", "mts", "cts"],
            Self::Go => &["go"],
            Self::C => &["c", "h"],
            Self::Cpp => &["cpp", "cc", "cxx", "h", "hpp", "hh", "hxx"],
            Self::CSharp => &["cs"],
            Self::Kotlin => &["kt", "kts", "ktm"],
            Self::Scala => &["scala", "sc", "sbt"],
        }
    }

    /// sg CLI の --lang 引数に渡す文字列（Auto は None）
    #[allow(dead_code)]
    pub fn to_cli_lang_str(self) -> Option<&'static str> {
        match self {
            Self::Auto => None,
            Self::Rust => Some("rust"),
            Self::Java => Some("java"),
            Self::Python => Some("python"),
            Self::JavaScript => Some("javascript"),
            Self::TypeScript => Some("typescript"),
            Self::Go => Some("go"),
            Self::C => Some("c"),
            Self::Cpp => Some("cpp"),
            Self::CSharp => Some("csharp"),
            Self::Kotlin => Some("kotlin"),
            Self::Scala => Some("scala"),
        }
    }

    /// CLI の言語文字列から SupportedLanguage に変換
    pub fn from_cli_str(s: &str) -> Option<SupportedLanguage> {
        match s.to_lowercase().as_str() {
            "rust" | "rs" => Some(Self::Rust),
            "java" => Some(Self::Java),
            "python" | "py" => Some(Self::Python),
            "javascript" | "js" => Some(Self::JavaScript),
            "typescript" | "ts" => Some(Self::TypeScript),
            "go" => Some(Self::Go),
            "c" => Some(Self::C),
            "cpp" | "c++" | "cc" | "cxx" => Some(Self::Cpp),
            "csharp" | "cs" | "c#" => Some(Self::CSharp),
            "kotlin" | "kt" | "kts" | "ktm" => Some(Self::Kotlin),
            "scala" | "sc" | "sbt" => Some(Self::Scala),
            _ => None,
        }
    }

    /// Auto では None（呼び出し側でファイルごとに解決すること）
    pub fn to_support_lang(self) -> Option<SupportLang> {
        match self {
            Self::Auto => None,
            Self::Rust => Some(SupportLang::Rust),
            Self::Java => Some(SupportLang::Java),
            Self::Python => Some(SupportLang::Python),
            Self::JavaScript => Some(SupportLang::JavaScript),
            Self::TypeScript => Some(SupportLang::TypeScript),
            Self::Go => Some(SupportLang::Go),
            Self::C => Some(SupportLang::C),
            Self::Cpp => Some(SupportLang::Cpp),
            Self::CSharp => Some(SupportLang::CSharp),
            Self::Kotlin => Some(SupportLang::Kotlin),
            Self::Scala => Some(SupportLang::Scala),
        }
    }

    /// syntect用の言語名（シンタックスハイライト）
    pub fn syntect_name(&self) -> &'static str {
        match self {
            Self::Auto => "Plain Text",
            Self::Rust => "Rust",
            Self::Java => "Java",
            Self::Python => "Python",
            Self::JavaScript => "JavaScript (Babel)",
            Self::TypeScript => "TypeScript",
            Self::Go => "Go",
            Self::C => "C",
            Self::Cpp => "C++",
            Self::CSharp => "C#",
            Self::Kotlin => "Kotlin",
            Self::Scala => "Scala",
        }
    }

    /// 拡張子（ドットなし・小文字想定）から対応する言語を返す
    pub fn from_extension(ext: &str) -> Option<SupportedLanguage> {
        let ext = ext.trim_start_matches('.').to_lowercase();
        for &lang in Self::all() {
            if lang.extensions().iter().any(|e| *e == ext.as_str()) {
                return Some(lang);
            }
        }
        None
    }

    /// パスから拡張子に基づき言語を推定
    pub fn from_path(path: &Path) -> Option<SupportedLanguage> {
        path.extension()
            .and_then(|e| e.to_str())
            .and_then(Self::from_extension)
    }

    /// Auto モードで対象に含める全拡張子（走査フィルタ用）
    pub fn union_extensions_for_auto_filter() -> HashSet<&'static str> {
        Self::all()
            .iter()
            .flat_map(|l| l.extensions().iter().copied())
            .collect()
    }

    /// 現在の言語モードで AST 検索に使う「拡張子 → 解析言語」の一覧（UI 表示用）
    pub fn ast_grep_extension_mapping(self) -> Vec<(String, String)> {
        match self {
            SupportedLanguage::Auto => {
                let mut seen = HashSet::new();
                let mut pairs: Vec<(String, String)> = Vec::new();
                for &lang in Self::all() {
                    for ext in lang.extensions() {
                        if seen.insert(ext) {
                            if let Some(detected) = Self::from_extension(ext) {
                                pairs.push((
                                    format!(".{}", ext),
                                    detected.display_name().to_string(),
                                ));
                            }
                        }
                    }
                }
                pairs.sort_by(|a, b| a.0.cmp(&b.0));
                pairs
            }
            lang => lang
                .extensions()
                .iter()
                .map(|ext| (format!(".{}", ext), lang.display_name().to_string()))
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn from_extension_rust() {
        assert_eq!(SupportedLanguage::from_extension("rs"), Some(SupportedLanguage::Rust));
    }

    #[test]
    fn from_extension_python_multiple_exts() {
        assert_eq!(SupportedLanguage::from_extension("py"), Some(SupportedLanguage::Python));
        assert_eq!(SupportedLanguage::from_extension("pyi"), Some(SupportedLanguage::Python));
    }

    #[test]
    fn from_extension_javascript_multiple_exts() {
        assert_eq!(SupportedLanguage::from_extension("js"), Some(SupportedLanguage::JavaScript));
        assert_eq!(SupportedLanguage::from_extension("jsx"), Some(SupportedLanguage::JavaScript));
        assert_eq!(SupportedLanguage::from_extension("mjs"), Some(SupportedLanguage::JavaScript));
        assert_eq!(SupportedLanguage::from_extension("cjs"), Some(SupportedLanguage::JavaScript));
    }

    #[test]
    fn from_extension_typescript_multiple_exts() {
        assert_eq!(SupportedLanguage::from_extension("ts"), Some(SupportedLanguage::TypeScript));
        assert_eq!(SupportedLanguage::from_extension("tsx"), Some(SupportedLanguage::TypeScript));
        assert_eq!(SupportedLanguage::from_extension("mts"), Some(SupportedLanguage::TypeScript));
        assert_eq!(SupportedLanguage::from_extension("cts"), Some(SupportedLanguage::TypeScript));
    }

    #[test]
    fn from_extension_cpp_multiple_exts() {
        assert_eq!(SupportedLanguage::from_extension("cpp"), Some(SupportedLanguage::Cpp));
        assert_eq!(SupportedLanguage::from_extension("cc"), Some(SupportedLanguage::Cpp));
        assert_eq!(SupportedLanguage::from_extension("cxx"), Some(SupportedLanguage::Cpp));
        assert_eq!(SupportedLanguage::from_extension("hpp"), Some(SupportedLanguage::Cpp));
    }

    #[test]
    fn from_extension_kotlin_multiple_exts() {
        assert_eq!(SupportedLanguage::from_extension("kt"), Some(SupportedLanguage::Kotlin));
        assert_eq!(SupportedLanguage::from_extension("kts"), Some(SupportedLanguage::Kotlin));
        assert_eq!(SupportedLanguage::from_extension("ktm"), Some(SupportedLanguage::Kotlin));
    }

    #[test]
    fn from_extension_scala_multiple_exts() {
        assert_eq!(SupportedLanguage::from_extension("scala"), Some(SupportedLanguage::Scala));
        assert_eq!(SupportedLanguage::from_extension("sc"), Some(SupportedLanguage::Scala));
        assert_eq!(SupportedLanguage::from_extension("sbt"), Some(SupportedLanguage::Scala));
    }

    #[test]
    fn from_extension_unknown_returns_none() {
        assert_eq!(SupportedLanguage::from_extension("txt"), None);
        assert_eq!(SupportedLanguage::from_extension(""), None);
        assert_eq!(SupportedLanguage::from_extension("xyz"), None);
        assert_eq!(SupportedLanguage::from_extension("html"), None);
    }

    #[test]
    fn from_extension_is_case_insensitive() {
        assert_eq!(SupportedLanguage::from_extension("RS"), Some(SupportedLanguage::Rust));
        assert_eq!(SupportedLanguage::from_extension("Java"), Some(SupportedLanguage::Java));
        assert_eq!(SupportedLanguage::from_extension("PY"), Some(SupportedLanguage::Python));
    }

    #[test]
    fn from_extension_strips_leading_dot() {
        assert_eq!(SupportedLanguage::from_extension(".rs"), Some(SupportedLanguage::Rust));
        assert_eq!(SupportedLanguage::from_extension(".java"), Some(SupportedLanguage::Java));
    }

    #[test]
    fn from_path_detects_language() {
        assert_eq!(
            SupportedLanguage::from_path(Path::new("src/main.rs")),
            Some(SupportedLanguage::Rust)
        );
        assert_eq!(
            SupportedLanguage::from_path(Path::new("App.java")),
            Some(SupportedLanguage::Java)
        );
        assert_eq!(
            SupportedLanguage::from_path(Path::new("script.py")),
            Some(SupportedLanguage::Python)
        );
    }

    #[test]
    fn from_path_no_extension_returns_none() {
        assert_eq!(SupportedLanguage::from_path(Path::new("Makefile")), None);
        assert_eq!(SupportedLanguage::from_path(Path::new("README")), None);
    }

    #[test]
    fn from_cli_str_all_variants() {
        let cases: &[(&str, SupportedLanguage)] = &[
            ("rust", SupportedLanguage::Rust),
            ("rs", SupportedLanguage::Rust),
            ("java", SupportedLanguage::Java),
            ("python", SupportedLanguage::Python),
            ("py", SupportedLanguage::Python),
            ("javascript", SupportedLanguage::JavaScript),
            ("js", SupportedLanguage::JavaScript),
            ("typescript", SupportedLanguage::TypeScript),
            ("ts", SupportedLanguage::TypeScript),
            ("go", SupportedLanguage::Go),
            ("c", SupportedLanguage::C),
            ("cpp", SupportedLanguage::Cpp),
            ("c++", SupportedLanguage::Cpp),
            ("cc", SupportedLanguage::Cpp),
            ("cxx", SupportedLanguage::Cpp),
            ("csharp", SupportedLanguage::CSharp),
            ("cs", SupportedLanguage::CSharp),
            ("c#", SupportedLanguage::CSharp),
            ("kotlin", SupportedLanguage::Kotlin),
            ("kt", SupportedLanguage::Kotlin),
            ("kts", SupportedLanguage::Kotlin),
            ("ktm", SupportedLanguage::Kotlin),
            ("scala", SupportedLanguage::Scala),
            ("sc", SupportedLanguage::Scala),
            ("sbt", SupportedLanguage::Scala),
        ];
        for (s, expected) in cases {
            assert_eq!(
                SupportedLanguage::from_cli_str(s),
                Some(*expected),
                "failed for: {s}"
            );
        }
    }

    #[test]
    fn from_cli_str_case_insensitive() {
        assert_eq!(SupportedLanguage::from_cli_str("RUST"), Some(SupportedLanguage::Rust));
        assert_eq!(SupportedLanguage::from_cli_str("Python"), Some(SupportedLanguage::Python));
        assert_eq!(SupportedLanguage::from_cli_str("GO"), Some(SupportedLanguage::Go));
    }

    #[test]
    fn from_cli_str_unknown_returns_none() {
        assert_eq!(SupportedLanguage::from_cli_str("cobol"), None);
        assert_eq!(SupportedLanguage::from_cli_str(""), None);
        assert_eq!(SupportedLanguage::from_cli_str("auto"), None);
    }

    #[test]
    fn to_cli_lang_str_auto_is_none() {
        assert_eq!(SupportedLanguage::Auto.to_cli_lang_str(), None);
    }

    #[test]
    fn to_cli_lang_str_fixed_langs() {
        assert_eq!(SupportedLanguage::Rust.to_cli_lang_str(), Some("rust"));
        assert_eq!(SupportedLanguage::Java.to_cli_lang_str(), Some("java"));
        assert_eq!(SupportedLanguage::Python.to_cli_lang_str(), Some("python"));
        assert_eq!(SupportedLanguage::Go.to_cli_lang_str(), Some("go"));
        assert_eq!(SupportedLanguage::Cpp.to_cli_lang_str(), Some("cpp"));
        assert_eq!(SupportedLanguage::CSharp.to_cli_lang_str(), Some("csharp"));
    }

    #[test]
    fn all_does_not_include_auto() {
        assert!(!SupportedLanguage::all().contains(&SupportedLanguage::Auto));
    }

    #[test]
    fn all_with_auto_starts_with_auto() {
        assert_eq!(SupportedLanguage::all_with_auto()[0], SupportedLanguage::Auto);
    }

    #[test]
    fn union_extensions_contains_common_extensions() {
        let set = SupportedLanguage::union_extensions_for_auto_filter();
        assert!(!set.is_empty());
        for ext in ["rs", "java", "py", "js", "ts", "go", "cpp", "c", "cs", "kt", "scala"] {
            assert!(set.contains(ext), "missing: {ext}");
        }
    }

    #[test]
    fn auto_extensions_is_empty() {
        assert!(SupportedLanguage::Auto.extensions().is_empty());
    }

    #[test]
    fn ast_grep_extension_mapping_rust() {
        let pairs = SupportedLanguage::Rust.ast_grep_extension_mapping();
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].0, ".rs");
        assert_eq!(pairs[0].1, "Rust");
    }

    #[test]
    fn ast_grep_extension_mapping_auto_is_sorted() {
        let pairs = SupportedLanguage::Auto.ast_grep_extension_mapping();
        let is_sorted = pairs.windows(2).all(|w| w[0].0 <= w[1].0);
        assert!(is_sorted, "auto mapping should be sorted by extension");
    }
}

/// プリセットパターン（言語ごと）
pub struct Preset {
    pub label: String,
    pub pattern: &'static str,
    pub description: String,
}

pub fn presets_for(lang: SupportedLanguage, ui: UiLanguage) -> Vec<Preset> {
    let t = Tr(ui);
    match lang {
        SupportedLanguage::Auto => vec![Preset {
            label: t.preset_generic_any().to_string(),
            pattern: "$EXPR",
            description: t.preset_generic_any_desc().to_string(),
        }],
        SupportedLanguage::Rust => vec![
            Preset {
                label: t.preset_rust_fn().to_string(),
                pattern: "fn $NAME($$$ARGS)",
                description: t.preset_rust_fn_desc().to_string(),
            },
            Preset {
                label: t.preset_rust_trait_impl().to_string(),
                pattern: "impl $TRAIT for $TYPE",
                description: t.preset_rust_trait_impl_desc().to_string(),
            },
            Preset {
                label: t.preset_rust_unwrap().to_string(),
                pattern: "$VAR.unwrap()",
                description: t.preset_rust_unwrap_desc().to_string(),
            },
            Preset {
                label: t.preset_rust_clone().to_string(),
                pattern: "$VAR.clone()",
                description: t.preset_rust_clone_desc().to_string(),
            },
            Preset {
                label: t.preset_rust_panic().to_string(),
                pattern: "panic!($$$ARGS)",
                description: t.preset_rust_panic_desc().to_string(),
            },
        ],
        SupportedLanguage::Java => vec![
            Preset {
                label: t.preset_java_null().to_string(),
                pattern: "$VAR == null",
                description: t.preset_java_null_desc().to_string(),
            },
            Preset {
                label: t.preset_java_println().to_string(),
                pattern: "System.out.println($$$ARGS)",
                description: t.preset_java_println_desc().to_string(),
            },
        ],
        SupportedLanguage::Python => vec![
            Preset {
                label: t.preset_py_print().to_string(),
                pattern: "print($$$ARGS)",
                description: t.preset_py_print_desc().to_string(),
            },
            Preset {
                label: t.preset_py_import().to_string(),
                pattern: "import $MODULE",
                description: t.preset_py_import_desc().to_string(),
            },
        ],
        SupportedLanguage::JavaScript | SupportedLanguage::TypeScript => vec![Preset {
            label: t.preset_js_console().to_string(),
            pattern: "console.log($$$ARGS)",
            description: t.preset_js_console_desc().to_string(),
        }],
        SupportedLanguage::Go => vec![Preset {
            label: t.preset_go_err().to_string(),
            pattern: "if err != nil { $$$BODY }",
            description: t.preset_go_err_desc().to_string(),
        }],
        SupportedLanguage::Kotlin => vec![
            Preset {
                label: t.preset_kotlin_println().to_string(),
                pattern: "println($$$ARGS)",
                description: t.preset_kotlin_println_desc().to_string(),
            },
            Preset {
                label: t.preset_kotlin_fun().to_string(),
                pattern: "fun $NAME($$$ARGS) { $$$BODY }",
                description: t.preset_kotlin_fun_desc().to_string(),
            },
        ],
        SupportedLanguage::Scala => vec![
            Preset {
                label: t.preset_scala_println().to_string(),
                pattern: "println($$$ARGS)",
                description: t.preset_scala_println_desc().to_string(),
            },
            Preset {
                label: t.preset_scala_def().to_string(),
                pattern: "def $NAME($$$ARGS) = $EXPR",
                description: t.preset_scala_def_desc().to_string(),
            },
        ],
        _ => vec![Preset {
            label: t.preset_generic_any().to_string(),
            pattern: "$EXPR",
            description: t.preset_generic_any_desc().to_string(),
        }],
    }
}
