//! 検索結果に基づく C/C++ `#include` 解決の診断パネル（詳細設定内）。

use egui::Ui;

use crate::app::AstGrepApp;
use crate::search::{
    compute_cpp_include_path_diagnostics, cpp_include_diagnostic_cache_key,
};

/// AST モード時、詳細設定のインクルード欄の下に折りたたみで表示する。
pub fn show_collapsing(app: &mut AstGrepApp, ui: &mut Ui) {
    if !app.search_mode.is_ast_mode() {
        return;
    }
    let t = app.tr();

    let key = cpp_include_diagnostic_cache_key(
        app.results_generation,
        app.cpp_include_dirs.as_str(),
        app.pattern.as_str(),
        app.results.len(),
        app.stats.total_matches,
    );
    let needs_refresh = match &app.cpp_include_diagnostic_cache {
        Some((k, _)) => k != &key,
        None => true,
    };
    if needs_refresh {
        let d = compute_cpp_include_path_diagnostics(
            app.results.as_slice(),
            app.cpp_include_dirs.as_str(),
            app.pattern.as_str(),
        );
        app.cpp_include_diagnostic_cache = Some((key, d));
    }
    let Some((_, diag)) = app.cpp_include_diagnostic_cache.as_ref() else {
        return;
    };

    egui::CollapsingHeader::new(t.cpp_include_diagnostic_header())
        .id_salt("cpp_include_diagnostic_v1")
        .show(ui, |ui| {
        ui.label(
            egui::RichText::new(t.cpp_include_diagnostic_intro())
                .small()
                .color(egui::Color32::GRAY),
        );

        if app.results.is_empty() {
            ui.label(
                egui::RichText::new(t.cpp_include_diagnostic_no_results())
                    .small()
                    .italics(),
            );
            return;
        }

        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(t.cpp_include_diagnostic_cpp_files(diag.distinct_cpp_result_files))
                    .small(),
            );
        });

        if diag.source_read_errors > 0 {
            ui.label(
                egui::RichText::new(t.cpp_include_diagnostic_read_errors(diag.source_read_errors))
                    .small()
                    .color(egui::Color32::from_rgb(200, 140, 80)),
            );
        }

        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(t.cpp_include_diagnostic_unresolved_section(
                diag.unresolved_include_total_hits,
                diag.unresolved_include_distinct,
            ))
            .strong(),
        );
        if diag.top_unresolved_includes.is_empty() {
            ui.label(
                egui::RichText::new(t.cpp_include_diagnostic_all_resolved())
                    .small()
                    .color(egui::Color32::from_rgb(100, 160, 100)),
            );
        } else {
            egui::ScrollArea::vertical()
                .max_height(480.0)
                .show(ui, |ui| {
                    for e in &diag.top_unresolved_includes {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                egui::RichText::new(format!("{}×", e.occurrence_count))
                                    .small()
                                    .monospace(),
                            );
                            ui.label(
                                egui::RichText::new(format!("\"{}\"", e.include_spec))
                                    .small()
                                    .monospace(),
                            );
                        });
                        if !e.example_relative_paths.is_empty() {
                            ui.label(
                                egui::RichText::new(t.cpp_include_diagnostic_examples(
                                    &e.example_relative_paths.join(", "),
                                ))
                                .small()
                                .color(egui::Color32::GRAY),
                            );
                        }
                    }
                });
            ui.label(
                egui::RichText::new(t.cpp_include_diagnostic_add_i_hint())
                    .small()
                    .color(egui::Color32::GRAY),
            );
        }

        if diag.cpp_type_hint_total_cells > 0 {
            ui.add_space(6.0);
            let pct = (diag.cpp_type_hint_unknown_cells as f64 * 100.0)
                / diag.cpp_type_hint_total_cells as f64;
            ui.label(
                egui::RichText::new(t.cpp_include_diagnostic_hint_stats(
                    diag.cpp_type_hint_unknown_cells,
                    diag.cpp_type_hint_total_cells,
                    pct,
                ))
                .small(),
            );
        } else {
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(t.cpp_include_diagnostic_no_hint_columns())
                    .small()
                    .color(egui::Color32::GRAY),
            );
        }
        });
}
