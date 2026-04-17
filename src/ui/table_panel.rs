use egui::text::LayoutJob;
use egui::{Align, Color32, FontId, Label, Rect, RichText, Sense, Ui, Vec2};

use crate::app::{AstGrepApp, TableColumnWidths, TablePreviewState, TableRowRef};
use crate::highlight::build_layout_job_from_line;
use crate::search::{type_hint_column_keys, MatchItem, TypeHintCell, UnknownHintDetail};
use crate::ui::scroll_keyboard;

/// 列間のドラッグ用（Excel の境界に相当）
const RESIZE_HANDLE_W: f32 = 6.0;
const MIN_COL_WIDTH: f32 = 40.0;

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
        type_hints: m.type_hints.clone(),
    }
}

fn flat_widths(w: &TableColumnWidths, n_hints: usize) -> Vec<f32> {
    let mut v = vec![w.file, w.line, w.col, w.matched, w.source];
    v.extend_from_slice(&w.hint_cols[..n_hints]);
    v.push(w.action);
    v
}

fn apply_flat_to_widths(flat: &[f32], w: &mut TableColumnWidths, n_hints: usize) {
    debug_assert_eq!(flat.len(), 6 + n_hints);
    w.file = flat[0];
    w.line = flat[1];
    w.col = flat[2];
    w.matched = flat[3];
    w.source = flat[4];
    w.hint_cols.clear();
    w.hint_cols.extend_from_slice(&flat[5..5 + n_hints]);
    w.action = flat[5 + n_hints];
}

fn total_width_from_flat(flat: &[f32]) -> f32 {
    let n = flat.len();
    let handles = if n > 1 { n - 1 } else { 0 };
    flat.iter().sum::<f32>() + handles as f32 * RESIZE_HANDLE_W
}

fn column_resize_handle(ui: &mut Ui, row_height: f32, left_idx: usize, widths: &mut [f32]) {
    let (_, response) =
        ui.allocate_exact_size(Vec2::new(RESIZE_HANDLE_W, row_height), Sense::drag());
    let dx = response.drag_delta().x;
    if dx != 0.0 {
        widths[left_idx] = (widths[left_idx] + dx).max(MIN_COL_WIDTH);
    }
    if response.hovered() || response.dragged() {
        ui.painter().rect_filled(
            response.rect,
            0.0,
            ui.visuals().widgets.hovered.bg_fill.gamma_multiply(0.35),
        );
    }
}

fn resize_spacer(ui: &mut Ui, row_height: f32) {
    ui.allocate_exact_size(Vec2::new(RESIZE_HANDLE_W, row_height), Sense::hover());
}

fn header_label(i: usize, n_hints: usize, keys: &[String], t: crate::i18n::Tr) -> RichText {
    let last = 5 + n_hints;
    match i {
        0 => RichText::new(t.table_col_file()).strong(),
        1 => RichText::new(t.table_col_line()).strong(),
        2 => RichText::new(t.table_col_col()).strong(),
        3 => RichText::new(t.table_col_text()).strong(),
        4 => RichText::new(t.table_col_source_context()).strong(),
        x if x >= 5 && x < 5 + n_hints => RichText::new(format!("${}", keys[x - 5])).strong(),
        x if x == last => RichText::new(t.table_col_action()).strong(),
        _ => RichText::new("").strong(),
    }
}

fn render_header_row(
    ui: &mut Ui,
    t: crate::i18n::Tr,
    column_keys: &[String],
    flat: &mut [f32],
    header_h: f32,
) {
    let n_hints = column_keys.len();
    ui.horizontal(|ui| {
        for i in 0..flat.len() {
            ui.add_sized(
                [flat[i], header_h],
                Label::new(header_label(i, n_hints, column_keys, t)).truncate(),
            );
            if i + 1 < flat.len() {
                column_resize_handle(ui, header_h, i, flat);
            }
        }
    });
}

/// 表セル幅向けに推論失敗表示を切り詰める（ホバーで全文）。
fn unknown_hint_table_display(d: &UnknownHintDetail) -> String {
    let full = if d.source_snippet.is_empty() {
        format!("? ({})", d.kind_label)
    } else {
        format!("? ({}) ({})", d.kind_label, d.source_snippet)
    };
    const MAX: usize = 72;
    if full.chars().count() <= MAX {
        full
    } else {
        format!("{}…", full.chars().take(MAX.saturating_sub(1)).collect::<String>())
    }
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

    let column_keys = type_hint_column_keys(app.pattern.as_str(), &app.results);
    let n_hints = column_keys.len();
    app.table_column_widths.sync_hint_cols(n_hints);

    let mut flat = flat_widths(&app.table_column_widths, n_hints);

    let row_unit_height = ui.text_style_height(&egui::TextStyle::Body).max(ui.spacing().interact_size.y);
    let header_h = row_unit_height.max(ui.spacing().interact_size.y);

    let table_interact_rect = ui.available_rect_before_wrap();

    let ctx_table = ui.ctx().clone();
    let sid_h = scroll_keyboard::scroll_area_persistent_id(ui, "table_view_h");
    scroll_keyboard::apply_keyboard_horizontal_scroll_before_show(
        &ctx_table,
        ui,
        sid_h,
        table_interact_rect,
        true,
        false,
    );

    let scroll_h_out = egui::ScrollArea::horizontal()
        .id_salt("table_view_h")
        .max_height(table_interact_rect.height())
        .auto_shrink([false, false])
        .show(ui, |ui_h| {
            let ctx = ui_h.ctx().clone();
            ui_h.vertical(|ui_v| {
                let sid = scroll_keyboard::scroll_area_persistent_id(ui_v, "table_view");
                scroll_keyboard::apply_keyboard_scroll_before_show(
                    &ctx,
                    ui_v,
                    sid,
                    table_interact_rect,
                    egui::Vec2b::from([true, true]),
                    false,
                );

                render_header_row(ui_v, t, &column_keys, &mut flat, header_h);
                apply_flat_to_widths(&flat, &mut app.table_column_widths, n_hints);
                let total_w = total_width_from_flat(&flat);

                ui_v.separator();

                let scroll_out = egui::ScrollArea::vertical()
                    .id_salt("table_view")
                    .min_scrolled_height(8.0)
                    .max_height(ui_v.available_height())
                    .auto_shrink([false, false])
                    .show_viewport(ui_v, |ui, viewport| {
                        ui.set_min_width(total_w);
                        let row_count = app.table_rows.len();
                        let content_top = 0.0_f32;
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
                        let start = row_index_for_unit_offset(
                            &app.table_row_prefix_units,
                            row_count,
                            viewport_min_units,
                        )
                        .min(row_count);
                        let end = (row_index_for_unit_offset(
                            &app.table_row_prefix_units,
                            row_count,
                            viewport_max_units.saturating_add(1),
                        ) + 2)
                            .min(row_count);

                        let start_units = app.table_row_prefix_units.get(start).copied().unwrap_or(0);
                        ui.add_space(start_units as f32 * row_unit_height);

                        let file_w = flat[0];
                        let line_w = flat[1];
                        let col_w = flat[2];
                        let match_w = flat[3];
                        let source_w = flat[4];
                        let action_w = flat[5 + n_hints];

                        for row_idx in start..end {
                            let TableRowRef { file_idx, match_idx } = app.table_rows[row_idx];
                            let file = &app.results[file_idx];
                            let m = &file.matches[match_idx];
                            let (
                                path,
                                relative_path,
                                line_start,
                                col_start,
                                matched_text,
                                source_context_job,
                                full_context,
                                matches,
                                source_language,
                                text_encoding,
                            ) = {
                                let full_context = m.program_with_context();
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

                            let hint_cells: Vec<(RichText, String)> = column_keys
                                .iter()
                                .map(|key| {
                                    match m.type_hint_cell(key) {
                                        TypeHintCell::Inferred(s) => {
                                            let display = if s.chars().count() > 28 {
                                                format!("{}…", s.chars().take(25).collect::<String>())
                                            } else {
                                                s.clone()
                                            };
                                            (
                                                RichText::new(display)
                                                    .monospace()
                                                    .color(Color32::from_rgb(220, 200, 100)),
                                                s,
                                            )
                                        }
                                        TypeHintCell::NoSlot => (
                                            RichText::new("·")
                                                .monospace()
                                                .color(Color32::from_rgb(130, 135, 150)),
                                            t.table_type_hint_no_slot_tooltip(key),
                                        ),
                                        TypeHintCell::Unknown(detail) => {
                                            let q = Color32::from_rgb(200, 145, 90);
                                            let (rt, hover_body) = match &detail {
                                                None => (
                                                    RichText::new("?").monospace().color(q),
                                                    String::new(),
                                                ),
                                                Some(d) => {
                                                    let display = unknown_hint_table_display(d);
                                                    let full = TypeHintCell::Unknown(Some(d.clone()))
                                                        .to_export_string();
                                                    (
                                                        RichText::new(display)
                                                            .monospace()
                                                            .color(q),
                                                        full,
                                                    )
                                                }
                                            };
                                            let tip_base = if key.ends_with("#arity") {
                                                t.table_type_hint_arity_empty_tooltip().to_string()
                                            } else {
                                                t.table_type_hint_column_empty_tooltip(key)
                                            };
                                            let tip = if hover_body.is_empty() {
                                                tip_base
                                            } else {
                                                format!("{tip_base}\n\n{hover_body}")
                                            };
                                            (rt, tip)
                                        }
                                    }
                                })
                                .collect();

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
                                    let r_file = label_cell(ui, file_w, file_label, Sense::click())
                                        .on_hover_text(&relative_path);
                                    resize_spacer(ui, row_height);
                                    let r_line =
                                        label_cell(ui, line_w, line_start.to_string(), Sense::click());
                                    resize_spacer(ui, row_height);
                                    let r_col =
                                        label_cell(ui, col_w, col_start.to_string(), Sense::click());
                                    resize_spacer(ui, row_height);
                                    let r_matched = left_aligned_text_cell(
                                        ui,
                                        match_w,
                                        row_height,
                                        matched_text.as_str(),
                                        FontId::monospace(
                                            ui.text_style_height(&egui::TextStyle::Body) - 2.0,
                                        ),
                                        Color32::from_rgb(220, 200, 100),
                                        Sense::click(),
                                    )
                                    .on_hover_text(&matched_text);
                                    resize_spacer(ui, row_height);
                                    let r_src = left_aligned_layout_job_cell(
                                        ui,
                                        source_w,
                                        row_height,
                                        source_context_job,
                                        Color32::from_rgb(180, 190, 210),
                                        Sense::click(),
                                    )
                                    .on_hover_text(&full_context);
                                    resize_spacer(ui, row_height);

                                    let mut r_hint_cols: Vec<egui::Response> = Vec::new();
                                    for (hi, (disp, hov)) in hint_cells.iter().enumerate() {
                                        let hw = flat[5 + hi];
                                        r_hint_cols.push(
                                            label_cell(ui, hw, disp.clone(), Sense::click())
                                                .on_hover_text(hov.as_str()),
                                        );
                                        if hi + 1 < hint_cells.len() {
                                            resize_spacer(ui, row_height);
                                        }
                                    }
                                    if !hint_cells.is_empty() {
                                        resize_spacer(ui, row_height);
                                    }

                                    let r_assist = ui
                                        .add_sized(
                                            [action_w, row_unit_height],
                                            egui::Button::new(t.to_assist()).small(),
                                        )
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
                                    for r in &r_hint_cols {
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
                                    for r in &r_hint_cols {
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

                scroll_keyboard::store_scroll_metrics(&ctx, sid, &scroll_out, table_interact_rect);
            });
        });

    scroll_keyboard::store_horizontal_scroll_metrics(&ctx_table, sid_h, &scroll_h_out, table_interact_rect);

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
