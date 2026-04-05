use egui::text::LayoutJob;
use egui::{Align, Color32, FontId, Label, Rect, RichText, Sense, Ui, Vec2};

use crate::app::{AstGrepApp, TablePreviewState, TableRowRef};
use crate::ui::scroll_keyboard;
use crate::highlight::build_layout_job_from_line;
use crate::search::{pattern_contains_dollar_recv, MatchItem};

/// 長いパスは先頭を「...」で省略し末尾のみ表示する（UTF-8 の文字境界で切る）。
fn ellipsis_path_tail(path: &str, max_chars: usize, tail_chars: usize) -> String {
    let n = path.chars().count();
    if n <= max_chars {
        return path.to_string();
    }
    let skip = n.saturating_sub(tail_chars);
    format!("...{}", path.chars().skip(skip).collect::<String>())
}

fn label_cell(
    ui: &mut Ui,
    width: f32,
    text: impl Into<egui::WidgetText>,
    sense: Sense,
) -> egui::Response {
    ui.add_sized(
        [width, ui.spacing().interact_size.y],
        Label::new(text).truncate().sense(sense),
    )
}

fn left_aligned_text_cell(
    ui: &mut Ui,
    width: f32,
    height: f32,
    text: &str,
    font_id: FontId,
    color: Color32,
    sense: Sense,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), sense);
    let galley = ui.fonts(|fonts| fonts.layout_no_wrap(text.to_owned(), font_id, color));
    let pos = rect.min + egui::vec2(4.0, 0.0);
    ui.painter().with_clip_rect(rect).galley(pos, galley, color);
    response
}

fn left_aligned_layout_job_cell(
    ui: &mut Ui,
    width: f32,
    height: f32,
    job: LayoutJob,
    fallback_color: Color32,
    sense: Sense,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), sense);
    let galley = ui.fonts(|fonts| fonts.layout_job(job));
    let pos = rect.min + egui::vec2(4.0, 0.0);
    ui.painter()
        .with_clip_rect(rect)
        .galley(pos, galley, fallback_color);
    response
}

fn context_match_item(m: &MatchItem) -> MatchItem {
    MatchItem {
        line_start: m.line_start,
        col_start: m.col_start,
        line_end: m.line_end,
        col_end: m.col_end,
        matched_text: m.matched_text.clone(),
        span_lines_text: m.span_lines_text.clone(),
        context_before: Vec::new(),
        context_after: Vec::new(),
        recv_type_hint: m.recv_type_hint.clone(),
    }
}

fn render_header(ui: &mut Ui, t: crate::i18n::Tr, show_recv_col: bool) {
    ui.horizontal(|ui| {
        ui.add_sized(
            [240.0, ui.spacing().interact_size.y],
            Label::new(RichText::new(t.table_col_file()).strong()),
        );
        ui.add_sized(
            [60.0, ui.spacing().interact_size.y],
            Label::new(RichText::new(t.table_col_line()).strong()),
        );
        ui.add_sized(
            [60.0, ui.spacing().interact_size.y],
            Label::new(RichText::new(t.table_col_col()).strong()),
        );
        ui.add_sized(
            [280.0, ui.spacing().interact_size.y],
            Label::new(RichText::new(t.table_col_text()).strong()),
        );
        ui.add_sized(
            [520.0, ui.spacing().interact_size.y],
            Label::new(RichText::new(t.table_col_source_context()).strong()),
        );
        if show_recv_col {
            ui.add_sized(
                [160.0, ui.spacing().interact_size.y],
                Label::new(RichText::new(t.table_col_recv_hint()).strong()),
            );
        }
        ui.add_sized(
            [90.0, ui.spacing().interact_size.y],
            Label::new(RichText::new(t.table_col_action()).strong()),
        );
    });
}

fn row_index_for_unit_offset(prefix_units: &[usize], row_count: usize, unit_offset: usize) -> usize {
    if row_count == 0 {
        return 0;
    }
    match prefix_units[..row_count].binary_search(&unit_offset) {
        Ok(i) => i.min(row_count),
        Err(i) => i.saturating_sub(1).min(row_count),
    }
}

pub fn show(app: &mut AstGrepApp, ui: &mut Ui) {
    let t = app.tr();
    if app.results.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.label(
                egui::RichText::new(t.table_empty())
                    .color(egui::Color32::GRAY),
            );
        });
        return;
    }

    // パターン支援へ転送するスニペットを一時保存
    let mut send_to_assist: Option<String> = None;
    let mut open_table_preview: Option<TablePreviewState> = None;

    ui.label(
        egui::RichText::new(t.table_double_click_hint())
            .small()
            .color(egui::Color32::GRAY),
    );
    ui.add_space(4.0);

    const FILE_W: f32 = 240.0;
    const LINE_W: f32 = 60.0;
    const COL_W: f32 = 60.0;
    const MATCH_W: f32 = 280.0;
    const RECV_HINT_W: f32 = 160.0;
    const SOURCE_W: f32 = 520.0;
    const ACTION_W: f32 = 90.0;
    let show_recv_col = pattern_contains_dollar_recv(app.pattern.as_str());
    let total_w = FILE_W
        + LINE_W
        + COL_W
        + MATCH_W
        + SOURCE_W
        + (if show_recv_col { RECV_HINT_W } else { 0.0 })
        + ACTION_W
        + 48.0;

    let row_unit_height = ui.text_style_height(&egui::TextStyle::Body).max(ui.spacing().interact_size.y);

    let sid = scroll_keyboard::scroll_area_persistent_id(ui, "table_view");
    let rect = ui.available_rect_before_wrap();
    scroll_keyboard::apply_keyboard_scroll_before_show(
        ui.ctx(),
        ui,
        sid,
        rect,
        egui::Vec2b::from([true, true]),
    );

    let scroll_out = egui::ScrollArea::both()
        .id_salt("table_view")
        .auto_shrink([false, false])
        .show_viewport(ui, |ui, viewport| {
            ui.set_min_width(total_w);
            render_header(ui, t, show_recv_col);
            ui.separator();

            let header_height = row_unit_height + ui.spacing().item_spacing.y + 6.0;
            let row_count = app.table_rows.len();
            let content_top = header_height;
            let total_units = app.table_row_prefix_units.last().copied().unwrap_or(0);

            if let Some(target_row) = app.table_scroll_to_row {
                let target_units = app.table_row_prefix_units.get(target_row).copied().unwrap_or(0);
                let target_height_units = app.table_row_units.get(target_row).copied().unwrap_or(1);
                let target_rect = Rect::from_min_size(
                    egui::pos2(0.0, content_top + target_units as f32 * row_unit_height),
                    Vec2::new(total_w, target_height_units as f32 * row_unit_height),
                );
                ui.scroll_to_rect(target_rect, Some(Align::Center));
            }

            let viewport_min_units = ((viewport.min.y - content_top) / row_unit_height)
                .floor()
                .max(0.0) as usize;
            let viewport_max_units = ((viewport.max.y - content_top) / row_unit_height)
                .ceil()
                .max(0.0) as usize;
            let start = row_index_for_unit_offset(&app.table_row_prefix_units, row_count, viewport_min_units)
                .min(row_count);
            let end = (row_index_for_unit_offset(
                &app.table_row_prefix_units,
                row_count,
                viewport_max_units.saturating_add(1),
            ) + 2)
                .min(row_count);

            let start_units = app.table_row_prefix_units.get(start).copied().unwrap_or(0);
            ui.add_space(start_units as f32 * row_unit_height);

            for row_idx in start..end {
                let TableRowRef { file_idx, match_idx } = app.table_rows[row_idx];
                let (
                    path,
                    relative_path,
                    line_start,
                    col_start,
                    matched_text,
                    recv_type_hint,
                    source_context_job,
                    full_context,
                    matches,
                    source_language,
                    text_encoding,
                ) = {
                    let file = &app.results[file_idx];
                    let m = &file.matches[match_idx];
                    let recv_type_hint = m.recv_type_hint.clone();
                    let full_context = m.program_with_context();
                    // コンテキスト行数が変わると full_context の行数も変わる。ハイライトキャッシュキーに含めないと
                    // 古い syntect 行配列が再利用され、行番号オフセットと表示テキストがずれる。
                    let snippet_cache_key = format!(
                        "table:{}:{match_idx}:{}:{}:{}:{}",
                        file.relative_path,
                        m.line_start,
                        m.col_start,
                        m.context_before.len(),
                        m.context_after.len(),
                    );
                    let snippet_matches = vec![context_match_item(m)];
                    let source_context_start_line = m.line_start.saturating_sub(m.context_before.len());
                    let snippet_highlighted = app.highlighter.highlight_source(
                        &snippet_cache_key,
                        &full_context,
                        file.source_language,
                    );
                    (
                        file.path.clone(),
                        file.relative_path.clone(),
                        m.line_start,
                        m.col_start,
                        m.matched_text.clone(),
                        recv_type_hint,
                        build_layout_job_from_line(
                            snippet_highlighted,
                            &snippet_matches,
                            13.0,
                            source_context_start_line,
                        ),
                        full_context,
                        file.matches.clone(),
                        file.source_language,
                        file.text_encoding.clone(),
                    )
                };

                let recv_hint_cell: Option<(String, String)> = if show_recv_col {
                    Some(
                        match recv_type_hint.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
                            None => (
                                "—".to_string(),
                                t.table_recv_hint_none_tooltip().to_string(),
                            ),
                            Some(s) => {
                                let display = if s.chars().count() > 28 {
                                    format!("{}…", s.chars().take(25).collect::<String>())
                                } else {
                                    s.to_string()
                                };
                                (display, s.to_string())
                            }
                        },
                    )
                } else {
                    None
                };

                let file_label = ellipsis_path_tail(&relative_path, 40, 37);
                let row_height = app
                    .table_row_units
                    .get(row_idx)
                    .copied()
                    .unwrap_or(1) as f32
                    * row_unit_height;

                let row_bg = if row_idx % 2 == 0 {
                    egui::Color32::TRANSPARENT
                } else {
                    ui.visuals().faint_bg_color
                };

                egui::Frame::default().fill(row_bg).show(ui, |ui| {
                    ui.set_min_width(total_w);
                    ui.set_min_height(row_height);
                    ui.horizontal(|ui| {
                        let r_file =
                            label_cell(ui, FILE_W, file_label, Sense::click()).on_hover_text(&relative_path);
                        let r_line = label_cell(ui, LINE_W, line_start.to_string(), Sense::click());
                        let r_col = label_cell(ui, COL_W, col_start.to_string(), Sense::click());
                        let r_matched = left_aligned_text_cell(
                            ui,
                            MATCH_W,
                            row_height,
                            matched_text.as_str(),
                            FontId::monospace(ui.text_style_height(&egui::TextStyle::Body) - 2.0),
                            Color32::from_rgb(220, 200, 100),
                            Sense::click(),
                        )
                        .on_hover_text(&matched_text);
                        let r_src = left_aligned_layout_job_cell(
                            ui,
                            SOURCE_W,
                            row_height,
                            source_context_job,
                            Color32::from_rgb(180, 190, 210),
                            Sense::click(),
                        )
                            .on_hover_text(&full_context);

                        let r_recv_hint = recv_hint_cell.as_ref().map(|(disp, hov)| {
                            label_cell(ui, RECV_HINT_W, disp.as_str(), Sense::click())
                                .on_hover_text(hov.as_str())
                        });

                        let r_assist = ui
                            .add_sized([ACTION_W, row_unit_height], egui::Button::new(t.to_assist()).small())
                            .on_hover_text(t.to_assist_tooltip());

                        if r_assist.clicked() {
                            send_to_assist = Some(matched_text.clone());
                        }

                        let mut any_click = r_file.clicked()
                            || r_line.clicked()
                            || r_col.clicked()
                            || r_matched.clicked()
                            || r_src.clicked()
                            || r_assist.clicked();
                        if let Some(ref r) = r_recv_hint {
                            any_click |= r.clicked();
                        }
                        if any_click {
                            app.table_last_clicked_row = Some(row_idx);
                        }

                        let mut any_dbl = r_file.double_clicked()
                            || r_line.double_clicked()
                            || r_col.double_clicked()
                            || r_matched.double_clicked()
                            || r_src.double_clicked();
                        if let Some(ref r) = r_recv_hint {
                            any_dbl |= r.double_clicked();
                        }
                        if any_dbl {
                            open_table_preview = Some(TablePreviewState {
                                path,
                                relative_path,
                                line: line_start,
                                col: col_start,
                                matches,
                                source_language,
                                text_encoding,
                                pending_scroll_line: Some(line_start),
                            });
                        }
                    });
                });
            }

            let end_units = app.table_row_prefix_units.get(end).copied().unwrap_or(total_units);
            ui.add_space(total_units.saturating_sub(end_units) as f32 * row_unit_height);
        });
    scroll_keyboard::store_scroll_metrics(ui.ctx(), sid, &scroll_out, rect);

    app.table_scroll_to_row = None;

    // パターン支援へ転送
    if let Some(snippet) = send_to_assist {
        app.pending_pattern_assist_snippet = Some(snippet);
        app.show_pattern_assist = true;
    }

    if let Some(preview) = open_table_preview {
        app.table_preview = Some(preview);
    }
}
