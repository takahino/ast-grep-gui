use egui::Ui;

use crate::app::AstGrepApp;

pub fn show(app: &mut AstGrepApp, ui: &mut Ui) {
    let t = app.tr();
    ui.heading(t.file_list_heading());
    ui.separator();

    if app.results.is_empty() {
        ui.label(
            egui::RichText::new(t.file_list_empty())
                .color(egui::Color32::GRAY),
        );
        return;
    }

    egui::ScrollArea::vertical()
        .id_salt("file_list")
        .show(ui, |ui| {
            for (idx, file) in app.results.iter().enumerate() {
                let label = format!(
                    "{} ({})",
                    file.relative_path,
                    file.matches.len()
                );
                let is_selected = app.selected_file_idx == Some(idx);

                let response = ui
                    .selectable_label(is_selected, &label)
                    .on_hover_text(file.text_encoding.detail_text(app.ui_lang()));
                if response.clicked() {
                    let changed = app.selected_file_idx != Some(idx);
                    app.selected_file_idx = Some(idx);
                    // ファイルが切り替わった場合は最初のヒット行へジャンプ
                    if changed {
                        if let Some(first_match) = file.matches.first() {
                            app.pending_scroll_line = Some(first_match.line_start);
                        }
                    }
                }
            }
        });
}
