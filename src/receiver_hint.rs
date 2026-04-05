//! `$RECV` に束縛されたノードから、表示用の receiver 型ヒントを推定する（構文ベース・best-effort）。

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use ast_grep_core::{Doc, Node};
use ast_grep_language::{LanguageExt, SupportLang};

use crate::lang::SupportedLanguage;

/// 検索対象ファイルのパスとソース（C++ の `#include` 解決などに使用）。
#[derive(Debug, Clone, Copy)]
pub struct RecvHintContext<'a> {
    pub file_path: &'a Path,
    pub source: &'a str,
}

/// パターンの `$RECV` に対応するノードから型ヒント文字列を返す。
pub fn infer_recv_type<D: Doc>(
    lang: SupportedLanguage,
    recv: &Node<'_, D>,
    ctx: Option<&RecvHintContext<'_>>,
) -> Option<String> {
    match lang {
        SupportedLanguage::Auto => None,
        SupportedLanguage::Rust => rust_hint(recv),
        SupportedLanguage::Go => go_hint(recv),
        SupportedLanguage::Java => java_hint(recv),
        SupportedLanguage::CSharp => csharp_hint(recv),
        SupportedLanguage::TypeScript | SupportedLanguage::JavaScript => ts_hint(recv),
        SupportedLanguage::Cpp => cpp_hint(recv, ctx),
        SupportedLanguage::C => c_hint(recv),
        SupportedLanguage::Python => python_hint(recv),
        SupportedLanguage::Kotlin => kotlin_hint(recv),
        SupportedLanguage::Scala => scala_hint(recv),
    }
}

fn rust_strip_receiver_text(s: &str) -> String {
    let mut t = s.trim().to_string();
    loop {
        let next = t
            .trim_start_matches("mut ")
            .trim_start_matches('&')
            .trim();
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

fn rust_let_type_in_block<D: Doc>(block: &Node<'_, D>, recv_name: &str, recv_start: usize) -> Option<String> {
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

fn java_local_in_block<D: Doc>(block: &Node<'_, D>, recv_name: &str, recv_start: usize) -> Option<String> {
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

/// `formal_parameters` 配下の `formal_parameter` から名前に一致する型を返す。
fn java_walk_formal_parameters<D: Doc>(node: &Node<'_, D>, name: &str, out: &mut Option<String>) {
    if out.is_some() {
        return;
    }
    if node.kind().as_ref() == "formal_parameter" {
        if let Some(ty) = node.field("type") {
            for c in node.children() {
                if c.kind().as_ref() == "_variable_declarator_id" {
                    if let Some(id) = c.field("name") {
                        let id_text = id.text();
                        if id_text.trim() == name {
                            let ty_text = ty.text();
                            *out = Some(ty_text.trim().to_string());
                            return;
                        }
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

fn java_parameters_from_formals_root<D: Doc>(executable: &Node<'_, D>, name: &str) -> Option<String> {
    let mut out = None;
    java_walk_formal_parameters(executable, name, &mut out);
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
    if let Some(block) = recv.ancestors().find(|n| n.kind().as_ref() == "block") {
        if let Some(ty) = java_local_in_block(&block, t, recv.range().start) {
            return Some(ty);
        }
    }
    if let Some(ty) = java_parameter_type_for_scope(recv, t) {
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

fn csharp_local_in_block<D: Doc>(block: &Node<'_, D>, recv_name: &str, recv_start: usize) -> Option<String> {
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
        k.as_ref() == "class_declaration" || k.as_ref() == "struct_declaration" || k.as_ref() == "record_declaration"
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

fn ts_lexical_in_block<D: Doc>(block: &Node<'_, D>, recv_name: &str, recv_start: usize) -> Option<String> {
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
    let class_decl = recv.ancestors().find(|n| n.kind().as_ref() == "class_declaration")?;
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
                let s = tanno_trim
                    .strip_prefix(':')
                    .unwrap_or(tanno_trim)
                    .trim();
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
    if let Some(d) = decl.field("declarator") {
        if d.kind().as_ref() == "init_declarator" {
            if let Some(inner) = d.field("declarator") {
                return cpp_declarator_matches_name(&inner, name);
            }
            return false;
        }
        return cpp_declarator_matches_name(&d, name);
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

fn cpp_walk_parameter_declarations<D: Doc>(node: &Node<'_, D>, name: &str, out: &mut Option<String>) {
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
        k.as_ref() == "class_specifier" || k.as_ref() == "struct_specifier" || k.as_ref() == "union_specifier"
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
    let fd = recv.ancestors().find(|n| n.kind().as_ref() == "function_definition")?;
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

const CPP_INCLUDE_MAX_DEPTH: usize = 8;
const CPP_INCLUDE_MAX_FILE_BYTES: usize = 512 * 1024;

fn cpp_try_header_file_for_field(
    path: &Path,
    class_name: &str,
    field_name: &str,
    visited: &mut HashSet<PathBuf>,
    depth: usize,
) -> Option<String> {
    if depth == 0 {
        return None;
    }
    let key = cpp_path_key(path);
    if visited.contains(&key) {
        return None;
    }
    let len = fs::metadata(path).ok()?.len();
    if len > CPP_INCLUDE_MAX_FILE_BYTES as u64 {
        return None;
    }
    let text = fs::read_to_string(path).ok()?;
    let grep = SupportLang::Cpp.ast_grep(&text);
    let root = grep.root();
    if let Some(ty) = cpp_field_in_named_translation_unit(&root, class_name, field_name) {
        visited.insert(key);
        return Some(ty);
    }
    visited.insert(key);
    if depth <= 1 {
        return None;
    }
    let base = path.parent()?;
    for inc in cpp_include_paths_from_source(&text) {
        let p = base.join(&inc);
        if p.is_file() {
            if let Some(ty) = cpp_try_header_file_for_field(&p, class_name, field_name, visited, depth - 1)
            {
                return Some(ty);
            }
        }
    }
    None
}

fn cpp_field_from_included_headers<D: Doc>(
    ctx: &RecvHintContext<'_>,
    recv: &Node<'_, D>,
    field_name: &str,
) -> Option<String> {
    let class_name = cpp_scope_class_name(recv)?;
    let base = ctx.file_path.parent()?;
    let mut visited = HashSet::new();
    visited.insert(cpp_path_key(ctx.file_path));
    for inc in cpp_include_paths_from_source(ctx.source) {
        let p = base.join(&inc);
        if p.is_file() {
            if let Some(ty) = cpp_try_header_file_for_field(
                &p,
                class_name.as_str(),
                field_name,
                &mut visited,
                CPP_INCLUDE_MAX_DEPTH,
            ) {
                return Some(ty);
            }
        }
    }
    None
}

fn cpp_hint<D: Doc>(recv: &Node<'_, D>, ctx: Option<&RecvHintContext<'_>>) -> Option<String> {
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
    }
    None
}

fn c_hint<D: Doc>(recv: &Node<'_, D>) -> Option<String> {
    let _ = recv;
    None
}

/// クラス body 内の `annotated_assignment`（クラス変数の型注釈）と名前を照合する。
fn python_field_in_class<D: Doc>(recv: &Node<'_, D>, name: &str) -> Option<String> {
    let class_def = recv.ancestors().find(|n| n.kind().as_ref() == "class_definition")?;
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
    let id = node.children().find(|c| c.kind().as_ref() == "type_identifier")?;
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
        matches!(k.as_ref(), "class_definition" | "object_definition" | "trait_definition")
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
        assert_eq!(hint.as_deref(), Some("CTime"));
    }
}
