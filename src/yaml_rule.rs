//! 内蔵 ast-grep YAML rule エンジン（外部 CLI 不要）

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use ast_grep_config::{
    from_yaml_string, DeserializeEnv, GlobalRules, RuleConfig, SerializableGlobalRule, Severity,
};
use ast_grep_language::SupportLang;
use regex::Regex;
use serde::Deserialize;

use crate::lang::SupportedLanguage;

/// YAML rule 検索モードのオプション
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct YamlRuleOptions {
    /// `sgconfig.yml` のパス（空なら検索ディレクトリから上方向に自動検出）
    #[serde(default)]
    pub config_path: String,
    /// 単一 rule YAML ファイル（空なら `ruleDirs` を使用）
    #[serde(default)]
    pub rule_file: String,
    /// rule id の正規表現フィルタ（空なら全 rule）
    #[serde(default)]
    pub rule_filter: String,
}

impl YamlRuleOptions {
    pub fn is_configured(&self) -> bool {
        !self.config_path.trim().is_empty() || !self.rule_file.trim().is_empty()
    }
}

/// ロード済み rule セット（検索スレッドで共有）
pub struct YamlRuleSet {
    pub rules: Vec<RuleConfig<SupportLang>>,
    /// 走査対象の拡張子（ドットなし・小文字）
    pub extensions: HashSet<String>,
    /// `languageGlobs` 由来の (glob, language)
    glob_langs: Vec<(GlobMatcher, SupportLang)>,
}

/// sgconfig.yml の最小サブセット
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectConfigFile {
    #[serde(default)]
    rule_dirs: Vec<String>,
    #[serde(default)]
    util_dirs: Vec<String>,
    #[serde(default)]
    language_globs: HashMap<String, Vec<String>>,
    #[serde(default)]
    custom_languages: Option<serde_yaml::Value>,
}

#[derive(Clone)]
struct GlobMatcher {
    regex: Regex,
}

impl GlobMatcher {
    fn new(glob: &str) -> Option<Self> {
        let mut re = String::from("^");
        let glob = glob.replace('\\', "/");
        let mut chars = glob.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '*' => {
                    if chars.peek() == Some(&'*') {
                        chars.next();
                        re.push_str(".*");
                    } else {
                        re.push_str("[^/]*");
                    }
                }
                '?' => re.push('.'),
                '.' | '+' | '^' | '$' | '|' | '(' | ')' | '[' | ']' | '{' | '}' => {
                    re.push('\\');
                    re.push(c);
                }
                _ => re.push(c),
            }
        }
        re.push('$');
        Regex::new(&re).ok().map(|regex| Self { regex })
    }

    fn is_match(&self, path: &str) -> bool {
        let path = path.replace('\\', "/");
        self.regex.is_match(&path)
    }
}

/// 検索開始に必要な YAML 入力があるか
pub fn yaml_rule_input_ready(search_dir: &str, options: &YamlRuleOptions) -> bool {
    if options.is_configured() {
        return true;
    }
    find_sgconfig(Path::new(search_dir)).is_some()
}

/// 検索ディレクトリから `sgconfig.yml` / `sgconfig.yaml` を上方向に探す
pub fn find_sgconfig(start: &Path) -> Option<PathBuf> {
    let mut dir = if start.is_dir() {
        start.to_path_buf()
    } else {
        start.parent()?.to_path_buf()
    };
    loop {
        for name in ["sgconfig.yml", "sgconfig.yaml"] {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        if !dir.pop() {
            break;
        }
    }
    None
}

fn severity_to_string(severity: &Severity) -> String {
    format!("{severity:?}")
}

fn parse_support_lang_name(name: &str) -> Option<SupportLang> {
    let normalized = name.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "bash" | "sh" => Some(SupportLang::Bash),
        "c" => Some(SupportLang::C),
        "cpp" | "c++" => Some(SupportLang::Cpp),
        "csharp" | "c#" | "cs" => Some(SupportLang::CSharp),
        "css" => Some(SupportLang::Css),
        "elixir" => Some(SupportLang::Elixir),
        "go" => Some(SupportLang::Go),
        "html" => Some(SupportLang::Html),
        "java" => Some(SupportLang::Java),
        "javascript" | "js" => Some(SupportLang::JavaScript),
        "json" => Some(SupportLang::Json),
        "kotlin" | "kt" => Some(SupportLang::Kotlin),
        "lua" => Some(SupportLang::Lua),
        "php" => Some(SupportLang::Php),
        "python" | "py" => Some(SupportLang::Python),
        "ruby" | "rb" => Some(SupportLang::Ruby),
        "rust" | "rs" => Some(SupportLang::Rust),
        "scala" => Some(SupportLang::Scala),
        "solidity" | "sol" => Some(SupportLang::Solidity),
        "swift" => Some(SupportLang::Swift),
        "typescript" | "ts" => Some(SupportLang::TypeScript),
        "yaml" | "yml" => Some(SupportLang::Yaml),
        _ => None,
    }
}

/// SupportLang のデフォルト拡張子（内蔵言語）
pub fn default_extensions_for_lang(lang: SupportLang) -> &'static [&'static str] {
    match lang {
        SupportLang::Rust => &["rs"],
        SupportLang::Java => &["java"],
        SupportLang::Python => &["py", "pyi"],
        SupportLang::JavaScript => &["js", "jsx", "mjs", "cjs"],
        SupportLang::TypeScript => &["ts", "tsx", "mts", "cts"],
        SupportLang::Go => &["go"],
        SupportLang::C => &["c", "h"],
        SupportLang::Cpp => &["cpp", "cc", "cxx", "h", "hpp", "hh", "hxx"],
        SupportLang::CSharp => &["cs"],
        SupportLang::Kotlin => &["kt", "kts", "ktm"],
        SupportLang::Scala => &["scala", "sc", "sbt"],
        SupportLang::Html => &["html", "htm"],
        SupportLang::Css => &["css"],
        SupportLang::Json => &["json"],
        SupportLang::Yaml => &["yaml", "yml"],
        SupportLang::Bash => &["sh", "bash"],
        SupportLang::Php => &["php"],
        SupportLang::Ruby => &["rb"],
        SupportLang::Lua => &["lua"],
        SupportLang::Swift => &["swift"],
        SupportLang::Solidity => &["sol"],
        SupportLang::Elixir => &["ex", "exs"],
        _ => &[],
    }
}

pub fn support_lang_to_supported_language(lang: SupportLang) -> Option<SupportedLanguage> {
    match lang {
        SupportLang::Rust => Some(SupportedLanguage::Rust),
        SupportLang::Java => Some(SupportedLanguage::Java),
        SupportLang::Python => Some(SupportedLanguage::Python),
        SupportLang::JavaScript => Some(SupportedLanguage::JavaScript),
        SupportLang::TypeScript => Some(SupportedLanguage::TypeScript),
        SupportLang::Go => Some(SupportedLanguage::Go),
        SupportLang::C => Some(SupportedLanguage::C),
        SupportLang::Cpp => Some(SupportedLanguage::Cpp),
        SupportLang::CSharp => Some(SupportedLanguage::CSharp),
        SupportLang::Kotlin => Some(SupportedLanguage::Kotlin),
        SupportLang::Scala => Some(SupportedLanguage::Scala),
        _ => None,
    }
}

fn collect_yaml_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if !dir.is_dir() {
        return out;
    }
    let walker = jwalk::WalkDir::new(dir).follow_links(false);
    for entry in walker.into_iter().flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("yml") | Some("yaml")
        ) {
            out.push(path);
        }
    }
    out.sort();
    out
}

fn read_yaml_documents(path: &Path) -> Result<Vec<String>, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(split_yaml_documents(&text))
}

fn split_yaml_documents(text: &str) -> Vec<String> {
    let mut docs = Vec::new();
    let mut current = String::new();
    for line in text.lines() {
        if line.trim() == "---" {
            if !current.trim().is_empty() {
                docs.push(current.clone());
            }
            current.clear();
        } else {
            if !current.is_empty() {
                current.push('\n');
            }
            current.push_str(line);
        }
    }
    if !current.trim().is_empty() {
        docs.push(current);
    }
    if docs.is_empty() && !text.trim().is_empty() {
        docs.push(text.to_string());
    }
    docs
}

fn load_global_rules(util_dirs: &[PathBuf]) -> Result<GlobalRules, String> {
    let mut globals = Vec::new();
    for dir in util_dirs {
        for path in collect_yaml_files(dir) {
            let docs = read_yaml_documents(&path)?;
            for doc in docs {
                match serde_yaml::from_str::<SerializableGlobalRule<SupportLang>>(&doc) {
                    Ok(rule) => globals.push(rule),
                    Err(e) => {
                        return Err(format!(
                            "util rule {}: {e}",
                            path.display()
                        ));
                    }
                }
            }
        }
    }
    DeserializeEnv::parse_global_utils(globals).map_err(|e| format!("global utils: {e}"))
}

fn load_rules_from_text(
    text: &str,
    globals: &GlobalRules,
    id_filter: Option<&Regex>,
) -> Result<Vec<RuleConfig<SupportLang>>, String> {
    let docs = split_yaml_documents(text);
    let mut all = Vec::new();
    for doc in docs {
        match from_yaml_string::<SupportLang>(&doc, globals) {
            Ok(mut rules) => all.append(&mut rules),
            Err(e) => return Err(format!("rule YAML: {e}")),
        }
    }
    if let Some(re) = id_filter {
        all.retain(|r| re.is_match(&r.id));
    }
    Ok(all)
}

fn load_rules_from_paths(
    paths: &[PathBuf],
    globals: &GlobalRules,
    id_filter: Option<&Regex>,
) -> Result<Vec<RuleConfig<SupportLang>>, String> {
    let mut all = Vec::new();
    for path in paths {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        all.extend(load_rules_from_text(&text, globals, id_filter)?);
    }
    Ok(all)
}

fn custom_languages_present(value: &serde_yaml::Value) -> bool {
    match value {
        serde_yaml::Value::Null => false,
        serde_yaml::Value::Mapping(m) => !m.is_empty(),
        serde_yaml::Value::Sequence(s) => !s.is_empty(),
        _ => true,
    }
}

fn resolve_config_path(search_dir: &Path, options: &YamlRuleOptions) -> Result<PathBuf, String> {
    let explicit = options.config_path.trim();
    if !explicit.is_empty() {
        let p = PathBuf::from(explicit);
        if p.is_file() {
            return Ok(p);
        }
        return Err(format!("sgconfig not found: {explicit}"));
    }
    find_sgconfig(search_dir).ok_or_else(|| {
        "sgconfig.yml が見つかりません。パスを指定するか rule YAML ファイルを指定してください"
            .to_string()
    })
}

/// rule YAML / sgconfig から rule をロードする
pub fn load_yaml_rules(
    search_dir: &Path,
    options: &YamlRuleOptions,
) -> Result<YamlRuleSet, String> {
    let id_filter = if options.rule_filter.trim().is_empty() {
        None
    } else {
        Some(
            Regex::new(options.rule_filter.trim())
                .map_err(|e| format!("rule id filter: {e}"))?,
        )
    };

    let rule_file = options.rule_file.trim();
    let (globals, project, config_root) = if rule_file.is_empty() {
        let config_path = resolve_config_path(search_dir, options)?;
        let config_root = config_path
            .parent()
            .unwrap_or(search_dir)
            .to_path_buf();
        let text = std::fs::read_to_string(&config_path)
            .map_err(|e| format!("{}: {e}", config_path.display()))?;
        let project: ProjectConfigFile = serde_yaml::from_str(&text)
            .map_err(|e| format!("sgconfig parse: {e}"))?;
        if let Some(ref custom) = project.custom_languages {
            if custom_languages_present(custom) {
                return Err(
                    "customLanguages は exe 単体モードでは非対応です（内蔵言語のみ使用できます）"
                        .into(),
                );
            }
        }
        let util_dirs: Vec<PathBuf> = project
            .util_dirs
            .iter()
            .map(|d| config_root.join(d))
            .collect();
        let globals = load_global_rules(&util_dirs)?;
        (globals, Some(project), config_root)
    } else {
        let globals = GlobalRules::default();
        (globals, None, search_dir.to_path_buf())
    };

    let language_globs = project
        .as_ref()
        .map(|p| p.language_globs.clone())
        .unwrap_or_default();

    let rules = if !rule_file.is_empty() {
        let path = PathBuf::from(rule_file);
        if !path.is_file() {
            return Err(format!("rule file not found: {rule_file}"));
        }
        load_rules_from_paths(&[path], &globals, id_filter.as_ref())?
    } else {
        let project = project.expect("project config");
        let mut rule_paths = Vec::new();
        for dir in &project.rule_dirs {
            rule_paths.extend(collect_yaml_files(&config_root.join(dir)));
        }
        load_rules_from_paths(&rule_paths, &globals, id_filter.as_ref())?
    };

    if rules.is_empty() {
        return Err("実行可能な rule がありません（フィルタ条件を確認してください）".into());
    }

    let mut extensions = HashSet::new();
    let mut glob_langs = Vec::new();

    if !language_globs.is_empty() {
        for (lang_name, globs) in language_globs {
            let Some(lang) = parse_support_lang_name(&lang_name) else {
                continue;
            };
            for ext in default_extensions_for_lang(lang) {
                extensions.insert(ext.to_string());
            }
            for glob in globs {
                if let Some(matcher) = GlobMatcher::new(&glob) {
                    glob_langs.push((matcher, lang));
                }
            }
        }
    }

    for rule in &rules {
        for ext in default_extensions_for_lang(rule.language) {
            extensions.insert(ext.to_string());
        }
    }

    Ok(YamlRuleSet {
        rules,
        extensions,
        glob_langs,
    })
}

impl YamlRuleSet {
    /// 走査フィルタ: 拡張子が rule 言語と一致するか
    pub fn extension_might_match(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| self.extensions.contains(&e.to_lowercase()))
            .unwrap_or(false)
    }

    /// ファイルの解析言語を決定（`languageGlobs` → 拡張子）
    pub fn resolve_file_language(
        &self,
        path: &Path,
        relative_path: &str,
        selected: SupportedLanguage,
    ) -> Option<SupportLang> {
        for (matcher, lang) in &self.glob_langs {
            if matcher.is_match(relative_path) {
                if let Some(sel) = selected.to_support_lang() {
                    if sel != *lang {
                        return None;
                    }
                }
                return Some(*lang);
            }
        }

        let ext = path.extension().and_then(|e| e.to_str())?.to_lowercase();
        let mut candidate = None;
        for rule in &self.rules {
            if default_extensions_for_lang(rule.language)
                .iter()
                .any(|e| *e == ext.as_str())
            {
                candidate = Some(rule.language);
                break;
            }
        }
        let lang = candidate?;

        if let Some(sel) = selected.to_support_lang() {
            if sel != lang {
                return None;
            }
        }
        Some(lang)
    }

    /// rule の `files` / `ignores`（ast-grep-config 0.42 では型が非公開のため未適用）
    pub fn rule_applies_to_path(_rule: &RuleConfig<SupportLang>, _relative_path: &str) -> bool {
        true
    }

    pub fn rules_for_file(
        &self,
        path: &Path,
        relative_path: &str,
        selected: SupportedLanguage,
    ) -> Vec<&RuleConfig<SupportLang>> {
        let Some(file_lang) = self.resolve_file_language(path, relative_path, selected) else {
            return Vec::new();
        };
        self.rules
            .iter()
            .filter(|r| r.language == file_lang)
            .filter(|r| Self::rule_applies_to_path(r, relative_path))
            .collect()
    }
}

fn extract_fix_from_rule(rule: &RuleConfig<SupportLang>) -> Option<String> {
    let value = serde_yaml::to_value(&rule.core).ok()?;
    let fix = value.get("fix")?;
    fix_value_to_string(fix)
}

fn fix_value_to_string(v: &serde_yaml::Value) -> Option<String> {
    match v {
        serde_yaml::Value::String(s) => Some(s.clone()),
        serde_yaml::Value::Mapping(m) => m
            .get("template")
            .and_then(|t| t.as_str())
            .map(str::to_string),
        serde_yaml::Value::Sequence(seq) => {
            let parts: Vec<String> = seq.iter().filter_map(fix_value_to_string).collect();
            if parts.is_empty() {
                None
            } else {
                Some(parts.join("; "))
            }
        }
        _ => None,
    }
}

/// rule metadata を MatchItem 用フィールドに変換
pub fn rule_metadata(rule: &RuleConfig<SupportLang>) -> (String, String, String, Option<String>) {
    let replacement = extract_fix_from_rule(rule);
    (
        rule.id.clone(),
        rule.message.clone(),
        severity_to_string(&rule.severity),
        replacement,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("{prefix}-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_rule(dir: &Path, name: &str, body: &str) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, body).unwrap();
        p
    }

    #[test]
    fn split_yaml_documents_multiple() {
        let text = "id: a\nrule:\n  pattern: foo\n---\nid: b\nrule:\n  pattern: bar\n";
        let docs = split_yaml_documents(text);
        assert_eq!(docs.len(), 2);
    }

    #[test]
    fn load_single_rule_file() {
        let dir = temp_dir("yaml-rule-single");
        write_rule(
            &dir,
            "test.yml",
            r#"
id: test-rule
language: Rust
rule:
  kind: function_item
  pattern: fn $NAME
message: found function
severity: warning
"#,
        );
        let opts = YamlRuleOptions {
            rule_file: dir.join("test.yml").to_string_lossy().into(),
            ..Default::default()
        };
        let set = load_yaml_rules(&dir, &opts).unwrap();
        assert_eq!(set.rules.len(), 1);
        assert_eq!(set.rules[0].id, "test-rule");
    }

    #[test]
    fn rule_filter_regex() {
        let dir = temp_dir("yaml-rule-filter");
        write_rule(
            &dir,
            "rules.yml",
            r#"---
id: keep-me
language: Rust
rule:
  kind: function_item
  pattern: fn $X
message: ok
---
id: drop-me
language: Rust
rule:
  kind: struct_item
  pattern: struct $X
message: no
"#,
        );
        let opts = YamlRuleOptions {
            rule_file: dir.join("rules.yml").to_string_lossy().into(),
            rule_filter: "keep".into(),
            ..Default::default()
        };
        let set = load_yaml_rules(&dir, &opts).unwrap();
        assert_eq!(set.rules.len(), 1);
        assert_eq!(set.rules[0].id, "keep-me");
    }

    #[test]
    fn sgconfig_rule_dirs_recursive() {
        let dir = temp_dir("yaml-rule-dirs");
        let rules_dir = dir.join("rules").join("nested");
        std::fs::create_dir_all(&rules_dir).unwrap();
        write_rule(
            &rules_dir,
            "r.yml",
            r#"
id: nested-rule
language: JavaScript
rule:
  pattern: console.log($X)
message: log
"#,
        );
        let sgconfig = dir.join("sgconfig.yml");
        std::fs::write(
            &sgconfig,
            "ruleDirs:\n  - rules\n",
        )
        .unwrap();
        let set = load_yaml_rules(&dir, &YamlRuleOptions::default()).unwrap();
        assert!(set.rules.iter().any(|r| r.id == "nested-rule"));
    }

    #[test]
    fn custom_languages_error() {
        let dir = temp_dir("yaml-rule-custom");
        std::fs::write(
            dir.join("sgconfig.yml"),
            "customLanguages:\n  mylang:\n    libraryPath: foo.so\n",
        )
        .unwrap();
        let Err(err) = load_yaml_rules(&dir, &YamlRuleOptions::default()) else {
            panic!("expected customLanguages error");
        };
        assert!(err.contains("customLanguages"));
    }

    #[test]
    fn yaml_rule_input_ready_without_pattern() {
        assert!(crate::yaml_rule::yaml_rule_input_ready(
            ".",
            &crate::search::YamlRuleOptions {
                rule_file: "rules.yml".into(),
                ..Default::default()
            },
        ));
        assert!(!crate::search::YamlRuleOptions::default().is_configured());
    }

    #[test]
    fn glob_matcher_basic() {
        let m = GlobMatcher::new("*.vue").unwrap();
        assert!(m.is_match("App.vue"));
        assert!(!m.is_match("src/App.vue"));
        let m2 = GlobMatcher::new("**/*.vue").unwrap();
        assert!(m2.is_match("src/App.vue"));
    }

    #[test]
    fn yaml_rule_matches_rust_function() {
        use ast_grep_language::LanguageExt;

        let dir = temp_dir("yaml-rule-match");
        write_rule(
            &dir,
            "match.yml",
            r#"
id: fn-rule
language: Rust
rule:
  kind: function_item
  pattern: fn $NAME
message: function
severity: warning
fix: pub fn $NAME
"#,
        );
        std::fs::write(dir.join("sample.rs"), "fn hello() {}\n").unwrap();
        let opts = YamlRuleOptions {
            rule_file: dir.join("match.yml").to_string_lossy().into(),
            ..Default::default()
        };
        let set = load_yaml_rules(&dir, &opts).unwrap();
        let rules = set.rules_for_file(
            &dir.join("sample.rs"),
            "sample.rs",
            SupportedLanguage::Auto,
        );
        assert_eq!(rules.len(), 1);
        let source = std::fs::read_to_string(dir.join("sample.rs")).unwrap();
        let root = SupportLang::Rust.ast_grep(&source);
        let matches: Vec<_> = root.root().find_all(&rules[0].matcher).collect();
        assert_eq!(matches.len(), 1);
        let (id, msg, sev, fix) = rule_metadata(rules[0]);
        assert_eq!(id, "fn-rule");
        assert_eq!(msg, "function");
        assert_eq!(sev, "Warning");
        assert_eq!(fix.as_deref(), Some("pub fn $NAME"));
    }
}
