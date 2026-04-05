use std::collections::BTreeMap;

use egui::Ui;

use crate::app::{AstGrepApp, SearchState, ViewMode};
use crate::file_encoding::FileEncodingPreference;
use crate::i18n::UiLanguagePreference;
use crate::lang::SupportedLanguage;
use crate::search::SearchMode;

pub fn show(app: &mut AstGrepApp, ui: &mut Ui) {
    let t = app.tr();
    let eff = app.ui_language_preference.effective();

    ui.horizontal(|ui| {
        ui.label(t.directory_label())
            .on_hover_text(t.directory_tooltip());
        ui.add(
            egui::TextEdit::singleline(&mut app.search_dir)
                .desired_width(300.0)
                .hint_text(t.directory_hint()),
        );
        if ui.button(t.browse()).clicked() {
            if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                app.search_dir = dir.to_string_lossy().to_string();
            }
        }

        ui.separator();

        ui.label(t.ui_language_label())
            .on_hover_text(t.ui_language_tooltip());
        egui::ComboBox::from_id_salt("ui_lang_pref")
            .selected_text(app.ui_language_preference.display_label(eff))
            .show_ui(ui, |ui| {
                for pref in [
                    UiLanguagePreference::Auto,
                    UiLanguagePreference::Japanese,
                    UiLanguagePreference::English,
                ] {
                    ui.selectable_value(
                        &mut app.ui_language_preference,
                        pref,
                        pref.display_label(eff),
                    );
                }
            });
    });

    ui.horizontal(|ui| {
        // 検索モード切り替え
        ui.label(t.mode_label()).on_hover_text(t.mode_tooltip());
        ui.selectable_value(&mut app.search_mode, SearchMode::AstGrep, t.mode_ast())
            .on_hover_text(t.mode_ast_tooltip());
        ui.selectable_value(&mut app.search_mode, SearchMode::AstGrepRaw, t.mode_ast_raw())
            .on_hover_text(t.mode_ast_raw_tooltip());
        ui.selectable_value(&mut app.search_mode, SearchMode::PlainText, t.mode_plain())
            .on_hover_text(t.mode_plain_tooltip());
        ui.selectable_value(&mut app.search_mode, SearchMode::Regex, t.mode_regex())
            .on_hover_text(t.mode_regex_tooltip());

        ui.separator();

        // AST モードのみ言語選択を表示
        if app.search_mode.is_ast_mode() {
            ui.label(t.search_lang_label())
                .on_hover_text(t.search_lang_tooltip());
            let ui_lang = app.ui_lang();
            egui::ComboBox::from_id_salt("lang_select")
                .selected_text(app.selected_lang.combo_label(ui_lang))
                .show_ui(ui, |ui| {
                    for lang in SupportedLanguage::all_with_auto() {
                        ui.selectable_value(
                            &mut app.selected_lang,
                            *lang,
                            lang.combo_label(ui_lang),
                        );
                    }
                });
        } else {
            // PlainText/Regex では言語選択の代わりに補足説明を表示
            ui.label(
                egui::RichText::new(t.all_files_note())
                    .small()
                    .color(egui::Color32::GRAY),
            )
            .on_hover_text(t.all_files_tooltip());
        }

        ui.label(t.context_lines_label())
            .on_hover_text(t.context_lines_tooltip());
        ui.horizontal(|ui| {
            if ui
                .small_button("−")
                .on_hover_text(t.context_lines_decrease_tooltip())
                .clicked()
            {
                app.context_lines = app.context_lines.saturating_sub(1);
            }
            ui.add(egui::DragValue::new(&mut app.context_lines).range(0..=10))
                .on_hover_text(t.context_drag_tooltip());
            if ui
                .small_button("+")
                .on_hover_text(t.context_lines_increase_tooltip())
                .clicked()
            {
                app.context_lines = (app.context_lines + 1).min(10);
            }
        });

        ui.separator();

        ui.label(t.file_filter_label())
            .on_hover_text(t.file_filter_tooltip());
        ui.add(
            egui::TextEdit::singleline(&mut app.file_filter)
                .desired_width(200.0)
                .hint_text(t.file_filter_hint()),
        )
        .on_hover_text(t.file_filter_hover());
    });

    // 現在モードでの拡張子 → 解析言語（grep）
    if app.search_mode.is_ast_mode() {
        let pairs = app.selected_lang.ast_grep_extension_mapping();
        egui::CollapsingHeader::new(
            egui::RichText::new(t.ext_mapping_title())
                .small()
                .color(egui::Color32::GRAY),
        )
        .default_open(true)
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .max_height(120.0)
                .id_salt("ext_mapping_scroll")
                .show(ui, |ui| {
                    egui::Grid::new("ext_mapping_grid")
                        .num_columns(2)
                        .striped(true)
                        .spacing([16.0, 3.0])
                        .show(ui, |ui| {
                            for (ext, lang) in pairs {
                                ui.label(
                                    egui::RichText::new(ext)
                                        .small()
                                        .monospace()
                                        .color(egui::Color32::from_rgb(200, 200, 150)),
                                );
                                ui.label(
                                    egui::RichText::new(lang)
                                        .small()
                                        .color(egui::Color32::LIGHT_GRAY),
                                );
                                ui.end_row();
                            }
                        });
                });
            if !app.file_filter.trim().is_empty() {
                ui.label(
                    egui::RichText::new(t.ext_mapping_file_filter_note())
                        .small()
                        .color(egui::Color32::from_rgb(120, 160, 200)),
                );
            }
        });
    } else {
        ui.add(
            egui::Label::new(
                egui::RichText::new(t.ext_mapping_plain_regex())
                    .small()
                    .color(egui::Color32::GRAY),
            )
            .wrap(),
        );
    }

    ui.horizontal(|ui| {
        let (pattern_label_tooltip, pattern_hint) = match app.search_mode {
            SearchMode::AstGrep => (t.pattern_label_tooltip_ast(), t.pattern_hint_ast()),
            SearchMode::AstGrepRaw => (t.pattern_label_tooltip_ast_raw(), t.pattern_hint_ast_raw()),
            SearchMode::PlainText => (t.pattern_label_tooltip_plain(), t.pattern_hint_plain()),
            SearchMode::Regex => (t.pattern_label_tooltip_regex(), t.pattern_hint_regex()),
        };

        ui.label(t.pattern_colon()).on_hover_text(pattern_label_tooltip);
        let response = ui.add(
            egui::TextEdit::singleline(&mut app.pattern)
                .desired_width(350.0)
                .hint_text(pattern_hint),
        );

        // テキスト変更時に候補選択インデックスをリセット
        if response.changed() {
            app.pattern_suggest_idx = None;
        }

        // AST / Regex モードのみ履歴サジェストを表示
        let show_suggest = app.search_mode.is_ast_mode()
            || app.search_mode == SearchMode::Regex;

        let popup_id = ui.make_persistent_id("pattern_autocomplete");

        // 候補リスト: 現在の入力を含む履歴（最大8件、完全一致除外）
        let suggestions: Vec<String> = if show_suggest && !app.pattern.is_empty() {
            app.pattern_history
                .iter()
                .filter(|h| *h != &app.pattern && h.contains(app.pattern.as_str()))
                .take(8)
                .cloned()
                .collect()
        } else {
            vec![]
        };

        // フォーカス中は候補の有無でポップアップを開閉
        if response.has_focus() {
            if !suggestions.is_empty() {
                ui.memory_mut(|mem| mem.open_popup(popup_id));
            } else if ui.memory(|mem| mem.is_popup_open(popup_id)) {
                ui.memory_mut(|mem| mem.close_popup());
                app.pattern_suggest_idx = None;
            }
        }

        let popup_is_open = ui.memory(|mem| mem.is_popup_open(popup_id));

        // 矢印キーで候補をナビゲート
        if popup_is_open && !suggestions.is_empty() {
            let n = suggestions.len();
            ui.input_mut(|i| {
                if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
                    app.pattern_suggest_idx =
                        Some(app.pattern_suggest_idx.map_or(0, |idx| (idx + 1).min(n - 1)));
                }
                if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
                    app.pattern_suggest_idx = Some(
                        app.pattern_suggest_idx.map_or(n - 1, |idx| idx.saturating_sub(1)),
                    );
                }
            });
        }

        // 候補ポップアップを描画
        let suggest_idx = app.pattern_suggest_idx;
        let input_width = response.rect.width().max(280.0);
        let mut apply_from_popup: Option<String> = None;
        egui::popup_below_widget(
            ui,
            popup_id,
            &response,
            egui::PopupCloseBehavior::CloseOnClickOutside,
            |ui| {
                ui.set_min_width(input_width);
                egui::ScrollArea::vertical()
                    .max_height(200.0)
                    .id_salt("pattern_suggest_scroll")
                    .show(ui, |ui| {
                        for (i, sug) in suggestions.iter().enumerate() {
                            let is_selected = suggest_idx == Some(i);
                            let resp = ui.selectable_label(
                                is_selected,
                                egui::RichText::new(sug).monospace(),
                            );
                            if resp.clicked() {
                                apply_from_popup = Some(sug.clone());
                            }
                            if is_selected {
                                resp.scroll_to_me(None);
                            }
                        }
                    });
            },
        );

        // クリックによる候補の適用
        if let Some(sug) = apply_from_popup {
            app.pattern = sug;
            app.pattern_suggest_idx = None;
            ui.memory_mut(|mem| mem.close_popup());
        }

        // Enterキーで検索開始（候補選択中ならまず適用）
        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            if popup_is_open {
                if let Some(idx) = app.pattern_suggest_idx {
                    if let Some(sug) = suggestions.get(idx) {
                        app.pattern = sug.clone();
                    }
                }
                app.pattern_suggest_idx = None;
                ui.memory_mut(|mem| mem.close_popup());
            }
            app.start_search();
        }

        let is_running = matches!(app.search_state, SearchState::Running);

        if is_running {
            if ui.button(t.stop()).on_hover_text(t.stop_tooltip()).clicked() {
                app.stop_search();
            }
        } else {
            let search_tooltip = match app.search_mode {
                SearchMode::AstGrep => t.search_tooltip_ast(),
                SearchMode::AstGrepRaw => t.search_tooltip_ast_raw(),
                SearchMode::PlainText => t.search_tooltip_plain(),
                SearchMode::Regex => t.search_tooltip_regex(),
            };
            if ui.button(t.search_btn()).on_hover_text(search_tooltip).clicked() {
                app.start_search();
            }
        }

        if ui.button(t.clear_results())
            .on_hover_text(t.clear_results_tooltip())
            .clicked()
        {
            app.clear_results();
        }

        // ASTモードのときのみヘルプ・パターン支援を表示
        if app.search_mode.is_ast_mode() {
            if ui.button(t.help_btn())
                .on_hover_text(t.help_btn_tooltip())
                .clicked()
            {
                app.show_help = !app.show_help;
            }

            if ui.button(t.pattern_assist_btn())
                .on_hover_text(t.pattern_assist_btn_tooltip())
                .clicked()
            {
                app.show_pattern_assist = !app.show_pattern_assist;
            }
        } else if app.search_mode == SearchMode::Regex {
            if ui.button(t.regex_visualizer_btn())
                .on_hover_text(t.regex_visualizer_btn_tooltip())
                .clicked()
            {
                app.show_regex_visualizer = !app.show_regex_visualizer;
            }
        }

        ui.separator();

        // ビューモード切り替え
        ui.selectable_value(&mut app.view_mode, ViewMode::Code, t.view_code())
            .on_hover_text(t.view_code_tooltip());
        ui.selectable_value(&mut app.view_mode, ViewMode::Table, t.view_table())
            .on_hover_text(t.view_table_tooltip());

        ui.separator();

        // ターミナルパネルトグル
        if ui
            .selectable_label(app.show_terminal, "⌨ Terminal")
            .on_hover_text("PowerShell ターミナルパネルを表示（sg コマンドは内蔵エンジンで実行）")
            .clicked()
        {
            app.show_terminal = !app.show_terminal;
        }
    });

    // 詳細設定（折りたたみ）
    egui::CollapsingHeader::new(
        egui::RichText::new(t.advanced_settings())
            .small()
            .color(egui::Color32::GRAY),
    )
    .default_open(false)
    .show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(t.file_encoding_label())
                .on_hover_text(t.file_encoding_tooltip());
            let ui_lang = app.ui_lang();
            let selected_encoding = app
                .selected_file_idx
                .and_then(|idx| app.results.get(idx))
                .map(|file| &file.text_encoding);
            let combo_text = match (app.file_encoding_preference, selected_encoding) {
                (FileEncodingPreference::Auto, Some(encoding)) => match ui_lang {
                    crate::i18n::UiLanguage::Japanese => {
                        format!("自動判定 -> {}", encoding.display_label())
                    }
                    crate::i18n::UiLanguage::English => {
                        format!("Auto detect -> {}", encoding.display_label())
                    }
                },
                _ => app.file_encoding_preference.display_label(ui_lang).to_string(),
            };
            egui::ComboBox::from_id_salt("file_encoding_preference")
                .selected_text(combo_text)
                .show_ui(ui, |ui| {
                    for pref in FileEncodingPreference::ALL {
                        ui.selectable_value(
                            &mut app.file_encoding_preference,
                            pref,
                            pref.display_label(ui_lang),
                        );
                    }
                });
        });
        if let Some(message) = auto_encoding_feedback(app) {
            ui.label(
                egui::RichText::new(message)
                    .small()
                    .color(egui::Color32::GRAY),
            );
        }

        ui.horizontal(|ui| {
            ui.label(t.max_file_size_label())
                .on_hover_text(t.max_file_size_tooltip());
            ui.add(
                egui::DragValue::new(&mut app.max_file_size_mb)
                    .range(1..=500)
                    .suffix(" MB"),
            )
            .on_hover_text(t.max_file_size_drag_tooltip());
        });

        ui.horizontal(|ui| {
            ui.label(t.max_search_hits_label())
                .on_hover_text(t.max_search_hits_tooltip());
            ui.add(
                egui::DragValue::new(&mut app.max_search_hits)
                    .range(0..=10_000_000)
                    .speed(1000.0),
            )
            .on_hover_text(t.max_search_hits_drag_tooltip());
        });

        ui.horizontal(|ui| {
            ui.label(t.skip_dirs_label())
                .on_hover_text(t.skip_dirs_tooltip());
            ui.add(
                egui::TextEdit::singleline(&mut app.skip_dirs)
                    .desired_width(ui.available_width() - 4.0)
                    .hint_text(t.skip_dirs_hint()),
            )
            .on_hover_text(t.skip_dirs_hover());
        });
    });

    ui.horizontal(|ui| {
        let hint = if app.search_mode.is_ast_mode() {
            t.footer_hint_ast()
        } else {
            t.footer_hint_non_ast()
        };
        ui.label(
            egui::RichText::new(hint)
                .small()
                .color(egui::Color32::GRAY),
        );
    });
}

fn auto_encoding_feedback(app: &AstGrepApp) -> Option<String> {
    if app.file_encoding_preference != FileEncodingPreference::Auto {
        return None;
    }

    if let Some(file) = app.selected_file_idx.and_then(|idx| app.results.get(idx)) {
        return Some(file.text_encoding.auto_feedback_text(app.ui_lang()));
    }

    if app.results.is_empty() {
        return None;
    }

    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for file in &app.results {
        *counts
            .entry(file.text_encoding.display_label().into_owned())
            .or_default() += 1;
    }
    let summary = counts
        .into_iter()
        .map(|(label, count)| format!("{label} x{count}"))
        .collect::<Vec<_>>()
        .join(", ");

    Some(match app.ui_lang() {
        crate::i18n::UiLanguage::Japanese => format!("自動判定内訳: {summary}"),
        crate::i18n::UiLanguage::English => format!("Detected encodings: {summary}"),
    })
}
