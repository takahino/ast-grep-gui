use egui::Ui;

use crate::app::{AstGrepApp, SearchState};
use crate::export::{
    copy_to_clipboard, export_html_to_file, export_json_to_file, export_markdown_to_file,
    export_text_to_file, export_xlsx_to_file, results_to_text_for_mode,
};

pub fn show(app: &mut AstGrepApp, ui: &mut Ui) {
    let t = app.tr();
    let ui_lang = app.ui_lang();

    ui.horizontal(|ui| {
        // 状態表示
        match &app.search_state {
            SearchState::Idle => {
                ui.label(t.status_idle());
            }
            SearchState::Running => {
                ui.spinner();
                ui.label(t.status_searching(app.stats.scanned));
            }
            SearchState::Done => {
                ui.label(
                    egui::RichText::new(t.status_done(
                        app.stats.total_matches,
                        app.stats.total_files,
                        app.stats.elapsed_ms,
                        app.stats.hit_limit_reached,
                    ))
                    .color(egui::Color32::from_rgb(100, 200, 100)),
                );
            }
            SearchState::Error(msg) => {
                ui.label(
                    egui::RichText::new(t.status_error(msg))
                        .color(egui::Color32::RED),
                );
            }
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let has_results = !app.results.is_empty();

            // Excel エクスポート
            if ui
                .add_enabled(has_results, egui::Button::new(t.export_excel()))
                .on_hover_text(t.export_excel_tooltip())
                .clicked()
            {
                if let Some(path) = rfd::FileDialog::new()
                    .set_file_name("results.xlsx")
                    .add_filter("Excel", &["xlsx"])
                    .save_file()
                {
                    if let Err(e) = export_xlsx_to_file(
                        &path,
                        &app.results,
                        &app.stats,
                        &app.search_conditions_for_export(),
                        ui_lang,
                    ) {
                        eprintln!("{} {e}", t.err_export_excel());
                    }
                }
            }

            // HTML エクスポート
            if ui
                .add_enabled(has_results, egui::Button::new(t.export_html()))
                .on_hover_text(t.export_html_tooltip())
                .clicked()
            {
                if let Some(path) = rfd::FileDialog::new()
                    .set_file_name("results.html")
                    .add_filter("HTML", &["html"])
                    .save_file()
                {
                    if let Err(e) = export_html_to_file(
                        &path,
                        &app.results,
                        &app.stats,
                        &app.search_conditions_for_export(),
                        ui_lang,
                    ) {
                        eprintln!("{} {e}", t.err_export_html());
                    }
                }
            }

            // Markdown エクスポート
            if ui
                .add_enabled(has_results, egui::Button::new(t.export_md()))
                .on_hover_text(t.export_md_tooltip())
                .clicked()
            {
                if let Some(path) = rfd::FileDialog::new()
                    .set_file_name("results.md")
                    .add_filter("Markdown", &["md"])
                    .save_file()
                {
                    if let Err(e) = export_markdown_to_file(
                        &path,
                        &app.results,
                        &app.stats,
                        &app.search_conditions_for_export(),
                        ui_lang,
                    ) {
                        eprintln!("{} {e}", t.err_export_md());
                    }
                }
            }

            // JSON エクスポート
            if ui
                .add_enabled(has_results, egui::Button::new(t.export_json()))
                .on_hover_text(t.export_json_tooltip())
                .clicked()
            {
                if let Some(path) = rfd::FileDialog::new()
                    .set_file_name("results.json")
                    .add_filter("JSON", &["json"])
                    .save_file()
                {
                    if let Err(e) = export_json_to_file(
                        &path,
                        &app.results,
                        &app.stats,
                        &app.search_conditions_for_export(),
                    ) {
                        eprintln!("{} {e}", t.err_export_json());
                    }
                }
            }

            // テキスト エクスポート
            if ui
                .add_enabled(has_results, egui::Button::new(t.export_txt()))
                .on_hover_text(t.export_txt_tooltip())
                .clicked()
            {
                if let Some(path) = rfd::FileDialog::new()
                    .set_file_name("results.txt")
                    .add_filter(t.file_filter_txt(), &["txt"])
                    .save_file()
                {
                    if let Err(e) = export_text_to_file(
                        &path,
                        &app.results,
                        &app.stats,
                        &app.search_conditions_for_export(),
                        app.search_mode,
                        ui_lang,
                    ) {
                        eprintln!("{} {e}", t.err_export_txt());
                    }
                }
            }

            ui.separator();

            // クリップボードコピー
            if ui
                .add_enabled(has_results, egui::Button::new(t.copy_results()))
                .on_hover_text(t.copy_results_tooltip())
                .clicked()
            {
                let text = results_to_text_for_mode(
                    &app.results,
                    &app.stats,
                    &app.search_conditions_for_export(),
                    app.search_mode,
                    ui_lang,
                );
                if let Err(e) = copy_to_clipboard(&text) {
                    eprintln!("{} {e}", t.err_clipboard());
                }
            }
        });
    });
}
