use egui::text::LayoutJob;
use egui::Ui;

use crate::app::AstGrepApp;
use crate::pattern_assist::generate_patterns;

/// `generate_patterns` と同じ trim 後座標の範囲を、表示中の全文 `full` 上のバイト範囲へ写す
fn map_ranges_trimmed_to_full(full: &str, ranges_in_trimmed: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let ts = full.len() - full.trim_start().len();
    let trimmed_len = full.trim().len();
    ranges_in_trimmed
        .iter()
        .filter_map(|&(a, b)| {
            let a = a.min(trimmed_len);
            let b = b.min(trimmed_len);
            if a >= b {
                return None;
            }
            Some((ts + a, ts + b))
        })
        .collect()
}

/// マッチごとに色を変えて背景を付けた LayoutJob（バイト範囲は `text` 上）
fn snippet_highlight_layout_job(
    ui: &Ui,
    text: &str,
    ranges_full: &[(usize, usize)],
) -> LayoutJob {
    const PALETTE: [egui::Color32; 8] = [
        egui::Color32::from_rgb(200, 80, 80),
        egui::Color32::from_rgb(80, 160, 220),
        egui::Color32::from_rgb(120, 200, 80),
        egui::Color32::from_rgb(220, 160, 60),
        egui::Color32::from_rgb(180, 100, 200),
        egui::Color32::from_rgb(80, 200, 180),
        egui::Color32::from_rgb(220, 100, 140),
        egui::Color32::from_rgb(140, 140, 220),
    ];

    let font_id = ui
        .style()
        .text_styles
        .get(&egui::TextStyle::Monospace)
        .cloned()
        .unwrap_or_else(|| egui::FontId::monospace(13.0));
    let base_color = ui.visuals().text_color();

    let mut job = LayoutJob::default();
    if ranges_full.is_empty() || text.is_empty() {
        job.append(
            text,
            0.0,
            egui::TextFormat {
                font_id,
                color: base_color,
                ..Default::default()
            },
        );
        return job;
    }

    let mut items: Vec<(usize, usize, usize)> = ranges_full
        .iter()
        .enumerate()
        .map(|(i, &(s, e))| (s.min(e), s.max(e), i))
        .filter(|(s, e, _)| s < e)
        .collect();
    items.sort_by_key(|(s, _, _)| *s);

    let mut pos = 0usize;
    for (s, e, ri) in items {
        let s = s.min(text.len());
        let e = e.min(text.len());
        if s >= e {
            continue;
        }
        let s = s.max(pos);
        if s >= e {
            continue;
        }
        if pos < s {
            job.append(
                &text[pos..s],
                0.0,
                egui::TextFormat {
                    font_id: font_id.clone(),
                    color: base_color,
                    ..Default::default()
                },
            );
        }
        let col = PALETTE[ri % PALETTE.len()];
        let bg = egui::Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), 110);
        job.append(
            &text[s..e],
            0.0,
            egui::TextFormat {
                font_id: font_id.clone(),
                color: base_color,
                background: bg,
                ..Default::default()
            },
        );
        pos = e.max(pos);
    }
    if pos < text.len() {
        job.append(
            &text[pos..],
            0.0,
            egui::TextFormat {
                font_id,
                color: base_color,
                ..Default::default()
            },
        );
    }
    job
}

fn toggle_pattern_assist_row_selection(app: &mut AstGrepApp, row: usize) {
    app.pattern_assist_selected_row = if app.pattern_assist_selected_row == Some(row) {
        None
    } else {
        Some(row)
    };
}

/// スニペットから候補を生成（空なら結果をクリア）
fn run_pattern_generate(app: &mut AstGrepApp) {
    app.pattern_assist_selected_row = None;
    if app.pattern_assist_snippet.trim().is_empty() {
        app.pattern_assist_results.clear();
        return;
    }
    app.pattern_assist_results = generate_patterns(
        &app.pattern_assist_snippet,
        app.pattern_assist_resolve_lang(),
        app.ui_lang(),
    );
}

pub fn show(app: &mut AstGrepApp, ctx: &egui::Context) {
    // 検索結果・表などから転送されたスニペットがあれば反映し、パターン生成まで実行
    if let Some(snippet) = app.pending_pattern_assist_snippet.take() {
        app.pattern_assist_snippet = snippet;
        run_pattern_generate(app);
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
        .constrain_to(ctx.screen_rect())
        .show(ctx, |ui| {
            pattern_assist_content(ui, app, &mut apply_pattern);
        });

    app.show_pattern_assist = show;
    if !show {
        app.pattern_assist_selected_row = None;
    }

    if let Some(pat) = apply_pattern {
        app.pattern = pat;
        app.show_pattern_assist = false;
        app.pattern_assist_selected_row = None;
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

    // スニペット入力エリア（選択行に応じてマッチ箇所をハイライト）
    ui.label(t.pa_snippet_label());
    let selected_for_snippet = app.pattern_assist_selected_row.and_then(|idx| {
        app.pattern_assist_results
            .get(idx)
            .filter(|s| !s.match_ranges.is_empty())
    });
    egui::ScrollArea::vertical()
        .id_salt("snippet_scroll")
        .max_height(160.0)
        .show(ui, |ui| {
            if let Some(suggestion) = selected_for_snippet {
                let mapped =
                    map_ranges_trimmed_to_full(&app.pattern_assist_snippet, &suggestion.match_ranges);
                let job = snippet_highlight_layout_job(ui, &app.pattern_assist_snippet, &mapped);
                ui.add(egui::Label::new(job).wrap());
            } else {
                ui.add(
                    egui::TextEdit::multiline(&mut app.pattern_assist_snippet)
                        .desired_width(f32::INFINITY)
                        .desired_rows(6)
                        .font(egui::TextStyle::Monospace)
                        .hint_text(t.pa_snippet_hint()),
                );
            }
        });

    ui.add_space(4.0);

    ui.horizontal(|ui| {
        let can_generate = !app.pattern_assist_snippet.trim().is_empty();

        if ui
            .add_enabled(can_generate, egui::Button::new(t.pa_generate()))
            .on_hover_text(t.pa_generate_tooltip())
            .clicked()
        {
            run_pattern_generate(app);
        }

        if ui.button(t.pa_clear()).clicked() {
            app.pattern_assist_snippet.clear();
            app.pattern_assist_results.clear();
            app.pattern_assist_selected_row = None;
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

            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.set_min_width(available_width);
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
                });
                ui.separator();

                let n_rows = app.pattern_assist_results.len();
                for i in 0..n_rows {
                    let pattern_str = app.pattern_assist_results[i].pattern.clone();
                    let description = app.pattern_assist_results[i].description.clone();
                    let match_count = app.pattern_assist_results[i].match_count;
                    let selected = app.pattern_assist_selected_row == Some(i);
                    ui.horizontal(|ui| {
                        ui.set_min_width(available_width);
                        let pat_r = ui
                            .add_sized(
                                [pattern_width, 0.0],
                                egui::SelectableLabel::new(
                                    selected,
                                    egui::RichText::new(&pattern_str)
                                        .monospace()
                                        .color(egui::Color32::from_rgb(180, 220, 120)),
                                ),
                            )
                            .on_hover_text(t.pa_pat_hover());

                        let desc_r = ui.add_sized(
                            [desc_width, 0.0],
                            egui::SelectableLabel::new(
                                selected,
                                egui::RichText::new(&description)
                                    .small()
                                    .color(egui::Color32::GRAY),
                            ),
                        );

                        let count_r = ui.add_sized(
                            [count_width, 0.0],
                            egui::SelectableLabel::new(
                                selected,
                                egui::RichText::new(format!("{}", match_count))
                                    .color(egui::Color32::from_rgb(100, 200, 100)),
                            ),
                        );

                        if pat_r.clicked() || desc_r.clicked() || count_r.clicked() {
                            toggle_pattern_assist_row_selection(app, i);
                        }

                        ui.allocate_ui_with_layout(
                            egui::vec2(action_width, 0.0),
                            egui::Layout::left_to_right(egui::Align::Min),
                            |ui| {
                                if ui
                                    .button(t.pa_apply())
                                    .on_hover_text(t.pa_apply_tooltip())
                                    .clicked()
                                {
                                    *apply_pattern = Some(pattern_str.clone());
                                }
                                if ui
                                    .button(t.pa_copy())
                                    .on_hover_text(t.pa_copy_tooltip())
                                    .clicked()
                                {
                                    if let Ok(mut cb) = arboard::Clipboard::new() {
                                        let _ = cb.set_text(&pattern_str);
                                    }
                                }
                            },
                        );
                    });
                }
            });
        });
}
