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
