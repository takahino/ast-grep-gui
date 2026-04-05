//! バッチ検索の集約レポート表示

use egui::Ui;

use crate::app::AstGrepApp;
use crate::ui::scroll_keyboard;
use crate::batch::BatchRunResult;
use crate::export::{file_filter_display, plain_text_options_export_value};
use crate::search::SearchConditions;

pub fn show(app: &mut AstGrepApp, ui: &mut Ui) {
    let t = app.tr();
    let Some(ref report) = app.batch_report else {
        ui.label(t.batch_report_empty());
        return;
    };

    ui.heading(t.batch_report_title());
    ui.label(t.batch_report_summary(
        report.total_elapsed_ms,
        report.total_matches(),
        report.total_files(),
        report.runs.len(),
        report.failed_count(),
    ));
    ui.add_space(8.0);

    let sid = scroll_keyboard::scroll_area_persistent_id(ui, "batch_report_scroll");
    let rect = ui.available_rect_before_wrap();
    scroll_keyboard::apply_keyboard_scroll_before_show(
        ui.ctx(),
        ui,
        sid,
        rect,
        egui::Vec2b::from([false, true]),
        false,
    );

    let scroll_out = egui::ScrollArea::vertical()
        .id_salt("batch_report_scroll")
        .show(ui, |ui| {
            for (i, run) in report.runs.iter().enumerate() {
                ui.group(|ui| {
                    ui.heading(format!("{}. {} (id={})", i + 1, run.label, run.job_id));
                    if let Some(ref err) = run.error {
                        ui.colored_label(egui::Color32::RED, format!("{}: {err}", t.batch_report_error()));
                    } else {
                        ui.label(t.batch_report_job_stats(
                            run.stats.total_matches,
                            run.stats.total_files,
                            run.stats.elapsed_ms,
                            run.stats.hit_limit_reached,
                        ));
                    }

                    // 同一ラベルの CollapsingHeader はジョブ間で ID が衝突するため id_salt が必須
                    egui::CollapsingHeader::new(t.batch_report_conditions())
                        .id_salt((run.job_id, "conditions"))
                        .show(ui, |ui| {
                            show_conditions(ui, app, &run.conditions);
                        });

                    if run.error.is_none() && !run.results.is_empty() {
                        egui::CollapsingHeader::new(t.batch_report_matches())
                            .id_salt((run.job_id, "matches"))
                            .show(ui, |ui| {
                                show_run_matches(ui, run, t);
                            });
                    }
                });
                ui.add_space(6.0);
            }
        });
    scroll_keyboard::store_scroll_metrics(ui.ctx(), sid, &scroll_out, rect);
}

fn show_conditions(ui: &mut Ui, app: &AstGrepApp, c: &SearchConditions) {
    let t = app.tr();
    let lang = app.ui_lang();
    ui.label(format!("{}: {}", t.export_cond_root(), c.search_dir));
    ui.label(format!("{}: {}", t.export_cond_pattern(), c.pattern));
    ui.label(format!(
        "{}: {}",
        t.export_cond_lang(),
        c.selected_lang.combo_label(lang)
    ));
    ui.label(format!("{}: {}", t.export_cond_context_lines(), c.context_lines));
    ui.label(format!(
        "{}: {}",
        t.export_cond_file_filter(),
        file_filter_display(t, c)
    ));
    ui.label(format!(
        "{}: {}",
        t.export_cond_file_encoding(),
        c.file_encoding_preference.display_label(lang)
    ));
    ui.label(format!("{}: {}", t.export_cond_max_file_mb(), c.max_file_size_mb));
    ui.label(format!("{}: {}", t.export_cond_max_search_hits(), c.max_search_hits));
    ui.label(format!("{}: {}", t.export_cond_skip_dirs(), c.skip_dirs));
    ui.label(format!(
        "{}: {}",
        t.export_cond_search_mode(),
        crate::export::search_mode_label_for_export(t, c.search_mode)
    ));
    ui.label(format!(
        "{}: {}",
        t.export_cond_plain_text_options(),
        plain_text_options_export_value(t, c)
    ));
}

fn show_run_matches(ui: &mut Ui, run: &BatchRunResult, t: crate::i18n::Tr) {
    egui::Grid::new(format!("batch_run_{}", run.job_id))
        .num_columns(4)
        .spacing([12.0, 4.0])
        .striped(true)
        .show(ui, |ui| {
            ui.label(t.table_col_file());
            ui.label(t.table_col_line());
            ui.label(t.table_col_col());
            ui.label(t.table_col_text());
            ui.end_row();
            for file in &run.results {
                for m in &file.matches {
                    ui.monospace(&file.relative_path);
                    ui.label(m.line_start.to_string());
                    ui.label(m.col_start.to_string());
                    ui.monospace(&m.matched_text);
                    ui.end_row();
                }
            }
        });
}
