//! バッチ検索の集約レポート表示

use egui::Ui;

use crate::app::AstGrepApp;
use crate::batch::BatchRunResult;
use crate::export::search_condition_entries;
use crate::search::SearchConditions;
use crate::ui::scroll_keyboard;

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
    if let Some(note) = &app.cli_export_note {
        ui.colored_label(egui::Color32::LIGHT_GREEN, note);
    }
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
                        ui.colored_label(
                            egui::Color32::RED,
                            format!("{}: {err}", t.batch_report_error()),
                        );
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
    for (label, value) in search_condition_entries(t, c, lang) {
        if value.contains('\n') {
            ui.label(format!("{label}:"));
            ui.monospace(value);
        } else {
            ui.label(format!("{label}: {value}"));
        }
    }
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
                    ui.monospace(&m.matched_text_for_file(file));
                    ui.end_row();
                }
            }
        });
}
