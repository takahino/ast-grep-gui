use egui::Ui;

use crate::app::AstGrepApp;
use crate::export::file_to_ast_grep_console;
use crate::file_encoding::read_text_file_as;
use crate::highlight::build_layout_job;
use crate::search::SearchMode;

pub fn show(app: &mut AstGrepApp, ui: &mut Ui) {
    let t = app.tr();
    let Some(idx) = app.selected_file_idx else {
        ui.centered_and_justified(|ui| {
            ui.label(
                egui::RichText::new(t.code_select_file())
                    .color(egui::Color32::GRAY),
            );
        });
        return;
    };

    let Some(file_result) = app.results.get(idx) else {
        return;
    };

    let path = file_result.path.clone();
    let relative_path = file_result.relative_path.clone();
    let matches = file_result.matches.clone();
    let lang = file_result.source_language;
    let text_encoding = file_result.text_encoding.clone();

    // ファイル内容を読み込む
    let source = match read_text_file_as(&path, text_encoding) {
        Ok(s) => s,
        Err(e) => {
            ui.label(t.code_read_error_fmt(e));
            return;
        }
    };

    // ヘッダー行：ファイル名とパターン支援への連携ボタン
    ui.horizontal(|ui| {
        ui.heading(&relative_path);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(t.code_match_count(matches.len()))
                    .small()
                    .color(egui::Color32::from_rgb(100, 200, 100)),
            );
        });
    });
    ui.label(
        egui::RichText::new(file_result.text_encoding.detail_text(app.ui_lang()))
            .small()
            .color(egui::Color32::GRAY),
    );

    if app.search_mode == SearchMode::AstGrepRaw {
        ui.separator();
        let mut console_text = file_to_ast_grep_console(file_result);
        egui::ScrollArea::both()
            .id_salt("ast_grep_raw_console")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut console_text)
                        .font(egui::TextStyle::Monospace)
                        .desired_width(f32::INFINITY)
                        .interactive(false),
                );
            });
        return;
    }

    // マッチ一覧（コンパクト表示）：各マッチに「→支援」ボタン
    if !matches.is_empty() {
        egui::CollapsingHeader::new(
            egui::RichText::new(t.code_match_list_header(matches.len()))
                .small()
                .color(egui::Color32::GRAY),
        )
        .default_open(false)
        .show(ui, |ui| {
            let mut send_to_assist: Option<String> = None;
            egui::ScrollArea::vertical()
                .id_salt("match_list_panel")
                .max_height(120.0)
                .show(ui, |ui| {
                    for m in &matches {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format!("L{}:{}", m.line_start, m.col_start))
                                    .small()
                                    .monospace()
                                    .color(egui::Color32::GRAY),
                            );
                            let block = m.text_with_context();
                            let preview = block.lines().next().unwrap_or("").trim();
                            let short = if preview.len() > 60 {
                                format!("{}…", &preview[..57])
                            } else {
                                preview.to_string()
                            };
                            ui.label(
                                egui::RichText::new(&short)
                                    .small()
                                    .monospace()
                                    .color(egui::Color32::from_rgb(220, 200, 100)),
                            )
                            .on_hover_text(&block);
                            if ui
                                .small_button(t.to_assist())
                                .on_hover_text(t.to_assist_tooltip())
                                .clicked()
                            {
                                send_to_assist = Some(m.matched_text.clone());
                            }
                        });
                    }
                });
            if let Some(snippet) = send_to_assist {
                app.pending_pattern_assist_snippet = Some(snippet);
                app.show_pattern_assist = true;
            }
        });
    }

    ui.separator();

    // フォントサイズとそこから算出した1行の高さ
    const FONT_SIZE: f32 = 13.0;
    let line_height = ui.fonts(|f| f.row_height(&egui::FontId::monospace(FONT_SIZE)));

    // ハイライト処理
    let cache_key = relative_path.clone();
    let highlighted = app
        .highlighter
        .highlight_source(&cache_key, &source, lang)
        .clone();

    let job = build_layout_job(&highlighted, &matches, FONT_SIZE);

    // ジャンプ先のスクロールオフセットを計算（クリック時に一度だけ適用）
    let scroll_offset = app.pending_scroll_line.take().map(|line| {
        // line は 1-based。少し上に余白を持たせて表示する
        let target_line = line.saturating_sub(3) as f32;
        egui::Vec2::new(0.0, target_line * line_height)
    });

    let mut scroll = egui::ScrollArea::both()
        .id_salt("code_view")
        .auto_shrink([false, false]);

    if let Some(offset) = scroll_offset {
        scroll = scroll.scroll_offset(offset);
    }

    scroll.show(ui, |ui| {
        let galley = ui.fonts(|f| f.layout_job(job));
        ui.add(egui::Label::new(galley).selectable(true));
    });
}
