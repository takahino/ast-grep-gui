//! パターンのメタ変数に束縛されたノードから、表示用の型ヒントを推定する（構文ベース・best-effort）。

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ast_grep_core::{Doc, Node};
use ast_grep_language::{LanguageExt, SupportLang};

use crate::lang::SupportedLanguage;
use crate::type_hint_config::TypeHintConfig;

macro_rules! cpp_ast_grep_with_profile {
    ($cache:expr, $source:expr) => {{
        let __start = Instant::now();
        let __grep = SupportLang::Cpp.ast_grep($source);
        if let Some(__cache) = $cache {
            __cache.record_ast_parse(__start.elapsed());
        }
        __grep
    }};
}

/// 型ヒント推定の計測（検索ジョブ内で共有、rayon ワーカーから更新）。
#[derive(Debug, Default)]
pub struct TypeHintProfile {
    pub infer_calls: AtomicU64,
    pub infer_nanos: AtomicU64,
    pub header_reads: AtomicU64,
    pub header_read_nanos: AtomicU64,
    pub header_cache_hits: AtomicU64,
    pub ast_parses: AtomicU64,
    pub ast_parse_nanos: AtomicU64,
    pub lookup_cache_hits: AtomicU64,
}

impl TypeHintProfile {
    pub fn record_infer(&self, elapsed: Duration) {
        self.infer_calls.fetch_add(1, Ordering::Relaxed);
        self.infer_nanos
            .fetch_add(elapsed.as_nanos() as u64, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> TypeHintProfileSnapshot {
        TypeHintProfileSnapshot {
            infer_calls: self.infer_calls.load(Ordering::Relaxed),
            infer_total_us: self.infer_nanos.load(Ordering::Relaxed) / 1_000,
            header_reads: self.header_reads.load(Ordering::Relaxed),
            header_read_total_us: self.header_read_nanos.load(Ordering::Relaxed) / 1_000,
            header_cache_hits: self.header_cache_hits.load(Ordering::Relaxed),
            ast_parses: self.ast_parses.load(Ordering::Relaxed),
            ast_parse_total_us: self.ast_parse_nanos.load(Ordering::Relaxed) / 1_000,
            lookup_cache_hits: self.lookup_cache_hits.load(Ordering::Relaxed),
        }
    }
}

/// 検索完了時に UI / エクスポートへ載せる型ヒント計測スナップショット。
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct TypeHintProfileSnapshot {
    pub infer_calls: u64,
    pub infer_total_us: u64,
    pub header_reads: u64,
    pub header_read_total_us: u64,
    pub header_cache_hits: u64,
    pub ast_parses: u64,
    pub ast_parse_total_us: u64,
    pub lookup_cache_hits: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CppMemberLookupKey {
    path: PathBuf,
    class_name: String,
    member_name: String,
}

/// 1 検索ジョブ内で C++ include 読み込み・メンバ検索結果を共有する。
#[derive(Debug)]
pub struct RecvHintJobCache {
    profile: Arc<TypeHintProfile>,
    header_text: Mutex<HashMap<PathBuf, Option<Arc<str>>>>,
    header_field_lookup: Mutex<HashMap<CppMemberLookupKey, Option<String>>>,
    header_method_lookup: Mutex<HashMap<CppMemberLookupKey, Option<String>>>,
    source_field_lookup: Mutex<HashMap<CppMemberLookupKey, Option<String>>>,
    source_method_lookup: Mutex<HashMap<CppMemberLookupKey, Option<String>>>,
}

impl RecvHintJobCache {
    pub fn new() -> Self {
        Self::with_profile(Arc::new(TypeHintProfile::default()))
    }

    pub fn with_profile(profile: Arc<TypeHintProfile>) -> Self {
        Self {
            profile,
            header_text: Mutex::new(HashMap::new()),
            header_field_lookup: Mutex::new(HashMap::new()),
            header_method_lookup: Mutex::new(HashMap::new()),
            source_field_lookup: Mutex::new(HashMap::new()),
            source_method_lookup: Mutex::new(HashMap::new()),
        }
    }

    pub fn profile(&self) -> &TypeHintProfile {
        &self.profile
    }

    fn lookup_field(
        &self,
        path: &Path,
        class_name: &str,
        field_name: &str,
        in_source: bool,
    ) -> Option<Option<String>> {
        let key = CppMemberLookupKey {
            path: cpp_path_key(path),
            class_name: class_name.to_string(),
            member_name: field_name.to_string(),
        };
        let map = if in_source {
            self.source_field_lookup.lock().ok()?
        } else {
            self.header_field_lookup.lock().ok()?
        };
        if let Some(v) = map.get(&key) {
            self.profile
                .lookup_cache_hits
                .fetch_add(1, Ordering::Relaxed);
            return Some(v.clone());
        }
        None
    }

    fn store_field(
        &self,
        path: &Path,
        class_name: &str,
        field_name: &str,
        in_source: bool,
        value: Option<String>,
    ) {
        let key = CppMemberLookupKey {
            path: cpp_path_key(path),
            class_name: class_name.to_string(),
            member_name: field_name.to_string(),
        };
        if in_source {
            if let Ok(mut map) = self.source_field_lookup.lock() {
                map.insert(key, value);
            }
        } else if let Ok(mut map) = self.header_field_lookup.lock() {
            map.insert(key, value);
        }
    }

    fn lookup_method(
        &self,
        path: &Path,
        class_name: &str,
        method_name: &str,
        in_source: bool,
    ) -> Option<Option<String>> {
        let key = CppMemberLookupKey {
            path: cpp_path_key(path),
            class_name: class_name.to_string(),
            member_name: method_name.to_string(),
        };
        let map = if in_source {
            self.source_method_lookup.lock().ok()?
        } else {
            self.header_method_lookup.lock().ok()?
        };
        if let Some(v) = map.get(&key) {
            self.profile
                .lookup_cache_hits
                .fetch_add(1, Ordering::Relaxed);
            return Some(v.clone());
        }
        None
    }

    fn store_method(
        &self,
        path: &Path,
        class_name: &str,
        method_name: &str,
        in_source: bool,
        value: Option<String>,
    ) {
        let key = CppMemberLookupKey {
            path: cpp_path_key(path),
            class_name: class_name.to_string(),
            member_name: method_name.to_string(),
        };
        if in_source {
            if let Ok(mut map) = self.source_method_lookup.lock() {
                map.insert(key, value);
            }
        } else if let Ok(mut map) = self.header_method_lookup.lock() {
            map.insert(key, value);
        }
    }

    fn load_header_text(&self, path: &Path) -> Option<Arc<str>> {
        let key = cpp_path_key(path);
        if let Ok(map) = self.header_text.lock() {
            if let Some(cached) = map.get(&key) {
                self.profile
                    .header_cache_hits
                    .fetch_add(1, Ordering::Relaxed);
                return cached.clone();
            }
        }

        let start = Instant::now();
        let len = fs::metadata(path).ok()?.len();
        if len > CPP_INCLUDE_MAX_FILE_BYTES as u64 {
            self.store_header_text(&key, None);
            return None;
        }
        let text = fs::read_to_string(path).ok()?;
        self.profile.header_reads.fetch_add(1, Ordering::Relaxed);
        self.profile.header_read_nanos.fetch_add(
            start.elapsed().as_nanos() as u64,
            Ordering::Relaxed,
        );
        let arc: Arc<str> = Arc::from(text.as_str());
        self.store_header_text(&key, Some(Arc::clone(&arc)));
        Some(arc)
    }

    fn store_header_text(&self, key: &PathBuf, value: Option<Arc<str>>) {
        if let Ok(mut map) = self.header_text.lock() {
            map.insert(key.clone(), value);
        }
    }

    pub(crate) fn record_ast_parse(&self, elapsed: Duration) {
        self.profile.ast_parses.fetch_add(1, Ordering::Relaxed);
        self.profile.ast_parse_nanos.fetch_add(
            elapsed.as_nanos() as u64,
            Ordering::Relaxed,
        );
    }
}

/// 検索対象ファイルのパスとソース（C++ の `#include` 解決などに使用）。
#[derive(Debug, Clone, Copy)]
pub struct RecvHintContext<'a> {
    pub file_path: &'a Path,
    pub source: &'a str,
    /// C++ 型ヒント用。コンパイラの `-I` に相当するディレクトリ（空なら従来どおりソースの親のみ）。
    /// `#include "x"` / `#include <x>` はまず `file_path` の親を基準にし、見つからなければ本配列を順に試す。
    pub cpp_include_dirs: &'a [PathBuf],
    /// 検索ジョブ内共有キャッシュ（C++ include 読み込み・メンバ検索の再利用）。
    pub job_cache: Option<&'a RecvHintJobCache>,
    /// YAML 等で読み込んだ型ヒント補助ルール。
    pub type_hint_config: Option<&'a TypeHintConfig>,
}

/// パターンの `$RECV` に対応するノードから型ヒント文字列を返す。
#[allow(dead_code)] // 公開 API・テストから利用。内部は `infer_capture_type` に集約。
pub fn infer_recv_type<D: Doc>(
    lang: SupportedLanguage,
    recv: &Node<'_, D>,
    ctx: Option<&RecvHintContext<'_>>,
) -> Option<String> {
    infer_capture_type(lang, "RECV", recv, ctx)
}

/// tree-sitter のノード種別（`string_literal` など）を `StringLiteral` 形式に整形する（表示用）。
pub fn humanize_tree_sitter_kind(kind: &str) -> String {
    kind.split('_')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect()
}

/// 型名としては推定できないが、ノード種別だけは確定しているときの表示ラベル（`string_literal` → `StringLiteral`）。
fn syntax_kind_literal_hint<D: Doc>(node: &Node<'_, D>) -> Option<String> {
    let k = node.kind();
    let k = k.as_ref();
    if matches!(
        k,
        "string_literal"
            | "char_literal"
            | "raw_string_literal"
            | "integer_literal"
            | "floating_point_literal"
            | "number_literal"
            | "decimal_literal"
            | "hexadecimal_literal"
            | "binary_literal"
            | "octal_literal"
            | "true"
            | "false"
            | "null"
            | "nullptr"
            | "interpreted_string_literal"
            | "rune_literal"
    ) {
        return Some(humanize_tree_sitter_kind(k));
    }
    // C++ など: `argument` が `string_literal` を包む
    if matches!(k, "argument" | "parenthesized_expression") {
        for c in node.children() {
            if let Some(h) = syntax_kind_literal_hint(&c) {
                return Some(h);
            }
        }
    }
    None
}

/// 型推定が空のとき、`?:` 列に入れる表示名（リテラルは内側の種別、それ以外はノード種別を整形）。
pub fn hint_fallback_label<D: Doc>(node: &Node<'_, D>) -> String {
    syntax_kind_literal_hint(node)
        .unwrap_or_else(|| humanize_tree_sitter_kind(node.kind().as_ref()))
}

/// `type_hints` に保存する `?:` 以降の文字列（`種別\u{1f}ソース断片`）。断片は長さを抑える。
pub fn format_stored_unknown_hint<D: Doc>(node: &Node<'_, D>) -> String {
    let kind = hint_fallback_label(node);
    let snippet = truncate_hint_snippet(&node.text());
    if snippet.is_empty() {
        kind
    } else {
        format!("{kind}\u{1f}{snippet}")
    }
}

const HINT_SNIPPET_MAX_CHARS: usize = 120;

/// 推論失敗時のソース断片（表示・保存用）：区切り文字を除き、空白を圧縮し最大文字数で切る。
pub fn truncate_hint_snippet(s: &str) -> String {
    let cleaned: String = s.chars().filter(|c| *c != '\u{1f}' && *c != '\r').collect();
    let collapsed = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= HINT_SNIPPET_MAX_CHARS {
        collapsed
    } else {
        format!(
            "{}…",
            collapsed
                .chars()
                .take(HINT_SNIPPET_MAX_CHARS.saturating_sub(1))
                .collect::<String>()
        )
    }
}

/// 単一メタ変数に束縛されたノードから型ヒントを返す（ドット/アローチェインは可能な言語で逐次解決）。
pub fn infer_capture_type<D: Doc>(
    lang: SupportedLanguage,
    capture_name: &str,
    node: &Node<'_, D>,
    ctx: Option<&RecvHintContext<'_>>,
) -> Option<String> {
    let start = Instant::now();
    let out = infer_capture_type_inner(lang, capture_name, node, ctx);
    if let Some(c) = ctx.and_then(|x| x.job_cache) {
        c.profile().record_infer(start.elapsed());
    }
    out
}

fn infer_capture_type_inner<D: Doc>(
    lang: SupportedLanguage,
    _capture_name: &str,
    node: &Node<'_, D>,
    ctx: Option<&RecvHintContext<'_>>,
) -> Option<String> {
    if lang == SupportedLanguage::Auto {
        return None;
    }
    if lang == SupportedLanguage::Cpp {
        if node.kind().as_ref() == "parenthesized_expression" {
            if let Some(inner) = node.children().find(|c| c.is_named()) {
                return infer_capture_type_inner(lang, _capture_name, &inner, ctx);
            }
        }
        if let Some(c) = ctx {
            if let Some(ty) = cpp_config_call_return(node, c) {
                return Some(ty);
            }
            if let Some(ty) = cpp_binary_result_type(node, Some(c)) {
                return Some(ty);
            }
        }
    }
    // `$RECV` が `time.Format(...)` のような `call_expression` のときは、左端の `CTime` ではなく `CTime.Format` を優先
    if lang == SupportedLanguage::Cpp && _capture_name == "RECV" {
        if let Some(l) = cpp_recv_receiver_method_label(node, ctx) {
            return Some(l);
        }
    }
    if let Some(t) = chain_expression_result_type(lang, node, ctx) {
        return Some(t);
    }
    // `$A` / `$B` / `$$$` スロットでも `time.Format(...)` を `CTime`（レシーバ変数の型）だけにしない。
    // `cpp_hint` は `call_expression` を左端識別子に潰すため、戻り型がソースから取れなかった
    // メソッド呼び出しは `CTime.Format` のように表示する（`$RECV` は上で既に同様に処理）。
    if lang == SupportedLanguage::Cpp {
        if let Some(l) = cpp_recv_receiver_method_label(node, ctx) {
            return Some(l);
        }
    }
    let out = match lang {
        SupportedLanguage::Rust => rust_hint(node),
        SupportedLanguage::Go => go_hint(node),
        SupportedLanguage::Java => java_hint(node),
        SupportedLanguage::CSharp => csharp_hint(node),
        SupportedLanguage::TypeScript | SupportedLanguage::JavaScript => ts_hint(node),
        SupportedLanguage::Cpp => cpp_hint(node, ctx),
        SupportedLanguage::C => c_hint(node),
        SupportedLanguage::Python => python_hint(node),
        SupportedLanguage::Kotlin => kotlin_hint(node),
        SupportedLanguage::Scala => scala_hint(node),
        SupportedLanguage::Auto => None,
    };
    out.or_else(|| syntax_kind_literal_hint(node))
}

fn chain_expression_result_type<D: Doc>(
    lang: SupportedLanguage,
    node: &Node<'_, D>,
    ctx: Option<&RecvHintContext<'_>>,
) -> Option<String> {
    match lang {
        SupportedLanguage::Cpp => ctx.and_then(|c| cpp_chain_result_type(node, c)),
        SupportedLanguage::Java => java_chain_result_type(node),
        SupportedLanguage::TypeScript | SupportedLanguage::JavaScript => ts_chain_result_type(node),
        _ => None,
    }
}

fn cpp_simplify_type_name(ty: &str) -> String {
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

fn cpp_field_type_for_class_in_sources(
    ctx: &RecvHintContext<'_>,
    class_name: &str,
    field_name: &str,
) -> Option<String> {
    if let Some(config) = ctx.type_hint_config {
        if let Some(ty) = config.lookup_cpp_field_type(class_name, field_name) {
            return Some(ty);
        }
    }
    if let Some(cache) = ctx.job_cache {
        if let Some(cached) = cache.lookup_field(ctx.file_path, class_name, field_name, true) {
            return cached;
        }
    }

    let grep = cpp_ast_grep_with_profile!(ctx.job_cache, ctx.source);
    let root = grep.root();
    if let Some(ty) = cpp_field_in_named_translation_unit(&root, class_name, field_name) {
        if let Some(cache) = ctx.job_cache {
            cache.store_field(ctx.file_path, class_name, field_name, true, Some(ty.clone()));
        }
        return Some(ty);
    }
    let base = ctx.file_path.parent()?;
    let mut visited = HashSet::new();
    visited.insert(cpp_path_key(ctx.file_path));
    for inc in cpp_include_paths_from_source(ctx.source) {
        if let Some(p) = cpp_resolve_include_file(base, &inc, ctx.cpp_include_dirs) {
            if let Some(ty) = cpp_try_header_file_for_field(
                &p,
                class_name,
                field_name,
                &mut visited,
                CPP_INCLUDE_MAX_DEPTH,
                ctx.cpp_include_dirs,
                ctx.job_cache,
            ) {
                if let Some(cache) = ctx.job_cache {
                    cache.store_field(ctx.file_path, class_name, field_name, true, Some(ty.clone()));
                }
                return Some(ty);
            }
        }
    }
    if let Some(cache) = ctx.job_cache {
        cache.store_field(ctx.file_path, class_name, field_name, true, None);
    }
    None
}

fn cpp_declarator_has_method_name<D: Doc>(decl: &Node<'_, D>, method_name: &str) -> bool {
    let mut found = false;
    cpp_for_each_descendant(decl, &mut |d| {
        if matches!(
            d.kind().as_ref(),
            "identifier" | "field_identifier" | "destructor_name"
        ) {
            if d.text().trim() == method_name {
                found = true;
            }
        }
    });
    found
}

fn cpp_find_method_return_in_named_class<D: Doc>(
    node: &Node<'_, D>,
    class_name: &str,
    method_name: &str,
    out: &mut Option<String>,
) {
    if out.is_some() {
        return;
    }
    let kind = node.kind();
    if matches!(
        kind.as_ref(),
        "class_specifier" | "struct_specifier" | "union_specifier"
    ) {
        if let Some(n) = node.field("name") {
            if n.text().trim() == class_name {
                if let Some(body) = node.field("body") {
                    for c in body.children() {
                        if c.kind().as_ref() != "function_definition" {
                            continue;
                        }
                        let Some(decl) = c.field("declarator") else {
                            continue;
                        };
                        if !cpp_declarator_has_method_name(&decl, method_name) {
                            continue;
                        }
                        if let Some(ty) = c.field("type") {
                            *out = Some(ty.text().trim().to_string());
                            return;
                        }
                    }
                }
            }
        }
    }
    for c in node.children() {
        cpp_find_method_return_in_named_class(&c, class_name, method_name, out);
        if out.is_some() {
            return;
        }
    }
}

fn cpp_method_return_in_named_translation_unit<D: Doc>(
    root: &Node<'_, D>,
    class_name: &str,
    method_name: &str,
) -> Option<String> {
    let mut out = None;
    cpp_find_method_return_in_named_class(root, class_name, method_name, &mut out);
    out
}

fn cpp_try_header_file_for_method(
    path: &Path,
    class_name: &str,
    method_name: &str,
    visited: &mut HashSet<PathBuf>,
    depth: usize,
    cpp_include_dirs: &[PathBuf],
    job_cache: Option<&RecvHintJobCache>,
) -> Option<String> {
    if depth == 0 {
        return None;
    }
    let key = cpp_path_key(path);
    if visited.contains(&key) {
        return None;
    }
    if let Some(cache) = job_cache {
        if let Some(cached) = cache.lookup_method(path, class_name, method_name, false) {
            return cached;
        }
    }
    let text = if let Some(cache) = job_cache {
        cache.load_header_text(path)?.to_string()
    } else {
        let len = fs::metadata(path).ok()?.len();
        if len > CPP_INCLUDE_MAX_FILE_BYTES as u64 {
            return None;
        }
        fs::read_to_string(path).ok()?
    };
    let grep = cpp_ast_grep_with_profile!(job_cache, &text);
    let root = grep.root();
    let result = if let Some(ty) =
        cpp_method_return_in_named_translation_unit(&root, class_name, method_name)
    {
        visited.insert(key);
        Some(ty)
    } else {
        visited.insert(key);
        if depth <= 1 {
            None
        } else {
            let base = path.parent()?;
            let mut found = None;
            for inc in cpp_include_paths_from_source(&text) {
                if let Some(p) = cpp_resolve_include_file(base, &inc, cpp_include_dirs) {
                    if let Some(ty) = cpp_try_header_file_for_method(
                        &p,
                        class_name,
                        method_name,
                        visited,
                        depth - 1,
                        cpp_include_dirs,
                        job_cache,
                    ) {
                        found = Some(ty);
                        break;
                    }
                }
            }
            found
        }
    };
    if let Some(cache) = job_cache {
        cache.store_method(path, class_name, method_name, false, result.clone());
    }
    result
}

fn cpp_method_return_for_class_in_sources(
    ctx: &RecvHintContext<'_>,
    class_name: &str,
    method_name: &str,
    arg_types: &[String],
) -> Option<String> {
    if let Some(config) = ctx.type_hint_config {
        if let Some(ty) = config.lookup_cpp_method_return(class_name, method_name, arg_types) {
            return Some(ty);
        }
    }
    if let Some(cache) = ctx.job_cache {
        if let Some(cached) = cache.lookup_method(ctx.file_path, class_name, method_name, true) {
            return cached;
        }
    }

    let grep = cpp_ast_grep_with_profile!(ctx.job_cache, ctx.source);
    let root = grep.root();
    if let Some(ty) = cpp_method_return_in_named_translation_unit(&root, class_name, method_name) {
        if let Some(cache) = ctx.job_cache {
            cache.store_method(ctx.file_path, class_name, method_name, true, Some(ty.clone()));
        }
        return Some(ty);
    }
    let base = ctx.file_path.parent()?;
    let mut visited = HashSet::new();
    visited.insert(cpp_path_key(ctx.file_path));
    for inc in cpp_include_paths_from_source(ctx.source) {
        if let Some(p) = cpp_resolve_include_file(base, &inc, ctx.cpp_include_dirs) {
            if let Some(ty) = cpp_try_header_file_for_method(
                &p,
                class_name,
                method_name,
                &mut visited,
                CPP_INCLUDE_MAX_DEPTH,
                ctx.cpp_include_dirs,
                ctx.job_cache,
            ) {
                if let Some(cache) = ctx.job_cache {
                    cache.store_method(
                        ctx.file_path,
                        class_name,
                        method_name,
                        true,
                        Some(ty.clone()),
                    );
                }
                return Some(ty);
            }
        }
    }
    if let Some(cache) = ctx.job_cache {
        cache.store_method(ctx.file_path, class_name, method_name, true, None);
    }
    None
}

fn cpp_chain_result_type<D: Doc>(node: &Node<'_, D>, ctx: &RecvHintContext<'_>) -> Option<String> {
    let kind = node.kind();
    let k = kind.as_ref();
    if k == "field_expression" {
        let arg = node.field("argument")?;
        let field = node.field("field")?;
        let field_name = field.text().trim().to_string();
        let arg_ty = cpp_chain_result_type(&arg, ctx).or_else(|| cpp_hint(&arg, Some(ctx)))?;
        let class_name = cpp_simplify_type_name(&arg_ty);
        return cpp_field_type_for_class_in_sources(ctx, class_name.as_str(), field_name.as_str());
    }
    if k == "call_expression" {
        if let Some(ty) = cpp_config_call_return(node, ctx) {
            return Some(ty);
        }
        let func = node.field("function")?;
        if func.kind().as_ref() == "field_expression" {
            let arg = func.field("argument")?;
            let field = func.field("field")?;
            let method_name = field.text().trim().to_string();
            let arg_ty = cpp_chain_result_type(&arg, ctx).or_else(|| cpp_hint(&arg, Some(ctx)))?;
            let class_name = cpp_simplify_type_name(&arg_ty);
            let arg_types = cpp_collect_call_arg_types(node, ctx);
            return cpp_method_return_for_class_in_sources(
                ctx,
                class_name.as_str(),
                method_name.as_str(),
                arg_types.as_slice(),
            );
        }
    }
    None
}

fn java_simplify_type_name(ty: &str) -> String {
    let s = ty.trim();
    let s = s.split('<').next().unwrap_or(s).trim();
    if let Some(i) = s.rfind('.') {
        s[i + 1..].trim().to_string()
    } else {
        s.to_string()
    }
}

fn java_find_member_type_in_tree<D: Doc>(
    node: &Node<'_, D>,
    class_name: &str,
    member: &str,
) -> Option<String> {
    let kind = node.kind();
    let k = kind.as_ref();
    if matches!(
        k,
        "class_declaration" | "interface_declaration" | "record_declaration"
    ) {
        if node.field("name")?.text().trim() == class_name {
            if let Some(body) = node.field("body") {
                return java_member_type_in_class_body(&body, member);
            }
        }
    }
    for c in node.children() {
        if let Some(t) = java_find_member_type_in_tree(&c, class_name, member) {
            return Some(t);
        }
    }
    None
}

fn java_member_type_in_class_body<D: Doc>(body: &Node<'_, D>, member: &str) -> Option<String> {
    for child in body.children() {
        if child.kind().as_ref() == "field_declaration" {
            let ty = child.field("type")?;
            for c in child.children() {
                if c.kind().as_ref() == "variable_declarator" {
                    let id = c
                        .field("name")
                        .or_else(|| c.children().find(|x| x.kind().as_ref() == "identifier"))?;
                    if id.text().trim() == member {
                        return Some(ty.text().trim().to_string());
                    }
                }
            }
        }
        if child.kind().as_ref() == "method_declaration" {
            let name = child.field("name")?;
            if name.text().trim() != member {
                continue;
            }
            return child.field("type").map(|t| t.text().trim().to_string());
        }
    }
    None
}

fn java_chain_result_type<D: Doc>(node: &Node<'_, D>) -> Option<String> {
    let kind = node.kind();
    let k = kind.as_ref();
    if k == "field_access" {
        let obj = node.field("object")?;
        let field = node.field("field")?;
        let field_name = field.text().trim().to_string();
        let obj_ty = java_hint(&obj)?;
        let cn = java_simplify_type_name(&obj_ty);
        let mut root = node.clone();
        while let Some(p) = root.parent() {
            root = p;
        }
        return java_find_member_type_in_tree(&root, cn.as_str(), field_name.as_str());
    }
    if k == "method_invocation" {
        let obj = node.field("object")?;
        let name_node = node.field("name")?;
        let name = name_node.text().trim().to_string();
        let obj_ty = java_hint(&obj)?;
        let cn = java_simplify_type_name(&obj_ty);
        let mut root = node.clone();
        while let Some(p) = root.parent() {
            root = p;
        }
        return java_find_member_type_in_tree(&root, cn.as_str(), name.as_str());
    }
    None
}

fn ts_simplify_type_name(ty: &str) -> String {
    let s = ty.trim();
    let s = s.split('<').next().unwrap_or(s).trim();
    if let Some(i) = s.rfind('.') {
        s[i + 1..].trim().to_string()
    } else {
        s.to_string()
    }
}

fn ts_find_member_type_in_tree<D: Doc>(
    node: &Node<'_, D>,
    class_name: &str,
    member: &str,
) -> Option<String> {
    if node.kind().as_ref() == "class_declaration" {
        if node.field("name")?.text().trim() == class_name {
            if let Some(body) = node.field("body") {
                return ts_member_type_in_class_body(&body, member);
            }
        }
    }
    for c in node.children() {
        if let Some(t) = ts_find_member_type_in_tree(&c, class_name, member) {
            return Some(t);
        }
    }
    None
}

fn ts_member_type_in_class_body<D: Doc>(body: &Node<'_, D>, member: &str) -> Option<String> {
    for child in body.children() {
        let kind = child.kind();
        let k = kind.as_ref();
        if matches!(
            k,
            "public_field_definition"
                | "private_field_definition"
                | "protected_field_definition"
                | "field_definition"
        ) {
            let name_node = child.field("name")?;
            if name_node.text().trim() != member {
                continue;
            }
            if let Some(tanno) = child.field("type") {
                let raw = tanno.text();
                let t = raw.trim();
                let t = t.strip_prefix(':').unwrap_or(t).trim();
                return Some(t.to_string());
            }
        }
        if k == "method_definition" {
            let name_node = child.field("name")?;
            if name_node.text().trim() != member {
                continue;
            }
            if let Some(tanno) = child.field("return_type").or_else(|| child.field("type")) {
                let tt = tanno.text();
                return Some(tt.trim().to_string());
            }
        }
    }
    None
}

fn ts_chain_result_type<D: Doc>(node: &Node<'_, D>) -> Option<String> {
    let kind = node.kind();
    let k = kind.as_ref();
    if k == "member_expression" {
        let obj = node.field("object")?;
        let prop = node.field("property")?;
        let prop_name = prop.text().trim().to_string();
        let obj_ty = ts_hint(&obj)?;
        let cn = ts_simplify_type_name(&obj_ty);
        let mut root = node.clone();
        while let Some(p) = root.parent() {
            root = p;
        }
        return ts_find_member_type_in_tree(&root, cn.as_str(), prop_name.as_str());
    }
    if k == "call_expression" {
        let func = node.field("function")?;
        if func.kind().as_ref() == "member_expression" {
            let obj = func.field("object")?;
            let prop = func.field("property")?;
            let method_name = prop.text().trim().to_string();
            let obj_ty = ts_hint(&obj)?;
            let cn = ts_simplify_type_name(&obj_ty);
            let mut root = node.clone();
            while let Some(p) = root.parent() {
                root = p;
            }
            return ts_find_member_type_in_tree(&root, cn.as_str(), method_name.as_str());
        }
    }
    None
}

fn rust_strip_receiver_text(s: &str) -> String {
    let mut t = s.trim().to_string();
    loop {
        let next = t.trim_start_matches("mut ").trim_start_matches('&').trim();
        if next == t {
            break;
        }
        t = next.to_string();
    }
    t
}

fn rust_pattern_ident_matches<D: Doc>(pattern: &Node<'_, D>, name: &str) -> bool {
    let p_text = pattern.text();
    let p = p_text.trim();
    if p == name {
        return true;
    }
    if let Some(r) = p.strip_prefix("mut ") {
        return r.trim() == name;
    }
    false
}

fn rust_impl_type_from_impl<D: Doc>(impl_node: &Node<'_, D>) -> Option<String> {
    let text = impl_node.text();
    let first = text.lines().next()?.trim();
    let rest = first.strip_prefix("impl")?.trim_start();
    if let Some(idx) = rest.find(" for ") {
        let after = rest[idx + 5..].trim_start();
        return Some(trim_type_tail(after));
    }
    let before_brace = rest.split('{').next()?.trim();
    let before_where = before_brace.split(" where ").next()?.trim();
    Some(trim_type_tail(before_where))
}

fn trim_type_tail(s: &str) -> String {
    s.split('{')
        .next()
        .unwrap_or(s)
        .split(" where ")
        .next()
        .unwrap_or(s)
        .trim()
        .to_string()
}

fn rust_let_type_in_block<D: Doc>(
    block: &Node<'_, D>,
    recv_name: &str,
    recv_start: usize,
) -> Option<String> {
    let mut last: Option<String> = None;
    for child in block.children() {
        if child.kind().as_ref() != "let_declaration" {
            continue;
        }
        if child.range().end >= recv_start {
            continue;
        }
        let pat = child.field("pattern")?;
        if !rust_pattern_ident_matches(&pat, recv_name) {
            continue;
        }
        if let Some(ty) = child.field("type") {
            last = Some(ty.text().trim().to_string());
        }
    }
    last
}

fn rust_hint<D: Doc>(recv: &Node<'_, D>) -> Option<String> {
    let name = rust_strip_receiver_text(&recv.text());
    if name == "self" || name == "Self" {
        return recv
            .ancestors()
            .find(|n| n.kind().as_ref() == "impl_item")
            .and_then(|n| rust_impl_type_from_impl(&n));
    }
    let block = recv.ancestors().find(|n| n.kind().as_ref() == "block")?;
    rust_let_type_in_block(&block, &name, recv.range().start)
}

fn go_hint<D: Doc>(recv: &Node<'_, D>) -> Option<String> {
    let method = recv
        .ancestors()
        .find(|n| n.kind().as_ref() == "method_declaration")?;
    let receiver = method.field("receiver")?;
    let text = receiver.text();
    let inner = text.trim().strip_prefix('(')?.strip_suffix(')')?.trim();
    let parts: Vec<&str> = inner.split_whitespace().collect();
    if parts.len() >= 2 {
        return Some(parts[parts.len() - 1].trim().to_string());
    }
    None
}

fn java_class_name<D: Doc>(recv: &Node<'_, D>) -> Option<String> {
    recv.ancestors()
        .find(|n| {
            let k = n.kind();
            k.as_ref() == "class_declaration" || k.as_ref() == "interface_declaration"
        })
        .and_then(|n| n.field("name").map(|x| x.text().trim().to_string()))
}

/// ローカル宣言の型が `var` / `val` のときは `{keyword}:(右辺)` 形式を返す（Java / C#）。
fn hint_var_type_or_rhs<D: Doc>(ty: &Node<'_, D>, declarator: &Node<'_, D>) -> String {
    let ty_text = ty.text();
    let type_text = ty_text.trim();
    if matches!(type_text, "var" | "val") {
        if let Some(v) = declarator.field("value") {
            let vt = v.text();
            return format!("{}:({})", type_text, vt.trim());
        }
    }
    type_text.to_string()
}

fn java_local_in_block<D: Doc>(
    block: &Node<'_, D>,
    recv_name: &str,
    recv_start: usize,
) -> Option<String> {
    let mut last = None;
    for child in block.children() {
        if child.kind().as_ref() != "local_variable_declaration" {
            continue;
        }
        if child.range().end >= recv_start {
            continue;
        }
        let ty = child.field("type")?;
        for c in child.children() {
            if c.kind().as_ref() == "variable_declarator" {
                let id = c
                    .field("name")
                    .or_else(|| c.children().find(|x| x.kind().as_ref() == "identifier"))?;
                if id.text().trim() == recv_name {
                    last = Some(hint_var_type_or_rhs(&ty, &c));
                }
            }
        }
    }
    last
}

/// 内側の `block` から順に、レシーバ位置より前のローカル宣言を照合する。
fn java_local_in_enclosing_blocks<D: Doc>(recv: &Node<'_, D>, recv_name: &str) -> Option<String> {
    let recv_start = recv.range().start;
    for block in recv.ancestors().filter(|n| n.kind().as_ref() == "block") {
        if let Some(ty) = java_local_in_block(&block, recv_name, recv_start) {
            return Some(ty);
        }
    }
    None
}

/// `_variable_declarator_id` が束ねる名前が `name` と一致するか（`field("name")` が取れない場合のフォールバック付き）。
fn java_declarator_id_matches<D: Doc>(decl_id: &Node<'_, D>, name: &str) -> bool {
    if decl_id.kind().as_ref() != "_variable_declarator_id" {
        return false;
    }
    if let Some(id) = decl_id.field("name") {
        if id.text().trim() == name {
            return true;
        }
    }
    decl_id.children().any(|c| {
        matches!(
            c.kind().as_ref(),
            "identifier" | "_reserved_identifier" | "underscore_pattern"
        ) && c.text().trim() == name
    })
}

/// `formal_parameters` 配下の `formal_parameter` / `spread_parameter` から名前に一致する型を返す。
fn java_walk_formal_parameters<D: Doc>(node: &Node<'_, D>, name: &str, out: &mut Option<String>) {
    if out.is_some() {
        return;
    }
    let node_kind = node.kind();
    let kind = node_kind.as_ref();
    if kind == "formal_parameter" {
        if let Some(ty) = node.field("type") {
            // tree-sitter-java は `formal_parameter` に `name` / `type` を直接載せる（`_variable_declarator_id` は子に出ない）
            let name_ok = node
                .field("name")
                .map(|n| n.text().trim() == name)
                .unwrap_or_else(|| {
                    node.children().any(|c| {
                        c.kind().as_ref() == "_variable_declarator_id"
                            && java_declarator_id_matches(&c, name)
                    })
                });
            if name_ok {
                *out = Some(ty.text().trim().to_string());
                return;
            }
        }
    } else if kind == "spread_parameter" {
        if let Some(ty) = node.field("type") {
            for c in node.children() {
                if c.kind().as_ref() != "variable_declarator" {
                    continue;
                }
                for cc in c.children() {
                    if cc.kind().as_ref() == "_variable_declarator_id"
                        && java_declarator_id_matches(&cc, name)
                    {
                        *out = Some(ty.text().trim().to_string());
                        return;
                    }
                }
            }
        }
    }
    for c in node.children() {
        java_walk_formal_parameters(&c, name, out);
        if out.is_some() {
            return;
        }
    }
}

fn java_parameters_from_formals_root<D: Doc>(
    executable: &Node<'_, D>,
    name: &str,
) -> Option<String> {
    let mut out = None;
    // `method_declaration` / `constructor_declaration` は `field("parameters")` で `formal_parameters` に直結できる
    if let Some(params) = executable.field("parameters") {
        java_walk_formal_parameters(&params, name, &mut out);
    }
    if out.is_none() {
        java_walk_formal_parameters(executable, name, &mut out);
    }
    out
}

/// 型推論のみのラムダ引数（`(s)` など）が `name` を束ねているとき true（フィールド照合を避ける）。
fn java_lambda_inferred_shadows_name<D: Doc>(recv: &Node<'_, D>, name: &str) -> bool {
    for a in recv.ancestors() {
        let k = a.kind();
        if k.as_ref() == "lambda_expression" {
            let Some(params) = a.field("parameters") else {
                continue;
            };
            if params.kind().as_ref() != "inferred_parameters" {
                continue;
            }
            for c in params.children() {
                if c.kind().as_ref() == "identifier" {
                    let t = c.text();
                    if t.trim() == name {
                        return true;
                    }
                }
            }
            continue;
        }
        if k.as_ref() == "method_declaration" || k.as_ref() == "constructor_declaration" {
            return false;
        }
    }
    false
}

/// メソッド／コンストラクタ／（入れ子の）ラムダの仮引数を、内側のスコープから順に照合する。
fn java_parameter_type_for_scope<D: Doc>(recv: &Node<'_, D>, name: &str) -> Option<String> {
    for a in recv.ancestors() {
        let k = a.kind();
        if k.as_ref() == "lambda_expression" {
            let Some(params) = a.field("parameters") else {
                continue;
            };
            let pk = params.kind();
            if pk.as_ref() == "inferred_parameters" {
                let mut matched = false;
                for c in params.children() {
                    if c.kind().as_ref() == "identifier" {
                        let t = c.text();
                        if t.trim() == name {
                            matched = true;
                            break;
                        }
                    }
                }
                if matched {
                    return None;
                }
                continue;
            }
            if pk.as_ref() == "formal_parameters" {
                if let Some(ty) = java_parameters_from_formals_root(&params, name) {
                    return Some(ty);
                }
            }
            continue;
        }
        if k.as_ref() == "method_declaration" || k.as_ref() == "constructor_declaration" {
            return java_parameters_from_formals_root(&a, name);
        }
    }
    None
}

/// 拡張 `for (Type name : expr)` の反復変数を、内側のスコープから順に照合する。
fn java_enhanced_for_type_for_scope<D: Doc>(recv: &Node<'_, D>, name: &str) -> Option<String> {
    for a in recv.ancestors() {
        let k = a.kind();
        let kind = k.as_ref();
        if kind == "enhanced_for_statement" {
            let Some(body) = a.field("body") else {
                continue;
            };
            if recv.range().start < body.range().start {
                continue;
            }
            let Some(ty) = a.field("type") else {
                continue;
            };
            let name_ok = a
                .field("name")
                .map(|n| n.text().trim() == name)
                .unwrap_or(false);
            if name_ok {
                return Some(ty.text().trim().to_string());
            }
            continue;
        }
        if kind == "method_declaration" || kind == "constructor_declaration" {
            break;
        }
    }
    None
}

/// 同一 `class` / `interface` / `record` body 内の `field_declaration` と名前を照合する。
fn java_field_in_class<D: Doc>(recv: &Node<'_, D>, name: &str) -> Option<String> {
    let class_like = recv.ancestors().find(|n| {
        let k = n.kind();
        k.as_ref() == "class_declaration"
            || k.as_ref() == "interface_declaration"
            || k.as_ref() == "record_declaration"
    })?;
    let body = class_like.field("body")?;
    for child in body.children() {
        if child.kind().as_ref() != "field_declaration" {
            continue;
        }
        let ty = child.field("type")?;
        for c in child.children() {
            if c.kind().as_ref() == "variable_declarator" {
                let id = c
                    .field("name")
                    .or_else(|| c.children().find(|x| x.kind().as_ref() == "identifier"))?;
                if id.text().trim() == name {
                    return Some(ty.text().trim().to_string());
                }
            }
        }
    }
    None
}

fn java_hint<D: Doc>(recv: &Node<'_, D>) -> Option<String> {
    let recv_text = recv.text();
    let t = recv_text.trim();
    if t == "this" || t == "super" {
        return java_class_name(recv);
    }
    if let Some(ty) = java_local_in_enclosing_blocks(recv, t) {
        return Some(ty);
    }
    if let Some(ty) = java_parameter_type_for_scope(recv, t) {
        return Some(ty);
    }
    if let Some(ty) = java_enhanced_for_type_for_scope(recv, t) {
        return Some(ty);
    }
    if java_lambda_inferred_shadows_name(recv, t) {
        return None;
    }
    java_field_in_class(recv, t)
}

fn csharp_class_name<D: Doc>(recv: &Node<'_, D>) -> Option<String> {
    recv.ancestors()
        .find(|n| n.kind().as_ref() == "class_declaration")
        .and_then(|n| n.field("name").map(|x| x.text().trim().to_string()))
}

fn csharp_local_in_block<D: Doc>(
    block: &Node<'_, D>,
    recv_name: &str,
    recv_start: usize,
) -> Option<String> {
    let mut last = None;
    for child in block.children() {
        let k = child.kind();
        if k.as_ref() != "local_declaration_statement" {
            continue;
        }
        if child.range().end >= recv_start {
            continue;
        }
        let ty = child.field("type")?;
        for c in child.children() {
            if c.kind().as_ref() == "variable_declarator" {
                if let Some(id) = c.field("name") {
                    if id.text().trim() == recv_name {
                        last = Some(hint_var_type_or_rhs(&ty, &c));
                    }
                }
            }
        }
    }
    last
}

/// 同一 `class` / `struct` / `record` body 内のフィールド宣言と名前を照合する。
fn csharp_field_in_class<D: Doc>(recv: &Node<'_, D>, name: &str) -> Option<String> {
    let class_like = recv.ancestors().find(|n| {
        let k = n.kind();
        k.as_ref() == "class_declaration"
            || k.as_ref() == "struct_declaration"
            || k.as_ref() == "record_declaration"
    })?;
    let body = class_like.field("body")?;
    for child in body.children() {
        let k = child.kind();
        if k.as_ref() != "field_declaration" && k.as_ref() != "event_field_declaration" {
            continue;
        }
        let ty = child.field("type")?;
        for c in child.children() {
            if c.kind().as_ref() == "variable_declaration" {
                for d in c.children() {
                    if d.kind().as_ref() == "variable_declarator" {
                        if let Some(id) = d.field("name") {
                            if id.text().trim() == name {
                                return Some(ty.text().trim().to_string());
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

fn csharp_hint<D: Doc>(recv: &Node<'_, D>) -> Option<String> {
    let recv_text = recv.text();
    let t = recv_text.trim();
    if t == "this" || t == "base" {
        return csharp_class_name(recv);
    }
    if let Some(block) = recv.ancestors().find(|n| n.kind().as_ref() == "block") {
        if let Some(ty) = csharp_local_in_block(&block, t, recv.range().start) {
            return Some(ty);
        }
    }
    csharp_field_in_class(recv, t)
}

fn ts_lexical_in_block<D: Doc>(
    block: &Node<'_, D>,
    recv_name: &str,
    recv_start: usize,
) -> Option<String> {
    let mut last = None;
    for child in block.children() {
        let k = child.kind();
        if k.as_ref() != "lexical_declaration" && k.as_ref() != "variable_declaration" {
            continue;
        }
        if child.range().end >= recv_start {
            continue;
        }
        let is_var_declaration = k.as_ref() == "variable_declaration";
        for c in child.children() {
            if c.kind().as_ref() == "variable_declarator" {
                if let Some(id) = c.field("name") {
                    if id.text().trim() == recv_name {
                        if let Some(ty) = c.field("type") {
                            let ty_text = ty.text();
                            let s = ty_text.trim();
                            last = Some(if matches!(s, "var" | "val") {
                                c.field("value")
                                    .map(|v| {
                                        let vt = v.text();
                                        format!("{}:({})", s, vt.trim())
                                    })
                                    .unwrap_or_else(|| s.to_string())
                            } else {
                                s.to_string()
                            });
                        } else if is_var_declaration {
                            last = c.field("value").map(|v| {
                                let vt = v.text();
                                format!("var:({})", vt.trim())
                            });
                        }
                    }
                }
            }
        }
    }
    last
}

/// `class` body 内のフィールド（型注釈付き）と名前を照合する。
fn ts_field_in_class<D: Doc>(recv: &Node<'_, D>, name: &str) -> Option<String> {
    let class_decl = recv
        .ancestors()
        .find(|n| n.kind().as_ref() == "class_declaration")?;
    let body = class_decl.field("body")?;
    for child in body.children() {
        let kind = child.kind();
        let k = kind.as_ref();
        if k == "method_definition" {
            continue;
        }
        if matches!(
            k,
            "public_field_definition"
                | "private_field_definition"
                | "protected_field_definition"
                | "field_definition"
        ) {
            let name_node = child.field("name")?;
            if name_node.text().trim() != name {
                continue;
            }
            if let Some(tanno) = child.field("type") {
                let tanno_text = tanno.text();
                let tanno_trim = tanno_text.trim();
                let s = tanno_trim.strip_prefix(':').unwrap_or(tanno_trim).trim();
                return Some(s.to_string());
            }
        }
        if k == "property_signature" {
            let name_node = child.field("name")?;
            if name_node.text().trim() != name {
                continue;
            }
            if let Some(tanno) = child.field("type") {
                return Some(tanno.text().trim().to_string());
            }
        }
    }
    None
}

fn ts_hint<D: Doc>(recv: &Node<'_, D>) -> Option<String> {
    let recv_text = recv.text();
    let t = recv_text.trim();
    if t == "this" {
        return recv
            .ancestors()
            .find(|n| n.kind().as_ref() == "class_declaration")
            .and_then(|n| n.field("name").map(|x| x.text().trim().to_string()));
    }
    if let Some(block) = recv.ancestors().find(|n| {
        let k = n.kind();
        k.as_ref() == "statement_block" || k.as_ref() == "block"
    }) {
        if let Some(ty) = ts_lexical_in_block(&block, t, recv.range().start) {
            return Some(ty);
        }
    }
    if let Some(ty) = ts_field_in_class(recv, t) {
        return Some(ty);
    }
    if let Some(p) = recv.parent() {
        if p.kind().as_ref() == "as_expression" {
            if let Some(ty) = p.field("type") {
                return Some(ty.text().trim().to_string());
            }
        }
    }
    None
}

fn cpp_class_name<D: Doc>(recv: &Node<'_, D>) -> Option<String> {
    recv.ancestors()
        .find(|n| {
            let k = n.kind();
            k.as_ref() == "class_specifier" || k.as_ref() == "struct_specifier"
        })
        .and_then(|n| n.field("name").map(|x| x.text().trim().to_string()))
}

/// `$RECV` が `call_expression` のとき、`time.Format("%Y")` のような式に対して
/// `CTime.Format` 形式の表示用ラベルを返す（左端の `time` だけの `CTime` ではなく、
/// **この呼び出し**のレシーバ型とメソッド名を結合する）。
fn cpp_recv_receiver_method_label<D: Doc>(
    node: &Node<'_, D>,
    ctx: Option<&RecvHintContext<'_>>,
) -> Option<String> {
    if node.kind().as_ref() != "call_expression" {
        return None;
    }
    let func = node.field("function")?;
    if func.kind().as_ref() != "field_expression" {
        return None;
    }
    let arg = func.field("argument")?;
    let field = func.field("field")?;
    let method = field.text().trim().to_string();
    let class_ty = cpp_type_of_direct_receiver_expr(&arg, ctx)?;
    let class_name = cpp_simplify_type_name(&class_ty);
    Some(format!("{class_name}.{method}"))
}

/// `call_expression` / `field_expression` の **直接の** レシーバ式について型文字列を得る（チェーンは左へ辿る）。
fn cpp_type_of_direct_receiver_expr<D: Doc>(
    node: &Node<'_, D>,
    ctx: Option<&RecvHintContext<'_>>,
) -> Option<String> {
    match node.kind().as_ref() {
        "identifier" => cpp_hint(node, ctx),
        "call_expression" => {
            let func = node.field("function")?;
            if func.kind().as_ref() != "field_expression" {
                return cpp_hint(node, ctx);
            }
            let inner_arg = func.field("argument")?;
            cpp_type_of_direct_receiver_expr(&inner_arg, ctx)
        }
        "field_expression" => {
            let a = node.field("argument")?;
            cpp_type_of_direct_receiver_expr(&a, ctx)
        }
        _ => cpp_hint(node, ctx),
    }
}

/// メソッドチェーンの2番目以降では `$RECV` が `a.b()` のような `call_expression` になる。
/// ローカル変数・フィールド・ヘッダ探索は左端のベース式（通常は識別子）に対して行う。
fn cpp_recv_base_name<D: Doc>(recv: &Node<'_, D>) -> String {
    let kind = recv.kind();
    let k = kind.as_ref();
    if k == "call_expression" {
        if let Some(f) = recv.field("function") {
            return cpp_recv_base_name(&f);
        }
    }
    if k == "field_expression" {
        if let Some(a) = recv.field("argument") {
            return cpp_recv_base_name(&a);
        }
    }
    if k == "subscript_expression" {
        if let Some(a) = recv.field("argument") {
            return cpp_recv_base_name(&a);
        }
    }
    recv.text().trim().to_string()
}

fn cpp_for_each_descendant<D: Doc, F: FnMut(&Node<'_, D>)>(node: &Node<'_, D>, f: &mut F) {
    f(node);
    for c in node.children() {
        cpp_for_each_descendant(&c, f);
    }
}

/// `class` / `struct` / `union` body 内の `field_declaration` と `field_identifier` を照合する。
/// `init_declarator` / `_declarator` 側の識別子が `name` と一致するか（初期化子は見ない）。
/// `function_declarator` は `field("declarator")` を優先し、`parameter_list` 内の識別子に誤マッチしない。
fn cpp_declarator_matches_name<D: Doc>(d: &Node<'_, D>, name: &str) -> bool {
    let kind = d.kind();
    let k = kind.as_ref();
    if k == "identifier" {
        return d.text().trim() == name;
    }
    if let Some(inner) = d.field("declarator") {
        if cpp_declarator_matches_name(&inner, name) {
            return true;
        }
    }
    if k == "parenthesized_declarator" {
        for c in d.children() {
            if cpp_declarator_matches_name(&c, name) {
                return true;
            }
        }
        return false;
    }
    if matches!(k, "scoped_identifier" | "qualified_identifier") {
        for c in d.children() {
            if cpp_declarator_matches_name(&c, name) {
                return true;
            }
        }
        return false;
    }
    for c in d.children() {
        if cpp_declarator_matches_name(&c, name) {
            return true;
        }
    }
    false
}

/// `init_declarator` の `value` には入らず、宣言ツリーから名前を探す。
fn cpp_declaration_declares_name<D: Doc>(decl: &Node<'_, D>, name: &str) -> bool {
    fn walk<D: Doc>(n: &Node<'_, D>, name: &str) -> bool {
        if n.kind().as_ref() == "init_declarator" {
            if let Some(d) = n.field("declarator") {
                return cpp_declarator_matches_name(&d, name);
            }
            return false;
        }
        for c in n.children() {
            if walk(&c, name) {
                return true;
            }
        }
        false
    }
    if walk(decl, name) {
        return true;
    }
    // `CString a, b, c;` は `declaration` に `declarator` フィールドが複数付く。`field()` は先頭のみ。
    for d in decl.field_children("declarator") {
        if d.kind().as_ref() == "init_declarator" {
            if let Some(inner) = d.field("declarator") {
                if cpp_declarator_matches_name(&inner, name) {
                    return true;
                }
            }
        } else if cpp_declarator_matches_name(&d, name) {
            return true;
        }
    }
    false
}

/// `declaration` の先頭の型・指定子テキスト（`_declaration_specifiers` またはフラット化された `primitive_type` 等）。
fn cpp_declaration_specifiers_text<D: Doc>(decl: &Node<'_, D>) -> Option<String> {
    let mut buf = String::new();
    for c in decl.children() {
        let kind = c.kind();
        let k = kind.as_ref();
        if matches!(
            k,
            "init_declarator"
                | "_declarator"
                | "pointer_declarator"
                | "reference_declarator"
                | "function_declarator"
                | "array_declarator"
                | "identifier"
                | "field_identifier"
                | ";"
        ) {
            break;
        }
        if k == "_declaration_specifiers" {
            return Some(c.text().trim().to_string());
        }
        if !buf.is_empty() {
            buf.push(' ');
        }
        buf.push_str(c.text().trim());
    }
    let s = buf.trim();
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

fn cpp_declaration_type_if_name<D: Doc>(decl: &Node<'_, D>, name: &str) -> Option<String> {
    if !cpp_declaration_declares_name(decl, name) {
        return None;
    }
    cpp_declaration_specifiers_text(decl)
}

/// `compound_statement` の直下に `declaration` が来ない場合（中間ノードがある実装）も拾う。
fn cpp_try_declarations_from_block_item<D: Doc>(
    item: &Node<'_, D>,
    recv_start: usize,
    name: &str,
    last: &mut Option<String>,
) {
    if item.kind().as_ref() == "declaration" {
        if item.range().end < recv_start {
            if let Some(ty) = cpp_declaration_type_if_name(item, name) {
                *last = Some(ty);
            }
        }
        return;
    }
    for c in item.children() {
        if c.kind().as_ref() != "declaration" {
            continue;
        }
        if c.range().end >= recv_start {
            continue;
        }
        if let Some(ty) = cpp_declaration_type_if_name(&c, name) {
            *last = Some(ty);
        }
    }
}

/// 内側の `compound_statement` から順に、レシーバー位置より前のローカル宣言を照合する。
fn cpp_local_in_enclosing_blocks<D: Doc>(recv: &Node<'_, D>, name: &str) -> Option<String> {
    let recv_start = recv.range().start;
    for block in recv
        .ancestors()
        .filter(|n| n.kind().as_ref() == "compound_statement")
    {
        let mut last: Option<String> = None;
        for child in block.children() {
            cpp_try_declarations_from_block_item(&child, recv_start, name, &mut last);
        }
        if last.is_some() {
            return last;
        }
    }
    None
}

fn cpp_inner_declarator<D: Doc>(d: Node<'_, D>) -> Option<Node<'_, D>> {
    if let Some(inner) = d.field("declarator") {
        return Some(inner);
    }
    let children: Vec<_> = d.children().collect();
    children.into_iter().rev().find(|c| {
        let kind = c.kind();
        let k = kind.as_ref();
        !matches!(k, "*" | "&" | "&&" | "[" | "]" | "(" | ")" | "," | ";")
    })
}

fn cpp_declarator_type_ops<D: Doc>(d: Node<'_, D>) -> String {
    let Some(inner) = cpp_inner_declarator(d.clone()) else {
        return String::new();
    };
    let mut out = String::new();
    let kind = d.kind();
    let k = kind.as_ref();
    let d_text = d.text();
    let inner_text = inner.text();
    if matches!(k, "pointer_declarator" | "reference_declarator") {
        if let Some(pos) = d_text.rfind(inner_text.as_ref()) {
            out.push_str(d_text[..pos].trim());
        }
    }
    out.push_str(&cpp_declarator_type_ops(inner));
    out
}

fn cpp_parameter_specifiers_text<D: Doc>(param: &Node<'_, D>) -> Option<String> {
    let declarator_start = param.field("declarator").map(|d| d.range().start);
    let mut buf = String::new();
    for c in param.children() {
        if declarator_start.is_some_and(|start| c.range().start >= start) {
            break;
        }
        let kind = c.kind();
        let k = kind.as_ref();
        if k == "_declaration_specifiers" {
            return Some(c.text().trim().to_string());
        }
        if !buf.is_empty() {
            buf.push(' ');
        }
        buf.push_str(c.text().trim());
    }
    let s = buf.trim();
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

fn cpp_parameter_type_if_name<D: Doc>(param: &Node<'_, D>, name: &str) -> Option<String> {
    if param.kind().as_ref() != "parameter_declaration" {
        return None;
    }
    let declarator = param.field("declarator")?;
    if !cpp_declarator_matches_name(&declarator, name) {
        return None;
    }
    let mut ty = cpp_parameter_specifiers_text(param)?;
    let ops = cpp_declarator_type_ops(declarator);
    if !ops.is_empty() {
        ty.push(' ');
        ty.push_str(ops.as_str());
    }
    Some(ty)
}

fn cpp_walk_parameter_declarations<D: Doc>(
    node: &Node<'_, D>,
    name: &str,
    out: &mut Option<String>,
) {
    if out.is_some() {
        return;
    }
    if let Some(ty) = cpp_parameter_type_if_name(node, name) {
        *out = Some(ty);
        return;
    }
    for c in node.children() {
        cpp_walk_parameter_declarations(&c, name, out);
        if out.is_some() {
            return;
        }
    }
}

fn cpp_parameter_type_for_scope<D: Doc>(recv: &Node<'_, D>, name: &str) -> Option<String> {
    for a in recv.ancestors() {
        if a.kind().as_ref() == "function_definition" {
            let decl = a.field("declarator")?;
            let mut out = None;
            cpp_walk_parameter_declarations(&decl, name, &mut out);
            return out;
        }
    }
    None
}

fn cpp_field_in_class<D: Doc>(recv: &Node<'_, D>, name: &str) -> Option<String> {
    let spec = recv.ancestors().find(|n| {
        let k = n.kind();
        k.as_ref() == "class_specifier"
            || k.as_ref() == "struct_specifier"
            || k.as_ref() == "union_specifier"
    })?;
    let body = spec.field("body")?;
    let mut out: Option<String> = None;
    cpp_walk_field_declarations(&body, name, &mut out);
    out
}

fn cpp_walk_field_declarations<D: Doc>(node: &Node<'_, D>, name: &str, out: &mut Option<String>) {
    if out.is_some() {
        return;
    }
    if node.kind().as_ref() == "field_declaration" {
        if let Some(ty) = node.field("type") {
            let mut found = false;
            cpp_for_each_descendant(node, &mut |d| {
                if d.kind().as_ref() == "field_identifier" && d.text().trim() == name {
                    found = true;
                }
            });
            if found {
                let ty_text = ty.text();
                *out = Some(ty_text.trim().to_string());
                return;
            }
        }
    }
    for c in node.children() {
        cpp_walk_field_declarations(&c, name, out);
    }
}

/// クラス／構造体名が `class_name` の定義内で `field_name` に対応するフィールド型を探す。
fn cpp_find_field_in_named_class<D: Doc>(
    node: &Node<'_, D>,
    class_name: &str,
    field_name: &str,
    out: &mut Option<String>,
) {
    if out.is_some() {
        return;
    }
    let kind = node.kind();
    if matches!(
        kind.as_ref(),
        "class_specifier" | "struct_specifier" | "union_specifier"
    ) {
        if let Some(n) = node.field("name") {
            let nt = n.text();
            if nt.trim() == class_name {
                if let Some(body) = node.field("body") {
                    cpp_walk_field_declarations(&body, field_name, out);
                }
            }
        }
    }
    for c in node.children() {
        cpp_find_field_in_named_class(&c, class_name, field_name, out);
        if out.is_some() {
            return;
        }
    }
}

fn cpp_field_in_named_translation_unit<D: Doc>(
    root: &Node<'_, D>,
    class_name: &str,
    field_name: &str,
) -> Option<String> {
    let mut out = None;
    cpp_find_field_in_named_class(root, class_name, field_name, &mut out);
    out
}

fn cpp_scope_class_name<D: Doc>(recv: &Node<'_, D>) -> Option<String> {
    recv.ancestors()
        .find(|n| {
            let k = n.kind();
            k.as_ref() == "class_specifier" || k.as_ref() == "struct_specifier"
        })
        .and_then(|n| n.field("name").map(|x| x.text().trim().to_string()))
        .or_else(|| cpp_out_of_line_class_name(recv))
}

/// `void Foo::bar()` のようなメンバ定義からクラス名 `Foo` を推定する。
fn cpp_out_of_line_class_name<D: Doc>(recv: &Node<'_, D>) -> Option<String> {
    let fd = recv
        .ancestors()
        .find(|n| n.kind().as_ref() == "function_definition")?;
    let decl = fd.field("declarator")?;
    let text = decl.text();
    let t = text.trim();
    let pos = t.rfind("::")?;
    let before = t[..pos].trim();
    let last = before.rsplit("::").next()?.trim();
    let last = last.split_whitespace().last().unwrap_or(last);
    let last = last.trim_start_matches('*').trim_end_matches('*').trim();
    if last.is_empty() || last == "operator" {
        return None;
    }
    Some(last.to_string())
}

/// `#include` 行からパス文字列を列挙（型ヒント・インクルード診断用）。
pub fn cpp_scan_include_directives(source: &str) -> Vec<String> {
    cpp_include_paths_from_source(source)
}

fn cpp_include_paths_from_source(source: &str) -> Vec<String> {
    let mut v = Vec::new();
    for line in source.lines() {
        let t = line.trim();
        let rest = match t.strip_prefix("#include") {
            Some(r) => r.trim_start(),
            None => continue,
        };
        if let Some(rest) = rest.strip_prefix('"') {
            if let Some(end) = rest.find('"') {
                v.push(rest[..end].to_string());
            }
        } else if let Some(rest) = rest.strip_prefix('<') {
            if let Some(end) = rest.find('>') {
                v.push(rest[..end].to_string());
            }
        }
    }
    v
}

fn cpp_path_key(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// `#include` の相対パスを、含み元ディレクトリの直下と `-I` 相当ディレクトリから解決する（診断 UI 用に公開）。
pub fn cpp_resolve_include_path(
    base_dir: &Path,
    inc: &str,
    extra_include_dirs: &[PathBuf],
) -> Option<PathBuf> {
    cpp_resolve_include_file(base_dir, inc, extra_include_dirs)
}

/// `#include` の相対パスを、含み元ディレクトリの直下と `-I` 相当ディレクトリから解決する。
fn cpp_resolve_include_file(
    base_dir: &Path,
    inc: &str,
    extra_include_dirs: &[PathBuf],
) -> Option<PathBuf> {
    let primary = base_dir.join(inc);
    if primary.is_file() {
        return Some(primary);
    }
    for root in extra_include_dirs {
        let p = root.join(inc);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

const CPP_INCLUDE_MAX_DEPTH: usize = 8;
const CPP_INCLUDE_MAX_FILE_BYTES: usize = 512 * 1024;

fn cpp_try_header_file_for_field(
    path: &Path,
    class_name: &str,
    field_name: &str,
    visited: &mut HashSet<PathBuf>,
    depth: usize,
    cpp_include_dirs: &[PathBuf],
    job_cache: Option<&RecvHintJobCache>,
) -> Option<String> {
    if depth == 0 {
        return None;
    }
    let key = cpp_path_key(path);
    if visited.contains(&key) {
        return None;
    }
    if let Some(cache) = job_cache {
        if let Some(cached) = cache.lookup_field(path, class_name, field_name, false) {
            return cached;
        }
    }
    let text = if let Some(cache) = job_cache {
        cache.load_header_text(path)?.to_string()
    } else {
        let len = fs::metadata(path).ok()?.len();
        if len > CPP_INCLUDE_MAX_FILE_BYTES as u64 {
            return None;
        }
        fs::read_to_string(path).ok()?
    };
    let grep = cpp_ast_grep_with_profile!(job_cache, &text);
    let root = grep.root();
    let result = if let Some(ty) = cpp_field_in_named_translation_unit(&root, class_name, field_name)
    {
        visited.insert(key);
        Some(ty)
    } else {
        visited.insert(key);
        if depth <= 1 {
            None
        } else {
            let base = path.parent()?;
            let mut found = None;
            for inc in cpp_include_paths_from_source(&text) {
                if let Some(p) = cpp_resolve_include_file(base, &inc, cpp_include_dirs) {
                    if let Some(ty) = cpp_try_header_file_for_field(
                        &p,
                        class_name,
                        field_name,
                        visited,
                        depth - 1,
                        cpp_include_dirs,
                        job_cache,
                    ) {
                        found = Some(ty);
                        break;
                    }
                }
            }
            found
        }
    };
    if let Some(cache) = job_cache {
        cache.store_field(path, class_name, field_name, false, result.clone());
    }
    result
}

fn cpp_field_from_included_headers<D: Doc>(
    ctx: &RecvHintContext<'_>,
    recv: &Node<'_, D>,
    field_name: &str,
) -> Option<String> {
    let class_name = cpp_scope_class_name(recv)?;
    cpp_field_type_for_class_in_sources(ctx, class_name.as_str(), field_name)
}

fn cpp_hint<D: Doc>(recv: &Node<'_, D>, ctx: Option<&RecvHintContext<'_>>) -> Option<String> {
    let kind_k = recv.kind();
    let k = kind_k.as_ref();
    if matches!(k, "identifier" | "qualified_identifier" | "field_identifier") {
        let name = recv.text().trim().to_string();
        if let Some(ctx) = ctx {
            if let Some(config) = ctx.type_hint_config {
                if let Some(ty) = config.lookup_cpp_constant_type(name.as_str()) {
                    return Some(ty);
                }
            }
        }
    }
    let t = cpp_recv_base_name(recv);
    if t == "this" {
        return cpp_class_name(recv);
    }
    if let Some(ty) = cpp_local_in_enclosing_blocks(recv, &t) {
        return Some(ty);
    }
    if let Some(ty) = cpp_parameter_type_for_scope(recv, &t) {
        return Some(ty);
    }
    if let Some(ty) = cpp_field_in_class(recv, &t) {
        return Some(ty);
    }
    if let Some(ctx) = ctx {
        if let Some(ty) = cpp_field_from_included_headers(ctx, recv, &t) {
            return Some(ty);
        }
        if let Some(config) = ctx.type_hint_config {
            if let Some(ty) = config.lookup_cpp_constant_type(t.as_str()) {
                return Some(ty);
            }
        }
    }
    None
}

fn cpp_config_call_return<D: Doc>(
    node: &Node<'_, D>,
    ctx: &RecvHintContext<'_>,
) -> Option<String> {
    let config = ctx.type_hint_config?;
    if node.kind().as_ref() != "call_expression" {
        return None;
    }
    let func = node.field("function")?;
    let arg_types = cpp_collect_call_arg_types(node, ctx);
    if func.kind().as_ref() == "field_expression" {
        let arg = func.field("argument")?;
        let field = func.field("field")?;
        let method_name = field.text().trim().to_string();
        let class_ty = cpp_type_of_direct_receiver_expr(&arg, Some(ctx))?;
        let class_name = cpp_simplify_type_name(&class_ty);
        return config.lookup_cpp_method_return(class_name.as_str(), method_name.as_str(), &arg_types);
    }
    let name = func.text().trim().to_string();
    if name.is_empty() {
        return None;
    }
    config
        .lookup_cpp_macro_return(name.as_str(), &arg_types)
        .or_else(|| config.lookup_cpp_function_return(name.as_str(), &arg_types))
}

fn cpp_collect_call_arg_types<D: Doc>(
    node: &Node<'_, D>,
    ctx: &RecvHintContext<'_>,
) -> Vec<String> {
    let Some(args) = node.field("arguments") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for c in args.children() {
        if !c.is_named() {
            continue;
        }
        let expr = if c.kind().as_ref() == "argument" {
            if let Some(inner) = c.children().find(|x| x.is_named()) {
                inner
            } else {
                continue;
            }
        } else {
            c
        };
        if let Some(ty) = cpp_expr_type_label_for_config(&expr, ctx) {
            out.push(ty);
        } else {
            out.push("?".to_string());
        }
    }
    out
}

fn cpp_expr_type_label_for_config<D: Doc>(
    node: &Node<'_, D>,
    ctx: &RecvHintContext<'_>,
) -> Option<String> {
    if let Some(lit) = syntax_kind_literal_hint(node) {
        return Some(lit);
    }
    let kind_k = node.kind();
    let k = kind_k.as_ref();
    if matches!(k, "identifier" | "qualified_identifier" | "field_identifier") {
        return cpp_hint(node, Some(ctx));
    }
    if k == "call_expression" {
        if let Some(ty) = cpp_config_call_return(node, ctx) {
            return Some(ty);
        }
    }
    if k == "parenthesized_expression" {
        if let Some(inner) = node.children().find(|c| c.is_named()) {
            return cpp_expr_type_label_for_config(&inner, ctx);
        }
    }
    if k == "binary_expression" {
        return cpp_binary_result_type(node, Some(ctx));
    }
    if k == "field_expression" {
        let arg = node.field("argument")?;
        let field = node.field("field")?;
        let field_name = field.text().trim().to_string();
        let arg_ty = cpp_expr_type_label_for_config(&arg, ctx)?;
        let class_name = cpp_simplify_type_name(&arg_ty);
        if let Some(config) = ctx.type_hint_config {
            if let Some(ty) = config.lookup_cpp_field_type(class_name.as_str(), field_name.as_str()) {
                return Some(ty);
            }
        }
        return cpp_field_type_for_class_in_sources(ctx, class_name.as_str(), field_name.as_str());
    }
    cpp_hint(node, Some(ctx))
}

fn cpp_binary_operator_text<D: Doc>(node: &Node<'_, D>) -> Option<String> {
    let left = node.field("left")?;
    let right = node.field("right")?;
    let full = node.text().trim().to_string();
    let lt = left.text().trim().to_string();
    let rt = right.text().trim().to_string();
    let li = full.find(lt.as_str())?;
    let after_left = li + lt.len();
    let ri = full[after_left..].find(rt.as_str())?;
    let op = full[after_left..after_left + ri].trim();
    if op.is_empty() {
        None
    } else {
        Some(op.to_string())
    }
}

fn cpp_normalize_binary_operand_label(ty: &str) -> String {
    match ty {
        "NumberLiteral"
        | "DecimalLiteral"
        | "HexadecimalLiteral"
        | "BinaryLiteral"
        | "OctalLiteral" => "IntegerLiteral".to_string(),
        other => other.to_string(),
    }
}

fn cpp_binary_result_type<D: Doc>(
    node: &Node<'_, D>,
    ctx: Option<&RecvHintContext<'_>>,
) -> Option<String> {
    if node.kind().as_ref() != "binary_expression" {
        return None;
    }
    let left = node.field("left")?;
    let right = node.field("right")?;
    let op = cpp_binary_operator_text(node)?;
    let lhs_ty = ctx
        .and_then(|c| cpp_expr_type_label_for_config(&left, c))
        .or_else(|| syntax_kind_literal_hint(&left))?;
    let rhs_ty = ctx
        .and_then(|c| cpp_expr_type_label_for_config(&right, c))
        .or_else(|| syntax_kind_literal_hint(&right))?;
    if let Some(c) = ctx {
        if let Some(config) = c.type_hint_config {
            let lhs_cfg = cpp_normalize_binary_operand_label(lhs_ty.as_str());
            let rhs_cfg = cpp_normalize_binary_operand_label(rhs_ty.as_str());
            if let Some(ty) =
                config.lookup_cpp_binary_op_return(op.as_str(), &lhs_cfg, &rhs_cfg)
            {
                return Some(ty);
            }
        }
    }
    cpp_default_binary_type(lhs_ty.as_str(), rhs_ty.as_str(), op.as_str())
}

fn cpp_default_binary_type(lhs: &str, rhs: &str, op: &str) -> Option<String> {
    if matches!(op, "==" | "!=" | "<" | ">" | "<=" | ">=" | "&&" | "||") {
        return Some("bool".to_string());
    }
    if cpp_is_floating_label(lhs) || cpp_is_floating_label(rhs) {
        if matches!(op, "+" | "-" | "*" | "/" | "%") {
            return Some("double".to_string());
        }
    }
    if cpp_is_numeric_label(lhs) && cpp_is_numeric_label(rhs) {
        if matches!(op, "+" | "-" | "*" | "/" | "%") {
            return Some("int".to_string());
        }
    }
    if matches!(op, "+" | "-" | "*" | "/" | "%") {
        if cpp_is_numeric_label(lhs) && cpp_looks_integral_type(rhs) {
            return Some(rhs.to_string());
        }
        if cpp_is_numeric_label(rhs) && cpp_looks_integral_type(lhs) {
            return Some(lhs.to_string());
        }
    }
    None
}

fn cpp_is_numeric_label(s: &str) -> bool {
    matches!(
        s,
        "IntegerLiteral"
            | "NumberLiteral"
            | "FloatingPointLiteral"
            | "int"
            | "long"
            | "short"
            | "double"
            | "float"
            | "size_t"
            | "UINT"
            | "DWORD"
            | "bool"
    )
}

fn cpp_is_floating_label(s: &str) -> bool {
    matches!(s, "FloatingPointLiteral" | "double" | "float")
}

fn cpp_looks_integral_type(s: &str) -> bool {
    !matches!(s, "?" | "StringLiteral" | "void*" | "bool")
        && !s.ends_with('*')
        && s != "FloatingPointLiteral"
}

fn c_hint<D: Doc>(recv: &Node<'_, D>) -> Option<String> {
    let _ = recv;
    None
}

/// クラス body 内の `annotated_assignment`（クラス変数の型注釈）と名前を照合する。
fn python_field_in_class<D: Doc>(recv: &Node<'_, D>, name: &str) -> Option<String> {
    let class_def = recv
        .ancestors()
        .find(|n| n.kind().as_ref() == "class_definition")?;
    let body = class_def.field("body")?;
    for child in body.children() {
        if child.kind().as_ref() != "expression_statement" {
            continue;
        }
        let inner = child.child(0)?;
        if inner.kind().as_ref() != "annotated_assignment" {
            continue;
        }
        let left = inner.field("left")?;
        if left.text().trim() != name {
            continue;
        }
        return inner.field("type").map(|t| t.text().trim().to_string());
    }
    None
}

fn python_hint<D: Doc>(recv: &Node<'_, D>) -> Option<String> {
    let recv_text = recv.text();
    let t = recv_text.trim();
    if t == "self" {
        return recv
            .ancestors()
            .find(|n| n.kind().as_ref() == "class_definition")
            .and_then(|n| n.field("name").map(|x| x.text().trim().to_string()));
    }
    python_field_in_class(recv, t)
}

fn kotlin_class_name<D: Doc>(recv: &Node<'_, D>) -> Option<String> {
    let node = recv.ancestors().find(|n| {
        let k = n.kind();
        k.as_ref() == "class_declaration" || k.as_ref() == "object_declaration"
    })?;
    let id = node
        .children()
        .find(|c| c.kind().as_ref() == "type_identifier")?;
    let s = id.text();
    Some(s.trim().to_string())
}

fn kotlin_hint<D: Doc>(recv: &Node<'_, D>) -> Option<String> {
    let recv_text = recv.text();
    let t = recv_text.trim();
    if matches!(t, "this" | "super") {
        return kotlin_class_name(recv);
    }
    None
}

fn scala_class_name<D: Doc>(recv: &Node<'_, D>) -> Option<String> {
    let node = recv.ancestors().find(|n| {
        let k = n.kind();
        matches!(
            k.as_ref(),
            "class_definition" | "object_definition" | "trait_definition"
        )
    })?;
    for c in node.children() {
        if c.kind().as_ref() == "identifier" {
            let s = c.text();
            return Some(s.trim().to_string());
        }
        for cc in c.children() {
            if cc.kind().as_ref() == "identifier" {
                let s = cc.text();
                return Some(s.trim().to_string());
            }
        }
    }
    None
}

fn scala_hint<D: Doc>(recv: &Node<'_, D>) -> Option<String> {
    let recv_text = recv.text();
    let t = recv_text.trim();
    if matches!(t, "this" | "super") {
        return scala_class_name(recv);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use ast_grep_core::Pattern;
    use ast_grep_language::SupportLang;

    use crate::lang::SupportedLanguage;

    fn cpp_recv_hint(src: &str, pattern: &str) -> Option<String> {
        let grep = SupportLang::Cpp.ast_grep(src);
        let pat = Pattern::try_new(pattern, SupportLang::Cpp).unwrap();
        let root = grep.root();
        let m = root.find_all(&pat).next().expect("one match");
        let recv = m.get_env().get_match("RECV").expect("RECV");
        infer_recv_type(SupportedLanguage::Cpp, recv, None)
    }

    fn java_recv_hint(src: &str, pattern: &str) -> Option<String> {
        let grep = SupportLang::Java.ast_grep(src);
        let pat = Pattern::try_new(pattern, SupportLang::Java).unwrap();
        let root = grep.root();
        let m = root.find_all(&pat).next().expect("one match");
        let recv = m.get_env().get_match("RECV").expect("RECV");
        infer_recv_type(SupportedLanguage::Java, recv, None)
    }

    #[test]
    fn java_method_parameter_receiver_in_if_condition() {
        let src = r#"
class Foo {
  public static String stripComments(String s) {
    if (s == null || s.isEmpty()) return "";
    try {
      return "";
    } catch (Exception e) {
      return "";
    }
  }
}
"#;
        let hint = java_recv_hint(src, "$RECV.isEmpty()");
        assert_eq!(hint.as_deref(), Some("String"));
    }

    #[test]
    fn java_method_parameter_via_method_pattern() {
        let src = r#"
class Bar {
  void f(String s) {
    s.trim();
  }
}
"#;
        let hint = java_recv_hint(src, "$RECV.$METHOD($$$ARGS)");
        assert_eq!(hint.as_deref(), Some("String"));
    }

    #[test]
    fn java_enhanced_for_variable_receiver_in_loop_body() {
        let src = r#"
class Foo {
  List<String> toIfElseLines(List<String> thenLines) {
    java.util.List<String> out = new java.util.ArrayList<>();
    for (String line : thenLines) {
      out.add(line.trim());
    }
    return out;
  }
}
"#;
        let hint = java_recv_hint(src, "$RECV.trim()");
        assert_eq!(hint.as_deref(), Some("String"));
    }

    #[test]
    fn java_method_pattern_resolves_line_trim_inside_add_argument() {
        let src = r#"
class Foo {
  List<String> toIfElseLines(List<String> thenLines, List<String> elseLines) {
    java.util.List<String> out = new java.util.ArrayList<>();
    String blockIndent = "    ";
    out.add("if");
    for (String line : thenLines) {
      out.add(blockIndent + line.trim());
    }
    out.add("else");
    for (String line : elseLines) {
      out.add(blockIndent + line.trim());
    }
    return out;
  }
}
"#;
        let grep = SupportLang::Java.ast_grep(src);
        let pat = Pattern::try_new("$RECV.$METHOD($$$ARGS)", SupportLang::Java).unwrap();
        let root = grep.root();
        let hints: Vec<(String, Option<String>)> = root
            .find_all(&pat)
            .map(|m| {
                let recv = m.get_env().get_match("RECV").expect("RECV");
                let recv_text = recv.text().trim().to_string();
                let hint = infer_recv_type(SupportedLanguage::Java, recv, None);
                (recv_text, hint)
            })
            .collect();

        assert!(
            hints
                .iter()
                .any(|(recv, hint)| recv == "out"
                    && hint.as_deref() == Some("java.util.List<String>"))
        );
        assert!(hints
            .iter()
            .any(|(recv, hint)| recv == "line" && hint.as_deref() == Some("String")));
    }

    #[test]
    fn java_outer_local_receiver_resolves_inside_enhanced_for_block() {
        let src = r#"
class Foo {
  List<String> toIfElseLines(List<String> thenLines) {
    java.util.List<String> out = new java.util.ArrayList<>();
    for (String line : thenLines) {
      out.add(line.trim());
    }
    return out;
  }
}
"#;
        let grep = SupportLang::Java.ast_grep(src);
        let pat = Pattern::try_new("$RECV.add($$$ARGS)", SupportLang::Java).unwrap();
        let root = grep.root();
        let m = root.find_all(&pat).next().expect("one match");
        let recv = m.get_env().get_match("RECV").expect("RECV");
        let hint = infer_recv_type(SupportedLanguage::Java, recv, None);
        assert_eq!(recv.text().trim(), "out");
        assert_eq!(hint.as_deref(), Some("java.util.List<String>"));
    }

    #[test]
    fn cpp_simple_local_primitive_int() {
        let src = r#"
void f() {
  int x = 0;
  x.foo();
}
"#;
        let hint = cpp_recv_hint(src, "$RECV.$METHOD($$$ARGS)");
        assert_eq!(hint.as_deref(), Some("int"));
    }

    #[test]
    fn cpp_local_without_initializer_omits_variable_name() {
        let src = r#"
void f() {
  CString pat;
  pat.Format("x");
}
"#;
        let hint = cpp_recv_hint(src, "$RECV.$METHOD($$$ARGS)");
        assert_eq!(hint.as_deref(), Some("CString"));
    }

    #[test]
    fn cpp_local_name_is_not_taken_from_initializer_expression() {
        let src = r#"
void f() {
  CString pat;
  int i = src.Find(pat);
  pat.GetLength();
}
"#;
        let hint = cpp_recv_hint(src, "$RECV.GetLength()");
        assert_eq!(hint.as_deref(), Some("CString"));
    }

    #[test]
    fn cpp_parameter_reference_type() {
        let src = r#"
void JsonEscape(const CString& s) {
  s.GetLength();
}
"#;
        let hint = cpp_recv_hint(src, "$RECV.$METHOD($$$ARGS)");
        assert_eq!(hint.as_deref(), Some("const CString &"));
    }

    #[test]
    fn cpp_parameter_pointer_type_for_arrow_call() {
        let src = r#"
void Use(CString* s) {
  s->GetLength();
}
"#;
        let hint = cpp_recv_hint(src, "$RECV->$METHOD($$$ARGS)");
        assert_eq!(hint.as_deref(), Some("CString *"));
    }

    #[test]
    fn cpp_parameter_rvalue_reference_type() {
        let src = r#"
void Use(CString&& s) {
  s.GetLength();
}
"#;
        let hint = cpp_recv_hint(src, "$RECV.$METHOD($$$ARGS)");
        assert_eq!(hint.as_deref(), Some("CString &&"));
    }

    #[test]
    fn cpp_parameter_qualified_type_name() {
        let src = r#"
void Use(const ATL::CStringW& s) {
  s.GetLength();
}
"#;
        let hint = cpp_recv_hint(src, "$RECV.$METHOD($$$ARGS)");
        assert_eq!(hint.as_deref(), Some("const ATL::CStringW &"));
    }

    #[test]
    fn cpp_parameter_in_out_of_line_member_definition() {
        let src = r#"
struct Foo {
  void Use(const CString& s);
};

void Foo::Use(const CString& s) {
  s.GetLength();
}
"#;
        let hint = cpp_recv_hint(src, "$RECV.$METHOD($$$ARGS)");
        assert_eq!(hint.as_deref(), Some("const CString &"));
    }

    #[test]
    fn cpp_second_call_in_chain_resolves_local_type() {
        let src = r#"
void f() {
  CTime time(2024, 3, 15, 10, 30, 0);
  time.Format("%Y").Format("[%s]");
}
"#;
        let grep = SupportLang::Cpp.ast_grep(src);
        let pat = Pattern::try_new("$RECV.$METHOD($$$ARGS)", SupportLang::Cpp).unwrap();
        let root = grep.root();
        let matches: Vec<_> = root.find_all(&pat).collect();
        assert!(
            matches.len() >= 2,
            "expected chain to yield two matches, got {}",
            matches.len()
        );
        let recv_call = matches
            .iter()
            .map(|m| m.get_env().get_match("RECV").expect("RECV capture"))
            .find(|r| r.kind().as_ref() == "call_expression")
            .expect("expected a chain match where $RECV is a call_expression");
        assert_eq!(cpp_recv_base_name(recv_call).as_str(), "time");
        let hint = infer_recv_type(SupportedLanguage::Cpp, recv_call, None);
        assert_eq!(hint.as_deref(), Some("CTime.Format"));
    }

    #[test]
    fn cpp_comma_separated_declarators_resolve_third_variable() {
        let src = r#"
void Pattern1_DirectFormatNest4()
{
    CString str1, str2, result_str;
    CTime   time(2024, 3, 15, 10, 30, 0);

    result_str.Format("LOG: %s",
        str2.Format(">> %s",
            str1.Format("[%s]",
                time.Format("%Y/%m/%d"))));
}
"#;
        let hint = cpp_recv_hint(src, r##"$RECV.Format("LOG: %s", $$$ARGS)"##);
        assert_eq!(hint.as_deref(), Some("CString"));
    }

    #[test]
    fn cpp_infer_capture_chain_field_expression_type() {
        let src = r#"
struct Inner { int z; };
struct Foo { Inner inner; };
void f() {
  Foo foo{};
  foo.inner;
}
"#;
        let grep = SupportLang::Cpp.ast_grep(src);
        let pat = Pattern::try_new("$CHAIN", SupportLang::Cpp).unwrap();
        let root = grep.root();
        let m = root
            .find_all(&pat)
            .find(|m| m.get_node().text().trim() == "foo.inner")
            .expect("match foo.inner");
        let cap = m.get_env().get_match("CHAIN").expect("CHAIN");
        let ctx = RecvHintContext {
            file_path: std::path::Path::new("test.cpp"),
            source: src,
            cpp_include_dirs: &[],
            job_cache: None,
            type_hint_config: None,
        };
        let hint = infer_capture_type(SupportedLanguage::Cpp, "CHAIN", cap, Some(&ctx));
        assert_eq!(hint.as_deref(), Some("Inner"));
    }

    #[test]
    fn java_infer_capture_chain_field_access_type() {
        let src = r#"
class Inner {
  int y;
}
class Foo {
  Inner inner;
  void m() {
    Foo f = new Foo();
    f.inner;
  }
}
"#;
        let grep = SupportLang::Java.ast_grep(src);
        let pat = Pattern::try_new("$CHAIN", SupportLang::Java).unwrap();
        let root = grep.root();
        let m = root
            .find_all(&pat)
            .find(|m| m.get_node().text().trim() == "f.inner")
            .expect("match f.inner");
        let cap = m.get_env().get_match("CHAIN").expect("CHAIN");
        let hint = infer_capture_type(SupportedLanguage::Java, "CHAIN", cap, None);
        assert_eq!(hint.as_deref(), Some("Inner"));
    }

    #[test]
    fn humanize_string_literal_kind() {
        assert_eq!(humanize_tree_sitter_kind("string_literal"), "StringLiteral");
    }

    #[test]
    fn cpp_infer_capture_string_literal_under_argument() {
        let src = r#"void f() { x.Format("[%c]", y); }"#;
        let grep = SupportLang::Cpp.ast_grep(src);
        let pat = Pattern::try_new("$RECV.$METHOD($$$ARGS)", SupportLang::Cpp).unwrap();
        let m = grep.root().find_all(&pat).next().expect("match");
        let caps: Vec<_> = m
            .get_env()
            .get_multiple_matches("ARGS")
            .into_iter()
            .filter(|n| n.is_named())
            .collect();
        let first = caps.first().expect("first arg");
        let hint = infer_capture_type(SupportedLanguage::Cpp, "ARGS", first, None);
        assert_eq!(hint.as_deref(), Some("StringLiteral"));
    }

    #[test]
    fn cpp_infer_capture_second_arg_nested_format_shows_class_dot_method_not_receiver_type() {
        let src = r#"
void f() {
  CString str;
  CTime time(2024, 3, 15, 10, 30, 0);
  str.Format("[%s]", time.Format("%Y/%m/%d"));
}
"#;
        let grep = SupportLang::Cpp.ast_grep(src);
        let pat = Pattern::try_new("$RECV.Format($A, $B)", SupportLang::Cpp).unwrap();
        let root = grep.root();
        let m = root.find_all(&pat).next().expect("match");
        let b = m.get_env().get_match("B").expect("B");
        let ctx = RecvHintContext {
            file_path: std::path::Path::new("test.cpp"),
            source: src,
            cpp_include_dirs: &[],
            job_cache: None,
            type_hint_config: None,
        };
        let hint = infer_capture_type(SupportedLanguage::Cpp, "B", b, Some(&ctx));
        assert_eq!(hint.as_deref(), Some("CTime.Format"));
    }

    #[test]
    fn cpp_infer_capture_multi_arg_nested_format_shows_class_dot_method() {
        let src = r#"
void f() {
  CString tmp;
  CTime time(2024, 3, 15, 10, 30, 0);
  CTime dt(2024, 3, 15, 10, 30, 0);
  tmp.Format("%s,%s", time.Format("%Y/%m/%d"), dt.Format("%H:%M:%S"));
}
"#;
        let grep = SupportLang::Cpp.ast_grep(src);
        let pat = Pattern::try_new("$RECV.Format($$$A)", SupportLang::Cpp).unwrap();
        let root = grep.root();
        let m = root.find_all(&pat).next().expect("match");
        let caps: Vec<_> = m
            .get_env()
            .get_multiple_matches("A")
            .into_iter()
            .filter(|n| n.is_named())
            .collect();
        assert!(
            caps.len() >= 3,
            "expected format + 2 nested Format calls, got {}",
            caps.len()
        );
        let ctx = RecvHintContext {
            file_path: std::path::Path::new("test.cpp"),
            source: src,
            cpp_include_dirs: &[],
            job_cache: None,
            type_hint_config: None,
        };
        let hint1 = infer_capture_type(SupportedLanguage::Cpp, "A", &caps[1], Some(&ctx));
        let hint2 = infer_capture_type(SupportedLanguage::Cpp, "A", &caps[2], Some(&ctx));
        assert_eq!(hint1.as_deref(), Some("CTime.Format"));
        assert_eq!(hint2.as_deref(), Some("CTime.Format"));
    }

    #[test]
    fn cpp_chain_resolves_field_type_via_extra_include_dir() {
        let base = std::env::temp_dir().join("ast_grep_gui_cpp_i_hint_test");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("inc")).expect("mkdir inc");
        std::fs::write(
            base.join("inc/foo.h"),
            "struct Inner { int z; };\nstruct Foo { Inner inner; };\n",
        )
        .expect("write foo.h");

        let src = "#include <foo.h>\nvoid f() {\n  Foo foo{};\n  foo.inner;\n}\n";
        let test_cpp = base.join("test.cpp");
        std::fs::write(&test_cpp, src).expect("write test.cpp");

        let extra = vec![base.join("inc")];
        let ctx = RecvHintContext {
            file_path: test_cpp.as_path(),
            source: src,
            cpp_include_dirs: &extra,
            job_cache: None,
            type_hint_config: None,
        };

        let grep = SupportLang::Cpp.ast_grep(src);
        let pat = Pattern::try_new("$CHAIN", SupportLang::Cpp).unwrap();
        let root = grep.root();
        let m = root
            .find_all(&pat)
            .find(|m| m.get_node().text().trim() == "foo.inner")
            .expect("match foo.inner");
        let cap = m.get_env().get_match("CHAIN").expect("CHAIN");
        let hint = infer_capture_type(SupportedLanguage::Cpp, "CHAIN", cap, Some(&ctx));
        assert_eq!(hint.as_deref(), Some("Inner"));
    }

    use crate::type_hint_config::{
        CppBinaryOpRule, CppCallableRule, CppConstantRule, CppFieldRule, CppMethodRule,
        CppTypeHintRules, TypeHintConfig, TypeHintConfigFile,
    };

    fn cpp_infer_pattern(
        src: &str,
        pattern: &str,
        config: &TypeHintConfig,
    ) -> Option<String> {
        let grep = SupportLang::Cpp.ast_grep(src);
        let pat = Pattern::try_new(pattern, SupportLang::Cpp).unwrap();
        let m = grep.root().find_all(&pat).next()?;
        let node = m.get_node();
        let ctx = RecvHintContext {
            file_path: std::path::Path::new("test.cpp"),
            source: src,
            cpp_include_dirs: &[],
            job_cache: None,
            type_hint_config: Some(config),
        };
        infer_capture_type(SupportedLanguage::Cpp, "X", &node, Some(&ctx))
    }

    fn sample_type_hint_config() -> TypeHintConfig {
        TypeHintConfig::from_file(TypeHintConfigFile::new(CppTypeHintRules {
            methods: vec![CppMethodRule {
                class: "CString".into(),
                method: "GetLength".into(),
                arity: Some(0),
                params: vec![],
                returns: "int".into(),
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
            binary_ops: vec![
                CppBinaryOpRule {
                    op: "+".into(),
                    lhs: "StringLiteral".into(),
                    rhs: "CString".into(),
                    returns: "CString".into(),
                    enabled: true,
                },
                CppBinaryOpRule {
                    op: "+".into(),
                    lhs: "LPCTSTR".into(),
                    rhs: "CString".into(),
                    returns: "CString".into(),
                    enabled: true,
                },
            ],
            ..Default::default()
        }))
    }

    #[test]
    fn cpp_config_method_return_overrides_label() {
        let src = "void f() { CString s; s.GetLength(); }";
        let cfg = sample_type_hint_config();
        let hint = cpp_infer_pattern(src, "s.GetLength()", &cfg);
        assert_eq!(hint.as_deref(), Some("int"));
    }

    #[test]
    fn cpp_config_macro_return() {
        let src = r#"void f() { _T("abc"); }"#;
        let cfg = sample_type_hint_config();
        let hint = cpp_infer_pattern(src, r#"_T("abc")"#, &cfg);
        assert_eq!(hint.as_deref(), Some("LPCTSTR"));
    }

    #[test]
    fn cpp_config_constant_type() {
        let src = "void f() { int x = IDC_OK; }";
        let cfg = sample_type_hint_config();
        let hint = cpp_infer_pattern(src, "IDC_OK", &cfg);
        assert_eq!(hint.as_deref(), Some("int"));
    }

    #[test]
    fn cpp_config_field_type() {
        let src = "void f() { CWnd w; w.m_hWnd; }";
        let cfg = sample_type_hint_config();
        let hint = cpp_infer_pattern(src, "w.m_hWnd", &cfg);
        assert_eq!(hint.as_deref(), Some("HWND"));
    }

    #[test]
    fn cpp_config_binary_string_plus() {
        let src = r#"void f(CString s) { "abc" + s; }"#;
        let cfg = sample_type_hint_config();
        let hint = cpp_infer_pattern(src, r#""abc" + s"#, &cfg);
        assert_eq!(hint.as_deref(), Some("CString"));
    }

    #[test]
    fn cpp_default_binary_int_chain() {
        let src = "void f() { int nSel; (nSel + 1) * 100; }";
        let cfg = TypeHintConfig::default();
        let hint = cpp_infer_pattern(src, "(nSel + 1) * 100", &cfg);
        assert_eq!(hint.as_deref(), Some("int"));
    }

    #[test]
    fn cpp_default_binary_int_add() {
        let src = "void f() { int nSel; nSel + 1; }";
        let cfg = TypeHintConfig::default();
        let hint = cpp_infer_pattern(src, "nSel + 1", &cfg);
        assert_eq!(hint.as_deref(), Some("int"));
    }

    #[test]
    fn cpp_parenthesized_binary_uses_config_rule() {
        let src = "void f() { (1 + 2); }";
        let cfg = TypeHintConfig::from_file(TypeHintConfigFile::new(CppTypeHintRules {
            binary_ops: vec![CppBinaryOpRule {
                op: "+".into(),
                lhs: "IntegerLiteral".into(),
                rhs: "IntegerLiteral".into(),
                returns: "CNumber".into(),
                enabled: true,
            }],
            ..Default::default()
        }));
        let hint = cpp_infer_pattern(src, "(1 + 2)", &cfg);
        assert_eq!(hint.as_deref(), Some("CNumber"));
    }

    #[test]
    fn recv_hint_job_cache_reuses_header_text_and_records_profile() {
        let base = std::env::temp_dir().join(format!(
            "ast_grep_gui_hint_cache_test_{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).expect("mkdir");
        let header = base.join("foo.h");
        std::fs::write(&header, "struct Foo { int x; };").expect("write header");

        let cache = RecvHintJobCache::new();
        let first = cache.load_header_text(&header).expect("read header");
        let second = cache.load_header_text(&header).expect("cached header");
        assert_eq!(first.as_ref(), second.as_ref());

        let snap = cache.profile().snapshot();
        assert_eq!(snap.header_reads, 1);
        assert_eq!(snap.header_cache_hits, 1);

        let _ = std::fs::remove_dir_all(&base);
    }
}
