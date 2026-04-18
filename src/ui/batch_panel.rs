//! バッチジョブ一覧・編集 UI

use egui::Ui;

use crate::app::AstGrepApp;
use crate::batch::{read_batch_jobs_file, write_batch_jobs_file};
use crate::file_encoding::FileEncodingPreference;
use crate::lang::SupportedLanguage;
use crate::search::SearchMode;

/// ツールバー内: バッチジョブ一覧と操作
pub fn show_job_section(app: &mut AstGrepApp, ui: &mut Ui) {
    let t = app.tr();
    let ui_lang = app.ui_lang();

    egui::CollapsingHeader::new(t.batch_jobs_header())
        .default_open(false)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button(t.batch_add_job()).on_hover_text(t.batch_add_job_tooltip()).clicked() {
                    app.add_pattern_job_from_current();
                }
                let can_run = app.batch_runner.is_none()
                    && !matches!(app.search_state, crate::app::SearchState::Running)
                    && app
                        .batch_jobs
                        .iter()
                        .any(|j| j.is_runnable());
                if ui
                    .add_enabled(can_run, egui::Button::new(t.batch_run_all()))
                    .on_hover_text(t.batch_run_all_tooltip())
                    .clicked()
                {
                    app.start_batch_search();
                }
            });
            ui.horizontal(|ui| {
                if ui
                    .button(t.batch_save_config())
                    .on_hover_text(t.batch_save_config_tooltip())
                    .clicked()
                {
                    if let Some(path) = rfd::FileDialog::new()
                        .set_file_name("batch-jobs.yaml")
                        .add_filter("YAML", &["yaml", "yml"])
                        .save_file()
                    {
                        if let Err(e) = write_batch_jobs_file(&path, &app.batch_jobs) {
                            eprintln!("{} {e}", t.err_batch_save());
                        }
                    }
                }
                if ui
                    .button(t.batch_load_config())
                    .on_hover_text(t.batch_load_config_tooltip())
                    .clicked()
                {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("YAML", &["yaml", "yml"])
                        .pick_file()
                    {
                        match read_batch_jobs_file(&path) {
                            Ok((jobs, next_id)) => {
                                app.batch_jobs = jobs;
                                app.next_pattern_job_id = next_id.max(1);
                                app.batch_edit_list_index = None;
                            }
                            Err(e) => eprintln!("{} {e}", t.err_batch_load()),
                        }
                    }
                }
            });

            if app.batch_jobs.is_empty() {
                ui.label(
                    egui::RichText::new(t.batch_jobs_empty_hint())
                        .small()
                        .color(egui::Color32::GRAY),
                );
            } else {
                egui::ScrollArea::vertical()
                    .max_height(180.0)
                    .id_salt("batch_jobs_scroll")
                    .show(ui, |ui| {
                        let mut remove_idx: Option<usize> = None;
                        let mut swap_up: Option<usize> = None;
                        let mut swap_down: Option<usize> = None;

                        egui::Grid::new("batch_jobs_grid")
                            .num_columns(5)
                            .spacing([8.0, 4.0])
                            .striped(true)
                            .show(ui, |ui| {
                                ui.label(t.batch_col_label());
                                ui.label(t.batch_col_pattern());
                                ui.label(t.batch_col_enabled());
                                ui.label(t.batch_col_actions());
                                ui.label("");
                                ui.end_row();

                                let n = app.batch_jobs.len();
                                for i in 0..n {
                                    let job = &app.batch_jobs[i];
                                    let pat_short = if job.pattern.chars().count() > 40 {
                                        format!("{}…", job.pattern.chars().take(40).collect::<String>())
                                    } else {
                                        job.pattern.clone()
                                    };
                                    ui.label(&job.label);
                                    ui.monospace(&pat_short).on_hover_text(&job.pattern);
                                    let mut en = app.batch_jobs[i].enabled;
                                    if ui.checkbox(&mut en, "").changed() {
                                        app.batch_jobs[i].enabled = en;
                                    }
                                    ui.horizontal(|ui| {
                                        if ui.small_button(t.batch_edit()).clicked() {
                                            app.batch_edit_list_index = Some(i);
                                        }
                                        if ui.small_button(t.batch_remove()).clicked() {
                                            remove_idx = Some(i);
                                        }
                                    });
                                    ui.horizontal(|ui| {
                                        if ui
                                            .small_button("↑")
                                            .on_hover_text(t.batch_move_up_tooltip())
                                            .clicked()
                                            && i > 0
                                        {
                                            swap_up = Some(i);
                                        }
                                        if ui
                                            .small_button("↓")
                                            .on_hover_text(t.batch_move_down_tooltip())
                                            .clicked()
                                            && i + 1 < n
                                        {
                                            swap_down = Some(i);
                                        }
                                    });
                                    ui.end_row();
                                }
                            });

                        if let Some(i) = swap_up {
                            app.batch_jobs.swap(i, i - 1);
                            app.batch_edit_list_index = None;
                        }
                        if let Some(i) = swap_down {
                            app.batch_jobs.swap(i, i + 1);
                            app.batch_edit_list_index = None;
                        }
                        if let Some(i) = remove_idx {
                            app.batch_jobs.remove(i);
                            if app.batch_edit_list_index == Some(i) {
                                app.batch_edit_list_index = None;
                            } else if let Some(ref mut e) = app.batch_edit_list_index {
                                if *e > i {
                                    *e -= 1;
                                }
                            }
                        }
                    });
            }
        });

    // 編集ウィンドウ
    if let Some(idx) = app.batch_edit_list_index {
        if idx < app.batch_jobs.len() {
            let mut open = true;
            let mut request_pattern_assist = false;
            egui::Window::new(t.batch_edit_window_title())
                .open(&mut open)
                .default_width(520.0)
                .show(ui.ctx(), |ui| {
                    let job = &mut app.batch_jobs[idx];
                    ui.horizontal(|ui| {
                        ui.label(t.batch_col_label());
                        ui.text_edit_singleline(&mut job.label);
                    });
                    ui.horizontal(|ui| {
                        ui.label(t.directory_label());
                        ui.add(
                            egui::TextEdit::singleline(&mut job.search_dir)
                                .desired_width(320.0)
                                .hint_text(t.directory_hint()),
                        );
                        if ui.button(t.browse()).clicked() {
                            if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                                job.search_dir = dir.to_string_lossy().to_string();
                            }
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label(t.mode_label());
                        ui.selectable_value(&mut job.search_mode, SearchMode::AstGrep, t.mode_ast());
                        ui.selectable_value(&mut job.search_mode, SearchMode::TokenSearch, t.mode_token());
                        ui.selectable_value(&mut job.search_mode, SearchMode::PlainText, t.mode_plain());
                        ui.selectable_value(&mut job.search_mode, SearchMode::Regex, t.mode_regex());
                    });

                    if job.search_mode.is_ast_mode() {
                        ui.horizontal(|ui| {
                            ui.label(t.search_lang_label());
                            egui::ComboBox::from_id_salt(format!("batch_job_lang_{}", job.id))
                                .selected_text(job.selected_lang.combo_label(ui_lang))
                                .show_ui(ui, |ui| {
                                    for lang in SupportedLanguage::all_with_auto() {
                                        ui.selectable_value(
                                            &mut job.selected_lang,
                                            *lang,
                                            lang.combo_label(ui_lang),
                                        );
                                    }
                                });
                        });
                    }

                    let (pattern_tooltip, pattern_hint) = match job.search_mode {
                        SearchMode::AstGrep => (t.pattern_label_tooltip_ast(), t.pattern_hint_ast()),
                        SearchMode::TokenSearch => (t.pattern_label_tooltip_token(), t.pattern_hint_token()),
                        SearchMode::PlainText => (t.pattern_label_tooltip_plain(), t.pattern_hint_plain()),
                        SearchMode::Regex => (t.pattern_label_tooltip_regex(), t.pattern_hint_regex()),
                    };
                    ui.horizontal(|ui| {
                        ui.label(t.pattern_colon()).on_hover_text(pattern_tooltip);
                        ui.add(
                            egui::TextEdit::singleline(&mut job.pattern)
                                .desired_width(400.0)
                                .hint_text(pattern_hint),
                        );
                    });

                    if job.search_mode == SearchMode::PlainText {
                        ui.horizontal(|ui| {
                            ui.checkbox(
                                &mut job.plain_text_options.case_insensitive,
                                t.plain_text_ignore_case(),
                            )
                            .on_hover_text(t.plain_text_ignore_case_tooltip());
                            ui.checkbox(
                                &mut job.plain_text_options.whole_word,
                                t.plain_text_whole_word(),
                            )
                            .on_hover_text(t.plain_text_whole_word_tooltip());
                        });
                    }

                    ui.horizontal(|ui| {
                        ui.label(t.context_lines_label());
                        ui.add(egui::DragValue::new(&mut job.context_lines).range(0..=10));
                    });

                    ui.horizontal(|ui| {
                        ui.label(t.file_filter_label());
                        ui.add(
                            egui::TextEdit::singleline(&mut job.file_filter)
                                .desired_width(280.0)
                                .hint_text(t.file_filter_hint()),
                        );
                    });

                    ui.horizontal(|ui| {
                        ui.label(t.file_encoding_label());
                        egui::ComboBox::from_id_salt(format!("batch_enc_{}", job.id))
                            .selected_text(job.file_encoding_preference.display_label(ui_lang))
                            .show_ui(ui, |ui| {
                                for pref in FileEncodingPreference::ALL {
                                    ui.selectable_value(
                                        &mut job.file_encoding_preference,
                                        pref,
                                        pref.display_label(ui_lang),
                                    );
                                }
                            });
                    });

                    ui.horizontal(|ui| {
                        ui.label(t.max_file_size_label());
                        ui.add(egui::DragValue::new(&mut job.max_file_size_mb).range(1..=500).suffix(" MB"));
                    });
                    ui.horizontal(|ui| {
                        ui.label(t.max_search_hits_label());
                        ui.add(egui::DragValue::new(&mut job.max_search_hits).range(0..=10_000_000).speed(1000.0));
                    });
                    ui.horizontal(|ui| {
                        ui.label(t.skip_dirs_label());
                        ui.add(
                            egui::TextEdit::singleline(&mut job.skip_dirs)
                                .desired_width(ui.available_width() - 4.0)
                                .hint_text(t.skip_dirs_hint()),
                        );
                    });
                    if job.search_mode.is_ast_mode() {
                        ui.horizontal(|ui| {
                            ui.label(t.cpp_include_dirs_label())
                                .on_hover_text(t.cpp_include_dirs_tooltip());
                            ui.add(
                                egui::TextEdit::singleline(&mut job.cpp_include_dirs)
                                    .desired_width(ui.available_width() - 4.0)
                                    .hint_text(t.cpp_include_dirs_hint()),
                            )
                            .on_hover_text(t.cpp_include_dirs_tooltip());
                        });
                    }

                    if job.search_mode.is_ast_mode() {
                        ui.horizontal(|ui| {
                            if ui.button(t.pattern_assist_btn()).clicked() {
                                request_pattern_assist = true;
                            }
                        });
                    }
                });
            if request_pattern_assist {
                let p = app.batch_jobs[idx].pattern.clone();
                app.pattern_assist_snippet = p;
                app.show_pattern_assist = true;
                app.pattern_assist_results = Vec::new();
                app.pattern_assist_selected_row = None;
            }
            if !open {
                app.batch_edit_list_index = None;
            }
        } else {
            app.batch_edit_list_index = None;
        }
    }
}
