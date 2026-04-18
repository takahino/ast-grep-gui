//! コードビュー / 表ビュー / 表プレビュー内のテキスト検索（Ctrl+F）。

use egui::Ui;
use regex::RegexBuilder;

use crate::app::{AstGrepApp, TableRowRef};
use crate::search::type_hint_column_keys;

/// パネルごとに最後にスクロール同期した (クエリ, マッチ番号) を保持し、同じ内容での二重申請を防ぐ。
#[derive(Debug, Default, Clone)]
pub struct InViewFindState {
    pub open: bool,
    pub query: String,
    /// true = 大文字小文字を区別
    pub case_sensitive: bool,
    /// 0 始まりの「現在のマッチ」インデックス
    pub current: usize,
    pub focus_request: bool,
    last_code: Option<(String, usize)>,
    last_table: Option<(String, usize)>,
    last_preview: Option<(String, usize)>,
}

impl InViewFindState {
    pub fn close(&mut self) {
        self.open = false;
        self.last_code = None;
        self.last_table = None;
        self.last_preview = None;
    }

    /// Ctrl+F 時: 開き、フォーカス要求し、直前のスクロール同期を忘れて再スクロール可能にする。
    pub fn begin_open(&mut self) {
        self.open = true;
        self.focus_request = true;
        self.last_code = None;
        self.last_table = None;
        self.last_preview = None;
    }
}

fn build_find_regex(needle: &str, case_sensitive: bool) -> Option<regex::Regex> {
    if needle.is_empty() {
        return None;
    }
    let mut b = RegexBuilder::new(&regex::escape(needle));
    if !case_sensitive {
        b.case_insensitive(true);
    }
    b.build().ok()
}

/// ソース文字列内の各マッチのバイト範囲 `[start, end)`（昇順）
pub fn find_byte_spans(haystack: &str, needle: &str, case_sensitive: bool) -> Vec<(usize, usize)> {
    let Some(re) = build_find_regex(needle, case_sensitive) else {
        return Vec::new();
    };
    re.find_iter(haystack).map(|m| (m.start(), m.end())).collect()
}

fn line_1based_at_byte(source: &str, byte_idx: usize) -> usize {
    source
        .get(..byte_idx.min(source.len()))
        .map(|s| s.bytes().filter(|&b| b == b'\n').count() + 1)
        .unwrap_or(1)
}

fn scroll_stamp_key(app: &AstGrepApp) -> (String, usize) {
    (app.in_view_find.query.clone(), app.in_view_find.current)
}

/// コードビュー用。`pending_scroll_line`（1-based）へ反映。
pub fn show_bar_code(app: &mut AstGrepApp, ui: &mut Ui, source: &str) {
    if !app.in_view_find.open {
        return;
    }
    let t = app.tr();

    let mut query_changed = false;
    let mut case_changed = false;
    let mut prev_clicked = false;
    let mut next_clicked = false;

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(t.in_view_find_label()).small());
        let edit = ui.add(
            egui::TextEdit::singleline(&mut app.in_view_find.query)
                .desired_width(220.0)
                .hint_text(t.in_view_find_hint()),
        );
        if app.in_view_find.focus_request {
            edit.request_focus();
        }
        query_changed = edit.changed();

        let cresp = ui.checkbox(
            &mut app.in_view_find.case_sensitive,
            t.in_view_find_case_sensitive(),
        );
        case_changed = cresp.changed();

        if ui
            .small_button("↑")
            .on_hover_text(t.in_view_find_prev_tooltip())
            .clicked()
        {
            prev_clicked = true;
        }
        if ui
            .small_button("↓")
            .on_hover_text(t.in_view_find_next_tooltip())
            .clicked()
        {
            next_clicked = true;
        }
        if ui
            .small_button("✕")
            .on_hover_text(t.in_view_find_close_tooltip())
            .clicked()
        {
            app.in_view_find.close();
        }
    });

    if app.in_view_find.focus_request {
        app.in_view_find.focus_request = false;
    }

    if !app.in_view_find.open {
        return;
    }

    if query_changed || case_changed {
        app.in_view_find.current = 0;
        app.in_view_find.last_code = None;
    }

    let occurrences = find_byte_spans(source, &app.in_view_find.query, app.in_view_find.case_sensitive);
    let n = occurrences.len();
    if n > 0 {
        if prev_clicked {
            app.in_view_find.current = app.in_view_find.current.saturating_sub(1);
        }
        if next_clicked {
            app.in_view_find.current = (app.in_view_find.current + 1) % n;
        }
        app.in_view_find.current = app.in_view_find.current.min(n - 1);
    } else {
        app.in_view_find.current = 0;
    }

    ui.horizontal(|ui| {
        let count_lbl: String = if n == 0 {
            t.in_view_find_count_zero().to_string()
        } else {
            t.in_view_find_count(app.in_view_find.current + 1, n)
        };
        ui.label(egui::RichText::new(count_lbl).small());
    });

    let key = scroll_stamp_key(app);
    if n > 0 {
        let b = occurrences[app.in_view_find.current].0;
        let line = line_1based_at_byte(source, b);
        if app.in_view_find.last_code.as_ref() != Some(&key) {
            app.pending_scroll_line = Some(line);
            app.in_view_find.last_code = Some(key);
        }
    } else {
        app.in_view_find.last_code = None;
    }
}

fn table_row_blob(app: &AstGrepApp, file_idx: usize, match_idx: usize) -> String {
    let column_keys = type_hint_column_keys(&app.pattern, &app.results);
    let file = &app.results[file_idx];
    let m = &file.matches[match_idx];
    let mut blob = format!(
        "{} {} {} {} {}",
        file.relative_path,
        m.line_start,
        m.col_start,
        m.line_end,
        m.col_end
    );
    blob.push(' ');
    blob.push_str(&m.matched_text);
    for k in &column_keys {
        blob.push(' ');
        blob.push_str(&m.type_hint_cell(k).to_export_string());
    }
    blob.push(' ');
    blob.push_str(&m.program_with_context());
    blob
}

/// 検索にヒットする表の行インデックス（`table_rows` の添字）
pub fn table_find_matching_row_indices(app: &AstGrepApp) -> Vec<usize> {
    let Some(re) = build_find_regex(&app.in_view_find.query, app.in_view_find.case_sensitive) else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    for (row_idx, TableRowRef { file_idx, match_idx }) in app.table_rows.iter().enumerate() {
        let blob = table_row_blob(app, *file_idx, *match_idx);
        if re.is_match(&blob) {
            rows.push(row_idx);
        }
    }
    rows
}

/// 表ビュー用。`table_scroll_to_row`（`table_rows` の添字）へ反映。
pub fn show_bar_table(app: &mut AstGrepApp, ui: &mut Ui) {
    if !app.in_view_find.open {
        return;
    }
    let t = app.tr();

    let mut query_changed = false;
    let mut case_changed = false;
    let mut prev_clicked = false;
    let mut next_clicked = false;

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(t.in_view_find_label()).small());
        let edit = ui.add(
            egui::TextEdit::singleline(&mut app.in_view_find.query)
                .desired_width(220.0)
                .hint_text(t.in_view_find_hint()),
        );
        if app.in_view_find.focus_request {
            edit.request_focus();
        }
        query_changed = edit.changed();
        let cresp = ui.checkbox(
            &mut app.in_view_find.case_sensitive,
            t.in_view_find_case_sensitive(),
        );
        case_changed = cresp.changed();
        if ui
            .small_button("↑")
            .on_hover_text(t.in_view_find_prev_tooltip())
            .clicked()
        {
            prev_clicked = true;
        }
        if ui
            .small_button("↓")
            .on_hover_text(t.in_view_find_next_tooltip())
            .clicked()
        {
            next_clicked = true;
        }
        if ui
            .small_button("✕")
            .on_hover_text(t.in_view_find_close_tooltip())
            .clicked()
        {
            app.in_view_find.close();
        }
    });

    if app.in_view_find.focus_request {
        app.in_view_find.focus_request = false;
    }

    if !app.in_view_find.open {
        return;
    }

    if query_changed || case_changed {
        app.in_view_find.current = 0;
        app.in_view_find.last_table = None;
    }

    let row_indices = table_find_matching_row_indices(app);
    let n = row_indices.len();
    if n > 0 {
        if prev_clicked {
            app.in_view_find.current = app.in_view_find.current.saturating_sub(1);
        }
        if next_clicked {
            app.in_view_find.current = (app.in_view_find.current + 1) % n;
        }
        app.in_view_find.current = app.in_view_find.current.min(n - 1);
    } else {
        app.in_view_find.current = 0;
    }

    ui.horizontal(|ui| {
        let count_lbl: String = if n == 0 {
            t.in_view_find_count_zero().to_string()
        } else {
            t.in_view_find_count(app.in_view_find.current + 1, n)
        };
        ui.label(egui::RichText::new(count_lbl).small());
    });

    let key = scroll_stamp_key(app);
    if n > 0 {
        let row = row_indices[app.in_view_find.current];
        if app.in_view_find.last_table.as_ref() != Some(&key) {
            app.table_scroll_to_row = Some(row);
            app.in_view_find.last_table = Some(key);
        }
    } else {
        app.in_view_find.last_table = None;
    }
}

/// プレビュー内のファイル全文向け。`on_scroll_line` に 1-based 行を渡す。
pub fn show_bar_preview(app: &mut AstGrepApp, ui: &mut Ui, source: &str, on_scroll_line: &mut dyn FnMut(usize)) {
    if !app.in_view_find.open {
        return;
    }
    let t = app.tr();

    let mut query_changed = false;
    let mut case_changed = false;
    let mut prev_clicked = false;
    let mut next_clicked = false;

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(t.in_view_find_label()).small());
        let edit = ui.add(
            egui::TextEdit::singleline(&mut app.in_view_find.query)
                .desired_width(220.0)
                .hint_text(t.in_view_find_hint()),
        );
        if app.in_view_find.focus_request {
            edit.request_focus();
        }
        query_changed = edit.changed();
        let cresp = ui.checkbox(
            &mut app.in_view_find.case_sensitive,
            t.in_view_find_case_sensitive(),
        );
        case_changed = cresp.changed();
        if ui
            .small_button("↑")
            .on_hover_text(t.in_view_find_prev_tooltip())
            .clicked()
        {
            prev_clicked = true;
        }
        if ui
            .small_button("↓")
            .on_hover_text(t.in_view_find_next_tooltip())
            .clicked()
        {
            next_clicked = true;
        }
        if ui
            .small_button("✕")
            .on_hover_text(t.in_view_find_close_tooltip())
            .clicked()
        {
            app.in_view_find.close();
        }
    });

    if app.in_view_find.focus_request {
        app.in_view_find.focus_request = false;
    }

    if !app.in_view_find.open {
        return;
    }

    if query_changed || case_changed {
        app.in_view_find.current = 0;
        app.in_view_find.last_preview = None;
    }

    let occurrences = find_byte_spans(source, &app.in_view_find.query, app.in_view_find.case_sensitive);
    let n = occurrences.len();
    if n > 0 {
        if prev_clicked {
            app.in_view_find.current = app.in_view_find.current.saturating_sub(1);
        }
        if next_clicked {
            app.in_view_find.current = (app.in_view_find.current + 1) % n;
        }
        app.in_view_find.current = app.in_view_find.current.min(n - 1);
    } else {
        app.in_view_find.current = 0;
    }

    ui.horizontal(|ui| {
        let count_lbl: String = if n == 0 {
            t.in_view_find_count_zero().to_string()
        } else {
            t.in_view_find_count(app.in_view_find.current + 1, n)
        };
        ui.label(egui::RichText::new(count_lbl).small());
    });

    let key = scroll_stamp_key(app);
    if n > 0 {
        let b = occurrences[app.in_view_find.current].0;
        let line = line_1based_at_byte(source, b);
        if app.in_view_find.last_preview.as_ref() != Some(&key) {
            on_scroll_line(line);
            app.in_view_find.last_preview = Some(key);
        }
    } else {
        app.in_view_find.last_preview = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_byte_spans_case_sensitive_differs() {
        assert!(find_byte_spans("AAA", "a", true).is_empty());
        assert_eq!(find_byte_spans("AaA", "A", true), vec![(0, 1), (2, 3)]);
    }

    #[test]
    fn find_byte_spans_insensitive_finds_lower() {
        let v = find_byte_spans("AaA", "a", false);
        assert_eq!(v, vec![(0, 1), (1, 2), (2, 3)]);
    }
}
