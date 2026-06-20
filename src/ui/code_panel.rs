use egui::Ui;

use crate::app::{AstGrepApp, CodeViewPaneFocus};
use crate::highlight::{build_layout_job, build_layout_job_with_in_view_find};
use crate::search::CODE_VIEW_MAX_HIGHLIGHT_LINES;
use crate::ui::{code_layout, in_view_find, scroll_keyboard};

pub fn show(app: &mut AstGrepApp, ui: &mut Ui) {
    let t = app.tr();
    let Some(idx) = app.selected_file_idx else {
        ui.centered_and_justified(|ui| {
            ui.label(egui::RichText::new(t.code_select_file()).color(egui::Color32::GRAY));
        });
        return;
    };

    let Some(file_snapshot) = app.results.get(idx).cloned() else {
        return;
    };

    let path = file_snapshot.path.clone();
    let relative_path = file_snapshot.relative_path.clone();
    let matches = file_snapshot.matches.clone();
    let lang = file_snapshot.source_language;
    let text_encoding = file_snapshot.text_encoding.clone();

    // ファイル内容を読み込む（アプリ内キャッシュ経由）
    let source = match app.file_source_by_path(&path, text_encoding.clone()) {
        Some(s) => (*s).clone(),
        None => {
            ui.label(t.code_read_error_fmt("read failed"));
            return;
        }
    };

    let total_lines = source.lines().count();
    let (highlight_source, source_truncated) = if total_lines > CODE_VIEW_MAX_HIGHLIGHT_LINES {
        let truncated = source
            .lines()
            .take(CODE_VIEW_MAX_HIGHLIGHT_LINES)
            .collect::<Vec<_>>()
            .join("\n");
        (truncated, true)
    } else {
        (source.clone(), false)
    };

    if source_truncated {
        ui.label(
            egui::RichText::new(format!(
                "⚠ {} / {} 行のみ表示（メモリ節約）",
                CODE_VIEW_MAX_HIGHLIGHT_LINES, total_lines
            ))
            .small()
            .color(egui::Color32::from_rgb(220, 180, 80)),
        );
    }

    // ヘッダー行：ファイル名とパターン支援への連携ボタン
    let open_path = path.clone();
    let mut open_clicked = false;
    ui.horizontal(|ui| {
        ui.heading(&relative_path);
        if ui
            .button(t.open_file_btn())
            .on_hover_text(t.open_file_tooltip())
            .clicked()
        {
            open_clicked = true;
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(t.code_match_count(matches.len()))
                    .small()
                    .color(egui::Color32::from_rgb(100, 200, 100)),
            );
        });
    });
    if open_clicked {
        app.open_file_externally(&open_path);
    }
    ui.label(
        egui::RichText::new(text_encoding.detail_text(app.ui_lang()))
            .small()
            .color(egui::Color32::GRAY),
    );

    // マッチ一覧（コンパクト表示）：各マッチに「→パターン支援」ボタン
    if !matches.is_empty() {
        egui::CollapsingHeader::new(
            egui::RichText::new(t.code_match_list_header(matches.len()))
                .small()
                .color(egui::Color32::GRAY),
        )
        .default_open(false)
        .show(ui, |ui| {
            let mut send_to_assist: Option<String> = None;
            let column_keys = app.type_hint_column_keys_cached().to_vec();
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
                            let block =
                                m.program_with_context_for_file(&file_snapshot, app.context_lines);
                            let hover = if column_keys.is_empty() {
                                block.clone()
                            } else {
                                let lines: Vec<String> = column_keys
                                    .iter()
                                    .map(|key| {
                                        let v = m.type_hint_cell(key).to_export_string();
                                        format!("${}: {}", key, v)
                                    })
                                    .collect();
                                format!("{}\n\n{}", lines.join("\n"), block)
                            };
                            let preview = block.lines().next().unwrap_or("").trim();
                            let short = if preview.chars().count() > 60 {
                                format!("{}…", preview.chars().take(57).collect::<String>())
                            } else {
                                preview.to_string()
                            };
                            ui.label(
                                egui::RichText::new(&short)
                                    .small()
                                    .monospace()
                                    .color(egui::Color32::from_rgb(220, 200, 100)),
                            )
                            .on_hover_text(&hover);
                            if ui
                                .small_button(t.to_assist())
                                .on_hover_text(t.to_assist_tooltip())
                                .clicked()
                            {
                                send_to_assist = Some(if !m.matched_text.is_empty() {
                                    m.matched_text.clone()
                                } else {
                                    m.matched_text_for_file(&file_snapshot)
                                });
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

    if app.table_preview.is_none() {
        in_view_find::show_bar_code(app, ui, source.as_str());
        ui.add_space(4.0);
    }

    // フォントサイズとそこから算出した1行の高さ
    const FONT_SIZE: f32 = 13.0;
    let line_height = ui.fonts(|f| f.row_height(&egui::FontId::monospace(FONT_SIZE)));

    // ハイライト処理
    let cache_key = relative_path.clone();
    let highlighted = app
        .highlighter
        .highlight_source(&cache_key, &highlight_source, lang)
        .clone();

    let jobs = if app.in_view_find.open && !app.in_view_find.query.is_empty() {
        let spans = in_view_find::find_byte_spans(
            source.as_str(),
            &app.in_view_find.query,
            app.in_view_find.case_sensitive,
        );
        build_layout_job_with_in_view_find(
            &highlighted,
            &matches,
            FONT_SIZE,
            1,
            highlight_source.as_str(),
            &spans,
            app.in_view_find.current,
        )
    } else {
        build_layout_job(&highlighted, &matches, FONT_SIZE)
    };

    // ジャンプ先のスクロールオフセットを計算（クリック時に一度だけ適用）
    let scroll_offset = app.pending_scroll_line.take().map(|line| {
        // line は 1-based。少し上に余白を持たせて表示する
        let target_line = line.saturating_sub(3) as f32;
        egui::Vec2::new(0.0, target_line * line_height)
    });

    let sid = scroll_keyboard::scroll_area_persistent_id(ui, "code_view");
    let rect = ui.available_rect_before_wrap();
    let pointer_on_code = ui.rect_contains_pointer(rect);
    let pointer_on_list = app.code_view_pointer_on_list;
    let allow_code_scroll = pointer_on_code
        || (matches!(app.code_view_pane_focus, CodeViewPaneFocus::Code) && !pointer_on_list);
    scroll_keyboard::apply_keyboard_scroll_before_show(
        ui.ctx(),
        ui,
        sid,
        rect,
        egui::Vec2b::from([true, true]),
        allow_code_scroll,
    );

    let mut scroll = egui::ScrollArea::both()
        .id_salt("code_view")
        .auto_shrink([false, false]);

    if let Some(offset) = scroll_offset {
        scroll = scroll.scroll_offset(offset);
    }

    let scroll_out = scroll.show(ui, |ui| {
        let label = code_layout::show_selectable_code(ui, jobs);
        if label.clicked() {
            app.code_view_pane_focus = CodeViewPaneFocus::Code;
        }
    });
    scroll_keyboard::store_scroll_metrics(ui.ctx(), sid, &scroll_out, rect);
    app.code_view_pointer_on_code = ui.rect_contains_pointer(scroll_out.inner_rect);
}
