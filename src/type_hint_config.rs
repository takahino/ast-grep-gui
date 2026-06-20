//! C++ 型ヒント補助設定（YAML 入出力・ルール照合）。

use std::path::Path;

use serde::{Deserialize, Serialize};

/// ファイルに保存する型ヒント補助設定（YAML）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TypeHintConfigFile {
    #[serde(default = "config_file_version_default")]
    pub version: u32,
    #[serde(default)]
    pub cpp: CppTypeHintRules,
}

fn config_file_version_default() -> u32 {
    1
}

impl TypeHintConfigFile {
    pub const CURRENT_VERSION: u32 = 1;

    pub fn new(cpp: CppTypeHintRules) -> Self {
        Self {
            version: Self::CURRENT_VERSION,
            cpp,
        }
    }

    pub fn rule_count(&self) -> usize {
        self.cpp.rule_count()
    }
}

/// C++ 向けルール群
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CppTypeHintRules {
    #[serde(default)]
    pub methods: Vec<CppMethodRule>,
    #[serde(default)]
    pub functions: Vec<CppCallableRule>,
    #[serde(default)]
    pub macros: Vec<CppCallableRule>,
    #[serde(default)]
    pub constants: Vec<CppConstantRule>,
    #[serde(default)]
    pub fields: Vec<CppFieldRule>,
    #[serde(default)]
    pub binary_ops: Vec<CppBinaryOpRule>,
}

impl CppTypeHintRules {
    pub fn rule_count(&self) -> usize {
        self.methods.len()
            + self.functions.len()
            + self.macros.len()
            + self.constants.len()
            + self.fields.len()
            + self.binary_ops.len()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CppMethodRule {
    pub class: String,
    pub method: String,
    #[serde(default)]
    pub arity: Option<usize>,
    #[serde(default)]
    pub params: Vec<String>,
    pub returns: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CppCallableRule {
    pub name: String,
    #[serde(default)]
    pub arity: Option<usize>,
    #[serde(default)]
    pub params: Vec<String>,
    pub returns: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CppConstantRule {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CppFieldRule {
    pub class: String,
    pub field: String,
    #[serde(rename = "type")]
    pub ty: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CppBinaryOpRule {
    pub op: String,
    pub lhs: String,
    pub rhs: String,
    pub returns: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

/// 実行時に参照する補助設定（Arc 共有用）
#[derive(Debug, Clone, Default)]
pub struct TypeHintConfig {
    pub cpp: CppTypeHintRules,
}

impl TypeHintConfig {
    pub fn from_file(file: TypeHintConfigFile) -> Self {
        Self { cpp: file.cpp }
    }

    pub fn rule_count(&self) -> usize {
        self.cpp.rule_count()
    }

    pub fn lookup_cpp_method_return(
        &self,
        class_name: &str,
        method_name: &str,
        arg_types: &[String],
    ) -> Option<String> {
        resolve_callable_rules(
            self.cpp
                .methods
                .iter()
                .filter(|r| r.enabled)
                .filter(|r| r.class == class_name && r.method == method_name)
                .map(|r| CallableRuleView {
                    arity: r.arity,
                    params: &r.params,
                    returns: &r.returns,
                }),
            arg_types,
        )
    }

    pub fn lookup_cpp_function_return(
        &self,
        name: &str,
        arg_types: &[String],
    ) -> Option<String> {
        resolve_callable_rules(
            self.cpp
                .functions
                .iter()
                .filter(|r| r.enabled)
                .filter(|r| r.name == name)
                .map(|r| CallableRuleView {
                    arity: r.arity,
                    params: &r.params,
                    returns: &r.returns,
                }),
            arg_types,
        )
    }

    pub fn lookup_cpp_macro_return(
        &self,
        name: &str,
        arg_types: &[String],
    ) -> Option<String> {
        resolve_callable_rules(
            self.cpp
                .macros
                .iter()
                .filter(|r| r.enabled)
                .filter(|r| r.name == name)
                .map(|r| CallableRuleView {
                    arity: r.arity,
                    params: &r.params,
                    returns: &r.returns,
                }),
            arg_types,
        )
    }

    pub fn lookup_cpp_constant_type(&self, name: &str) -> Option<String> {
        self.cpp
            .constants
            .iter()
            .filter(|r| r.enabled && r.name == name)
            .map(|r| r.ty.clone())
            .next()
    }

    pub fn lookup_cpp_field_type(&self, class_name: &str, field_name: &str) -> Option<String> {
        self.cpp
            .fields
            .iter()
            .filter(|r| r.enabled && r.class == class_name && r.field == field_name)
            .map(|r| r.ty.clone())
            .next()
    }

    pub fn lookup_cpp_binary_op_return(
        &self,
        op: &str,
        lhs_type: &str,
        rhs_type: &str,
    ) -> Option<String> {
        let mut matches: Vec<&CppBinaryOpRule> = self
            .cpp
            .binary_ops
            .iter()
            .filter(|r| r.enabled && r.op == op && r.lhs == lhs_type && r.rhs == rhs_type)
            .collect();
        if matches.len() == 1 {
            Some(matches.remove(0).returns.clone())
        } else {
            None
        }
    }
}

struct CallableRuleView<'a> {
    arity: Option<usize>,
    params: &'a [String],
    returns: &'a str,
}

fn resolve_callable_rules<'a, I>(rules: I, arg_types: &[String]) -> Option<String>
where
    I: Iterator<Item = CallableRuleView<'a>>,
{
    let rules: Vec<CallableRuleView<'a>> = rules.collect();
    if rules.is_empty() {
        return None;
    }

    let params_matches: Vec<_> = rules
        .iter()
        .filter(|r| !r.params.is_empty() && params_match(r.params, arg_types))
        .collect();
    if params_matches.len() == 1 {
        return Some(params_matches[0].returns.to_string());
    }
    if params_matches.len() > 1 {
        return None;
    }

    let arity = arg_types.len();
    let arity_matches: Vec<_> = rules
        .iter()
        .filter(|r| r.arity == Some(arity))
        .collect();
    if arity_matches.len() == 1 {
        return Some(arity_matches[0].returns.to_string());
    }
    if arity_matches.len() > 1 {
        return None;
    }

    let unconditional: Vec<_> = rules
        .iter()
        .filter(|r| r.arity.is_none() && r.params.is_empty())
        .collect();
    if unconditional.len() == 1 {
        return Some(unconditional[0].returns.to_string());
    }
    None
}

fn params_match(expected: &[String], actual: &[String]) -> bool {
    if expected.len() != actual.len() {
        return false;
    }
    expected
        .iter()
        .zip(actual.iter())
        .all(|(e, a)| simplify_type_name(e) == simplify_type_name(a))
}

/// 設定照合用の型名簡略化（`receiver_hint::cpp_simplify_type_name` と同等の方針）
pub fn simplify_type_name(ty: &str) -> String {
    let mut s = ty.trim();
    while s.ends_with('*') || s.ends_with('&') {
        s = s.trim_end_matches(|x: char| x == '*' || x == '&').trim();
    }
    for prefix in ["const ", "volatile ", "const volatile ", "volatile const "] {
        if let Some(rest) = s.strip_prefix(prefix) {
            s = rest.trim();
        }
    }
    let s = if let Some(i) = s.rfind("::") {
        s[i + 2..].trim()
    } else {
        s
    };
    s.split_whitespace().last().unwrap_or(s).to_string()
}

pub fn type_hint_config_to_yaml_string(config: &TypeHintConfigFile) -> anyhow::Result<String> {
    Ok(serde_yaml::to_string(config)?)
}

pub fn parse_type_hint_config_str(s: &str) -> anyhow::Result<TypeHintConfigFile> {
    let file: TypeHintConfigFile = serde_yaml::from_str(s)?;
    if file.version > TypeHintConfigFile::CURRENT_VERSION {
        anyhow::bail!(
            "unsupported type hint config version {} (max {})",
            file.version,
            TypeHintConfigFile::CURRENT_VERSION
        );
    }
    Ok(file)
}

pub fn write_type_hint_config_file(path: &Path, config: &TypeHintConfigFile) -> anyhow::Result<()> {
    let yaml = type_hint_config_to_yaml_string(config)?;
    std::fs::write(path, yaml)?;
    Ok(())
}

pub fn read_type_hint_config_file(path: &Path) -> anyhow::Result<TypeHintConfigFile> {
    let s = std::fs::read_to_string(path)?;
    parse_type_hint_config_str(&s)
}

// ─── GUI ルール種別 ─────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TypeHintRuleKind {
    #[default]
    Methods,
    Functions,
    Macros,
    Constants,
    Fields,
    BinaryOps,
}

/// 検索結果の未解決セルから設定画面へ渡すドラフト
#[derive(Debug, Clone, Default)]
pub struct PendingTypeHintRuleDraft {
    pub kind: TypeHintRuleKind,
    pub source_snippet: String,
    pub kind_label: String,
    pub column_key: String,
    pub file: String,
    pub line: usize,
    pub class: String,
    pub method: String,
    pub name: String,
    pub field: String,
    pub arity: Option<usize>,
    pub params: Vec<String>,
    pub returns: String,
    pub ty: String,
    pub op: String,
    pub lhs: String,
    pub rhs: String,
    pub focus_returns: bool,
}

/// 表・サマリーの表示文字列からドラフト素材を取り出す結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HintRuleDraftSource {
    pub kind_label: String,
    pub snippet: String,
}

/// `CTime.Format` のような推論済みクラスメソッド表示か（表セル右クリック対象の判定用）。
pub fn looks_like_class_method_label(s: &str) -> bool {
    draft_from_class_method_label(s.trim()).is_some()
}

/// 表・サマリーの表示文字列から、型補助ルール追加メニューを出せるか判定する。
pub fn hint_rule_draft_source_from_display(display: &str) -> Option<HintRuleDraftSource> {
    let d = display.trim();
    if d.is_empty() || d == "·" {
        return None;
    }
    if d.starts_with('?') {
        let (kind_label, snippet) = parse_unknown_hint_export_display(d);
        return Some(HintRuleDraftSource {
            kind_label,
            snippet,
        });
    }
    if looks_like_class_method_label(d) {
        return Some(HintRuleDraftSource {
            kind_label: "Inferred".to_string(),
            snippet: d.to_string(),
        });
    }
    None
}

/// 表示文字列（`? (kind) (snippet)` など）から型ラベル用の文字列を得る。
pub fn type_label_from_display_value(display: &str) -> String {
    if let Some(src) = hint_rule_draft_source_from_display(display) {
        if !src.snippet.is_empty() {
            return src.snippet;
        }
        if !src.kind_label.is_empty() && src.kind_label != "Inferred" {
            return src.kind_label;
        }
    }
    display.trim().to_string()
}

/// 表ビューの同一マッチ行から Methods ドラフト用の引数数・引数型ラベルを抽出する。
pub fn call_context_from_column_keys(
    column_keys: &[String],
    display_for_key: impl Fn(&str) -> String,
) -> (usize, Vec<String>) {
    let mut arity_from_col = None;
    let mut arg_entries: Vec<(usize, String)> = Vec::new();
    for key in column_keys {
        if key.ends_with("#arity") {
            let v = display_for_key(key);
            let label = type_label_from_display_value(&v);
            if let Ok(n) = label.parse::<usize>() {
                arity_from_col = Some(n);
            }
        } else if let Some((base, idx_str)) = key.rsplit_once('#') {
            if !base.is_empty()
                && !idx_str.is_empty()
                && idx_str.chars().all(|c| c.is_ascii_digit())
            {
                if let Ok(idx) = idx_str.parse::<usize>() {
                    arg_entries.push((idx, display_for_key(key)));
                }
            }
        }
    }
    if !arg_entries.is_empty() {
        arg_entries.sort_by_key(|(i, _)| *i);
        let labels: Vec<String> = arg_entries
            .iter()
            .map(|(_, s)| type_label_from_display_value(s))
            .collect();
        let arity = arity_from_col.unwrap_or(labels.len());
        return (arity, labels);
    }
    (arity_from_col.unwrap_or(0), Vec::new())
}

/// Methods ドラフトに同一行の呼び出し文脈（引数数・引数型）をマージする。
pub fn enrich_method_draft_from_call_context(
    draft: &mut PendingTypeHintRuleDraft,
    arity: usize,
    arg_type_labels: &[String],
) {
    if draft.kind != TypeHintRuleKind::Methods {
        return;
    }
    if draft.arity.is_none() {
        draft.arity = Some(arity);
    }
    if draft.params.is_empty() && !arg_type_labels.is_empty() {
        draft.params = arg_type_labels
            .iter()
            .map(|s| type_label_from_display_value(s))
            .collect();
    }
}

/// 検索結果の型ヒントセル（未解決・推論済み）からルール追加ドラフトを構築する。
pub fn draft_from_hint_cell(
    column_key: &str,
    kind_label: &str,
    source_snippet: &str,
    file: &str,
    line: usize,
) -> PendingTypeHintRuleDraft {
    draft_from_unknown_hint(column_key, kind_label, source_snippet, file, line)
}

/// 未解決ヒントのソース断片からルール追加ドラフトを構築する
pub fn draft_from_unknown_hint(
    column_key: &str,
    kind_label: &str,
    source_snippet: &str,
    file: &str,
    line: usize,
) -> PendingTypeHintRuleDraft {
    let original_snippet = source_snippet.trim().to_string();
    let mut draft = draft_body_from_snippet(&original_snippet, kind_label);
    draft.column_key = column_key.to_string();
    draft.kind_label = kind_label.to_string();
    draft.source_snippet = original_snippet;
    draft.file = file.to_string();
    draft.line = line;
    draft
}

fn draft_body_from_snippet(snippet: &str, kind_label: &str) -> PendingTypeHintRuleDraft {
    let mut work = snippet.trim().to_string();
    if kind_label == "ParenthesizedExpression" {
        work = strip_balanced_outer_parens(&work);
    } else {
        let stripped = strip_balanced_outer_parens(&work);
        if stripped != work {
            work = stripped;
        }
    }

    if let Some(d) = draft_from_class_method_label(&work) {
        return d;
    }
    if let Some(d) = draft_from_method_call(&work) {
        return d;
    }
    if let Some(d) = draft_from_macro_or_function_call(&work) {
        return d;
    }
    if let Some(d) = draft_from_field_access(&work) {
        return d;
    }
    if let Some(d) = draft_from_binary_op(&work) {
        return d;
    }
    if let Some(d) = draft_from_constant_identifier(&work, kind_label) {
        return d;
    }

    PendingTypeHintRuleDraft {
        source_snippet: snippet.to_string(),
        focus_returns: true,
        ..Default::default()
    }
}

/// [`TypeHintCell::to_export_string`] 形式 `? (kind) (snippet)` を逆パースする。
pub fn parse_unknown_hint_export_display(display: &str) -> (String, String) {
    let s = display.trim();
    if s == "?" {
        return (String::new(), String::new());
    }
    if !s.starts_with('?') {
        return (String::new(), s.to_string());
    }
    let rest = s[1..].trim();
    if rest.is_empty() {
        return (String::new(), String::new());
    }
    if !rest.starts_with('(') {
        return (String::new(), String::new());
    }
    let Some((kind, rest2)) = parse_paren_group(rest) else {
        return (String::new(), String::new());
    };
    let rest2 = rest2.trim();
    if rest2.is_empty() {
        return (kind, String::new());
    }
    if rest2.starts_with('(') {
        if let Some((snippet, _)) = parse_paren_group(rest2) {
            return (kind, snippet);
        }
    }
    (kind, String::new())
}

fn parse_paren_group(s: &str) -> Option<(String, String)> {
    let s = s.trim();
    if !s.starts_with('(') {
        return None;
    }
    let mut depth = 0i32;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    let inner = s[1..i].trim().to_string();
                    let rest = s[i + 1..].to_string();
                    return Some((inner, rest));
                }
            }
            _ => {}
        }
    }
    None
}

fn is_wrapped_in_balanced_parens(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.len() < 2 || bytes[0] != b'(' || bytes[bytes.len() - 1] != b')' {
        return false;
    }
    let mut depth = 0i32;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 && i < bytes.len() - 1 {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0
}

fn strip_balanced_outer_parens(s: &str) -> String {
    let mut s = s.trim().to_string();
    while is_wrapped_in_balanced_parens(&s) {
        s = s[1..s.len() - 1].trim().to_string();
    }
    s
}

fn draft_from_class_method_label(snippet: &str) -> Option<PendingTypeHintRuleDraft> {
    let (class, method) = snippet.split_once('.')?;
    if class.is_empty() || method.is_empty() || snippet.contains('(') {
        return None;
    }
    if class
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c == '_')
    {
        return None;
    }
    Some(PendingTypeHintRuleDraft {
        kind: TypeHintRuleKind::Methods,
        class: class.to_string(),
        method: method.to_string(),
        focus_returns: true,
        ..pending_with_snippet(snippet)
    })
}

fn draft_from_method_call(snippet: &str) -> Option<PendingTypeHintRuleDraft> {
    let open = snippet.find('(')?;
    let recv_method = snippet[..open].trim();
    let (recv, method) = recv_method.rsplit_once('.')?;
    if recv.is_empty() || method.is_empty() {
        return None;
    }
    let args = parse_call_args(&snippet[open..]);
    Some(PendingTypeHintRuleDraft {
        kind: TypeHintRuleKind::Methods,
        class: String::new(),
        method: method.to_string(),
        name: recv.to_string(),
        arity: Some(args.len()),
        params: args,
        focus_returns: true,
        ..pending_with_snippet(snippet)
    })
}

fn draft_from_macro_or_function_call(snippet: &str) -> Option<PendingTypeHintRuleDraft> {
    let open = snippet.find('(')?;
    let name = snippet[..open].trim();
    if name.is_empty() || name.contains('.') {
        return None;
    }
    let args = parse_call_args(&snippet[open..]);
    let is_macro = name.starts_with('_')
        || name.chars().all(|c| c.is_ascii_uppercase() || c == '_');
    Some(PendingTypeHintRuleDraft {
        kind: if is_macro {
            TypeHintRuleKind::Macros
        } else {
            TypeHintRuleKind::Functions
        },
        name: name.to_string(),
        arity: Some(args.len()),
        params: args,
        focus_returns: true,
        ..pending_with_snippet(snippet)
    })
}

fn draft_from_field_access(snippet: &str) -> Option<PendingTypeHintRuleDraft> {
    if snippet.contains('(') {
        return None;
    }
    let (obj, field) = snippet.rsplit_once('.')?;
    if obj.is_empty() || field.is_empty() {
        return None;
    }
    Some(PendingTypeHintRuleDraft {
        kind: TypeHintRuleKind::Fields,
        class: String::new(),
        field: field.to_string(),
        name: obj.to_string(),
        focus_returns: false,
        ..pending_with_snippet(snippet)
    })
}

fn draft_from_binary_op(snippet: &str) -> Option<PendingTypeHintRuleDraft> {
    for op in ["+", "-", "*", "/", "%", "==", "!=", "<", ">", "<=", ">=", "&&", "||"] {
        if let Some(idx) = find_binary_op(snippet, op) {
            let lhs = snippet[..idx].trim();
            let rhs = snippet[idx + op.len()..].trim();
            if lhs.is_empty() || rhs.is_empty() {
                continue;
            }
            return Some(PendingTypeHintRuleDraft {
                kind: TypeHintRuleKind::BinaryOps,
                op: op.to_string(),
                lhs: literal_or_placeholder_type(lhs),
                rhs: literal_or_placeholder_type(rhs),
                focus_returns: true,
                ..pending_with_snippet(snippet)
            });
        }
    }
    None
}

fn draft_from_constant_identifier(snippet: &str, kind_label: &str) -> Option<PendingTypeHintRuleDraft> {
    if snippet.contains('(') || snippet.contains('.') {
        return None;
    }
    let looks_constant = snippet.chars().all(|c| c.is_ascii_uppercase() || c == '_' || c == ':')
        || kind_label == "Identifier";
    if !looks_constant {
        return None;
    }
    Some(PendingTypeHintRuleDraft {
        kind: TypeHintRuleKind::Constants,
        name: snippet.to_string(),
        focus_returns: false,
        ..pending_with_snippet(snippet)
    })
}

fn pending_with_snippet(snippet: &str) -> PendingTypeHintRuleDraft {
    PendingTypeHintRuleDraft {
        source_snippet: snippet.to_string(),
        ..Default::default()
    }
}

fn find_binary_op(s: &str, op: &str) -> Option<usize> {
    let mut depth = 0i32;
    let bytes = s.as_bytes();
    let op_bytes = op.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            _ if depth == 0 && bytes[i..].starts_with(op_bytes) => return Some(i),
            _ => {}
        }
        i += 1;
    }
    None
}

fn literal_or_placeholder_type(expr: &str) -> String {
    let e = expr.trim();
    if e.starts_with('"') && e.ends_with('"') {
        return "StringLiteral".to_string();
    }
    if e.starts_with('\'') {
        return "CharLiteral".to_string();
    }
    if e.parse::<i64>().is_ok() || e.parse::<u64>().is_ok() {
        return "IntegerLiteral".to_string();
    }
    if e.parse::<f64>().is_ok() {
        return "FloatingPointLiteral".to_string();
    }
    if e == "true" || e == "false" {
        return "bool".to_string();
    }
    if e == "nullptr" || e == "NULL" {
        return "void*".to_string();
    }
    e.to_string()
}

fn parse_call_args(args_part: &str) -> Vec<String> {
    let inner = args_part.trim();
    let inner = inner
        .strip_prefix('(')
        .and_then(|s| s.strip_suffix(')'))
        .unwrap_or(inner)
        .trim();
    if inner.is_empty() {
        return Vec::new();
    }
    split_top_level_commas(inner)
        .into_iter()
        .map(|s| literal_or_placeholder_type(s.trim()))
        .collect()
}

fn split_top_level_commas(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                out.push(s[start..i].to_string());
                start = i + ch.len_utf8();
            }
            _ => {}
        }
    }
    out.push(s[start..].to_string());
    out
}

impl TypeHintConfigFile {
    pub fn from_config(config: &TypeHintConfig) -> Self {
        Self::new(config.cpp.clone())
    }
}

impl From<TypeHintConfigFile> for TypeHintConfig {
    fn from(file: TypeHintConfigFile) -> Self {
        TypeHintConfig::from_file(file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_config() -> TypeHintConfig {
        TypeHintConfig::from_file(TypeHintConfigFile::new(CppTypeHintRules {
            methods: vec![
                CppMethodRule {
                    class: "CString".into(),
                    method: "GetLength".into(),
                    arity: Some(0),
                    params: vec![],
                    returns: "int".into(),
                    enabled: true,
                },
                CppMethodRule {
                    class: "CTime".into(),
                    method: "Format".into(),
                    arity: None,
                    params: vec!["LPCTSTR".into()],
                    returns: "CString".into(),
                    enabled: true,
                },
            ],
            functions: vec![CppCallableRule {
                name: "MAKEINTRESOURCE".into(),
                arity: None,
                params: vec!["int".into()],
                returns: "LPCTSTR".into(),
                enabled: true,
            }],
            macros: vec![CppCallableRule {
                name: "_T".into(),
                arity: Some(1),
                params: vec![],
                returns: "LPCTSTR".into(),
                enabled: true,
            }],
            constants: vec![CppConstantRule {
                name: "IDC_OK".into(),
                ty: "int".into(),
                enabled: true,
            }],
            fields: vec![CppFieldRule {
                class: "CWnd".into(),
                field: "m_hWnd".into(),
                ty: "HWND".into(),
                enabled: true,
            }],
            binary_ops: vec![CppBinaryOpRule {
                op: "+".into(),
                lhs: "StringLiteral".into(),
                rhs: "CString".into(),
                returns: "CString".into(),
                enabled: true,
            }],
            ..Default::default()
        }))
    }

    #[test]
    fn yaml_round_trip() {
        let file = TypeHintConfigFile::from_config(&sample_config());
        let yaml = type_hint_config_to_yaml_string(&file).unwrap();
        let parsed = parse_type_hint_config_str(&yaml).unwrap();
        assert_eq!(parsed.cpp.methods.len(), 2);
        assert_eq!(parsed.cpp.macros[0].name, "_T");
    }

    #[test]
    fn unsupported_version_returns_error() {
        let yaml = "version: 9999\ncpp: {}\n";
        assert!(parse_type_hint_config_str(yaml).is_err());
    }

    #[test]
    fn lookup_method_by_arity_and_params() {
        let cfg = sample_config();
        assert_eq!(
            cfg.lookup_cpp_method_return("CString", "GetLength", &[]),
            Some("int".into())
        );
        assert_eq!(
            cfg.lookup_cpp_method_return("CTime", "Format", &["LPCTSTR".into()]),
            Some("CString".into())
        );
    }

    #[test]
    fn lookup_macro_function_constant_field_binary() {
        let cfg = sample_config();
        assert_eq!(
            cfg.lookup_cpp_macro_return("_T", &["StringLiteral".into()]),
            Some("LPCTSTR".into())
        );
        assert_eq!(
            cfg.lookup_cpp_function_return("MAKEINTRESOURCE", &["int".into()]),
            Some("LPCTSTR".into())
        );
        assert_eq!(cfg.lookup_cpp_constant_type("IDC_OK"), Some("int".into()));
        assert_eq!(
            cfg.lookup_cpp_field_type("CWnd", "m_hWnd"),
            Some("HWND".into())
        );
        assert_eq!(
            cfg.lookup_cpp_binary_op_return("+", "StringLiteral", "CString"),
            Some("CString".into())
        );
    }

    #[test]
    fn ambiguous_overload_returns_none() {
        let cfg = TypeHintConfig::from_file(TypeHintConfigFile::new(CppTypeHintRules {
            methods: vec![
                CppMethodRule {
                    class: "Foo".into(),
                    method: "Bar".into(),
                    arity: Some(1),
                    params: vec![],
                    returns: "int".into(),
                    enabled: true,
                },
                CppMethodRule {
                    class: "Foo".into(),
                    method: "Bar".into(),
                    arity: Some(1),
                    params: vec![],
                    returns: "double".into(),
                    enabled: true,
                },
            ],
            ..Default::default()
        }));
        assert_eq!(cfg.lookup_cpp_method_return("Foo", "Bar", &["int".into()]), None);
    }

    #[test]
    fn draft_from_t_macro() {
        let d = draft_from_unknown_hint("ARGS#0", "CallExpression", "_T(\"abc\")", "a.cpp", 10);
        assert_eq!(d.kind, TypeHintRuleKind::Macros);
        assert_eq!(d.name, "_T");
        assert_eq!(d.arity, Some(1));
    }

    #[test]
    fn draft_from_method_call() {
        let d = draft_from_unknown_hint("RECV", "CallExpression", "s.GetLength()", "a.cpp", 3);
        assert_eq!(d.kind, TypeHintRuleKind::Methods);
        assert_eq!(d.method, "GetLength");
        assert_eq!(d.name, "s");
    }

    #[test]
    fn draft_from_class_method_label() {
        let d = draft_from_unknown_hint("RECV", "CallExpression", "CString.GetLength", "a.cpp", 3);
        assert_eq!(d.class, "CString");
        assert_eq!(d.method, "GetLength");
        assert_eq!(d.column_key, "RECV");
        assert_eq!(d.file, "a.cpp");
        assert_eq!(d.line, 3);
    }

    #[test]
    fn draft_from_inferred_class_method_label() {
        assert!(looks_like_class_method_label("CTime.Format"));
        assert!(!looks_like_class_method_label("CString"));
        assert!(!looks_like_class_method_label("obj.member"));

        let d = draft_from_hint_cell("B", "Inferred", "CTime.Format", "fmt.cpp", 12);
        assert_eq!(d.kind, TypeHintRuleKind::Methods);
        assert_eq!(d.class, "CTime");
        assert_eq!(d.method, "Format");
        assert_eq!(d.kind_label, "Inferred");
        assert_eq!(d.column_key, "B");
        assert_eq!(d.file, "fmt.cpp");
        assert_eq!(d.line, 12);
        assert!(d.focus_returns);
    }

    #[test]
    fn draft_from_constant() {
        let d = draft_from_unknown_hint("ARGS#0", "Identifier", "IDC_OK", "a.cpp", 1);
        assert_eq!(d.kind, TypeHintRuleKind::Constants);
        assert_eq!(d.name, "IDC_OK");
    }

    #[test]
    fn draft_from_field() {
        let d = draft_from_unknown_hint("ARGS#0", "FieldExpression", "obj.member", "a.cpp", 1);
        assert_eq!(d.kind, TypeHintRuleKind::Fields);
        assert_eq!(d.field, "member");
        assert_eq!(d.name, "obj");
    }

    #[test]
    fn draft_from_binary_string_plus() {
        let d = draft_from_unknown_hint("ARGS#0", "BinaryExpression", "\"abc\" + s", "a.cpp", 1);
        assert_eq!(d.kind, TypeHintRuleKind::BinaryOps);
        assert_eq!(d.op, "+");
        assert_eq!(d.lhs, "StringLiteral");
        assert_eq!(d.rhs, "s");
    }

    #[test]
    fn draft_from_nested_binary() {
        let d = draft_from_unknown_hint(
            "ARGS#0",
            "BinaryExpression",
            "(nSel + 1) * 100",
            "a.cpp",
            1,
        );
        assert_eq!(d.kind, TypeHintRuleKind::BinaryOps);
        assert_eq!(d.op, "*");
    }

    #[test]
    fn parse_unknown_hint_export_round_trip() {
        assert_eq!(
            parse_unknown_hint_export_display("?"),
            (String::new(), String::new())
        );
        assert_eq!(
            parse_unknown_hint_export_display("? (CallExpression)"),
            ("CallExpression".into(), String::new())
        );
        assert_eq!(
            parse_unknown_hint_export_display("? (CallExpression) (time.Format(\"%Y\"))"),
            (
                "CallExpression".into(),
                "time.Format(\"%Y\")".into()
            )
        );
        assert_eq!(
            parse_unknown_hint_export_display("? (ParenthesizedExpression) ((1 + 2))"),
            (
                "ParenthesizedExpression".into(),
                "(1 + 2)".into()
            )
        );
    }

    #[test]
    fn hint_rule_draft_source_from_display_cases() {
        assert!(hint_rule_draft_source_from_display("·").is_none());
        assert!(hint_rule_draft_source_from_display("").is_none());
        let u = hint_rule_draft_source_from_display("? (Identifier) (foo)")
            .unwrap();
        assert_eq!(u.kind_label, "Identifier");
        assert_eq!(u.snippet, "foo");
        let inf = hint_rule_draft_source_from_display("CTime.Format").unwrap();
        assert_eq!(inf.kind_label, "Inferred");
        assert_eq!(inf.snippet, "CTime.Format");
    }

    #[test]
    fn draft_from_unknown_parenthesized_binary() {
        let d = draft_from_unknown_hint(
            "RECV",
            "ParenthesizedExpression",
            "(1 + 2)",
            "a.cpp",
            3,
        );
        assert_eq!(d.kind, TypeHintRuleKind::BinaryOps);
        assert_eq!(d.op, "+");
        assert_eq!(d.lhs, "IntegerLiteral");
        assert_eq!(d.rhs, "IntegerLiteral");
        assert_eq!(d.source_snippet, "(1 + 2)");
    }

    #[test]
    fn enrich_method_draft_from_call_context_fills_arity_and_params() {
        let mut d = draft_from_hint_cell("RECV", "Inferred", "CTime.Format", "a.cpp", 1);
        assert_eq!(d.kind, TypeHintRuleKind::Methods);
        assert!(d.arity.is_none());
        assert!(d.params.is_empty());
        enrich_method_draft_from_call_context(
            &mut d,
            1,
            &["StringLiteral".into(), "? (Identifier) (x)".into()],
        );
        assert_eq!(d.arity, Some(1));
        assert_eq!(d.params, vec!["StringLiteral".to_string(), "x".to_string()]);
    }

    #[test]
    fn enrich_method_draft_does_not_overwrite_existing() {
        let mut d = draft_from_unknown_hint("RECV", "CallExpression", "s.GetLength()", "a.cpp", 1);
        assert_eq!(d.arity, Some(0));
        enrich_method_draft_from_call_context(&mut d, 2, &["int".into()]);
        assert_eq!(d.arity, Some(0));
    }

    #[test]
    fn call_context_from_column_keys_extracts_arity_and_args() {
        let keys = vec![
            "RECV".into(),
            "ARGS#arity".into(),
            "ARGS#0".into(),
            "ARGS#1".into(),
        ];
        let displays = |k: &str| match k {
            "ARGS#arity" => "2".into(),
            "ARGS#0" => "StringLiteral".into(),
            "ARGS#1" => "? (Identifier) (n)".into(),
            _ => String::new(),
        };
        let (arity, args) = call_context_from_column_keys(&keys, displays);
        assert_eq!(arity, 2);
        assert_eq!(args, vec!["StringLiteral", "n"]);
    }
}
