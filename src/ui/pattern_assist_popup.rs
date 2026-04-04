use egui::Ui;

use crate::app::AstGrepApp;
use crate::pattern_assist::generate_patterns;

pub fn show(app: &mut AstGrepApp, ctx: &egui::Context) {
    // 検索結果から転送されたスニペットがあれば自動的に反映
    if let Some(snippet) = app.pending_pattern_assist_snippet.take() {
        app.pattern_assist_snippet = snippet;
        app.pattern_assist_results.clear();
    }

    if !app.show_pattern_assist {
        return;
    }

    let t = app.tr();
    let mut show = app.show_pattern_assist;
    let mut apply_pattern: Option<String> = None;

    egui::Window::new(t.pa_window_title())
        .open(&mut show)
        .resizable(true)
        .default_size([600.0, 500.0])
        .show(ctx, |ui| {
            pattern_assist_content(ui, app, &mut apply_pattern);
        });

    app.show_pattern_assist = show;

    if let Some(pat) = apply_pattern {
        app.pattern = pat;
        app.show_pattern_assist = false;
    }
}

fn pattern_assist_content(
    ui: &mut Ui,
    app: &mut AstGrepApp,
    apply_pattern: &mut Option<String>,
) {
    let t = app.tr();
    ui.label(
        egui::RichText::new(t.pa_intro())
            .small()
            .color(egui::Color32::GRAY),
    );
    ui.add_space(4.0);

    // スニペット入力エリア
    ui.label(t.pa_snippet_label());
    egui::ScrollArea::vertical()
        .id_salt("snippet_scroll")
        .max_height(160.0)
        .show(ui, |ui| {
            ui.add(
                egui::TextEdit::multiline(&mut app.pattern_assist_snippet)
                    .desired_width(f32::INFINITY)
                    .desired_rows(6)
                    .font(egui::TextStyle::Monospace)
                    .hint_text(t.pa_snippet_hint()),
            );
        });

    ui.add_space(4.0);

    ui.horizontal(|ui| {
        let can_generate = !app.pattern_assist_snippet.trim().is_empty();

        if ui
            .add_enabled(can_generate, egui::Button::new(t.pa_generate()))
            .on_hover_text(t.pa_generate_tooltip())
            .clicked()
        {
            app.pattern_assist_results = generate_patterns(
                &app.pattern_assist_snippet,
                app.pattern_assist_resolve_lang(),
                app.ui_lang(),
            );
        }

        if ui.button(t.pa_clear()).clicked() {
            app.pattern_assist_snippet.clear();
            app.pattern_assist_results.clear();
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(t.pa_lang_line(
                    &app.pattern_assist_resolve_lang().combo_label(app.ui_lang()),
                ))
                    .small()
                    .color(egui::Color32::GRAY),
            );
        });
    });

    ui.separator();

    if app.pattern_assist_results.is_empty() && app.pattern_assist_snippet.trim().is_empty() {
        return;
    }

    if app.pattern_assist_results.is_empty() {
        ui.label(
            egui::RichText::new(t.pa_no_candidates())
                .color(egui::Color32::from_rgb(200, 80, 80)),
        );
        return;
    }

    ui.label(
        egui::RichText::new(t.pa_candidates_count(app.pattern_assist_results.len()))
            .small()
            .color(egui::Color32::GRAY),
    );
    ui.add_space(4.0);

    egui::ScrollArea::vertical()
        .id_salt("pattern_results")
        .show(ui, |ui| {
            let available_width = ui.available_width().max(400.0);
            let count_width = 60.0;
            let action_width = 120.0;
            let desc_width = (available_width * 0.22).clamp(120.0, 220.0);
            let pattern_width =
                (available_width - count_width - action_width - desc_width - 32.0).max(180.0);

            egui::Grid::new("pattern_grid")
                .num_columns(4)
                .striped(true)
                .spacing([8.0, 4.0])
                .show(ui, |ui| {
                    // ヘッダー
                    ui.add_sized(
                        [pattern_width, 0.0],
                        egui::Label::new(egui::RichText::new(t.pa_col_pattern()).strong()).wrap(),
                    );
                    ui.add_sized(
                        [desc_width, 0.0],
                        egui::Label::new(egui::RichText::new(t.pa_col_desc()).strong()).wrap(),
                    );
                    ui.add_sized(
                        [count_width, 0.0],
                        egui::Label::new(egui::RichText::new(t.pa_col_count()).strong()).wrap(),
                    );
                    ui.add_sized(
                        [action_width, 0.0],
                        egui::Label::new(egui::RichText::new(t.pa_col_action()).strong()).wrap(),
                    );
                    ui.end_row();

                    for suggestion in &app.pattern_assist_results {
                        // パターン（クリックでコピー）
                        let pat_response = ui.add_sized(
                            [pattern_width, 0.0],
                            egui::Label::new(
                                egui::RichText::new(&suggestion.pattern)
                                    .monospace()
                                    .color(egui::Color32::from_rgb(180, 220, 120)),
                            )
                            .wrap()
                            .selectable(true),
                        );
                        pat_response.on_hover_text(t.pa_pat_hover());

                        // 説明
                        ui.add_sized(
                            [desc_width, 0.0],
                            egui::Label::new(
                                egui::RichText::new(&suggestion.description)
                                    .small()
                                    .color(egui::Color32::GRAY),
                            )
                            .wrap(),
                        );

                        // マッチ数
                        ui.add_sized(
                            [count_width, 0.0],
                            egui::Label::new(
                                egui::RichText::new(format!("{}", suggestion.match_count))
                                    .color(egui::Color32::from_rgb(100, 200, 100)),
                            )
                            .wrap(),
                        );

                        // 操作ボタン
                        ui.allocate_ui_with_layout(
                            egui::vec2(action_width, 0.0),
                            egui::Layout::left_to_right(egui::Align::Min),
                            |ui| {
                                if ui
                                    .button(t.pa_apply())
                                    .on_hover_text(t.pa_apply_tooltip())
                                    .clicked()
                                {
                                    *apply_pattern = Some(suggestion.pattern.clone());
                                }
                                if ui
                                    .button(t.pa_copy())
                                    .on_hover_text(t.pa_copy_tooltip())
                                    .clicked()
                                {
                                    if let Ok(mut cb) = arboard::Clipboard::new() {
                                        let _ = cb.set_text(&suggestion.pattern);
                                    }
                                }
                            },
                        );

                        ui.end_row();
                    }
                });
        });
}
