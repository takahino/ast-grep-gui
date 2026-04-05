//! AST パターンに基づく一括置換（プレビュー・適用）。
//! 検索と同じ `compile_strategies` / `ast-grep-core` の `Root::replace` を使用する。

use std::path::PathBuf;

use ast_grep_language::LanguageExt;
use crossbeam_channel::Sender;

use crate::ast_pattern::compile_strategies;
use crate::file_encoding::{read_text_file, write_text_file, FileEncoding, FileEncodingPreference};
use crate::lang::SupportedLanguage;
use crate::search::FileResult;

/// 1 ファイル分の置換プレビュー
#[derive(Debug, Clone)]
pub struct RewriteFilePreview {
    pub path: PathBuf,
    pub relative_path: String,
    pub source_before: String,
    pub source_after: String,
    pub text_encoding: FileEncoding,
    pub source_language: SupportedLanguage,
    /// このファイルで適用した置換回数（`replace` の成功回数）
    pub replacement_count: usize,
}

/// プレビュー全体
#[derive(Debug, Clone)]
pub struct RewritePreview {
    pub files: Vec<RewriteFilePreview>,
    pub elapsed_ms: u64,
}

#[derive(Debug)]
pub enum RewriteMessage {
    Progress { done: usize, total: usize },
    Done(RewritePreview),
    Error(String),
}

/// 検索ヒットファイルに対し、置換後ソースを生成する（ディスクには書かない）
pub fn spawn_rewrite_preview(
    results: Vec<FileResult>,
    pattern: String,
    rewrite_template: String,
    file_encoding_preference: FileEncodingPreference,
    tx: Sender<RewriteMessage>,
    egui_ctx: egui::Context,
) {
    std::thread::spawn(move || {
        let start = std::time::Instant::now();
        let total = results.len();
        let mut previews = Vec::new();

        for (idx, fr) in results.into_iter().enumerate() {
            let decoded = match read_text_file(&fr.path, file_encoding_preference) {
                Ok(d) => d,
                Err(_) => continue,
            };
            let source = decoded.text;
            let text_encoding = fr.text_encoding.clone();

            match apply_rewrite_to_string(
                &source,
                &pattern,
                &rewrite_template,
                fr.source_language,
            ) {
                Ok(Some((after, count))) if count > 0 && after != source => {
                    previews.push(RewriteFilePreview {
                        path: fr.path,
                        relative_path: fr.relative_path,
                        source_before: source,
                        source_after: after,
                        text_encoding,
                        source_language: fr.source_language,
                        replacement_count: count,
                    });
                }
                Ok(_) => {}
                Err(e) => {
                    let _ = tx.send(RewriteMessage::Error(e));
                    egui_ctx.request_repaint();
                    return;
                }
            }

            let _ = tx.send(RewriteMessage::Progress {
                done: idx + 1,
                total,
            });
            egui_ctx.request_repaint();
        }

        let elapsed_ms = start.elapsed().as_millis() as u64;
        let _ = tx.send(RewriteMessage::Done(RewritePreview {
            files: previews,
            elapsed_ms,
        }));
        egui_ctx.request_repaint();
    });
}

/// 1 パスあたりの編集回数に上限（異常時の無限ループ防止）
const MAX_REWRITE_EDITS_PER_PASS: usize = 500_000;
/// ネストなどで複数パスが必要な場合の上限
const MAX_REWRITE_PASSES: usize = 10_000;

/// 単一ソースに対して置換を適用。戻り値: `Some((新テキスト, 置換回数))`、マッチなしは `None`
///
/// `Root::replace` を `while` で回す方式は、置換後も同じパターンが再マッチする場合に
/// **無限ループ**する（例: `$METHOD($$$ARGS)` → `$METHOD()` は、空の `()` にもマッチし
/// 置換結果が元と同一になり続ける）。そのため `Node::replace_all` で編集を一括収集し、
/// 適用後にソースが変わらなければ打ち切る。
pub fn apply_rewrite_to_string(
    source: &str,
    pattern: &str,
    rewrite_template: &str,
    file_lang: SupportedLanguage,
) -> Result<Option<(String, usize)>, String> {
    let ast_lang = match file_lang.to_support_lang() {
        Some(l) => l,
        None => return Ok(None),
    };
    let compiled_patterns = compile_strategies(pattern, file_lang, ast_lang.clone());
    if compiled_patterns.is_empty() {
        return Ok(None);
    }

    let root_probe = ast_lang.ast_grep(source);
    for compiled_pat in &compiled_patterns {
        if root_probe.root().find_all(compiled_pat).next().is_none() {
            continue;
        }

        let mut current = source.to_string();
        let mut total_edits = 0usize;

        for _pass in 0..MAX_REWRITE_PASSES {
            let mut root = ast_lang.ast_grep(&current);
            let edits = root.root().replace_all(compiled_pat, rewrite_template);
            if edits.is_empty() {
                break;
            }
            if edits.len() > MAX_REWRITE_EDITS_PER_PASS {
                return Err(format!(
                    "replace_all: too many matches in one pass ({})",
                    edits.len()
                ));
            }
            let before = current.clone();
            let n = edits.len();
            for edit in edits.into_iter().rev() {
                root.edit(edit)?;
            }
            let next = root.generate();
            if next == before {
                break;
            }
            total_edits += n;
            current = next;
        }

        if total_edits == 0 || current == source {
            continue;
        }
        return Ok(Some((current, total_edits)));
    }
    Ok(None)
}

/// 書き戻しをバックグラウンドで実行（UI をブロックしない）
pub fn spawn_apply_rewrite(
    files: Vec<RewriteFilePreview>,
    tx: Sender<Result<usize, Vec<(PathBuf, String)>>>,
    egui_ctx: egui::Context,
) {
    std::thread::spawn(move || {
        let r = apply_preview_to_disk(&files);
        let _ = tx.send(r);
        egui_ctx.request_repaint();
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::SupportedLanguage;

    #[test]
    fn auto_lang_returns_none() {
        let result = apply_rewrite_to_string(
            "print('hello')",
            "print($$$ARGS)",
            "logger.info($$$ARGS)",
            SupportedLanguage::Auto,
        );
        assert_eq!(result, Ok(None));
    }

    #[test]
    fn no_match_returns_none() {
        let result = apply_rewrite_to_string(
            "x = 1",
            "print($$$ARGS)",
            "logger.info($$$ARGS)",
            SupportedLanguage::Python,
        );
        assert_eq!(result, Ok(None));
    }

    #[test]
    fn python_simple_replacement() {
        let source = "print(\"hello\")";
        let (after, count) = apply_rewrite_to_string(
            source,
            "print($$$ARGS)",
            "logger.info($$$ARGS)",
            SupportedLanguage::Python,
        )
        .unwrap()
        .unwrap();
        assert!(count > 0);
        assert!(after.contains("logger.info"));
        assert!(!after.contains("print("));
    }

    #[test]
    fn rust_unwrap_replacement() {
        let source = "fn main() { let x = foo.unwrap(); }";
        let (after, count) = apply_rewrite_to_string(
            source,
            "$E.unwrap()",
            "$E.expect(\"error\")",
            SupportedLanguage::Rust,
        )
        .unwrap()
        .unwrap();
        assert!(count > 0);
        assert!(after.contains("expect("));
        assert!(!after.contains(".unwrap()"));
    }

    #[test]
    fn rust_multiple_matches_counted() {
        let source =
            "fn main() {\n    let a = x.unwrap();\n    let b = y.unwrap();\n}";
        let (_, count) = apply_rewrite_to_string(
            source,
            "$E.unwrap()",
            "$E.expect(\"err\")",
            SupportedLanguage::Rust,
        )
        .unwrap()
        .unwrap();
        assert!(count >= 2, "expected >= 2 replacements, got {count}");
    }

    #[test]
    fn empty_pattern_returns_none() {
        let result = apply_rewrite_to_string(
            "fn main() {}",
            "",
            "replaced",
            SupportedLanguage::Rust,
        );
        assert_eq!(result, Ok(None));
    }
}

/// プレビュー内容をディスクに書き戻す
pub fn apply_preview_to_disk(files: &[RewriteFilePreview]) -> Result<usize, Vec<(PathBuf, String)>> {
    let mut errors = Vec::new();
    let mut ok = 0usize;
    for f in files {
        if let Err(e) = write_text_file(&f.path, &f.source_after, &f.text_encoding) {
            errors.push((f.path.clone(), e.to_string()));
        } else {
            ok += 1;
        }
    }
    if errors.is_empty() {
        Ok(ok)
    } else {
        Err(errors)
    }
}
