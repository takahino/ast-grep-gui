use crate::batch::BatchReport;
use crate::i18n::{Tr, UiLanguage};
use crate::search::{
    pattern_contains_dollar_recv, FileResult, SearchConditions, SearchMode, SearchStats,
};

/// Markdown 区切り行（列数 n）
fn markdown_table_sep(n: usize) -> String {
    format!("|{}|\n", vec!["---"; n].join("|"))
}

// Excel のセル1件あたりの文字数上限（xlsx仕様: 32,767文字）
const EXCEL_MAX_CELL_CHARS: usize = 32_000;

/// 長い文字列を Excel セル上限に切り詰める
fn truncate_for_excel(s: &str) -> &str {
    if s.len() <= EXCEL_MAX_CELL_CHARS {
        s
    } else {
        // バイト境界が文字境界と一致しない可能性があるため char_indices で切る
        let mut end = EXCEL_MAX_CELL_CHARS;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        &s[..end]
    }
}

/// UI / レポート用（他モジュールから参照）
pub fn search_mode_label_for_export(t: Tr, mode: SearchMode) -> &'static str {
    search_mode_label(t, mode)
}

fn search_mode_label(t: Tr, mode: SearchMode) -> &'static str {
    match mode {
        SearchMode::AstGrep => t.mode_ast(),
        SearchMode::AstGrepRaw => t.mode_ast_raw(),
        SearchMode::PlainText => t.mode_plain(),
        SearchMode::Regex => t.mode_regex(),
    }
}

pub fn file_filter_display<'a>(t: Tr, cond: &'a SearchConditions) -> std::borrow::Cow<'a, str> {
    if cond.file_filter.trim().is_empty() {
        std::borrow::Cow::Borrowed(t.export_cond_file_filter_default())
    } else {
        std::borrow::Cow::Borrowed(cond.file_filter.as_str())
    }
}

/// プレーンテキスト・コンソール向けの検索条件ブロック
fn format_search_conditions_plain(t: Tr, cond: &SearchConditions, lang: UiLanguage) -> String {
    let mut s = String::new();
    s.push_str(t.export_conditions_title());
    s.push('\n');
    s.push_str(&format!("- {}: {}\n", t.export_cond_root(), cond.search_dir));
    s.push_str(&format!("- {}: {}\n", t.export_cond_pattern(), cond.pattern));
    s.push_str(&format!(
        "- {}: {}\n",
        t.export_cond_lang(),
        cond.selected_lang.combo_label(lang)
    ));
    s.push_str(&format!("- {}: {}\n", t.export_cond_context_lines(), cond.context_lines));
    s.push_str(&format!(
        "- {}: {}\n",
        t.export_cond_file_filter(),
        file_filter_display(t, cond)
    ));
    s.push_str(&format!(
        "- {}: {}\n",
        t.export_cond_file_encoding(),
        cond.file_encoding_preference.display_label(lang)
    ));
    s.push_str(&format!("- {}: {}\n", t.export_cond_max_file_mb(), cond.max_file_size_mb));
    s.push_str(&format!(
        "- {}: {}\n",
        t.export_cond_max_search_hits(),
        cond.max_search_hits
    ));
    s.push_str(&format!("- {}: {}\n", t.export_cond_skip_dirs(), cond.skip_dirs));
    s.push_str(&format!(
        "- {}: {}\n",
        t.export_cond_search_mode(),
        search_mode_label(t, cond.search_mode)
    ));
    s.push('\n');
    s
}

/// Markdown 向け（見出し `##` 付き）
pub fn format_search_conditions_markdown(t: Tr, cond: &SearchConditions, lang: UiLanguage) -> String {
    let mut s = String::new();
    s.push_str("## ");
    s.push_str(t.export_conditions_title());
    s.push_str("\n\n");
    s.push_str(&format!("- **{}**: {}\n", t.export_cond_root(), cond.search_dir));
    s.push_str(&format!("- **{}**: {}\n", t.export_cond_pattern(), cond.pattern));
    s.push_str(&format!(
        "- **{}**: {}\n",
        t.export_cond_lang(),
        cond.selected_lang.combo_label(lang)
    ));
    s.push_str(&format!(
        "- **{}**: {}\n",
        t.export_cond_context_lines(),
        cond.context_lines
    ));
    s.push_str(&format!(
        "- **{}**: {}\n",
        t.export_cond_file_filter(),
        file_filter_display(t, cond)
    ));
    s.push_str(&format!(
        "- **{}**: {}\n",
        t.export_cond_file_encoding(),
        cond.file_encoding_preference.display_label(lang)
    ));
    s.push_str(&format!(
        "- **{}**: {}\n",
        t.export_cond_max_file_mb(),
        cond.max_file_size_mb
    ));
    s.push_str(&format!(
        "- **{}**: {}\n",
        t.export_cond_max_search_hits(),
        cond.max_search_hits
    ));
    s.push_str(&format!("- **{}**: {}\n", t.export_cond_skip_dirs(), cond.skip_dirs));
    s.push_str(&format!(
        "- **{}**: {}\n",
        t.export_cond_search_mode(),
        search_mode_label(t, cond.search_mode)
    ));
    s.push('\n');
    s
}

// ─── テキスト ─────────────────────────────────────────────────────────────

/// 検索結果をプレーンテキスト形式にフォーマットする
pub fn results_to_text(
    results: &[FileResult],
    stats: &SearchStats,
    cond: &SearchConditions,
    lang: UiLanguage,
) -> String {
    let t = Tr(lang);
    let mut out = String::new();
    out.push_str("# ");
    out.push_str(t.export_text_title());
    out.push('\n');
    out.push_str(&format_search_conditions_plain(t, cond, lang));
    out.push_str(&t.export_text_total(
        stats.total_matches,
        stats.total_files,
        stats.elapsed_ms,
        stats.hit_limit_reached,
    ));

    for file in results {
        out.push_str(&format!("## {}\n", file.relative_path));
        for m in &file.matches {
            out.push_str(&t.export_line_match_header(m.line_start, m.col_start));
            for line in m.text_with_context().lines() {
                out.push_str("  ");
                out.push_str(line);
                out.push('\n');
            }
        }
        out.push('\n');
    }

    out
}

/// ast-grep そのままモード向けのコンソール風テキスト
pub fn results_to_ast_grep_console(
    results: &[FileResult],
    stats: &SearchStats,
    cond: &SearchConditions,
    lang: UiLanguage,
) -> String {
    let t = Tr(lang);
    let mut out = String::new();
    out.push_str(&t.export_console_header(
        stats.total_matches,
        stats.total_files,
        stats.elapsed_ms,
        stats.hit_limit_reached,
    ));
    out.push_str(&format_search_conditions_plain(t, cond, lang));

    for file in results {
        out.push_str(&file_to_ast_grep_console(file));
        out.push('\n');
    }

    out
}

/// 1ファイル分を ast-grep コンソール風に整形
pub fn file_to_ast_grep_console(file: &FileResult) -> String {
    let mut out = String::new();
    out.push_str(&format!("{}\n", file.relative_path));

    for m in &file.matches {
        out.push_str(&format!("{}:{}\n", m.line_start, m.col_start));
        for line in m.matched_text.lines() {
            out.push_str("  ");
            out.push_str(line);
            out.push('\n');
        }
        if m.matched_text.is_empty() {
            out.push_str("  \n");
        }
    }

    out
}

pub fn results_to_text_for_mode(
    results: &[FileResult],
    stats: &SearchStats,
    cond: &SearchConditions,
    search_mode: SearchMode,
    lang: UiLanguage,
) -> String {
    match search_mode {
        SearchMode::AstGrepRaw => results_to_ast_grep_console(results, stats, cond, lang),
        _ => results_to_text(results, stats, cond, lang),
    }
}

// ─── Markdown テーブル ────────────────────────────────────────────────────

/// 検索結果を Markdown テーブル形式に変換する
pub fn results_to_markdown(
    results: &[FileResult],
    stats: &SearchStats,
    cond: &SearchConditions,
    lang: UiLanguage,
) -> String {
    let t = Tr(lang);
    let mut out = String::new();
    out.push_str("# ");
    out.push_str(t.export_md_heading());
    out.push('\n');
    out.push_str(&format_search_conditions_markdown(t, cond, lang));
    out.push_str(&t.export_md_stats(
        stats.total_matches,
        stats.total_files,
        stats.elapsed_ms,
        stats.hit_limit_reached,
    ));
    let show_recv = pattern_contains_dollar_recv(&cond.pattern);
    if show_recv {
        out.push_str(t.export_md_table_header_with_recv());
        out.push_str(&markdown_table_sep(6));
    } else {
        out.push_str(t.export_md_table_header());
        out.push_str(&markdown_table_sep(5));
    }

    for file in results {
        for m in &file.matches {
            let md_cell = |s: &str| s.replace('|', "\\|").replace('\n', "<br>");
            let matched_cell = md_cell(&m.matched_text);
            let program_cell = md_cell(&m.program_with_context());
            let path = file.relative_path.replace('|', "\\|");
            if show_recv {
                let hint = m
                    .recv_type_hint
                    .as_deref()
                    .map(md_cell)
                    .unwrap_or_else(|| md_cell(""));
                out.push_str(&format!(
                    "| `{}` | {} | {} | {} | {} | {} |\n",
                    path, m.line_start, m.col_start, matched_cell, program_cell, hint
                ));
            } else {
                out.push_str(&format!(
                    "| `{}` | {} | {} | {} | {} |\n",
                    path, m.line_start, m.col_start, matched_cell, program_cell
                ));
            }
        }
    }

    out
}

// ─── HTML テーブル ────────────────────────────────────────────────────────

/// 条件ブロック＋統計＋結果テーブル（`<body>` 内フラグメント）
fn html_conditions_stats_table_fragment(
    results: &[FileResult],
    stats: &SearchStats,
    cond: &SearchConditions,
    lang: UiLanguage,
) -> String {
    let t = Tr(lang);
    let escape = |s: &str| {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    };

    let mut out = String::new();
    out.push_str("<h2 class=\"cond\">");
    out.push_str(t.export_html_conditions_heading());
    out.push_str("</h2>\n<dl class=\"conditions\">\n");
    out.push_str(&format!(
        "<dt>{}</dt><dd>{}</dd>\n",
        escape(t.export_cond_root()),
        escape(&cond.search_dir)
    ));
    out.push_str(&format!(
        "<dt>{}</dt><dd><code>{}</code></dd>\n",
        escape(t.export_cond_pattern()),
        escape(&cond.pattern)
    ));
    out.push_str(&format!(
        "<dt>{}</dt><dd>{}</dd>\n",
        escape(t.export_cond_lang()),
        escape(&cond.selected_lang.combo_label(lang))
    ));
    out.push_str(&format!(
        "<dt>{}</dt><dd>{}</dd>\n",
        escape(t.export_cond_context_lines()),
        cond.context_lines
    ));
    out.push_str(&format!(
        "<dt>{}</dt><dd>{}</dd>\n",
        escape(t.export_cond_file_filter()),
        escape(file_filter_display(t, cond).as_ref())
    ));
    out.push_str(&format!(
        "<dt>{}</dt><dd>{}</dd>\n",
        escape(t.export_cond_file_encoding()),
        escape(cond.file_encoding_preference.display_label(lang))
    ));
    out.push_str(&format!(
        "<dt>{}</dt><dd>{}</dd>\n",
        escape(t.export_cond_max_file_mb()),
        cond.max_file_size_mb
    ));
    out.push_str(&format!(
        "<dt>{}</dt><dd>{}</dd>\n",
        escape(t.export_cond_max_search_hits()),
        cond.max_search_hits
    ));
    out.push_str(&format!(
        "<dt>{}</dt><dd>{}</dd>\n",
        escape(t.export_cond_skip_dirs()),
        escape(&cond.skip_dirs)
    ));
    out.push_str(&format!(
        "<dt>{}</dt><dd>{}</dd>\n",
        escape(t.export_cond_search_mode()),
        escape(search_mode_label(t, cond.search_mode))
    ));
    out.push_str("</dl>\n");
    out.push_str(&t.export_html_stats(
        stats.total_matches,
        stats.total_files,
        stats.elapsed_ms,
        stats.hit_limit_reached,
    ));
    let show_recv = pattern_contains_dollar_recv(&cond.pattern);
    out.push_str("<table>\n<thead>\n<tr>\n");
    out.push_str("<th>");
    out.push_str(t.export_html_th_file());
    out.push_str("</th><th>");
    out.push_str(t.export_html_th_line());
    out.push_str("</th><th>");
    out.push_str(t.export_html_th_col());
    out.push_str("</th><th>");
    out.push_str(t.export_html_th_match());
    out.push_str("</th><th>");
    out.push_str(t.export_html_th_source_context());
    out.push_str("</th>");
    if show_recv {
        out.push_str("<th>");
        out.push_str(t.export_html_th_recv_hint());
        out.push_str("</th>");
    }
    out.push_str("\n");
    out.push_str("</tr>\n</thead>\n<tbody>\n");

    for file in results {
        for m in &file.matches {
            let matched_html = escape(&m.matched_text).replace('\n', "<br>\n");
            let program_html = escape(&m.program_with_context()).replace('\n', "<br>\n");
            out.push_str("<tr>\n");
            out.push_str(&format!("<td><code>{}</code></td>\n", escape(&file.relative_path)));
            out.push_str(&format!("<td>{}</td>\n", m.line_start));
            out.push_str(&format!("<td>{}</td>\n", m.col_start));
            out.push_str(&format!("<td><code>{}</code></td>\n", matched_html));
            out.push_str(&format!("<td><code>{}</code></td>\n", program_html));
            if show_recv {
                let hint = m.recv_type_hint.as_deref().unwrap_or("");
                out.push_str(&format!("<td><code>{}</code></td>\n", escape(hint)));
            }
            out.push_str("</tr>\n");
        }
    }

    out.push_str("</tbody>\n</table>\n");
    out
}

/// 検索結果を HTML テーブル形式に変換する
pub fn results_to_html(
    results: &[FileResult],
    stats: &SearchStats,
    cond: &SearchConditions,
    lang: UiLanguage,
) -> String {
    let t = Tr(lang);
    let mut out = String::new();
    out.push_str("<!DOCTYPE html>\n<html lang=\"");
    out.push_str(t.export_html_lang());
    out.push_str("\">\n<head>\n");
    out.push_str("<meta charset=\"UTF-8\">\n");
    out.push_str("<title>");
    out.push_str(t.export_html_title());
    out.push_str("</title>\n");
    out.push_str("<style>\n");
    out.push_str("body { font-family: sans-serif; margin: 20px; }\n");
    out.push_str("h1 { font-size: 1.4em; }\n");
    out.push_str("h2.cond { font-size: 1.1em; margin: 1em 0 0.5em; }\n");
    out.push_str("dl.conditions { display: grid; grid-template-columns: 200px 1fr; column-gap: 8px; row-gap: 0.35em; margin: 0 0 14px; align-items: start; }\n");
    out.push_str("dl.conditions dt { font-weight: 600; margin: 0; padding: 0; }\n");
    out.push_str("dl.conditions dd { margin: 0; word-break: break-word; }\n");
    out.push_str(".stats { color: #555; font-size: 0.9em; margin-bottom: 12px; }\n");
    out.push_str("table { border-collapse: collapse; width: 100%; }\n");
    out.push_str("th { background: #333; color: #fff; padding: 6px 10px; text-align: left; }\n");
    out.push_str("td { padding: 5px 10px; border-bottom: 1px solid #ddd; vertical-align: top; }\n");
    out.push_str("tr:hover { background: #f5f5f5; }\n");
    out.push_str("code { background: #eee; padding: 1px 4px; border-radius: 3px; font-size: 0.9em; }\n");
    out.push_str("</style>\n</head>\n<body>\n");
    out.push_str("<h1>");
    out.push_str(t.export_html_h1());
    out.push_str("</h1>\n");
    out.push_str(&html_conditions_stats_table_fragment(results, stats, cond, lang));
    out.push_str("</body>\n</html>\n");
    out
}

// ─── Excel (xlsx) ─────────────────────────────────────────────────────────

/// 検索結果を Excel ファイル (.xlsx) として書き出す
///
/// セル1件の文字数が xlsx 仕様上限 32,767 文字を超えないよう切り詰める。
pub fn export_xlsx_to_file(
    path: &std::path::Path,
    results: &[FileResult],
    stats: &SearchStats,
    cond: &SearchConditions,
    lang: UiLanguage,
) -> anyhow::Result<()> {
    use rust_xlsxwriter::{Format, Workbook};

    let t = Tr(lang);
    let mut workbook = Workbook::new();
    let show_recv = pattern_contains_dollar_recv(&cond.pattern);

    // ─ 結果シート ─
    let sheet = workbook.add_worksheet();
    sheet.set_name(t.export_xlsx_sheet_results())?;

    // ヘッダー行のフォーマット
    let header_fmt = Format::new().set_bold().set_background_color(0x333333u32).set_font_color(0xFFFFFFu32);

    sheet.write_with_format(0, 0, t.export_xlsx_col_file(), &header_fmt)?;
    sheet.write_with_format(0, 1, t.export_xlsx_col_line(), &header_fmt)?;
    sheet.write_with_format(0, 2, t.export_xlsx_col_col(), &header_fmt)?;
    sheet.write_with_format(0, 3, t.export_xlsx_col_match(), &header_fmt)?;
    sheet.write_with_format(0, 4, t.export_xlsx_col_source_context(), &header_fmt)?;
    if show_recv {
        sheet.write_with_format(0, 5, t.export_xlsx_col_recv_hint(), &header_fmt)?;
    }

    // 列幅の設定
    sheet.set_column_width(0, 50)?;
    sheet.set_column_width(1, 8)?;
    sheet.set_column_width(2, 8)?;
    sheet.set_column_width(3, 40)?;
    sheet.set_column_width(4, 80)?;
    if show_recv {
        sheet.set_column_width(5, 36)?;
    }

    let mut row = 1u32;
    for file in results {
        for m in &file.matches {
            let program = m.program_with_context();
            sheet.write(row, 0, truncate_for_excel(&file.relative_path))?;
            sheet.write(row, 1, m.line_start as u32)?;
            sheet.write(row, 2, m.col_start as u32)?;
            sheet.write(row, 3, truncate_for_excel(&m.matched_text))?;
            sheet.write(row, 4, truncate_for_excel(&program))?;
            if show_recv {
                let hint = m.recv_type_hint.as_deref().unwrap_or("");
                sheet.write(row, 5, truncate_for_excel(hint))?;
            }
            row += 1;
        }
    }

    // ─ 統計・検索条件シート ─
    let stats_sheet = workbook.add_worksheet();
    stats_sheet.set_name(t.export_xlsx_sheet_stats())?;
    let sub_fmt = Format::new().set_bold();
    stats_sheet.write(0, 0, t.export_xlsx_total_matches())?;
    stats_sheet.write(0, 1, stats.total_matches as u32)?;
    stats_sheet.write(1, 0, t.export_xlsx_file_count())?;
    stats_sheet.write(1, 1, stats.total_files as u32)?;
    stats_sheet.write(2, 0, t.export_xlsx_elapsed())?;
    stats_sheet.write(2, 1, stats.elapsed_ms as u32)?;
    if stats.hit_limit_reached {
        stats_sheet.write(3, 0, t.export_xlsx_hit_limit_note())?;
        stats_sheet.write(3, 1, t.export_xlsx_hit_limit_truncated())?;
    }
    stats_sheet.write_with_format(4, 0, t.export_conditions_title(), &sub_fmt)?;
    stats_sheet.write(5, 0, t.export_cond_root())?;
    stats_sheet.write(5, 1, truncate_for_excel(&cond.search_dir))?;
    stats_sheet.write(6, 0, t.export_cond_pattern())?;
    stats_sheet.write(6, 1, truncate_for_excel(&cond.pattern))?;
    stats_sheet.write(7, 0, t.export_cond_lang())?;
    stats_sheet.write(7, 1, &cond.selected_lang.combo_label(lang))?;
    stats_sheet.write(8, 0, t.export_cond_context_lines())?;
    stats_sheet.write(8, 1, cond.context_lines as u32)?;
    stats_sheet.write(9, 0, t.export_cond_file_filter())?;
    stats_sheet.write(9, 1, truncate_for_excel(file_filter_display(t, cond).as_ref()))?;
    stats_sheet.write(10, 0, t.export_cond_file_encoding())?;
    stats_sheet.write(10, 1, cond.file_encoding_preference.display_label(lang))?;
    stats_sheet.write(11, 0, t.export_cond_max_file_mb())?;
    stats_sheet.write(11, 1, cond.max_file_size_mb as u32)?;
    stats_sheet.write(12, 0, t.export_cond_max_search_hits())?;
    stats_sheet.write(12, 1, cond.max_search_hits as u32)?;
    stats_sheet.write(13, 0, t.export_cond_skip_dirs())?;
    stats_sheet.write(13, 1, truncate_for_excel(&cond.skip_dirs))?;
    stats_sheet.write(14, 0, t.export_cond_search_mode())?;
    stats_sheet.write(14, 1, search_mode_label(t, cond.search_mode))?;
    stats_sheet.set_column_width(0, 28)?;
    stats_sheet.set_column_width(1, 72)?;

    workbook.save(path)?;
    Ok(())
}

// ─── JSON ─────────────────────────────────────────────────────────────────

/// 検索結果をJSON形式にシリアライズする
pub fn results_to_json(
    results: &[FileResult],
    stats: &SearchStats,
    cond: &SearchConditions,
) -> anyhow::Result<String> {
    #[derive(serde::Serialize)]
    struct Output<'a> {
        search: &'a SearchConditions,
        stats: StatsOutput,
        results: &'a [FileResult],
    }
    #[derive(serde::Serialize)]
    struct StatsOutput {
        total_matches: usize,
        total_files: usize,
        elapsed_ms: u64,
        hit_limit_reached: bool,
    }

    let output = Output {
        search: cond,
        stats: StatsOutput {
            total_matches: stats.total_matches,
            total_files: stats.total_files,
            elapsed_ms: stats.elapsed_ms,
            hit_limit_reached: stats.hit_limit_reached,
        },
        results,
    };
    Ok(serde_json::to_string_pretty(&output)?)
}

// ─── クリップボード ───────────────────────────────────────────────────────

/// クリップボードにテキストをコピーする
pub fn copy_to_clipboard(text: &str) -> anyhow::Result<()> {
    let mut clipboard = arboard::Clipboard::new()?;
    clipboard.set_text(text)?;
    Ok(())
}

// ─── ファイル書き出し ─────────────────────────────────────────────────────

pub fn export_json_to_file(
    path: &std::path::Path,
    results: &[FileResult],
    stats: &SearchStats,
    cond: &SearchConditions,
) -> anyhow::Result<()> {
    let json = results_to_json(results, stats, cond)?;
    std::fs::write(path, json)?;
    Ok(())
}

pub fn export_text_to_file(
    path: &std::path::Path,
    results: &[FileResult],
    stats: &SearchStats,
    cond: &SearchConditions,
    search_mode: SearchMode,
    lang: UiLanguage,
) -> anyhow::Result<()> {
    let text = results_to_text_for_mode(results, stats, cond, search_mode, lang);
    std::fs::write(path, text)?;
    Ok(())
}

pub fn export_markdown_to_file(
    path: &std::path::Path,
    results: &[FileResult],
    stats: &SearchStats,
    cond: &SearchConditions,
    lang: UiLanguage,
) -> anyhow::Result<()> {
    let md = results_to_markdown(results, stats, cond, lang);
    std::fs::write(path, md)?;
    Ok(())
}

pub fn export_html_to_file(
    path: &std::path::Path,
    results: &[FileResult],
    stats: &SearchStats,
    cond: &SearchConditions,
    lang: UiLanguage,
) -> anyhow::Result<()> {
    let html = results_to_html(results, stats, cond, lang);
    std::fs::write(path, html)?;
    Ok(())
}

// ─── バッチレポート ─────────────────────────────────────────────────────────

/// バッチ検索の集約結果を JSON でシリアライズ（`runs` 配列）
pub fn batch_report_to_json(report: &BatchReport) -> anyhow::Result<String> {
    #[derive(serde::Serialize)]
    struct Out<'a> {
        total_elapsed_ms: u64,
        runs: Vec<Run<'a>>,
    }
    #[derive(serde::Serialize)]
    struct Run<'a> {
        job_id: usize,
        label: &'a str,
        search: &'a SearchConditions,
        stats: &'a SearchStats,
        results: &'a [FileResult],
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<&'a str>,
    }

    let runs = report
        .runs
        .iter()
        .map(|r| Run {
            job_id: r.job_id,
            label: &r.label,
            search: &r.conditions,
            stats: &r.stats,
            results: &r.results,
            error: r.error.as_deref(),
        })
        .collect();

    let out = Out {
        total_elapsed_ms: report.total_elapsed_ms,
        runs,
    };
    Ok(serde_json::to_string_pretty(&out)?)
}

/// バッチレポートをプレーンテキスト化（ジョブごとに区切り）
pub fn batch_report_to_text(report: &BatchReport, lang: UiLanguage) -> String {
    let t = Tr(lang);
    let mut out = String::new();
    out.push_str(&format!(
        "# {}\n{}\n\n",
        t.batch_report_title(),
        t.batch_report_summary(
            report.total_elapsed_ms,
            report.total_matches(),
            report.total_files(),
            report.runs.len(),
            report.failed_count(),
        )
    ));
    for run in &report.runs {
        out.push_str(&format!("\n\n=== {} ===\n", run.label));
        if let Some(e) = &run.error {
            out.push_str(&format!("ERROR: {e}\n"));
            continue;
        }
        out.push_str(&results_to_text_for_mode(
            &run.results,
            &run.stats,
            &run.conditions,
            run.conditions.search_mode,
            lang,
        ));
    }
    out
}

/// バッチレポートを Markdown 化
pub fn batch_report_to_markdown(report: &BatchReport, lang: UiLanguage) -> String {
    let t = Tr(lang);
    let mut out = String::new();
    out.push_str(&format!("# {}\n\n", t.batch_report_title()));
    out.push_str(&format!(
        "{}\n\n",
        t.batch_report_summary(
            report.total_elapsed_ms,
            report.total_matches(),
            report.total_files(),
            report.runs.len(),
            report.failed_count(),
        )
    ));

    for run in &report.runs {
        out.push_str(&format!("## {}\n\n", run.label));
        if let Some(e) = &run.error {
            out.push_str(&format!("**ERROR:** {e}\n\n"));
            continue;
        }
        out.push_str(&format_search_conditions_markdown(t, &run.conditions, lang));
        out.push_str(&t.export_md_stats(
            run.stats.total_matches,
            run.stats.total_files,
            run.stats.elapsed_ms,
            run.stats.hit_limit_reached,
        ));
        let show_recv = pattern_contains_dollar_recv(&run.conditions.pattern);
        if show_recv {
            out.push_str(t.export_md_table_header_with_recv());
            out.push_str(&markdown_table_sep(6));
        } else {
            out.push_str(t.export_md_table_header());
            out.push_str(&markdown_table_sep(5));
        }

        for file in &run.results {
            for m in &file.matches {
                let md_cell = |s: &str| s.replace('|', "\\|").replace('\n', "<br>");
                let matched_cell = md_cell(&m.matched_text);
                let program_cell = md_cell(&m.program_with_context());
                let path = file.relative_path.replace('|', "\\|");
                if show_recv {
                    let hint = m
                        .recv_type_hint
                        .as_deref()
                        .map(md_cell)
                        .unwrap_or_else(|| md_cell(""));
                    out.push_str(&format!(
                        "| `{}` | {} | {} | {} | {} | {} |\n",
                        path, m.line_start, m.col_start, matched_cell, program_cell, hint
                    ));
                } else {
                    out.push_str(&format!(
                        "| `{}` | {} | {} | {} | {} |\n",
                        path, m.line_start, m.col_start, matched_cell, program_cell
                    ));
                }
            }
        }
        out.push('\n');
    }

    out
}

/// バッチレポートを HTML 化（ジョブごとにセクション）
pub fn batch_report_to_html(report: &BatchReport, lang: UiLanguage) -> String {
    let t = Tr(lang);
    let escape = |s: &str| {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    };

    let mut out = String::new();
    out.push_str("<!DOCTYPE html>\n<html lang=\"");
    out.push_str(t.export_html_lang());
    out.push_str("\">\n<head>\n<meta charset=\"UTF-8\">\n<title>");
    out.push_str(t.batch_report_title());
    out.push_str("</title>\n<style>\n");
    out.push_str("body { font-family: sans-serif; margin: 20px; }\n");
    out.push_str("h1 { font-size: 1.4em; }\n");
    out.push_str("h2.job { font-size: 1.15em; margin-top: 1.2em; border-bottom: 1px solid #ccc; }\n");
    out.push_str("h2.cond { font-size: 1.1em; margin: 1em 0 0.5em; }\n");
    out.push_str("dl.conditions { display: grid; grid-template-columns: 200px 1fr; column-gap: 8px; row-gap: 0.35em; margin: 0 0 14px; align-items: start; }\n");
    out.push_str("dl.conditions dt { font-weight: 600; margin: 0; padding: 0; }\n");
    out.push_str("dl.conditions dd { margin: 0; word-break: break-word; }\n");
    out.push_str(".stats { color: #555; font-size: 0.9em; margin-bottom: 12px; }\n");
    out.push_str("table { border-collapse: collapse; width: 100%; margin-bottom: 1em; }\n");
    out.push_str("th { background: #333; color: #fff; padding: 6px 10px; text-align: left; }\n");
    out.push_str("td { padding: 5px 10px; border-bottom: 1px solid #ddd; vertical-align: top; }\n");
    out.push_str("</style>\n</head>\n<body>\n");
    out.push_str("<h1>");
    out.push_str(t.batch_report_title());
    out.push_str("</h1>\n<p>");
    out.push_str(&escape(
        &t.batch_report_summary(
            report.total_elapsed_ms,
            report.total_matches(),
            report.total_files(),
            report.runs.len(),
            report.failed_count(),
        ),
    ));
    out.push_str("</p>\n");

    for run in &report.runs {
        out.push_str(&format!("<h2 class=\"job\">{}</h2>\n", escape(&run.label)));
        if let Some(e) = &run.error {
            out.push_str(&format!("<p><strong>ERROR:</strong> {}</p>\n", escape(e)));
            continue;
        }
        out.push_str(&html_conditions_stats_table_fragment(
            &run.results,
            &run.stats,
            &run.conditions,
            lang,
        ));
    }
    out.push_str("</body>\n</html>\n");
    out
}

fn sanitize_excel_sheet_name(name: &str, index: usize) -> String {
    let s: String = name
        .chars()
        .map(|c| match c {
            '[' | ']' | '*' | '?' | ':' | '/' | '\\' => '_',
            c => c,
        })
        .take(31)
        .collect();
    if s.is_empty() {
        format!("job_{index}")
    } else {
        s
    }
}

/// バッチレポートを Excel に書き出す（ジョブごとに結果シート＋メタシート）
pub fn export_batch_xlsx_to_file(
    path: &std::path::Path,
    report: &BatchReport,
    lang: UiLanguage,
) -> anyhow::Result<()> {
    use rust_xlsxwriter::{Format, Workbook};

    let t = Tr(lang);
    let mut workbook = Workbook::new();

    for (i, run) in report.runs.iter().enumerate() {
        let name = sanitize_excel_sheet_name(&format!("{i:02}_{}_{}", run.label, run.job_id), i);
        let sheet = workbook.add_worksheet();
        sheet.set_name(&name)?;

        if let Some(e) = &run.error {
            sheet.write(0, 0, format!("ERROR: {e}"))?;
            continue;
        }

        let cond = &run.conditions;
        let stats = &run.stats;
        let results = &run.results;
        let show_recv = pattern_contains_dollar_recv(&cond.pattern);

        let header_fmt = Format::new()
            .set_bold()
            .set_background_color(0x333333u32)
            .set_font_color(0xFFFFFFu32);

        sheet.write_with_format(0, 0, t.export_xlsx_col_file(), &header_fmt)?;
        sheet.write_with_format(0, 1, t.export_xlsx_col_line(), &header_fmt)?;
        sheet.write_with_format(0, 2, t.export_xlsx_col_col(), &header_fmt)?;
        sheet.write_with_format(0, 3, t.export_xlsx_col_match(), &header_fmt)?;
        sheet.write_with_format(0, 4, t.export_xlsx_col_source_context(), &header_fmt)?;
        if show_recv {
            sheet.write_with_format(0, 5, t.export_xlsx_col_recv_hint(), &header_fmt)?;
        }

        sheet.set_column_width(0, 50)?;
        sheet.set_column_width(1, 8)?;
        sheet.set_column_width(2, 8)?;
        sheet.set_column_width(3, 40)?;
        sheet.set_column_width(4, 80)?;
        if show_recv {
            sheet.set_column_width(5, 36)?;
        }

        let mut row = 1u32;
        for file in results {
            for m in &file.matches {
                let program = m.program_with_context();
                sheet.write(row, 0, truncate_for_excel(&file.relative_path))?;
                sheet.write(row, 1, m.line_start as u32)?;
                sheet.write(row, 2, m.col_start as u32)?;
                sheet.write(row, 3, truncate_for_excel(&m.matched_text))?;
                sheet.write(row, 4, truncate_for_excel(&program))?;
                if show_recv {
                    let hint = m.recv_type_hint.as_deref().unwrap_or("");
                    sheet.write(row, 5, truncate_for_excel(hint))?;
                }
                row += 1;
            }
        }

        let stats_sheet = workbook.add_worksheet();
        let sn = format!("m{}", i);
        stats_sheet.set_name(&sanitize_excel_sheet_name(&sn, i))?;
        let sub_fmt = Format::new().set_bold();
        stats_sheet.write(0, 0, t.export_xlsx_total_matches())?;
        stats_sheet.write(0, 1, stats.total_matches as u32)?;
        stats_sheet.write(1, 0, t.export_xlsx_file_count())?;
        stats_sheet.write(1, 1, stats.total_files as u32)?;
        stats_sheet.write(2, 0, t.export_xlsx_elapsed())?;
        stats_sheet.write(2, 1, stats.elapsed_ms as u32)?;
        if stats.hit_limit_reached {
            stats_sheet.write(3, 0, t.export_xlsx_hit_limit_note())?;
            stats_sheet.write(3, 1, t.export_xlsx_hit_limit_truncated())?;
        }
        stats_sheet.write_with_format(4, 0, t.export_conditions_title(), &sub_fmt)?;
        stats_sheet.write(5, 0, t.export_cond_root())?;
        stats_sheet.write(5, 1, truncate_for_excel(&cond.search_dir))?;
        stats_sheet.write(6, 0, t.export_cond_pattern())?;
        stats_sheet.write(6, 1, truncate_for_excel(&cond.pattern))?;
        stats_sheet.write(7, 0, t.export_cond_lang())?;
        stats_sheet.write(7, 1, &cond.selected_lang.combo_label(lang))?;
        stats_sheet.set_column_width(0, 28)?;
        stats_sheet.set_column_width(1, 72)?;
    }

    workbook.save(path)?;
    Ok(())
}

pub fn export_batch_json_to_file(path: &std::path::Path, report: &BatchReport) -> anyhow::Result<()> {
    std::fs::write(path, batch_report_to_json(report)?)?;
    Ok(())
}

pub fn export_batch_text_to_file(
    path: &std::path::Path,
    report: &BatchReport,
    lang: UiLanguage,
) -> anyhow::Result<()> {
    std::fs::write(path, batch_report_to_text(report, lang))?;
    Ok(())
}

pub fn export_batch_markdown_to_file(
    path: &std::path::Path,
    report: &BatchReport,
    lang: UiLanguage,
) -> anyhow::Result<()> {
    std::fs::write(path, batch_report_to_markdown(report, lang))?;
    Ok(())
}

pub fn export_batch_html_to_file(
    path: &std::path::Path,
    report: &BatchReport,
    lang: UiLanguage,
) -> anyhow::Result<()> {
    std::fs::write(path, batch_report_to_html(report, lang))?;
    Ok(())
}
