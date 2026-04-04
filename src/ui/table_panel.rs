use egui::{Sense, Ui};

use crate::app::{AstGrepApp, TablePreviewState};

/// 長いパスは先頭を「...」で省略し末尾のみ表示する（UTF-8 の文字境界で切る）。
fn ellipsis_path_tail(path: &str, max_chars: usize, tail_chars: usize) -> String {
    let n = path.chars().count();
    if n <= max_chars {
        return path.to_string();
    }
    let skip = n.saturating_sub(tail_chars);
    format!("...{}", path.chars().skip(skip).collect::<String>())
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

    // ヘッダー行
    egui::ScrollArea::both()
        .id_salt("table_view")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Grid::new("result_grid")
                .num_columns(6)
                .striped(true)
                .spacing([12.0, 4.0])
                .min_col_width(60.0)
                .show(ui, |ui| {
                    // ヘッダー
                    ui.label(egui::RichText::new(t.table_col_file()).strong());
                    ui.label(egui::RichText::new(t.table_col_line()).strong());
                    ui.label(egui::RichText::new(t.table_col_col()).strong());
                    ui.label(egui::RichText::new(t.table_col_text()).strong());
                    ui.label(egui::RichText::new(t.table_col_source_context()).strong());
                    ui.label(egui::RichText::new(t.table_col_action()).strong());
                    ui.end_row();

                    // 各マッチを行として表示
                    let mut row_idx = 0usize;
                    for file in &app.results {
                        for m in &file.matches {
                            let context_body = m.program_with_context();

                            // ファイル名（長い場合は省略）
                            let file_label =
                                ellipsis_path_tail(&file.relative_path, 40, 37);
                            let r_file = ui
                                .add(egui::Label::new(&file_label).sense(Sense::click()))
                                .on_hover_text(&file.relative_path);

                            let r_line = ui.add(
                                egui::Label::new(format!("{}", m.line_start)).sense(Sense::click()),
                            );
                            let r_col = ui.add(
                                egui::Label::new(format!("{}", m.col_start)).sense(Sense::click()),
                            );

                            // ast-grep と同じマッチ範囲（ノード／部分一致の原文）
                            let matched = &m.matched_text;
                            let r_matched = ui
                                .add(
                                    egui::Label::new(
                                        egui::RichText::new(matched)
                                            .monospace()
                                            .color(egui::Color32::from_rgb(220, 200, 100)),
                                    )
                                    .extend()
                                    .selectable(true),
                                )
                                .on_hover_text(matched);

                            // 該当行の全文＋前後コンテキスト（検索の context_lines 設定に従う）
                            let r_src = ui
                                .add(
                                    egui::Label::new(
                                        egui::RichText::new(&context_body)
                                            .monospace()
                                            .color(egui::Color32::from_rgb(180, 190, 210)),
                                    )
                                    .extend()
                                    .selectable(true),
                                )
                                .on_hover_text(&context_body);

                            // 操作ボタン
                            let r_assist = ui
                                .small_button(t.to_assist())
                                .on_hover_text(t.to_assist_tooltip());
                            if r_assist.clicked() {
                                send_to_assist = Some(m.matched_text.clone());
                            }

                            if r_file.clicked()
                                || r_line.clicked()
                                || r_col.clicked()
                                || r_matched.clicked()
                                || r_src.clicked()
                                || r_assist.clicked()
                            {
                                app.table_last_clicked_row = Some(row_idx);
                            }
                            if app.table_scroll_to_row == Some(row_idx) {
                                let row_rect = r_file
                                    .rect
                                    .union(r_line.rect)
                                    .union(r_col.rect)
                                    .union(r_matched.rect)
                                    .union(r_src.rect)
                                    .union(r_assist.rect);
                                ui.scroll_to_rect_animation(
                                    row_rect,
                                    Some(egui::Align::Center),
                                    egui::style::ScrollAnimation::none(),
                                );
                            }

                            if r_file.double_clicked()
                                || r_line.double_clicked()
                                || r_col.double_clicked()
                                || r_matched.double_clicked()
                                || r_src.double_clicked()
                            {
                                open_table_preview = Some(TablePreviewState {
                                    path: file.path.clone(),
                                    relative_path: file.relative_path.clone(),
                                    line: m.line_start,
                                    col: m.col_start,
                                    matches: file.matches.clone(),
                                    source_language: file.source_language,
                                    text_encoding: file.text_encoding.clone(),
                                    pending_scroll_line: Some(m.line_start),
                                });
                            }

                            row_idx += 1;
                            ui.end_row();
                        }
                    }
                });
        });

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
