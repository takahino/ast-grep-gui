//! パターンのメタ変数に束縛されたノードから、表示用の型ヒントを推定する（構文ベース・best-effort）。

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ast_grep_core::{Doc, Node};
use ast_grep_language::{LanguageExt, SupportLang};

use crate::file_encoding::{read_text_file, FileEncodingPreference};
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

/// ヘッダ/ソースから引くメンバ種別。キャッシュキーに含めることで同名のフィールドとメソッド
/// （さらに B-1 以降のフリー関数・グローバル変数・型別名）の検索結果が衝突しないようにする。
#[allow(dead_code)] // FreeFunction/GlobalVar/TypeAlias は B-1/B-2/B-3 で構築するまで未使用
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum CppLookupKind {
    Field,
    Method,
    FreeFunction,
    GlobalVar,
    TypeAlias,
    /// マクロ呼び出し形（is_call=true）の解決結果。キャッシュキーで呼び出し/値を区別。
    MacroCall,
    /// マクロ値形（is_call=false）の解決結果。
    MacroValue,
    /// クラスの基底クラス名リスト（P3）。class_name=派生クラス名、member_name=""。
    /// 値は基底名を `;` で連結した文字列（C++ クラス名に `;` は現れないため安全）。
    /// 空文字列＝基底無し、None＝未探索。負キャッシュ2層にそのまま相乗りする。
    BaseClasses,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CppMemberLookupKey {
    path: PathBuf,
    kind: CppLookupKind,
    class_name: String,
    member_name: String,
}

/// 1 検索ジョブ内で C++ include 読み込み・メンバ検索結果を共有する。
#[derive(Debug)]
pub struct RecvHintJobCache {
    profile: Arc<TypeHintProfile>,
    header_text: Mutex<HashMap<PathBuf, Option<Arc<str>>>>,
    /// 現ソース内の宣言由来のメンバ型キャッシュ（負キャッシュ含む）。
    source_lookup: Mutex<HashMap<CppMemberLookupKey, Option<String>>>,
    /// インクルードヘッダ由来のメンバ型キャッシュ（負キャッシュ含む）。
    header_lookup: Mutex<HashMap<CppMemberLookupKey, Option<String>>>,
    /// パスごとの #define 走査結果キャッシュ（C: マクロ限定解析）。
    defines: Mutex<HashMap<PathBuf, Arc<HashMap<String, CppMacroDef>>>>,
}

impl RecvHintJobCache {
    pub fn new() -> Self {
        Self::with_profile(Arc::new(TypeHintProfile::default()))
    }

    pub fn with_profile(profile: Arc<TypeHintProfile>) -> Self {
        Self {
            profile,
            header_text: Mutex::new(HashMap::new()),
            source_lookup: Mutex::new(HashMap::new()),
            header_lookup: Mutex::new(HashMap::new()),
            defines: Mutex::new(HashMap::new()),
        }
    }

    pub fn profile(&self) -> &TypeHintProfile {
        &self.profile
    }

    /// メンバ型キャッシュを引く。外側 None はキャッシュミス、
    /// 内側 None は「探索したが見つからなかった」負キャッシュ。
    fn lookup_member(
        &self,
        path: &Path,
        kind: CppLookupKind,
        class_name: &str,
        member_name: &str,
        in_source: bool,
    ) -> Option<Option<String>> {
        let key = CppMemberLookupKey {
            path: cpp_path_key(path),
            kind,
            class_name: class_name.to_string(),
            member_name: member_name.to_string(),
        };
        let map = if in_source {
            self.source_lookup.lock().ok()?
        } else {
            self.header_lookup.lock().ok()?
        };
        if let Some(v) = map.get(&key) {
            self.profile
                .lookup_cache_hits
                .fetch_add(1, Ordering::Relaxed);
            return Some(v.clone());
        }
        None
    }

    fn store_member(
        &self,
        path: &Path,
        kind: CppLookupKind,
        class_name: &str,
        member_name: &str,
        in_source: bool,
        value: Option<String>,
    ) {
        let key = CppMemberLookupKey {
            path: cpp_path_key(path),
            kind,
            class_name: class_name.to_string(),
            member_name: member_name.to_string(),
        };
        let guard = if in_source {
            self.source_lookup.lock()
        } else {
            self.header_lookup.lock()
        };
        if let Ok(mut map) = guard {
            map.insert(key, value);
        }
    }

    /// パスごとの #define 走査結果キャッシュを引く（C: マクロ限定解析）。
    fn load_defines(&self, path: &Path) -> Option<Arc<HashMap<String, CppMacroDef>>> {
        let key = cpp_path_key(path);
        let map = self.defines.lock().ok()?;
        map.get(&key).cloned()
    }

    fn store_defines(&self, path: &Path, defines: Arc<HashMap<String, CppMacroDef>>) {
        let key = cpp_path_key(path);
        if let Ok(mut map) = self.defines.lock() {
            map.insert(key, defines);
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
        // 読み込み失敗（存在しない・権限不足・バイナリ判定等）も負キャッシュする。
        // 同一ジョブ内で読めないヘッダを N 回参照しても再読み込みを繰り返さないため。
        let len = match fs::metadata(path) {
            Ok(m) => m.len(),
            Err(_) => {
                self.store_header_text(&key, None);
                return None;
            }
        };
        if len > CPP_INCLUDE_MAX_FILE_BYTES as u64 {
            self.store_header_text(&key, None);
            return None;
        }
        // ヘッダは本体と別エンコーディング（Shift_JIS / UTF-16 / BOM 付き）であり得るため
        // Auto 判定で読む。検索本体と同じ file_encoding::read_text_file 経路で非対称を解消。
        let text = match read_text_file(path, FileEncodingPreference::Auto) {
            Ok(t) => t.text,
            Err(_) => {
                self.store_header_text(&key, None);
                return None;
            }
        };
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
    // E: C 言語も C++ と同じ推論経路（設定ルール・バイナリ・チェイン・cpp_hint）を使う。
    if matches!(lang, SupportedLanguage::Cpp | SupportedLanguage::C) {
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
    if matches!(lang, SupportedLanguage::Cpp | SupportedLanguage::C) && _capture_name == "RECV" {
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
    if matches!(lang, SupportedLanguage::Cpp | SupportedLanguage::C) {
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
        // E: C も cpp_hint に統合（c_hint は削除）。
        SupportedLanguage::Cpp | SupportedLanguage::C => cpp_hint(node, ctx),
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
        // E: C 言語も C++ と同じチェイン解決経路を使う。
        SupportedLanguage::Cpp | SupportedLanguage::C => {
            ctx.and_then(|c| cpp_chain_result_type(node, c))
        }
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
    // テンプレート引数除去: < で切る（vector<int> → vector）。
    let s = s.split('<').next().unwrap_or(s).trim();
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
    cpp_lookup_member_in_sources(ctx, CppLookupKind::Field, class_name, field_name)
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
                    cpp_method_return_in_class_body(&body, method_name, out);
                    if out.is_some() {
                        return;
                    }
                }
            }
        }
    }
    // P1-c: 無名 typedef struct（`typedef struct { long x; } POINT;`）の body を直接検索。
    // タグ付き typedef struct は既存のタグ名経路（cpp_type_alias_target_from_node → typedef 1段展開）
    // に任せ、無名（type が name フィールド無しの struct/union specifier で body 持ち）の場合のみ
    // このアームが働く。declarator の名前が class_name（エイリアス名）と一致したら body を走査。
    // ポインタ typedef（`typedef struct {...} *P;`）も is_some で受理し、`p->M()` を解決する。
    if kind.as_ref() == "type_definition" {
        if let Some(t) = node.field("type") {
            if matches!(t.kind().as_ref(), "struct_specifier" | "union_specifier")
                && t.field("name").is_none()
            {
                if let Some(body) = t.field("body") {
                    for d in node.field_children("declarator") {
                        if cpp_typedef_declarator_pointer_count_if_named(&d, class_name).is_some() {
                            cpp_method_return_in_class_body(&body, method_name, out);
                            if out.is_some() {
                                return;
                            }
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

/// クラス／構造体 body 内のメソッド戻り値型を探す。インライン定義（function_definition）と
/// プロトタイプ宣言（field_declaration）の両方から既存の合成ヘルパで戻り値型を作る。
/// class_specifier アームと P1-c の無名 typedef struct アームで共有する。
fn cpp_method_return_in_class_body<D: Doc>(
    body: &Node<'_, D>,
    method_name: &str,
    out: &mut Option<String>,
) {
    if out.is_some() {
        return;
    }
    for c in body.children() {
        let ty = match c.kind().as_ref() {
            "function_definition" => cpp_function_definition_return(&c, method_name),
            "field_declaration" => cpp_function_declaration_return(&c, method_name),
            _ => None,
        };
        if let Some(ty) = ty {
            *out = Some(ty);
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
    if out.is_some() {
        return out;
    }
    // P2: in-class 検索（プロトタイプ／インライン定義）で解決できなければクラス外定義を試す。
    // `.cpp` 側の `CWnd* CMyApp::GetMainWnd() { ... }`（TU 直下の function_definition）や
    // クラス外プロトタイプ宣言から戻り値型を引く。走査は関数本体に入らないため誤ヒントリスク低。
    cpp_method_return_out_of_line_in_tree(root, class_name, method_name)
}

/// out-of-class メソッド定義（`.cpp` 側の `CWnd* CMyApp::GetMainWnd() { ... }`）または
/// クラス外プロトタイプ宣言から (class_name, method_name) の戻り値型を取り出す（P2）。
/// 走査規則は cpp_find_free_function_return_in_scope と同じ（TU 直下 + linkage_specification /
/// declaration_list + namespace 1 段）。関数本体・クラス内部には入らない（誤ヒント回避）。
fn cpp_method_return_out_of_line_in_tree<D: Doc>(
    root: &Node<'_, D>,
    class_name: &str,
    method_name: &str,
) -> Option<String> {
    cpp_find_out_of_line_method_return_in_scope(root, class_name, method_name, 0)
}

fn cpp_find_out_of_line_method_return_in_scope<D: Doc>(
    node: &Node<'_, D>,
    class_name: &str,
    method_name: &str,
    namespace_depth: usize,
) -> Option<String> {
    for c in node.children() {
        let k = c.kind();
        let k = k.as_ref();
        if let Some(ty) = cpp_out_of_line_method_return_from_decl(&c, class_name, method_name) {
            return Some(ty);
        }
        // `extern "C" { ... }`（linkage_specification）と declaration_list の中を 1 階層分試す。
        if k == "linkage_specification" || k == "declaration_list" {
            if let Some(ty) =
                cpp_find_out_of_line_method_return_in_scope(&c, class_name, method_name, namespace_depth)
            {
                return Some(ty);
            }
        }
        // namespace は 1 段だけ（無限展開防止）。
        if k == "namespace_definition" && namespace_depth < 1 {
            if let Some(ty) = cpp_find_out_of_line_method_return_in_scope(
                &c,
                class_name,
                method_name,
                namespace_depth + 1,
            ) {
                return Some(ty);
            }
        }
    }
    None
}

/// function_definition または declaration ノードが (class_name, method_name) の
/// out-of-class メソッド定義／プロトタイプなら戻り値型を合成する。
fn cpp_out_of_line_method_return_from_decl<D: Doc>(
    node: &Node<'_, D>,
    class_name: &str,
    method_name: &str,
) -> Option<String> {
    match node.kind().as_ref() {
        "function_definition" => {
            let decl = node.field("declarator")?;
            if !cpp_declarator_is_out_of_line_method(&decl, class_name, method_name) {
                return None;
            }
            let spec = node
                .field("type")
                .map(|t| t.text().trim().to_string())
                .or_else(|| cpp_declaration_specifiers_text(node))?;
            let ops = cpp_declarator_type_ops(decl);
            Some(cpp_combine_type(&spec, &ops))
        }
        "declaration" => {
            let spec = cpp_declaration_specifiers_text(node)?;
            for d in node.field_children("declarator") {
                let target = if d.kind().as_ref() == "init_declarator" {
                    d.field("declarator")
                } else {
                    Some(d.clone())
                };
                if let Some(t) = target {
                    if cpp_declarator_is_out_of_line_method(&t, class_name, method_name) {
                        let ops = cpp_declarator_type_ops(t);
                        return Some(cpp_combine_type(&spec, &ops));
                    }
                }
            }
            None
        }
        _ => None,
    }
}

/// declarator ツリーを下降して out-of-class メソッドの qualified_identifier を探し、
/// (class_name, method_name) に一致するか検証する（P2）。
/// `CWnd* CMyApp::GetMainWnd()` → declarator = function_declarator(declarator:
/// pointer_declarator(qualified_identifier(CMyApp::GetMainWnd)))。qualified_identifier のテキストを
/// 最後の `::` で分割し、suffix == method_name・prefix の最終 `::` セグメント == class_name を照合する
/// （cpp_out_of_line_class_name と同手法）。cpp_declarator_matches_name は leaf 名しか見ないため
/// 流用すると `COther::GetMainWnd` に誤マッチする→クラス prefix 検証が必須。
fn cpp_declarator_is_out_of_line_method<D: Doc>(
    d: &Node<'_, D>,
    class_name: &str,
    method_name: &str,
) -> bool {
    let Some(qid) = cpp_find_qualified_identifier_in_declarator(d.clone()) else {
        return false;
    };
    let qid_text = qid.text();
    let text = qid_text.trim();
    let Some(pos) = text.rfind("::") else {
        return false;
    };
    let suffix = text[pos + 2..].trim();
    let prefix = text[..pos].trim();
    let class_seg = prefix.rsplit("::").next().unwrap_or(prefix);
    let class_seg = class_seg.trim_start_matches('*').trim_end_matches('*').trim();
    suffix == method_name && class_seg == class_name
}

/// declarator ツリーを declarator フィールドで下降し、qualified_identifier を見つける。
/// function_declarator / pointer_declarator / init_declarator 等を辿る。
/// parameter_list 等には入らないため仮引数型の qualified_identifier に誤マッチしない。
fn cpp_find_qualified_identifier_in_declarator<D: Doc>(
    d: Node<'_, D>,
) -> Option<Node<'_, D>> {
    let kind = d.kind();
    let k = kind.as_ref();
    if k == "qualified_identifier" {
        return Some(d);
    }
    if let Some(inner) = d.field("declarator") {
        return cpp_find_qualified_identifier_in_declarator(inner);
    }
    None
}

/// translation_unit 直下から kind に応じたメンバの型を引く（ディスパッチ）。
/// Field/Method は既存の関数に委譲し、FreeFunction/GlobalVar/TypeAlias は B-1/B-2/B-3 で実装する。
fn cpp_lookup_in_translation_unit<D: Doc>(
    root: &Node<'_, D>,
    kind: CppLookupKind,
    class_name: &str,
    member_name: &str,
) -> Option<String> {
    match kind {
        CppLookupKind::Field => cpp_field_in_named_translation_unit(root, class_name, member_name),
        CppLookupKind::Method => {
            cpp_method_return_in_named_translation_unit(root, class_name, member_name)
        }
        CppLookupKind::FreeFunction => {
            cpp_free_function_return_in_translation_unit(root, member_name)
        }
        CppLookupKind::GlobalVar => {
            cpp_global_var_type_in_translation_unit(root, member_name)
        }
        CppLookupKind::TypeAlias => {
            cpp_type_alias_target_in_translation_unit(root, member_name)
        }
        // マクロは AST の translation_unit からは解決しない（#define は事前スキャン経由）。
        CppLookupKind::MacroCall | CppLookupKind::MacroValue => None,
        // P3: クラスの基底リストを `;` 連結文字列で返す（空 Vec は None＝負キャッシュ）。
        CppLookupKind::BaseClasses => cpp_find_class_bases_in_tree(root, class_name),
    }
}

// ===== P3: 継承（基底クラス遡り、AST 自動解析） =====

/// class_specifier / struct_specifier の base_class_clause から基底クラス名を抽出する（P3）。
/// base_class_clause は fields 無し・children のみ。子のうち:
/// - type_identifier → テキストそのまま
/// - qualified_identifier → cpp_simplify_type_name で最終 `::` セグメントに還元
/// - template_type → field("name") のテキスト（CArray<CString, CString&> → CArray）
/// access_specifier / attribute_declaration / virtual キーワード（無名ノード）はスキップ。
fn cpp_class_bases_from_specifier<D: Doc>(node: &Node<'_, D>) -> Vec<String> {
    let mut bases = Vec::new();
    for c in node.children() {
        if c.kind().as_ref() != "base_class_clause" {
            continue;
        }
        for b in c.children() {
            let k = b.kind();
            let k = k.as_ref();
            if k == "type_identifier" {
                let t = b.text().trim().to_string();
                if !t.is_empty() {
                    bases.push(t);
                }
            } else if k == "qualified_identifier" {
                let t = cpp_simplify_type_name(b.text().trim());
                if !t.is_empty() {
                    bases.push(t);
                }
            } else if k == "template_type" {
                if let Some(name) = b.field("name") {
                    let t = name.text().trim().to_string();
                    if !t.is_empty() {
                        bases.push(t);
                    }
                }
            }
            // access_specifier / attribute_declaration / その他は無視
        }
    }
    bases
}

/// translation_unit 全体から class_name のクラス定義を探し、基底リストを `;` 連結文字列で返す（P3）。
/// cpp_find_field_in_named_class と同じ全子孫再帰。見つからなければ None（負キャッシュ用）。
/// 空の基底リスト（基底無し）は空文字列 "" を返し、None とは区別する。
fn cpp_find_class_bases_in_tree<D: Doc>(
    root: &Node<'_, D>,
    class_name: &str,
) -> Option<String> {
    let mut out: Option<Vec<String>> = None;
    cpp_find_class_bases_recursive(root, class_name, &mut out);
    out.map(|bases| bases.join(";"))
}

fn cpp_find_class_bases_recursive<D: Doc>(
    node: &Node<'_, D>,
    class_name: &str,
    out: &mut Option<Vec<String>>,
) {
    if out.is_some() {
        return;
    }
    if matches!(
        node.kind().as_ref(),
        "class_specifier" | "struct_specifier"
    ) {
        if let Some(n) = node.field("name") {
            if n.text().trim() == class_name {
                *out = Some(cpp_class_bases_from_specifier(node));
                return;
            }
        }
    }
    for c in node.children() {
        cpp_find_class_bases_recursive(&c, class_name, out);
        if out.is_some() {
            return;
        }
    }
}

/// 現ソース + インクルードヘッダから class_name の基底クラス名リストを解決する（P3）。
/// cpp_lookup_member_in_sources(BaseClasses, ...) の薄いラッパ。結果を `;` で split して返す。
fn cpp_class_bases_for_sources(ctx: &RecvHintContext<'_>, class_name: &str) -> Vec<String> {
    match cpp_lookup_member_in_sources(ctx, CppLookupKind::BaseClasses, class_name, "") {
        Some(joined) => joined
            .split(';')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect(),
        None => Vec::new(),
    }
}

// ===== B-1: フリー関数プロトタイプの戻り値型 =====

/// declarator ツリーが function_declarator を含むか（変数宣言でなく関数宣言か）の判定。
fn cpp_declarator_contains_function_declarator<D: Doc>(d: &Node<'_, D>) -> bool {
    if d.kind().as_ref() == "function_declarator" {
        return true;
    }
    for c in d.children() {
        if cpp_declarator_contains_function_declarator(&c) {
            return true;
        }
    }
    false
}

/// declarator が name という名前の関数を宣言しているか（関数であり、かつ名前一致）。
fn cpp_declarator_is_function_named<D: Doc>(d: &Node<'_, D>, name: &str) -> bool {
    cpp_declarator_contains_function_declarator(d) && cpp_declarator_matches_name(d, name)
}

/// 戻り値型の specifier と declarator 由来のポインタ/参照修飾（`*`, `&` 等）を合成する。
/// `CWinApp` + `*` → `CWinApp *`。ops が空なら specifier をそのまま返す。
fn cpp_combine_type(spec: &str, ops: &str) -> String {
    let spec = spec.trim();
    let ops = ops.trim();
    if ops.is_empty() {
        spec.to_string()
    } else {
        format!("{spec} {ops}")
    }
}

/// function_definition ノードから name の戻り値型を取り出す。
fn cpp_function_definition_return<D: Doc>(fd: &Node<'_, D>, name: &str) -> Option<String> {
    let decl = fd.field("declarator")?;
    if !cpp_declarator_is_function_named(&decl, name) {
        return None;
    }
    let spec = fd
        .field("type")
        .map(|t| t.text().trim().to_string())
        .or_else(|| cpp_declaration_specifiers_text(fd))?;
    let ops = cpp_declarator_type_ops(decl);
    Some(cpp_combine_type(&spec, &ops))
}

/// declaration ノード（プロトタイプ）から name の関数の戻り値型を取り出す。
/// `CWinApp* AfxGetApp();` → `CWinApp *`。変数宣言（function_declarator を含まない）は除外する。
fn cpp_function_declaration_return<D: Doc>(decl: &Node<'_, D>, name: &str) -> Option<String> {
    let spec = cpp_declaration_specifiers_text(decl)?;
    for d in decl.field_children("declarator") {
        let target = if d.kind().as_ref() == "init_declarator" {
            d.field("declarator")
        } else {
            Some(d.clone())
        };
        if let Some(t) = target {
            if cpp_declarator_is_function_named(&t, name) {
                let ops = cpp_declarator_type_ops(t);
                return Some(cpp_combine_type(&spec, &ops));
            }
        }
    }
    None
}

fn cpp_free_function_return_from_decl<D: Doc>(
    node: &Node<'_, D>,
    name: &str,
) -> Option<String> {
    match node.kind().as_ref() {
        "function_definition" => cpp_function_definition_return(node, name),
        "declaration" => cpp_function_declaration_return(node, name),
        _ => None,
    }
}

/// スコープノードの直下と、namespace 1 段だけ潜ってフリー関数 name を探す。
/// 関数本体やクラス内部には入らない（誤ヒントを避ける）。
fn cpp_find_free_function_return_in_scope<D: Doc>(
    node: &Node<'_, D>,
    name: &str,
    namespace_depth: usize,
) -> Option<String> {
    for c in node.children() {
        let k = c.kind();
        let k = k.as_ref();
        if let Some(ty) = cpp_free_function_return_from_decl(&c, name) {
            return Some(ty);
        }
        // `extern "C" { ... }`（linkage_specification）と declaration_list の中を 1 階層分試す。
        if k == "linkage_specification" || k == "declaration_list" {
            if let Some(ty) = cpp_find_free_function_return_in_scope(&c, name, namespace_depth) {
                return Some(ty);
            }
        }
        // namespace は 1 段だけ（無限展開防止）。
        if k == "namespace_definition" && namespace_depth < 1 {
            if let Some(ty) =
                cpp_find_free_function_return_in_scope(&c, name, namespace_depth + 1)
            {
                return Some(ty);
            }
        }
    }
    None
}

/// translation_unit 直下（+ `extern "C"` ブロック・namespace 1 段）からフリー関数 name の戻り値型を引く。
/// `CWinApp* AfxGetApp();` → `CWinApp *`。
fn cpp_free_function_return_in_translation_unit<D: Doc>(
    root: &Node<'_, D>,
    name: &str,
) -> Option<String> {
    cpp_find_free_function_return_in_scope(root, name, 0)
}

// ===== B-2: extern グローバル変数の型 =====

/// 型文字列から記憶域クラス指定子（extern/static 等）を除去する。const/volatile は型修飾子なので残す。
fn cpp_strip_storage_class_specifiers(spec: &str) -> String {
    const STORAGE: &[&str] = &[
        "extern",
        "static",
        "register",
        "thread_local",
        "mutable",
        "inline",
        "constexpr",
        "consteval",
    ];
    spec.split_whitespace()
        .filter(|w| !STORAGE.contains(w))
        .collect::<Vec<_>>()
        .join(" ")
}

/// declaration ノードからグローバル変数 name の型を取り出す。
/// 関数宣言（function_declarator を含む）は除外し、extern/static 等は型文字列から除去する。
/// ポインタ/参照修飾（`*`, `&`）は declarator から合成する（`CWinApp* theApp;` → `CWinApp *`）。
fn cpp_global_var_type_from_decl<D: Doc>(
    decl: &Node<'_, D>,
    name: &str,
) -> Option<String> {
    if decl.kind().as_ref() != "declaration" {
        return None;
    }
    if !cpp_declaration_declares_name(decl, name) {
        return None;
    }
    // プロトタイプ（関数宣言）は対象外。
    if cpp_declarator_contains_function_declarator(decl) {
        return None;
    }
    let spec = cpp_declaration_specifiers_text(decl)?;
    let spec = cpp_strip_storage_class_specifiers(&spec);
    if spec.is_empty() {
        return None;
    }
    let mut ops = String::new();
    for d in decl.field_children("declarator") {
        let target = if d.kind().as_ref() == "init_declarator" {
            d.field("declarator")
        } else {
            Some(d.clone())
        };
        if let Some(t) = target {
            if cpp_declarator_matches_name(&t, name) {
                ops = cpp_declarator_type_ops(t);
                break;
            }
        }
    }
    Some(cpp_combine_type(&spec, &ops))
}

fn cpp_find_global_var_type_in_scope<D: Doc>(
    node: &Node<'_, D>,
    name: &str,
    namespace_depth: usize,
) -> Option<String> {
    for c in node.children() {
        let k = c.kind();
        let k = k.as_ref();
        if let Some(ty) = cpp_global_var_type_from_decl(&c, name) {
            return Some(ty);
        }
        if k == "linkage_specification" || k == "declaration_list" {
            if let Some(ty) = cpp_find_global_var_type_in_scope(&c, name, namespace_depth) {
                return Some(ty);
            }
        }
        if k == "namespace_definition" && namespace_depth < 1 {
            if let Some(ty) = cpp_find_global_var_type_in_scope(&c, name, namespace_depth + 1) {
                return Some(ty);
            }
        }
    }
    None
}

/// translation_unit 直下（+ `extern "C"` ブロック・namespace 1 段）からグローバル変数 name の型を引く。
/// `extern CWinApp theApp;` → `CWinApp`。`CWinApp* theApp;` → `CWinApp *`。
fn cpp_global_var_type_in_translation_unit<D: Doc>(
    root: &Node<'_, D>,
    name: &str,
) -> Option<String> {
    cpp_find_global_var_type_in_scope(root, name, 0)
}

/// 現ソース + インクルードヘッダからグローバル変数 name の型を解決するドライバ。
fn cpp_global_var_type_for_sources(
    ctx: &RecvHintContext<'_>,
    name: &str,
) -> Option<String> {
    cpp_lookup_member_in_sources(ctx, CppLookupKind::GlobalVar, "", name)
}

/// 現ソース + インクルードヘッダからフリー関数 name の戻り値型を解決するドライバ（D: チェイン起点用）。
fn cpp_free_function_return_for_sources(
    ctx: &RecvHintContext<'_>,
    name: &str,
) -> Option<String> {
    cpp_lookup_member_in_sources(ctx, CppLookupKind::FreeFunction, "", name)
}

/// マクロ解決結果のキャッシュ（設定ルール優先・ユーザー上書き可能）。
/// マクロ定義を型に解決する。3 パターン以外は未対応（None）。
fn cpp_resolve_macro_def(
    ctx: &RecvHintContext<'_>,
    def: &CppMacroDef,
    is_call: bool,
) -> Option<String> {
    match def {
        CppMacroDef::CastReturn(ty) => {
            if is_call {
                Some(ty.clone())
            } else {
                None
            }
        }
        CppMacroDef::DerefFreeFnCall(fn_name) => {
            if is_call {
                return None;
            }
            // (*AfxGetApp()) → AfxGetApp の戻り値型から * を 1 つ剥ぐ。
            let ret = cpp_free_function_return_for_sources(ctx, fn_name)?;
            cpp_strip_one_pointer(&ret)
        }
        CppMacroDef::IdentAlias(target) => {
            if is_call {
                // A(...) → target(...) とみなしてフリー関数戻り値型を引く。
                cpp_free_function_return_for_sources(ctx, target)
            } else {
                // A（値） → target のグローバル変数型を引く。
                cpp_global_var_type_for_sources(ctx, target)
            }
        }
    }
}

/// パス（ソース/ヘッダ）の #define 走査結果をキャッシュ付きで取得する。
fn cpp_defines_for_path(
    ctx: &RecvHintContext<'_>,
    path: &Path,
    text: &str,
) -> Arc<HashMap<String, CppMacroDef>> {
    if let Some(cache) = ctx.job_cache {
        if let Some(cached) = cache.load_defines(path) {
            return cached;
        }
        let map = Arc::new(cpp_scan_defines(text));
        cache.store_defines(path, map.clone());
        return map;
    }
    Arc::new(cpp_scan_defines(text))
}

/// インクルードヘッダを再帰的に走査してマクロ name の型を解決する（C）。
fn cpp_search_macro_in_headers(
    path: &Path,
    name: &str,
    is_call: bool,
    visited: &mut HashSet<PathBuf>,
    depth: usize,
    ctx: &RecvHintContext<'_>,
) -> Option<String> {
    if depth == 0 {
        return None;
    }
    let key = cpp_path_key(path);
    if visited.contains(&key) {
        return None;
    }
    visited.insert(key);
    let text = if let Some(cache) = ctx.job_cache {
        cache.load_header_text(path)?.to_string()
    } else {
        cpp_read_header_text(path)?
    };
    let defines = cpp_defines_for_path(ctx, path, &text);
    if let Some(def) = defines.get(name) {
        if let Some(ty) = cpp_resolve_macro_def(ctx, def, is_call) {
            return Some(ty);
        }
    }
    let base = path.parent()?;
    for inc in cpp_include_paths_from_source(&text) {
        if let Some(p) = cpp_resolve_include_file(base, &inc, ctx.cpp_include_dirs) {
            if let Some(ty) = cpp_search_macro_in_headers(&p, name, is_call, visited, depth - 1, ctx) {
                return Some(ty);
            }
        }
    }
    None
}

/// 現ソース → インクルードヘッダの順でマクロ name の型を解決する（C）。
/// `is_call` が真なら呼び出し形（キャストマクロ等）、偽なら値形（別名マクロ等）。
/// （識別子, is_call）の最終結果は `RecvHintJobCache` に負キャッシュ込みで載るため、
/// 同一未解決識別子が N 回マッチしてもインクルードグラフを再走査しない。
fn cpp_macro_return_for_sources(
    ctx: &RecvHintContext<'_>,
    name: &str,
    is_call: bool,
) -> Option<String> {
    let kind = if is_call {
        CppLookupKind::MacroCall
    } else {
        CppLookupKind::MacroValue
    };
    if let Some(cache) = ctx.job_cache {
        if let Some(cached) = cache.lookup_member(ctx.file_path, kind, "", name, true) {
            return cached;
        }
    }
    let result = cpp_macro_return_for_sources_uncached(ctx, name, is_call);
    if let Some(cache) = ctx.job_cache {
        cache.store_member(ctx.file_path, kind, "", name, true, result.clone());
    }
    result
}

/// `cpp_macro_return_for_sources` のキャッシュ前の実体。
fn cpp_macro_return_for_sources_uncached(
    ctx: &RecvHintContext<'_>,
    name: &str,
    is_call: bool,
) -> Option<String> {
    let source_defines = cpp_defines_for_path(ctx, ctx.file_path, ctx.source);
    if let Some(def) = source_defines.get(name) {
        if let Some(ty) = cpp_resolve_macro_def(ctx, def, is_call) {
            return Some(ty);
        }
    }
    let base = ctx.file_path.parent()?;
    let mut visited = HashSet::new();
    visited.insert(cpp_path_key(ctx.file_path));
    for inc in cpp_include_paths_from_source(ctx.source) {
        if let Some(p) = cpp_resolve_include_file(base, &inc, ctx.cpp_include_dirs) {
            if let Some(ty) = cpp_search_macro_in_headers(&p, name, is_call, &mut visited, CPP_INCLUDE_MAX_DEPTH, ctx) {
                return Some(ty);
            }
        }
    }
    None
}

// ===== B-3: typedef / using の 1 段展開 =====

/// type_definition（typedef）/ alias_declaration（using）ノードから alias のターゲット型を取り出す。
/// `typedef CWinApp App;` → `CWinApp`。`using App = CWinApp;` → `CWinApp`。
fn cpp_type_alias_target_from_node<D: Doc>(
    node: &Node<'_, D>,
    alias: &str,
) -> Option<String> {
    let k = node.kind();
    let k = k.as_ref();
    // using App = CWinApp; は name フィールドがエイリアス名。type 側にポインタ修飾も含まれる
    // ため declarator下降は不要。type_definition とは別経路で処理する。
    if k == "alias_declaration" {
        let name_node = node.field("name")?;
        if name_node.text().trim() != alias {
            return None;
        }
        let target = node.field("type")?;
        return cpp_alias_target_text_from_type(&target);
    }
    if k != "type_definition" {
        return None;
    }
    let target = node.field("type")?;
    let target_text = cpp_alias_target_text_from_type(&target)?;
    // P1-b: type_definition は declarator フィールドを複数持つ（`typedef int A, *PA;`）ため
    // field_children で全宣言子を走査する。各宣言子について P1-a のポインタ個数カウントで
    // 名前照合し、一致したら target 型に剥いだ個数分の `*` を付加して返す
    // （`typedef CWinApp* AppPtr;` → `CWinApp` + `*` → `CWinApp *`）。
    for d in node.field_children("declarator") {
        if let Some(ptr_count) = cpp_typedef_declarator_pointer_count_if_named(&d, alias) {
            let ops = "*".repeat(ptr_count);
            if ops.is_empty() {
                return Some(target_text.clone());
            }
            return Some(cpp_combine_type(&target_text, &ops));
        }
    }
    None
}

/// typedef の target 型ノードから、エイリアスが指す型文字列を取り出す。
/// target が struct/union/enum/class specifier のときはテキスト全体（本体含む）ではなく
/// タグ名（tagPOINT）を返す。タグ名なら cpp_find_field_in_named_class が struct tagPOINT の
/// body を直接探せる。無名 specifier（name フィールド無し）はタグが取れないため None。
/// それ以外（type_identifier 等）はテキストをそのまま返す。
fn cpp_alias_target_text_from_type<D: Doc>(target: &Node<'_, D>) -> Option<String> {
    if matches!(
        target.kind().as_ref(),
        "struct_specifier" | "union_specifier" | "enum_specifier" | "class_specifier"
    ) {
        return target.field("name").map(|n| n.text().trim().to_string());
    }
    let t = target.text().trim().to_string();
    if t.is_empty() {
        None
    } else {
        Some(t)
    }
}

/// typedef 宣言子ツリーを下降して alias 名を探し、途中の `*` の個数を返す（P1-a）。
/// `pointer_type_declarator` / `pointer_declarator` を辿って `*` を数え、内側の
/// `type_identifier`/`identifier`/`field_identifier` が alias と一致したら個数を返す。
/// 関数ポインタ（function_declarator）・配列（array_declarator）・参照（reference_declarator）
/// 等はレシーバになり得ないため対象外 → None（誤解決しない側に倒す）。
fn cpp_typedef_declarator_pointer_count_if_named<D: Doc>(
    d: &Node<'_, D>,
    alias: &str,
) -> Option<usize> {
    let kind = d.kind();
    let k = kind.as_ref();
    if matches!(k, "type_identifier" | "identifier" | "field_identifier") {
        return if d.text().trim() == alias {
            Some(0)
        } else {
            None
        };
    }
    if matches!(k, "pointer_type_declarator" | "pointer_declarator") {
        let inner = d.field("declarator")?;
        let count = cpp_typedef_declarator_pointer_count_if_named(&inner, alias)?;
        return Some(count + 1);
    }
    None
}

fn cpp_find_type_alias_target_in_scope<D: Doc>(
    node: &Node<'_, D>,
    alias: &str,
    namespace_depth: usize,
) -> Option<String> {
    for c in node.children() {
        let k = c.kind();
        let k = k.as_ref();
        if let Some(ty) = cpp_type_alias_target_from_node(&c, alias) {
            return Some(ty);
        }
        if k == "linkage_specification" || k == "declaration_list" {
            if let Some(ty) = cpp_find_type_alias_target_in_scope(&c, alias, namespace_depth) {
                return Some(ty);
            }
        }
        if k == "namespace_definition" && namespace_depth < 1 {
            if let Some(ty) = cpp_find_type_alias_target_in_scope(&c, alias, namespace_depth + 1)
            {
                return Some(ty);
            }
        }
    }
    None
}

/// translation_unit 直下（+ extern "C" ブロック・namespace 1 段）から型エイリアス alias のターゲット型を引く。
fn cpp_type_alias_target_in_translation_unit<D: Doc>(
    root: &Node<'_, D>,
    alias: &str,
) -> Option<String> {
    cpp_find_type_alias_target_in_scope(root, alias, 0)
}

// ===== C: マクロ限定3パターン解析 =====

/// 自動解析対象のマクロ定義。3 パターンのみ。それ以外は既存 macros 設定ルールへ誘導。
#[derive(Debug, Clone)]
enum CppMacroDef {
    /// `#define M(x) ((TYPE)(x))` — キャスト形式。呼び出し `M(...)` の戻り値型は `TYPE`。
    CastReturn(String),
    /// `#define theApp (*AfxGetApp())` — 別名式。`FN()` の戻り値型から `*` を 1 つ剥ぐ。
    DerefFreeFnCall(String),
    /// `#define A B` — 単純識別子別名。`target` の型に 1 段展開する。
    IdentAlias(String),
}

/// `CWinApp*` → `CWinApp *` のように識別子部とポインタ/参照修飾の間に空白を挟む。
fn cpp_normalize_type_string(s: &str) -> String {
    let s = s.trim();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i] != b'*' && bytes[i] != b'&' {
        i += 1;
    }
    let spec = s[..i].trim();
    let ops = s[i..].trim();
    if ops.is_empty() {
        spec.to_string()
    } else {
        format!("{spec} {ops}")
    }
}

/// 末尾のポインタ修飾を 1 つ剥ぐ（デリファレンス）。`CWinApp *` → `CWinApp`。
/// ポインタでなければ `None`（デリファレンス不可は未解決扱い）。
fn cpp_strip_one_pointer(ty: &str) -> Option<String> {
    let s = ty.trim();
    let rest = s.strip_suffix('*')?;
    Some(rest.trim().to_string())
}

fn is_cpp_identifier(s: &str) -> bool {
    let s = s.trim();
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_alphanumeric() || c == '_')
}

/// マクロ本体を3パターンに分類する。それ以外は `None`（対応しない側に倒す）。
fn cpp_classify_macro_body(body: &str, is_function_like: bool) -> Option<CppMacroDef> {
    let b = body.trim();
    if is_function_like {
        // パターン1: キャスト形式 ((TYPE)(...))
        let inner = b.strip_prefix("((")?.strip_suffix("))")?;
        let sep = inner.find(")(")?;
        let ty = inner[..sep].trim();
        if ty.is_empty() || !ty.chars().next().map(|c| c.is_alphabetic() || c == '_').unwrap_or(false) {
            return None;
        }
        return Some(CppMacroDef::CastReturn(cpp_normalize_type_string(ty)));
    }
    // パターン2: (*FN())
    if let Some(rest) = b.strip_prefix("(*").and_then(|r| r.strip_suffix(')')) {
        if let Some(fn_name) = rest.strip_suffix("()") {
            let fn_name = fn_name.trim();
            if is_cpp_identifier(fn_name) {
                return Some(CppMacroDef::DerefFreeFnCall(fn_name.to_string()));
            }
        }
        return None;
    }
    // パターン3: 単純識別子別名（本体が単一識別子のみ）。
    if is_cpp_identifier(b) {
        return Some(CppMacroDef::IdentAlias(b.to_string()));
    }
    None
}

/// ソーステキストから `#define` を走査しマクロ名 → `CppMacroDef` のマップを作る。
/// 行継続 `\` に対応。`#  define`（空白入り）・行頭 BOM も許容（A-2 と同じ方針）。
fn cpp_scan_defines(source: &str) -> HashMap<String, CppMacroDef> {
    let mut out: HashMap<String, CppMacroDef> = HashMap::new();
    // 行継続を結合した論理行リストを作る。
    let mut logical_lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    for line in source.lines() {
        if let Some(stripped) = line.strip_suffix('\\') {
            cur.push_str(stripped);
            cur.push(' ');
        } else {
            cur.push_str(line);
            logical_lines.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        logical_lines.push(cur);
    }
    for ll in &logical_lines {
        let mut t = ll.trim_start();
        if let Some(s) = t.strip_prefix('\u{feff}') {
            t = s.trim_start();
        }
        let after_hash = match t.strip_prefix('#') {
            Some(r) => r.trim_start(),
            None => continue,
        };
        // `define` の直後は語境界（空白）が必要。`#defineX` 等は対象外。
        let rest = match after_hash.strip_prefix("define") {
            Some(r) => r,
            None => continue,
        };
        let rest = match rest.strip_prefix(|c: char| c.is_whitespace()) {
            Some(r) => r.trim_start(),
            None => continue,
        };
        let name_end = rest
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(rest.len());
        if name_end == 0 {
            continue;
        }
        let name = rest[..name_end].to_string();
        let after_name = &rest[name_end..];
        let (body, is_function_like) = if after_name.strip_prefix('(').is_some() {
            let after_paren = &after_name[1..];
            let close = match after_paren.find(')') {
                Some(i) => i,
                None => continue,
            };
            (after_paren[close + 1..].trim(), true)
        } else {
            (after_name.trim(), false)
        };
        if body.is_empty() {
            continue;
        }
        if let Some(def) = cpp_classify_macro_body(body, is_function_like) {
            out.insert(name, def);
        }
    }
    out
}

/// インクルードヘッダを再帰的に走査して kind のメンバ型を解決する。
/// 深さ上限・visited による循環防止・負キャッシュは従来踏襲。
fn cpp_search_header_recursive(
    path: &Path,
    kind: CppLookupKind,
    class_name: &str,
    member_name: &str,
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
        if let Some(cached) = cache.lookup_member(path, kind, class_name, member_name, false) {
            return cached;
        }
    }
    let text = if let Some(cache) = job_cache {
        cache.load_header_text(path)?.to_string()
    } else {
        cpp_read_header_text(path)?
    };
    let grep = cpp_ast_grep_with_profile!(job_cache, &text);
    let root = grep.root();
    let result = if let Some(ty) =
        cpp_lookup_in_translation_unit(&root, kind, class_name, member_name)
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
                    if let Some(ty) = cpp_search_header_recursive(
                        &p,
                        kind,
                        class_name,
                        member_name,
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
        cache.store_member(path, kind, class_name, member_name, false, result.clone());
    }
    result
}

/// キャッシュ（ファイル単位）・エイリアス展開を除いた直接解決経路。
/// ソース TU → インクルードヘッダの順に探す。ヘッダ単位のキャッシュ（`cpp_search_header_recursive` 内）
/// は効くが、ファイル単位のメンバキャッシュは触らない（呼び出し元 `cpp_lookup_member_in_sources` で一括管理）。
fn cpp_lookup_member_direct(
    ctx: &RecvHintContext<'_>,
    kind: CppLookupKind,
    class_name: &str,
    member_name: &str,
) -> Option<String> {
    let grep = cpp_ast_grep_with_profile!(ctx.job_cache, ctx.source);
    let root = grep.root();
    if let Some(ty) = cpp_lookup_in_translation_unit(&root, kind, class_name, member_name) {
        return Some(ty);
    }
    let base = ctx.file_path.parent()?;
    let mut visited = HashSet::new();
    visited.insert(cpp_path_key(ctx.file_path));
    for inc in cpp_include_paths_from_source(ctx.source) {
        if let Some(p) = cpp_resolve_include_file(base, &inc, ctx.cpp_include_dirs) {
            if let Some(ty) = cpp_search_header_recursive(
                &p,
                kind,
                class_name,
                member_name,
                &mut visited,
                CPP_INCLUDE_MAX_DEPTH,
                ctx.cpp_include_dirs,
                ctx.job_cache,
            ) {
                return Some(ty);
            }
        }
    }
    None
}

/// 現ソース + インクルードヘッダから kind のメンバ型を解決する（設定ルール適用後の共通ドライバ）。
/// 設定ルール（type_hint_config）は呼び出し元で先に試すため、ここはキャッシュ→ソース→ヘッダの経路のみ。
/// Field/Method は typedef 1 段展開再試行（B-3）→ 基底クラス遡り（P3）の順で最後の手段を試す。
/// 継承遡りは内部関数 `cpp_lookup_member_with_bases` に分離し、visited でダイヤモンド・循環を防止する。
fn cpp_lookup_member_in_sources(
    ctx: &RecvHintContext<'_>,
    kind: CppLookupKind,
    class_name: &str,
    member_name: &str,
) -> Option<String> {
    let mut visited = HashSet::new();
    cpp_lookup_member_with_bases(ctx, kind, class_name, member_name, &mut visited, 0)
}

/// `cpp_lookup_member_in_sources` の本体。visited は現在の遡りパス中のクラス名
/// （cpp_simplify_type_name 適用後）の集合。挿入失敗（既訪問）なら基底遡りを止める。
/// depth は遡り深さ（派生クラス=0）。CPP_INHERIT_MAX_DEPTH 超で打ち切り。
fn cpp_lookup_member_with_bases(
    ctx: &RecvHintContext<'_>,
    kind: CppLookupKind,
    class_name: &str,
    member_name: &str,
    visited: &mut HashSet<String>,
    depth: usize,
) -> Option<String> {
    if depth > CPP_INHERIT_MAX_DEPTH {
        return None;
    }
    // メンバキャッシュ（負キャッシュ2層）。外側 None=キャッシュミス、内側 None=探索済み未発見。
    if let Some(cache) = ctx.job_cache {
        if let Some(cached) =
            cache.lookup_member(ctx.file_path, kind, class_name, member_name, true)
        {
            return cached;
        }
    }

    let mut result = cpp_lookup_member_direct(ctx, kind, class_name, member_name);

    // B-3: Field/Method の解決失敗時のみ、クラス名を typedef/using で 1 段展開して再試行。
    // エイリアス解決自体は cpp_lookup_member_in_sources(TypeAlias, ...) 経由だが、
    // TypeAlias は Field/Method でないため再帰的にエイリアス展開は起きず 1 段限定（無限展開防止）。
    // 再試行は cpp_lookup_member_direct を使うことで real_class 上の更なる展開を抑制する。
    if result.is_none()
        && matches!(kind, CppLookupKind::Field | CppLookupKind::Method)
        && !class_name.is_empty()
    {
        if let Some(real) =
            cpp_lookup_member_in_sources(ctx, CppLookupKind::TypeAlias, "", class_name)
        {
            let real_class = cpp_simplify_type_name(&real);
            if !real_class.is_empty() && real_class != class_name {
                result = cpp_lookup_member_direct(ctx, kind, &real_class, member_name);
            }
        }
    }

    // P3: Field/Method の解決失敗時（typedef 展開の後）に基底クラスへ遡る。
    // visited に cpp_simplify_type_name 適用後のクラス名を入れ、挿入成功（未訪問）のときだけ
    // 基底を展開する。ダイヤモンド・循環継承を両方防ぐ。多重継承は base_class_clause 記載順、
    // 最初に解決した型を採用。派生側同名メンバは direct で見つかっていれば既に打ち切り済み。
    if result.is_none()
        && matches!(kind, CppLookupKind::Field | CppLookupKind::Method)
        && !class_name.is_empty()
    {
        let simplified = cpp_simplify_type_name(class_name);
        if !simplified.is_empty() && visited.insert(simplified) {
            let bases = cpp_class_bases_for_sources(ctx, class_name);
            for base in bases {
                if let Some(ty) = cpp_lookup_member_with_bases(
                    ctx,
                    kind,
                    &base,
                    member_name,
                    visited,
                    depth + 1,
                ) {
                    result = Some(ty);
                    break;
                }
            }
        }
    }

    if let Some(cache) = ctx.job_cache {
        cache.store_member(
            ctx.file_path,
            kind,
            class_name,
            member_name,
            true,
            result.clone(),
        );
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
    cpp_lookup_member_in_sources(ctx, CppLookupKind::Method, class_name, method_name)
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
        // D-1: 呼び出し先が identifier/qualified_identifier（フリー関数）のチェイン起点。
        // 優先順位: 設定ルール（cpp_config_call_return で試済）→ ソース/ヘッダ（B-1）→ マクロ（C）。
        // AfxGetApp()->GetMainWnd()->... の左端フリー関数呼び出しの戻り値型を解決する。
        if matches!(func.kind().as_ref(), "identifier" | "qualified_identifier") {
            let name = func.text().trim().to_string();
            if let Some(ty) = cpp_free_function_return_for_sources(ctx, &name) {
                return Some(ty);
            }
            // C: マクロ限定解析（キャスト形式 #define M(x) ((TYPE)(x)) 等）。呼び出し形。
            if let Some(ty) = cpp_macro_return_for_sources(ctx, &name, true) {
                return Some(ty);
            }
            // 未解決は None で握る（誤推論より安全）。
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
                // D-2: フリー関数呼び出しの戻り値型を B-1 で先に試す（チェイン/レシーバ解決の起点）。
                if let Some(ctx) = ctx {
                    if matches!(func.kind().as_ref(), "identifier" | "qualified_identifier") {
                        let name = func.text().trim().to_string();
                        if let Some(ty) = cpp_free_function_return_for_sources(ctx, &name) {
                            return Some(ty);
                        }
                        // C: マクロ限定解析（キャスト形式 #define M(x) ((TYPE)(x)) 等）。D-1 と同じ優先順位。
                        if let Some(ty) = cpp_macro_return_for_sources(ctx, &name, true) {
                            return Some(ty);
                        }
                    }
                }
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
    // クラス本体内のメソッド名は field_identifier で現れる（データメンバと同じ）ため
    // identifier と並べて名前ノードとして扱う。これにより cpp_function_definition_return /
    // cpp_function_declaration_return がクラス本体メソッドでも名前一致する。
    if matches!(k, "identifier" | "field_identifier") {
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
    // P1-c: 無名 typedef struct（`typedef struct { long x; } POINT;`）の body を直接検索。
    // タグ付き typedef struct は既存のタグ名経路（cpp_type_alias_target_from_node → typedef 1段展開）
    // に任せ、無名（type が name フィールド無しの struct/union specifier で body 持ち）の場合のみ
    // このアームが働く。declarator の名前が class_name（エイリアス名）と一致したら body を走査。
    // ポインタ typedef（`typedef struct {...} *P;`）も is_some で受理し、`p->x` を解決する。
    if kind.as_ref() == "type_definition" {
        if let Some(t) = node.field("type") {
            if matches!(t.kind().as_ref(), "struct_specifier" | "union_specifier")
                && t.field("name").is_none()
            {
                if let Some(body) = t.field("body") {
                    for d in node.field_children("declarator") {
                        if cpp_typedef_declarator_pointer_count_if_named(&d, class_name).is_some() {
                            cpp_walk_field_declarations(&body, field_name, out);
                            if out.is_some() {
                                return;
                            }
                        }
                    }
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
        // 行頭の BOM（U+FEFF）を防御的に除去。`str::trim` は BOM を空白と見なさないため
        // 明示的に剥ぐ（read_text_file でも通常は除去済みだが、診断経路など万全を期す）。
        let mut t = line.trim();
        if let Some(stripped) = t.strip_prefix('\u{feff}') {
            t = stripped.trim();
        }
        // `#  include`（# と include の間に空白）を許容するため、
        // `#` → 残りを trim_start → `include` の順に分解する。
        // 単純な `strip_prefix("#include")` だと空白入り `#  include` を取りこぼす。
        let after_hash = match t.strip_prefix('#') {
            Some(r) => r.trim_start(),
            None => continue,
        };
        let rest = match after_hash.strip_prefix("include") {
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

/// ヘッダファイルをエンコーディング自動判定で読み込む。
/// 検索本体（`file_encoding::read_text_file` + Auto）と同じ経路で Shift_JIS / UTF-16 / BOM 付きヘッダを扱い、
/// 従来 `fs::read_to_string`（厳密 UTF-8）で黙って `None` になっていた「.h ロード不調」を解消する。
/// サイズ上限（`CPP_INCLUDE_MAX_FILE_BYTES`）を超える場合も `None` を返す。
fn cpp_read_header_text(path: &Path) -> Option<String> {
    let len = fs::metadata(path).ok()?.len();
    if len > CPP_INCLUDE_MAX_FILE_BYTES as u64 {
        return None;
    }
    read_text_file(path, FileEncodingPreference::Auto)
        .ok()
        .map(|d| d.text)
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
/// 継承遡りの深さ上限（P3）。MFC は CView → CWnd → CCmdTarget → CObject の 4〜5 段が
/// 実用最深。8 で余裕を持たせる。visited による循環防止と併用。
const CPP_INHERIT_MAX_DEPTH: usize = 8;

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
    // field_expression（`pt.x` 等）の捕捉: フィールド型解決に失敗した残ケースのみここに到達する
    // （解決可能なフィールドアクセスは chain_expression_result_type で既に処理済み）。
    // ベース識別子 `pt` の型（例: POINT）を代用表示すると「pt.x の型 = POINT」の誤解を招くため、
    // 未解決は None で返し format_stored_unknown_hint 経由の ? 表示に倒す。
    if k == "field_expression" {
        return None;
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
        // B-2: extern グローバル変数（例: theApp）の型をソース/ヘッダから解決する。
        if let Some(ty) = cpp_global_var_type_for_sources(ctx, &t) {
            return Some(ty);
        }
        // C: マクロ別名（#define theApp (*AfxGetApp()) や #define A B）を 1 段展開する。
        if let Some(ty) = cpp_macro_return_for_sources(ctx, &t, false) {
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
        // D-3: 設定ルール未命中ならフリー関数戻り値型を B-1 でフォールバック（引数型ラベル用）。
        if let Some(func) = node.field("function") {
            if matches!(func.kind().as_ref(), "identifier" | "qualified_identifier") {
                let name = func.text().trim().to_string();
                if let Some(ty) = cpp_free_function_return_for_sources(ctx, &name) {
                    return Some(ty);
                }
                // C: マクロ限定解析（キャスト形式等）。D-1/D-2 と同じ優先順位で揃える。
                if let Some(ty) = cpp_macro_return_for_sources(ctx, &name, true) {
                    return Some(ty);
                }
            }
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

    #[test]
    fn cpp_scan_include_directives_handles_spaces_after_hash_and_bom() {
        // `#  include`（# と include の間に空白）と、行頭 BOM（U+FEFF）があっても取りこぼさないこと。
        // 従来 `strip_prefix("#include")` だったため `#  include` を逃し、また trim は BOM を
        // 空白と見なさないため BOM 行の先頭 include を逃していた。
        let src = "\u{feff}#  include \"a.h\"\n# include <b.h>\n#include \"c.h\"\n#define X 1\nnot an include\n";
        let dirs = cpp_scan_include_directives(src);
        assert_eq!(
            dirs,
            vec!["a.h".to_string(), "b.h".to_string(), "c.h".to_string()]
        );
    }

    #[test]
    fn cpp_header_utf8_bom_resolves_field_type_via_cache() {
        let base = std::env::temp_dir().join("ast_grep_gui_cpp_utf8bom_header_test");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("inc")).expect("mkdir inc");
        // UTF-8 BOM 付きヘッダをキャッシュ経路（load_header_text）で読む。
        // read_text_file(Auto) は BOM を除去して返すため構文解析が壊れないことを検証する。
        let header_content = "struct Inner { int z; };\nstruct Foo { Inner inner; };\n";
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(header_content.as_bytes());
        std::fs::write(base.join("inc/foo.h"), &bytes).expect("write foo.h");

        let src = "#include <foo.h>\nvoid f() {\n  Foo foo{};\n  foo.inner;\n}\n";
        let test_cpp = base.join("test.cpp");
        std::fs::write(&test_cpp, src).expect("write test.cpp");

        let extra = vec![base.join("inc")];
        let cache = RecvHintJobCache::new();
        let ctx = RecvHintContext {
            file_path: test_cpp.as_path(),
            source: src,
            cpp_include_dirs: &extra,
            job_cache: Some(&cache),
            type_hint_config: None,
        };

        let grep = SupportLang::Cpp.ast_grep(src);
        let pat = Pattern::try_new("$CHAIN", SupportLang::Cpp).unwrap();
        let m = grep
            .root()
            .find_all(&pat)
            .find(|m| m.get_node().text().trim() == "foo.inner")
            .expect("match foo.inner");
        let cap = m.get_env().get_match("CHAIN").expect("CHAIN");
        let hint = infer_capture_type(SupportedLanguage::Cpp, "CHAIN", cap, Some(&ctx));
        assert_eq!(hint.as_deref(), Some("Inner"));

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn cpp_header_utf16le_resolves_field_type() {
        let base = std::env::temp_dir().join("ast_grep_gui_cpp_utf16le_header_test");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("inc")).expect("mkdir inc");
        // UTF-16LE BOM 付きヘッダ（fs::read_to_string（厳密 UTF-8）は null バイトで失敗する）。
        // read_text_file(Auto) は BOM から UTF-16LE を判定して読むことを fallback 経路で検証する。
        // ※ encoding_rs の UTF-16LE は WHATWG で decode-only のため encode でバイト列が作れない。
        //    よって str::encode_utf16 + to_le_bytes で確実に UTF-16LE バイトを構築する。
        let header_content = "struct Inner { int z; };\nstruct Foo { Inner inner; };\n";
        let mut bytes = vec![0xFF, 0xFE];
        for u in header_content.encode_utf16() {
            bytes.extend_from_slice(&u.to_le_bytes());
        }
        // UTF-16LE of ASCII は `73 00 74 00 ...` となり UTF-8 としては valid（null 交じり）なので
        // fs::read_to_string は“成功”するが文字化けし tree-sitter は構文を parse できない。
        // よって従来コードは receiver 型へフォールバックし、A-1 修正後のみ Inner に解決される。
        std::fs::write(base.join("inc/foo.h"), &bytes).expect("write foo.h");

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
        let m = grep
            .root()
            .find_all(&pat)
            .find(|m| m.get_node().text().trim() == "foo.inner")
            .expect("match foo.inner");
        let cap = m.get_env().get_match("CHAIN").expect("CHAIN");
        let hint = infer_capture_type(SupportedLanguage::Cpp, "CHAIN", cap, Some(&ctx));
        assert_eq!(hint.as_deref(), Some("Inner"));

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn cpp_header_shift_jis_resolves_field_type() {
        let base = std::env::temp_dir().join("ast_grep_gui_cpp_shiftjis_header_test");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("inc")).expect("mkdir inc");
        // Shift_JIS ヘッダ（日本語コメント入り）。Shift_JIS のひらがなは 0x82 系バイトから始まり
        // UTF-8 では継続バイト単独として不正になるため fs::read_to_string は失敗する。
        // read_text_file(Auto) は chardetng で Shift_JIS を判定して読むことを fallback 経路で検証する。
        let header_content =
            "// これは日本語コメントです。構造体の型名を確認します。\nstruct Inner { int z; };\nstruct Foo { Inner inner; };\n";
        let (cow, _, _) = encoding_rs::SHIFT_JIS.encode(header_content);
        // 確かに UTF-8 としては不正なバイト列（= 従来 fs::read_to_string が失敗する内容）であることを保証。
        assert!(
            String::from_utf8(cow.to_vec()).is_err(),
            "Shift_JIS encode must yield non-UTF-8 bytes for a meaningful regression test"
        );
        std::fs::write(base.join("inc/foo.h"), &cow[..]).expect("write foo.h");

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
        let m = grep
            .root()
            .find_all(&pat)
            .find(|m| m.get_node().text().trim() == "foo.inner")
            .expect("match foo.inner");
        let cap = m.get_env().get_match("CHAIN").expect("CHAIN");
        let hint = infer_capture_type(SupportedLanguage::Cpp, "CHAIN", cap, Some(&ctx));
        assert_eq!(hint.as_deref(), Some("Inner"));

        let _ = std::fs::remove_dir_all(&base);
    }

    // ===== B-1: フリー関数プロトタイプの戻り値型 =====

    fn cpp_free_fn_return(src: &str, name: &str) -> Option<String> {
        let grep = SupportLang::Cpp.ast_grep(src);
        cpp_free_function_return_in_translation_unit(&grep.root(), name)
    }

    #[test]
    fn cpp_free_function_return_prototype_pointer() {
        // `CWinApp* AfxGetApp();` → "CWinApp *"（ポインタ修飾を declarator から合成）
        let src = "CWinApp* AfxGetApp();\nvoid f() { AfxGetApp(); }\n";
        assert_eq!(
            cpp_free_fn_return(src, "AfxGetApp").as_deref(),
            Some("CWinApp *")
        );
    }

    #[test]
    fn cpp_free_function_return_definition() {
        // 定義本体があっても戻り値型は取れる。本体の中は走査しない。
        let src = "CWinApp* AfxGetApp() { return nullptr; }\n";
        assert_eq!(
            cpp_free_fn_return(src, "AfxGetApp").as_deref(),
            Some("CWinApp *")
        );
    }

    #[test]
    fn cpp_free_function_return_void() {
        let src = "void DoSomething();\n";
        assert_eq!(cpp_free_fn_return(src, "DoSomething").as_deref(), Some("void"));
    }

    #[test]
    fn cpp_free_function_return_in_namespace_one_level() {
        let src = "namespace N {\nCWinApp* AfxGetApp();\n}\n";
        assert_eq!(
            cpp_free_fn_return(src, "AfxGetApp").as_deref(),
            Some("CWinApp *")
        );
    }

    #[test]
    fn cpp_free_function_return_in_extern_c_block() {
        let src = "extern \"C\" {\nCWinApp* AfxGetApp();\n}\n";
        assert_eq!(
            cpp_free_fn_return(src, "AfxGetApp").as_deref(),
            Some("CWinApp *")
        );
    }

    #[test]
    fn cpp_free_function_return_ignores_variable() {
        // 変数宣言は関数でない（function_declarator を含まない）ため対象外。
        let src = "CWinApp* g_app;\n";
        assert_eq!(cpp_free_fn_return(src, "g_app"), None);
    }

    #[test]
    fn cpp_free_function_return_ignores_same_name_local_call_in_body() {
        // 関数本体の中にある呼び出し式からは拾わない（誤ヒート防止）。
        let src = "void f() {\n  CWinApp* AfxGetApp();\n}\n";
        // 本体内の declaration は走査対象外なのでトップレベルに無ければ None。
        assert_eq!(cpp_free_fn_return(src, "AfxGetApp"), None);
    }

    #[test]
    fn cpp_free_function_return_via_header() {
        let base = std::env::temp_dir().join("ast_grep_gui_cpp_freefn_header_test");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("inc")).expect("mkdir inc");
        std::fs::write(
            base.join("inc/afx.h"),
            "class CWinApp {};\nCWinApp* AfxGetApp();\n",
        )
        .expect("write afx.h");
        let src = "#include <afx.h>\nvoid f() { AfxGetApp(); }\n";
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
        let ty = cpp_lookup_member_in_sources(&ctx, CppLookupKind::FreeFunction, "", "AfxGetApp");
        assert_eq!(ty.as_deref(), Some("CWinApp *"));

        let _ = std::fs::remove_dir_all(&base);
    }

    // ===== B-2: extern グローバル変数の型 =====

    fn cpp_global_var_type(src: &str, name: &str) -> Option<String> {
        let grep = SupportLang::Cpp.ast_grep(src);
        cpp_global_var_type_in_translation_unit(&grep.root(), name)
    }

    #[test]
    fn cpp_global_var_type_extern_stripped() {
        // `extern CWinApp theApp;` → "CWinApp"（extern を除去）
        let src = "extern CWinApp theApp;\nvoid f() { theApp.m_x; }\n";
        assert_eq!(cpp_global_var_type(src, "theApp").as_deref(), Some("CWinApp"));
    }

    #[test]
    fn cpp_global_var_type_pointer() {
        // `CWinApp* theApp;` → "CWinApp *"（ポインタ修飾を declarator から合成）
        let src = "CWinApp* theApp;\n";
        assert_eq!(cpp_global_var_type(src, "theApp").as_deref(), Some("CWinApp *"));
    }

    #[test]
    fn cpp_global_var_type_static_stripped() {
        // `static CWinApp s_app;` → "CWinApp"（static を除去）
        let src = "static CWinApp s_app;\n";
        assert_eq!(cpp_global_var_type(src, "s_app").as_deref(), Some("CWinApp"));
    }

    #[test]
    fn cpp_global_var_type_ignores_function_prototype() {
        // 関数プロトタイプは対象外（function_declarator を含むため除外）。
        let src = "void f();\n";
        assert_eq!(cpp_global_var_type(src, "f"), None);
    }

    #[test]
    fn cpp_global_var_type_ignores_local_declaration_in_body() {
        // 関数本体内の宣言は走査対象外。
        let src = "void f() {\n  CWinApp theApp;\n}\n";
        assert_eq!(cpp_global_var_type(src, "theApp"), None);
    }

    #[test]
    fn cpp_global_var_type_via_header() {
        // ヘッダ経由で extern グローバルの型を解決する。
        let base = std::env::temp_dir().join("ast_grep_gui_cpp_globalvar_header_test");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("inc")).expect("mkdir inc");
        std::fs::write(
            base.join("inc/app.h"),
            "class CWinApp {};\nextern CWinApp theApp;\n",
        )
        .expect("write app.h");
        let src = "#include <app.h>\nvoid f() { theApp.m_x; }\n";
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
        let ty = cpp_global_var_type_for_sources(&ctx, "theApp");
        assert_eq!(ty.as_deref(), Some("CWinApp"));

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn cpp_hint_resolves_extern_global_as_receiver() {
        // theApp は extern グローバル。cpp_hint が theApp → CWinApp を解決するか。
        let src = "class CWinApp { public: int m_x; };\nextern CWinApp theApp;\nvoid f() { theApp.m_x; }\n";
        let grep = SupportLang::Cpp.ast_grep(src);
        let pat = Pattern::try_new("$RECV.m_x", SupportLang::Cpp).unwrap();
        let m = grep.root().find_all(&pat).next().expect("match");
        let recv = m.get_env().get_match("RECV").expect("RECV");
        let ctx = RecvHintContext {
            file_path: std::path::Path::new("test.cpp"),
            source: src,
            cpp_include_dirs: &[],
            job_cache: None,
            type_hint_config: None,
        };
        let hint = infer_recv_type(SupportedLanguage::Cpp, recv, Some(&ctx));
        assert_eq!(hint.as_deref(), Some("CWinApp"));
    }

    // ===== B-3: typedef / using の 1 段展開 =====

    fn cpp_alias_target(src: &str, alias: &str) -> Option<String> {
        let grep = SupportLang::Cpp.ast_grep(src);
        cpp_type_alias_target_in_translation_unit(&grep.root(), alias)
    }

    #[test]
    fn cpp_type_alias_target_typedef() {
        // `typedef CWinApp App;` → "CWinApp"
        let src = "typedef CWinApp App;\n";
        assert_eq!(cpp_alias_target(src, "App").as_deref(), Some("CWinApp"));
    }

    #[test]
    fn cpp_type_alias_target_using() {
        // `using App = CWinApp;` → "CWinApp"
        let src = "using App = CWinApp;\n";
        assert_eq!(cpp_alias_target(src, "App").as_deref(), Some("CWinApp"));
    }

    #[test]
    fn cpp_type_alias_target_in_namespace_one_level() {
        let src = "namespace N {\nusing App = CWinApp;\n}\n";
        assert_eq!(cpp_alias_target(src, "App").as_deref(), Some("CWinApp"));
    }

    #[test]
    fn cpp_type_alias_target_not_found() {
        let src = "typedef CWinApp App;\n";
        assert_eq!(cpp_alias_target(src, "Other"), None);
    }

    #[test]
    fn cpp_simplify_type_name_strips_template_args() {
        assert_eq!(cpp_simplify_type_name("std::vector<int>"), "vector");
        assert_eq!(cpp_simplify_type_name("MyTemplate<int>"), "MyTemplate");
        assert_eq!(cpp_simplify_type_name("std::vector<int>::size_type"), "size_type");
        assert_eq!(cpp_simplify_type_name("CWinApp *"), "CWinApp");
        assert_eq!(cpp_simplify_type_name("const CWinApp *"), "CWinApp");
    }

    #[test]
    fn cpp_field_resolves_via_type_alias_one_level() {
        // App は CWinApp の typedef。App::m_x 失敗 → App を CWinApp に 1 段展開 → CWinApp::m_x → int。
        let src = "class CWinApp { public: int m_x; };\ntypedef CWinApp App;\nvoid f() { App* p; p->m_x; }\n";
        let ctx = RecvHintContext {
            file_path: std::path::Path::new("test.cpp"),
            source: src,
            cpp_include_dirs: &[],
            job_cache: None,
            type_hint_config: None,
        };
        let ty = cpp_field_type_for_class_in_sources(&ctx, "App", "m_x");
        assert_eq!(ty.as_deref(), Some("int"));
    }

    #[test]
    fn cpp_field_resolves_via_type_alias_in_header() {
        // ヘッダ内の typedef を経由してフィールド型を解決する。
        let base = std::env::temp_dir().join("ast_grep_gui_cpp_alias_header_test");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("inc")).expect("mkdir inc");
        std::fs::write(
            base.join("inc/app.h"),
            "class CWinApp { public: int m_x; };\ntypedef CWinApp App;\n",
        )
        .expect("write app.h");
        let src = "#include <app.h>\nvoid f() { App* p; p->m_x; }\n";
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
        let ty = cpp_field_type_for_class_in_sources(&ctx, "App", "m_x");
        assert_eq!(ty.as_deref(), Some("int"));

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn cpp_alias_expansion_is_one_level_only() {
        // App = CWinApp, App2 = App。App2::m_x は 1 段展開で App までしか行かず
        // CWinApp までは届かない（1 段限定の確認）。App::m_x も失敗する前提。
        let src = "class CWinApp { public: int m_x; };\ntypedef CWinApp App;\ntypedef App App2;\n";
        let ctx = RecvHintContext {
            file_path: std::path::Path::new("test.cpp"),
            source: src,
            cpp_include_dirs: &[],
            job_cache: None,
            type_hint_config: None,
        };
        // App2 → App（1 段目）。App は CWinApp の別名だが、再試行は App 上で直接探すため
        // App::m_x は見つからず None。1 段限定で CWinApp までは展開しない。
        let ty = cpp_field_type_for_class_in_sources(&ctx, "App2", "m_x");
        assert_eq!(ty, None);
    }

    // ===== D: メソッドチェイン穴埋め =====

    #[test]
    fn cpp_chain_resolves_free_function_return_in_source() {
        // AfxGetApp()->GetCount() のチェイン起点 AfxGetApp() の戻り値型を
        // ソース内のフリー関数宣言から解決し、GetCount の戻り値型 int まで届くか。
        // GetCount はプロトタイプ宣言（field_declaration）。bug1 修正で本体付き定義でなくても解決する。
        let src = "class CWinApp { public: int GetCount(); };\nCWinApp* AfxGetApp();\nvoid f() { AfxGetApp()->GetCount(); }\n";
        let ctx = RecvHintContext {
            file_path: std::path::Path::new("test.cpp"),
            source: src,
            cpp_include_dirs: &[],
            job_cache: None,
            type_hint_config: None,
        };
        let grep = SupportLang::Cpp.ast_grep(src);
        let pat = Pattern::try_new("$CHAIN", SupportLang::Cpp).unwrap();
        let m = grep
            .root()
            .find_all(&pat)
            .find(|m| m.get_node().text().trim() == "AfxGetApp()->GetCount()")
            .expect("match chain");
        let cap = m.get_env().get_match("CHAIN").expect("CHAIN");
        let hint = infer_capture_type(SupportedLanguage::Cpp, "CHAIN", cap, Some(&ctx));
        assert_eq!(hint.as_deref(), Some("int"));
    }

    #[test]
    fn cpp_chain_resolves_free_function_return_via_header() {
        // ヘッダ経由で AfxGetApp()->GetCount() のチェインを解決する。
        let base = std::env::temp_dir().join("ast_grep_gui_cpp_chain_freefn_header_test");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("inc")).expect("mkdir inc");
        std::fs::write(
            base.join("inc/afx.h"),
            "class CWinApp { public: int GetCount(); };\nCWinApp* AfxGetApp();\n",
        )
        .expect("write afx.h");
        let src = "#include <afx.h>\nvoid f() { AfxGetApp()->GetCount(); }\n";
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
        let m = grep
            .root()
            .find_all(&pat)
            .find(|m| m.get_node().text().trim() == "AfxGetApp()->GetCount()")
            .expect("match chain");
        let cap = m.get_env().get_match("CHAIN").expect("CHAIN");
        let hint = infer_capture_type(SupportedLanguage::Cpp, "CHAIN", cap, Some(&ctx));
        assert_eq!(hint.as_deref(), Some("int"));

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn cpp_type_of_direct_receiver_expr_resolves_free_function() {
        // D-2: call_expression（func が field_expression でない）のレシーバ型解決で
        // B-1 を先に試す。AfxGetApp() 単体の型が CWinApp * になるか。
        let src = "class CWinApp {};\nCWinApp* AfxGetApp();\nvoid f() { AfxGetApp(); }\n";
        let ctx = RecvHintContext {
            file_path: std::path::Path::new("test.cpp"),
            source: src,
            cpp_include_dirs: &[],
            job_cache: None,
            type_hint_config: None,
        };
        let grep = SupportLang::Cpp.ast_grep(src);
        let pat = Pattern::try_new("$RECV", SupportLang::Cpp).unwrap();
        // $RECV は function_declarator にもマッチするため call_expression で絞る。
        let m = grep
            .root()
            .find_all(&pat)
            .find(|m| {
                m.get_node().kind().as_ref() == "call_expression"
                    && m.get_node().text().trim() == "AfxGetApp()"
            })
            .expect("match AfxGetApp() call_expression");
        let cap = m.get_env().get_match("RECV").expect("RECV");
        let hint = infer_recv_type(SupportedLanguage::Cpp, cap, Some(&ctx));
        assert_eq!(hint.as_deref(), Some("CWinApp *"));
    }

    #[test]
    fn cpp_arg_label_resolves_free_function_return() {
        let src = "class CWinApp {};\nCWinApp* AfxGetApp();\nvoid SomeFunc(CWinApp* p);\nvoid f() { SomeFunc(AfxGetApp()); }\n";
        let ctx = RecvHintContext {
            file_path: std::path::Path::new("test.cpp"),
            source: src,
            cpp_include_dirs: &[],
            job_cache: None,
            type_hint_config: None,
        };
        let grep = SupportLang::Cpp.ast_grep(src);
        let pat = Pattern::try_new("SomeFunc($ARG)", SupportLang::Cpp).unwrap();
        let m = grep.root().find_all(&pat).next().expect("match");
        let arg = m.get_env().get_match("ARG").expect("ARG");
        let label = cpp_expr_type_label_for_config(arg, &ctx);
        assert_eq!(label.as_deref(), Some("CWinApp *"));
    }

    // ===== C: マクロ限定3パターン解析 =====

    fn ctx_no_cache<'a>(src: &'a str) -> RecvHintContext<'a> {
        RecvHintContext {
            file_path: std::path::Path::new("test.cpp"),
            source: src,
            cpp_include_dirs: &[],
            job_cache: None,
            type_hint_config: None,
        }
    }

    #[test]
    fn cpp_scan_defines_cast_pattern() {
        let src = "#define M(x) ((CWinApp*)(x))\n";
        let defines = cpp_scan_defines(src);
        match defines.get("M") {
            Some(CppMacroDef::CastReturn(t)) => assert_eq!(t, "CWinApp *"),
            other => panic!("expected CastReturn, got {:?}", other),
        }
    }

    #[test]
    fn cpp_scan_defines_deref_freefn_pattern() {
        let src = "#define theApp (*AfxGetApp())\n";
        let defines = cpp_scan_defines(src);
        match defines.get("theApp") {
            Some(CppMacroDef::DerefFreeFnCall(n)) => assert_eq!(n, "AfxGetApp"),
            other => panic!("expected DerefFreeFnCall, got {:?}", other),
        }
    }

    #[test]
    fn cpp_scan_defines_ident_alias_pattern() {
        let src = "#define MYAPP theApp\n";
        let defines = cpp_scan_defines(src);
        match defines.get("MYAPP") {
            Some(CppMacroDef::IdentAlias(n)) => assert_eq!(n, "theApp"),
            other => panic!("expected IdentAlias, got {:?}", other),
        }
    }

    #[test]
    fn cpp_scan_defines_line_continuation() {
        let src = "#define M(x) \\\n  ((CWinApp*)(x))\n";
        let defines = cpp_scan_defines(src);
        assert!(matches!(defines.get("M"), Some(CppMacroDef::CastReturn(_))));
    }

    #[test]
    fn cpp_scan_defines_ignores_non_matching() {
        let src = "#define FOO 1 + 2\n#define BAR(x) f(x)\n#define EMPTY\n";
        let defines = cpp_scan_defines(src);
        assert!(!defines.contains_key("FOO"));
        assert!(!defines.contains_key("BAR"));
        assert!(!defines.contains_key("EMPTY"));
    }

    #[test]
    fn cpp_macro_cast_return_resolves_in_source() {
        // #define M(x) ((CWinApp*)(x)) → M(arg) の戻り値型は CWinApp *
        let src = "#define M(x) ((CWinApp*)(x))\nclass CWinApp {};\nvoid f() { M(123); }\n";
        let ctx = ctx_no_cache(src);
        let ty = cpp_macro_return_for_sources(&ctx, "M", true);
        assert_eq!(ty.as_deref(), Some("CWinApp *"));
    }

    #[test]
    fn cpp_macro_deref_alias_resolves_in_source() {
        // #define theApp (*AfxGetApp()) → theApp の型は CWinApp * から * を剥いだ CWinApp
        let src = "class CWinApp {};\nCWinApp* AfxGetApp();\n#define theApp (*AfxGetApp())\n";
        let ctx = ctx_no_cache(src);
        let ty = cpp_macro_return_for_sources(&ctx, "theApp", false);
        assert_eq!(ty.as_deref(), Some("CWinApp"));
    }

    #[test]
    fn cpp_macro_ident_alias_resolves_global_var() {
        // #define MYAPP theApp + extern CWinApp theApp; → MYAPP の型は CWinApp
        let src = "class CWinApp {};\nextern CWinApp theApp;\n#define MYAPP theApp\n";
        let ctx = ctx_no_cache(src);
        let ty = cpp_macro_return_for_sources(&ctx, "MYAPP", false);
        assert_eq!(ty.as_deref(), Some("CWinApp"));
    }

    #[test]
    fn cpp_macro_ident_alias_call_resolves_free_function() {
        // #define WRAP AfxGetApp + CWinApp* AfxGetApp(); → WRAP() の戻り値型は CWinApp *
        let src = "class CWinApp {};\nCWinApp* AfxGetApp();\n#define WRAP AfxGetApp\n";
        let ctx = ctx_no_cache(src);
        let ty = cpp_macro_return_for_sources(&ctx, "WRAP", true);
        assert_eq!(ty.as_deref(), Some("CWinApp *"));
    }

    #[test]
    fn cpp_macro_non_matching_returns_none() {
        let src = "#define FOO 1 + 2\n";
        let ctx = ctx_no_cache(src);
        assert_eq!(cpp_macro_return_for_sources(&ctx, "FOO", true), None);
        assert_eq!(cpp_macro_return_for_sources(&ctx, "FOO", false), None);
    }

    #[test]
    fn cpp_macro_cast_return_resolves_via_header() {
        let base = std::env::temp_dir().join("ast_grep_gui_cpp_macro_header_test");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("inc")).expect("mkdir inc");
        std::fs::write(
            base.join("inc/afx.h"),
            "#define M(x) ((CWinApp*)(x))\nclass CWinApp {};\n",
        )
        .expect("write afx.h");
        let src = "#include <afx.h>\nvoid f() { M(123); }\n";
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
        let ty = cpp_macro_return_for_sources(&ctx, "M", true);
        assert_eq!(ty.as_deref(), Some("CWinApp *"));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn cpp_chain_resolves_cast_macro_return() {
        // M(123)->GetCount() where M is a cast macro returning CWinApp*
        let src = "#define M(x) ((CWinApp*)(x))\nclass CWinApp { public: int GetCount() { return 0; } };\nvoid f() { M(123)->GetCount(); }\n";
        let ctx = ctx_no_cache(src);
        let grep = SupportLang::Cpp.ast_grep(src);
        let pat = Pattern::try_new("$CHAIN", SupportLang::Cpp).unwrap();
        let m = grep
            .root()
            .find_all(&pat)
            .find(|m| m.get_node().text().trim() == "M(123)->GetCount()")
            .expect("match chain");
        let cap = m.get_env().get_match("CHAIN").expect("CHAIN");
        let hint = infer_capture_type(SupportedLanguage::Cpp, "CHAIN", cap, Some(&ctx));
        assert_eq!(hint.as_deref(), Some("int"));
    }

    #[test]
    fn cpp_hint_resolves_deref_macro_alias_as_receiver() {
        // theApp.m_x where #define theApp (*AfxGetApp()) → theApp 型は CWinApp
        let src = "class CWinApp { public: int m_x; };\nCWinApp* AfxGetApp();\n#define theApp (*AfxGetApp())\nvoid f() { theApp.m_x; }\n";
        let ctx = ctx_no_cache(src);
        let grep = SupportLang::Cpp.ast_grep(src);
        let pat = Pattern::try_new("$RECV.m_x", SupportLang::Cpp).unwrap();
        let m = grep.root().find_all(&pat).next().expect("match");
        let recv = m.get_env().get_match("RECV").expect("RECV");
        let hint = infer_recv_type(SupportedLanguage::Cpp, recv, Some(&ctx));
        assert_eq!(hint.as_deref(), Some("CWinApp"));
    }

    #[test]
    fn cpp_config_macro_overrides_auto_macro_analysis() {
        // 設定ルール（M → Override）が自動マクロ解析（CWinApp *）より優先されるか。
        let src = "#define M(x) ((CWinApp*)(x))\nvoid f() { M(123); }\n";
        let config = TypeHintConfig::from_file(TypeHintConfigFile::new(CppTypeHintRules {
            macros: vec![CppCallableRule {
                name: "M".into(),
                arity: Some(1),
                params: vec![],
                returns: "Override".into(),
                enabled: true,
            }],
            ..Default::default()
        }));
        let hint = cpp_infer_pattern(src, "M($X)", &config);
        assert_eq!(hint.as_deref(), Some("Override"));
    }

    // ===== E: C 言語対応（c_hint 統合） =====

    fn c_ctx<'a>(src: &'a str) -> RecvHintContext<'a> {
        RecvHintContext {
            file_path: std::path::Path::new("test.c"),
            source: src,
            cpp_include_dirs: &[],
            job_cache: None,
            type_hint_config: None,
        }
    }

    fn c_infer_capture_by_kind(src: &str, kind: &str, text: &str, capture: &str) -> Option<String> {
        let grep = SupportLang::C.ast_grep(src);
        let pat = Pattern::try_new("$CHAIN", SupportLang::C).unwrap();
        let m = grep
            .root()
            .find_all(&pat)
            .find(|m| m.get_node().kind().as_ref() == kind && m.get_node().text().trim() == text)?;
        let cap = m.get_env().get_match(capture)?;
        let ctx = c_ctx(src);
        infer_capture_type(SupportedLanguage::C, capture, cap, Some(&ctx))
    }

    #[test]
    fn c_free_function_return_resolves() {
        // 純 C の関数プロトタイプ int get_count(); → get_count() の戻り値型は int
        let src = "int get_count();\nint f() { return get_count(); }\n";
        let hint = c_infer_capture_by_kind(src, "call_expression", "get_count()", "CHAIN");
        assert_eq!(hint.as_deref(), Some("int"));
    }

    #[test]
    fn c_struct_field_arrow_access_resolves() {
        // 純 C の struct + -> アクセス。p->x のフィールド型 int を解決するか。
        let src = "struct Foo { int x; };\nvoid f(struct Foo* p) { p->x; }\n";
        let hint = c_infer_capture_by_kind(src, "field_expression", "p->x", "CHAIN");
        assert_eq!(hint.as_deref(), Some("int"));
    }

    #[test]
    fn c_global_var_type_resolves_as_receiver() {
        // extern struct Foo g_foo; → g_foo の型は struct Foo
        let src = "struct Foo { int x; };\nextern struct Foo g_foo;\nvoid f() { g_foo.x; }\n";
        let grep = SupportLang::C.ast_grep(src);
        let pat = Pattern::try_new("$RECV.x", SupportLang::C).unwrap();
        let m = grep.root().find_all(&pat).next().expect("match");
        let recv = m.get_env().get_match("RECV").expect("RECV");
        let ctx = c_ctx(src);
        let hint = infer_recv_type(SupportedLanguage::C, recv, Some(&ctx));
        assert_eq!(hint.as_deref(), Some("struct Foo"));
    }

    // ===== fix.md レビュー指摘の回帰テスト（bug1〜bug7） =====

    fn infer_chain(src: &str, text: &str) -> Option<String> {
        let ctx = ctx_no_cache(src);
        let grep = SupportLang::Cpp.ast_grep(src);
        let pat = Pattern::try_new("$CHAIN", SupportLang::Cpp).unwrap();
        let m = grep
            .root()
            .find_all(&pat)
            .find(|m| m.get_node().text().trim() == text)
            .expect("match chain");
        let cap = m.get_env().get_match("CHAIN").expect("CHAIN");
        infer_capture_type(SupportedLanguage::Cpp, "CHAIN", cap, Some(&ctx))
    }

    #[test]
    fn cpp_method_prototype_return_in_source() {
        // クラス本体内のプロトタイプ宣言（field_declaration）からメソッド戻り値型を解決（bug1）。
        let src = "class Foo { public: int Bar(); };\nvoid f() { Foo o; o.Bar(); }\n";
        assert_eq!(infer_chain(src, "o.Bar()").as_deref(), Some("int"));
    }

    #[test]
    fn cpp_method_prototype_pointer_return_in_source() {
        // ポインタ戻り値プロトタイプ CWnd* GetMainWnd(); → CWnd *（ポインタ修飾の回復、bug1）。
        let src = "class CWnd {};\nclass CWinApp { public: CWnd* GetMainWnd(); };\nvoid f() { CWinApp a; a.GetMainWnd(); }\n";
        assert_eq!(infer_chain(src, "a.GetMainWnd()").as_deref(), Some("CWnd *"));
    }

    #[test]
    fn cpp_method_prototype_return_via_header() {
        // ヘッダ経由でプロトタイプ宣言のみのメソッド戻り値型を解決（bug1）。
        let base = std::env::temp_dir().join("ast_grep_gui_cpp_method_proto_header_test");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("inc")).expect("mkdir inc");
        std::fs::write(base.join("inc/foo.h"), "class Foo { public: int Bar(); };\n").expect("write foo.h");
        let src = "#include <foo.h>\nvoid f() { Foo o; o.Bar(); }\n";
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
        let m = grep
            .root()
            .find_all(&pat)
            .find(|m| m.get_node().text().trim() == "o.Bar()")
            .expect("match chain");
        let cap = m.get_env().get_match("CHAIN").expect("CHAIN");
        let hint = infer_capture_type(SupportedLanguage::Cpp, "CHAIN", cap, Some(&ctx));
        assert_eq!(hint.as_deref(), Some("int"));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn cpp_typedef_struct_tag_field_in_source() {
        // typedef struct tagPOINT { ... } POINT; の pt.x → long（bug2）。
        let src = "typedef struct tagPOINT { long x; long y; } POINT;\nvoid f() { POINT pt; pt.x; }\n";
        assert_eq!(infer_chain(src, "pt.x").as_deref(), Some("long"));
    }

    #[test]
    fn cpp_typedef_struct_tag_field_via_header() {
        // ヘッダ経由で typedef struct タグ付きのフィールド型を解決（bug2）。
        let base = std::env::temp_dir().join("ast_grep_gui_cpp_typedef_struct_header_test");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("inc")).expect("mkdir inc");
        std::fs::write(base.join("inc/app.h"), "typedef struct tagPOINT { long x; long y; } POINT;\n").expect("write app.h");
        let src = "#include <app.h>\nvoid f() { POINT pt; pt.x; }\n";
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
        let m = grep
            .root()
            .find_all(&pat)
            .find(|m| m.get_node().text().trim() == "pt.x")
            .expect("match field");
        let cap = m.get_env().get_match("CHAIN").expect("CHAIN");
        let hint = infer_capture_type(SupportedLanguage::Cpp, "CHAIN", cap, Some(&ctx));
        assert_eq!(hint.as_deref(), Some("long"));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn cpp_macro_resolution_is_cached_per_identifier() {
        // 未解決マクロ識別子の解決結果（負キャッシュ）が RecvHintJobCache に載り、
        // 2 回目はインクルードグラフを再走査しない（bug3）。
        let base = std::env::temp_dir().join("ast_grep_gui_cpp_macro_cache_test");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("inc")).expect("mkdir inc");
        std::fs::write(base.join("inc/a.h"), "// no NOPE define here\n").expect("write a.h");
        let src = "#include <a.h>\nvoid f() { NOPE; }\n";
        let test_cpp = base.join("test.cpp");
        std::fs::write(&test_cpp, src).expect("write test.cpp");
        let extra = vec![base.join("inc")];

        let cache = RecvHintJobCache::new();
        let ctx = RecvHintContext {
            file_path: test_cpp.as_path(),
            source: src,
            cpp_include_dirs: &extra,
            job_cache: Some(&cache),
            type_hint_config: None,
        };
        assert_eq!(cpp_macro_return_for_sources(&ctx, "NOPE", true), None);
        let reads_after_first = cache.profile().header_reads.load(Ordering::Relaxed);
        let hits_after_first = cache.profile().lookup_cache_hits.load(Ordering::Relaxed);
        assert!(reads_after_first >= 1, "first call should read the header");

        assert_eq!(cpp_macro_return_for_sources(&ctx, "NOPE", true), None);
        let reads_after_second = cache.profile().header_reads.load(Ordering::Relaxed);
        let hits_after_second = cache.profile().lookup_cache_hits.load(Ordering::Relaxed);
        assert_eq!(
            reads_after_second, reads_after_first,
            "second call must not re-read headers (negative cache)"
        );
        assert!(
            hits_after_second > hits_after_first,
            "second call must hit the member cache"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn cpp_include_diagnostic_cache_key_includes_search_dir() {
        // search_dir が異なれば診断キャッシュキーも異なる（bug4）。
        use crate::search::cpp_include_diagnostic_cache_key;
        let k1 = cpp_include_diagnostic_cache_key(1, "inc", "pat", true, 10, 20, "C:/proj");
        let k2 = cpp_include_diagnostic_cache_key(1, "inc", "pat", true, 10, 20, "C:/other");
        assert_ne!(k1, k2, "different search_dir must yield different cache keys");
        let k3 = cpp_include_diagnostic_cache_key(1, "inc", "pat", true, 10, 20, "C:/proj");
        assert_eq!(k1, k3, "same search_dir must yield same cache key");
    }

    #[test]
    fn cpp_recv_label_resolves_cast_macro_start() {
        // GETAPP()->Foo() のレシーバラベル。GETAPP はキャストマクロで CWinApp* を返す（bug5: D-2）。
        let src = "class CWinApp { public: void Foo(); };\n#define GETAPP() ((CWinApp*)(0))\nvoid f() { GETAPP()->Foo(); }\n";
        let ctx = ctx_no_cache(src);
        let grep = SupportLang::Cpp.ast_grep(src);
        let pat = Pattern::try_new("$RECV", SupportLang::Cpp).unwrap();
        let m = grep
            .root()
            .find_all(&pat)
            .find(|m| {
                m.get_node().kind().as_ref() == "call_expression"
                    && m.get_node().text().trim() == "GETAPP()->Foo()"
            })
            .expect("match GETAPP()->Foo()");
        let cap = m.get_env().get_match("RECV").expect("RECV");
        let hint = infer_recv_type(SupportedLanguage::Cpp, cap, Some(&ctx));
        assert_eq!(hint.as_deref(), Some("CWinApp.Foo"));
    }

    #[test]
    fn cpp_arg_label_resolves_cast_macro_start() {
        // SomeFunc(GETAPP()) の引数型ラベル。GETAPP はキャストマクロで CWinApp* を返す（bug5: D-3）。
        let src = "class CWinApp {};\n#define GETAPP() ((CWinApp*)(0))\nvoid SomeFunc(CWinApp* p);\nvoid f() { SomeFunc(GETAPP()); }\n";
        let ctx = ctx_no_cache(src);
        let grep = SupportLang::Cpp.ast_grep(src);
        let pat = Pattern::try_new("SomeFunc($ARG)", SupportLang::Cpp).unwrap();
        let m = grep.root().find_all(&pat).next().expect("match");
        let arg = m.get_env().get_match("ARG").expect("ARG");
        let label = cpp_expr_type_label_for_config(arg, &ctx);
        assert_eq!(label.as_deref(), Some("CWinApp *"));
    }

    #[test]
    fn load_header_text_caches_missing_header() {
        // 存在しないヘッダパスの読み込み失敗も負キャッシュされ、2 回目はキャッシュヒット（bug6）。
        let cache = RecvHintJobCache::new();
        let p = std::path::Path::new("definitely_nonexistent_header_xyz.h");
        assert_eq!(cache.load_header_text(p), None);
        let hits_after_first = cache.profile().header_cache_hits.load(Ordering::Relaxed);
        assert_eq!(cache.load_header_text(p), None);
        let hits_after_second = cache.profile().header_cache_hits.load(Ordering::Relaxed);
        assert!(
            hits_after_second > hits_after_first,
            "second call must hit the header_text cache"
        );
    }

    #[test]
    fn cpp_field_expression_unresolved_returns_none() {
        // フィールド型が未解決の field_expression 捕捉は ? 表示へ倒す（bug7）。
        // ベース変数 pt の型 POINT は取れるが unknownField は POINT に無いため未解決。
        // 従来はベース型 POINT を代用表示したが、bug7 で None になる。
        let src = "typedef struct tagPOINT { long x; } POINT;\nvoid f() { POINT pt; pt.unknownField; }\n";
        assert_eq!(infer_chain(src, "pt.unknownField"), None);
    }

    // ===== P1: typedef 拡張（ポインタ typedef / 複数宣言子 / 無名 typedef struct） =====

    #[test]
    fn cpp_pointer_typedef_resolves_member_in_source() {
        // typedef CWinApp* AppPtr; の AppPtr a; a->GetMainWnd() → CWnd *（P1-a）。
        // AppPtr → CWinApp *（typedef 1段展開）→ CWinApp::GetMainWnd → CWnd *。
        let src = "class CWnd {};\nclass CWinApp { public: CWnd* GetMainWnd(); };\ntypedef CWinApp* AppPtr;\nvoid f() { AppPtr a; a->GetMainWnd(); }\n";
        assert_eq!(infer_chain(src, "a->GetMainWnd()").as_deref(), Some("CWnd *"));
    }

    #[test]
    fn c_pointer_typedef_resolves_field_in_source() {
        // 純 C: typedef struct Foo* PFOO; の PFOO p; p->x → int（P1-a）。
        // PFOO → Foo * → struct Foo::x → int。C は C++ 推論経路に統合済み。
        let src = "struct Foo { int x; };\ntypedef struct Foo* PFOO;\nvoid f() { PFOO p; p->x; }\n";
        let hint = c_infer_capture_by_kind(src, "field_expression", "p->x", "CHAIN");
        assert_eq!(hint.as_deref(), Some("int"));
    }

    #[test]
    fn cpp_pointer_typedef_resolves_member_via_header() {
        // ヘッダ経由でポインタ typedef のメンバ型を解決（P1-a）。
        let base = std::env::temp_dir().join("ast_grep_gui_cpp_ptr_typedef_header_test");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("inc")).expect("mkdir inc");
        std::fs::write(
            base.join("inc/app.h"),
            "class CWnd {};\nclass CWinApp { public: CWnd* GetMainWnd(); };\ntypedef CWinApp* AppPtr;\n",
        )
        .expect("write app.h");
        let src = "#include <app.h>\nvoid f() { AppPtr a; a->GetMainWnd(); }\n";
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
        let m = grep
            .root()
            .find_all(&pat)
            .find(|m| m.get_node().text().trim() == "a->GetMainWnd()")
            .expect("match chain");
        let cap = m.get_env().get_match("CHAIN").expect("CHAIN");
        let hint = infer_capture_type(SupportedLanguage::Cpp, "CHAIN", cap, Some(&ctx));
        assert_eq!(hint.as_deref(), Some("CWnd *"));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn cpp_anon_typedef_struct_field_in_source() {
        // typedef struct { long x; } POINT; の pt.x → long（P1-c、無名 struct の直接 body 検索）。
        let src = "typedef struct { long x; long y; } POINT;\nvoid f() { POINT pt; pt.x; }\n";
        assert_eq!(infer_chain(src, "pt.x").as_deref(), Some("long"));
    }

    #[test]
    fn cpp_anon_typedef_struct_method_in_source() {
        // typedef struct { int Foo(); } S; の s.Foo() → int（P1-c、メソッド版）。
        let src = "typedef struct { int Foo(); } S;\nvoid f() { S s; s.Foo(); }\n";
        assert_eq!(infer_chain(src, "s.Foo()").as_deref(), Some("int"));
    }

    #[test]
    fn cpp_anon_typedef_struct_field_via_header() {
        // ヘッダ経由で無名 typedef struct のフィールド型を解決（P1-c）。
        let base = std::env::temp_dir().join("ast_grep_gui_cpp_anon_typedef_header_test");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("inc")).expect("mkdir inc");
        std::fs::write(
            base.join("inc/app.h"),
            "typedef struct { long x; long y; } POINT;\n",
        )
        .expect("write app.h");
        let src = "#include <app.h>\nvoid f() { POINT pt; pt.x; }\n";
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
        let m = grep
            .root()
            .find_all(&pat)
            .find(|m| m.get_node().text().trim() == "pt.x")
            .expect("match field");
        let cap = m.get_env().get_match("CHAIN").expect("CHAIN");
        let hint = infer_capture_type(SupportedLanguage::Cpp, "CHAIN", cap, Some(&ctx));
        assert_eq!(hint.as_deref(), Some("long"));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn cpp_typedef_multiple_declarators_resolve() {
        // typedef int A, *PA; の A → int、PA → int *（P1-b、複数宣言子）。
        let src = "typedef int A, *PA;\n";
        assert_eq!(cpp_alias_target(src, "A").as_deref(), Some("int"));
        assert_eq!(cpp_alias_target(src, "PA").as_deref(), Some("int *"));
    }

    #[test]
    fn cpp_typedef_pointer_to_named_struct_field() {
        // typedef struct Foo* PFOO; の PFOO p; p->x → int（P1-a + 既存タグ経路）。
        let src = "struct Foo { int x; };\ntypedef struct Foo* PFOO;\nvoid f() { PFOO p; p->x; }\n";
        assert_eq!(infer_chain(src, "p->x").as_deref(), Some("int"));
    }

    #[test]
    fn cpp_typedef_function_pointer_is_none() {
        // 関数ポインタ typedef typedef int (*FN)(void); は None（レシーバになり得ない、P1 negative）。
        let src = "typedef int (*FN)(void);\n";
        assert_eq!(cpp_alias_target(src, "FN"), None);
    }

    // ===== P2: クラス外定義（out-of-class definition）の戻り値型 =====

    #[test]
    fn cpp_out_of_class_definition_only_resolves() {
        // ヘッダにプロトタイプが無く .cpp 内のクラス外定義のみから解決（P2）。
        // CWnd* CMyApp::GetMainWnd() { ... } → a.GetMainWnd() は CWnd *。
        let src = "class CWnd {};\nclass CMyApp {};\nCWnd* CMyApp::GetMainWnd() { return 0; }\nvoid f() { CMyApp a; a.GetMainWnd(); }\n";
        assert_eq!(infer_chain(src, "a.GetMainWnd()").as_deref(), Some("CWnd *"));
    }

    #[test]
    fn cpp_in_class_prototype_and_out_of_class_definition_both_resolve() {
        // ヘッダのプロトタイプ宣言と .cpp のクラス外定義が両方あるケース。
        // in-class プロトタイプ経路が先に解決し、out-of-class と同値になる（P2）。
        let src = "class CWnd {};\nclass CMyApp { public: CWnd* GetMainWnd(); };\nCWnd* CMyApp::GetMainWnd() { return 0; }\nvoid f() { CMyApp a; a.GetMainWnd(); }\n";
        assert_eq!(infer_chain(src, "a.GetMainWnd()").as_deref(), Some("CWnd *"));
    }

    #[test]
    fn cpp_out_of_class_definition_wrong_class_no_false_match() {
        // negative: COther::GetMainWnd があっても CMyApp のメソッドとして誤マッチしない（P2）。
        // 戻り値型解決は失敗し、未解決表示の CMyApp.GetMainWnd ラベルに倒れる（CWnd * にならない）。
        let src = "class CWnd {};\nclass CMyApp {};\nclass COther {};\nCWnd* COther::GetMainWnd() { return 0; }\nvoid f() { CMyApp a; a.GetMainWnd(); }\n";
        assert_eq!(infer_chain(src, "a.GetMainWnd()").as_deref(), Some("CMyApp.GetMainWnd"));
    }

    #[test]
    fn cpp_out_of_class_definition_in_namespace_one_level() {
        // namespace 1 段内の ns::CMyApp::Foo 形。out-of-class 定義の qualified_identifier が
        // ns::CMyApp::GetMainWnd で class セグメント CMyApp に解決（P2）。
        let src = "class CWnd {};\nnamespace ns { class CMyApp {}; }\nCWnd* ns::CMyApp::GetMainWnd() { return 0; }\nvoid f() { ns::CMyApp a; a.GetMainWnd(); }\n";
        assert_eq!(infer_chain(src, "a.GetMainWnd()").as_deref(), Some("CWnd *"));
    }

    #[test]
    fn cpp_out_of_class_definition_class_in_header_resolves() {
        // クラス宣言がヘッダ、クラス外定義が .cpp にある MFC 典型ケース（P2）。
        let base = std::env::temp_dir().join("ast_grep_gui_cpp_out_of_class_header_test");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("inc")).expect("mkdir inc");
        std::fs::write(
            base.join("inc/app.h"),
            "class CWnd {};\nclass CMyApp {};\n",
        )
        .expect("write app.h");
        let src = "#include <app.h>\nCWnd* CMyApp::GetMainWnd() { return 0; }\nvoid f() { CMyApp a; a.GetMainWnd(); }\n";
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
        let m = grep
            .root()
            .find_all(&pat)
            .find(|m| m.get_node().text().trim() == "a.GetMainWnd()")
            .expect("match chain");
        let cap = m.get_env().get_match("CHAIN").expect("CHAIN");
        let hint = infer_capture_type(SupportedLanguage::Cpp, "CHAIN", cap, Some(&ctx));
        assert_eq!(hint.as_deref(), Some("CWnd *"));
        let _ = std::fs::remove_dir_all(&base);
    }

    // ===== P3: 継承（基底クラス遡り、AST 自動解析） =====

    #[test]
    fn cpp_inheritance_one_level_field() {
        // 1 段継承: Derived の m_x は Base から → int（P3）。
        let src = "class Base { public: int m_x; };\nclass Derived : public Base { };\nvoid f() { Derived d; d.m_x; }\n";
        assert_eq!(infer_chain(src, "d.m_x").as_deref(), Some("int"));
    }

    #[test]
    fn cpp_inheritance_one_level_method() {
        // 1 段継承のメソッド戻り値: Derived::GetMainWnd は Base から → CWnd *（P3）。
        let src = "class CWnd {};\nclass Base { public: CWnd* GetMainWnd(); };\nclass Derived : public Base { };\nvoid f() { Derived d; d.GetMainWnd(); }\n";
        assert_eq!(infer_chain(src, "d.GetMainWnd()").as_deref(), Some("CWnd *"));
    }

    #[test]
    fn cpp_inheritance_three_levels() {
        // 多段継承（3 段遡り）: C → B → A の m_x → int（P3）。
        let src = "class A { public: int m_x; };\nclass B : public A { };\nclass C : public B { };\nvoid f() { C c; c.m_x; }\n";
        assert_eq!(infer_chain(src, "c.m_x").as_deref(), Some("int"));
    }

    #[test]
    fn cpp_inheritance_multiple_inheritance_second_base() {
        // 多重継承: 第1基底 Base1 に無く第2基底 Base2 に有る m_y → int（P3）。
        let src = "class Base1 { };\nclass Base2 { public: int m_y; };\nclass Derived : public Base1, public Base2 { };\nvoid f() { Derived d; d.m_y; }\n";
        assert_eq!(infer_chain(src, "d.m_y").as_deref(), Some("int"));
    }

    #[test]
    fn cpp_inheritance_diamond_resolves_without_hang() {
        // ダイヤモンド継承: D → B,C; B,C → A。visited で A への2回目遡りを防止しつつ int を解決（P3）。
        let src = "class A { public: int m_x; };\nclass B : public A { };\nclass C : public A { };\nclass D : public B, public C { };\nvoid f() { D d; d.m_x; }\n";
        assert_eq!(infer_chain(src, "d.m_x").as_deref(), Some("int"));
    }

    #[test]
    fn cpp_inheritance_cycle_no_hang() {
        // 循環継承（不正コード）: A : B, B : A。visited でハングせず None（P3）。
        let src = "class A : public B { };\nclass B : public A { };\nvoid f() { A a; a.m_x; }\n";
        assert_eq!(infer_chain(src, "a.m_x"), None);
    }

    #[test]
    fn cpp_inheritance_base_in_header_resolves() {
        // 基底クラスが別ヘッダ（MFC 典型ケース）: CBase が app.h、CDerived が .cpp（P3）。
        let base = std::env::temp_dir().join("ast_grep_gui_cpp_inherit_header_test");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("inc")).expect("mkdir inc");
        std::fs::write(
            base.join("inc/app.h"),
            "class CBase { public: int m_x; };\n",
        )
        .expect("write app.h");
        let src = "#include <app.h>\nclass CDerived : public CBase { };\nvoid f() { CDerived d; d.m_x; }\n";
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
        let m = grep
            .root()
            .find_all(&pat)
            .find(|m| m.get_node().text().trim() == "d.m_x")
            .expect("match field");
        let cap = m.get_env().get_match("CHAIN").expect("CHAIN");
        let hint = infer_capture_type(SupportedLanguage::Cpp, "CHAIN", cap, Some(&ctx));
        assert_eq!(hint.as_deref(), Some("int"));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn cpp_inheritance_template_base_name() {
        // テンプレート基底 public CArray<int, int> → 基底名 CArray として扱う（P3）。
        let src = "class CArray { public: int GetSize(); };\nclass CMyArray : public CArray<int, int> { };\nvoid f() { CMyArray a; a.GetSize(); }\n";
        assert_eq!(infer_chain(src, "a.GetSize()").as_deref(), Some("int"));
    }

    #[test]
    fn cpp_inheritance_derived_overrides_base() {
        // 派生側に同名メンバがある場合は派生側優先（オーバーライド）。Base::m_x=int、Derived::m_x=long → long（P3）。
        let src = "class Base { public: int m_x; };\nclass Derived : public Base { public: long m_x; };\nvoid f() { Derived d; d.m_x; }\n";
        assert_eq!(infer_chain(src, "d.m_x").as_deref(), Some("long"));
    }

    #[test]
    fn cpp_inheritance_resolution_cached_with_job_cache() {
        // RecvHintJobCache 経由で継承解決結果と BaseClasses 負キャッシュが効くことを検証（P3）。
        // 2 回目の同一 (class, member) 解決はメンバキャッシュヒットになり再走査しない。
        let src = "class Base { public: int m_x; };\nclass Derived : public Base { };\n";
        let cache = RecvHintJobCache::new();
        let ctx = RecvHintContext {
            file_path: std::path::Path::new("test.cpp"),
            source: src,
            cpp_include_dirs: &[],
            job_cache: Some(&cache),
            type_hint_config: None,
        };
        assert_eq!(
            cpp_field_type_for_class_in_sources(&ctx, "Derived", "m_x").as_deref(),
            Some("int")
        );
        let hits_after_first = cache.profile().lookup_cache_hits.load(Ordering::Relaxed);
        assert_eq!(
            cpp_field_type_for_class_in_sources(&ctx, "Derived", "m_x").as_deref(),
            Some("int")
        );
        let hits_after_second = cache.profile().lookup_cache_hits.load(Ordering::Relaxed);
        assert!(
            hits_after_second > hits_after_first,
            "second call must hit the member cache (base traversal result cached under derived key)"
        );
    }
}
