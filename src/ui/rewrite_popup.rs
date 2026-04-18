use crate::app::{AstGrepApp, RewritePhase};
use crate::highlight::build_unified_diff_layout_job;

const REWRITE_PREVIEW_FONT_SIZE: f32 = 13.0;
/// unified diff の最大行数（ヘッダ 2 行含む。超えた分は省略メッセージ）
const REWRITE_UNIFIED_DIFF_MAX_LINES: usize = 800;

pub fn show(app: &mut AstGrepApp, ctx: &egui::Context) {
    if !app.show_rewrite_popup {
        return;
    }

    let Some(preview) = app.rewrite_preview.clone() else {
        app.show_rewrite_popup = false;
        return;
    };

    let mut open = true;
    let request_close = std::cell::Cell::new(false);
    let t = app.tr();

    egui::Window::new(t.rewrite_window_title())
        .open(&mut open)
        .resizable(true)
        .default_size([900.0, 600.0])
        .constrain_to(ctx.screen_rect())
        .show(ctx, |ui| {
            ui.label(
                egui::RichText::new(t.rewrite_preview_summary(
                    preview.files.len(),
                    preview.elapsed_ms,
                ))
                .small(),
            );
            ui.separator();

            if preview.files.is_empty() {
                ui.label(t.rewrite_no_changes());
                ui.horizontal(|ui| {
                    if ui.button(t.rewrite_close()).clicked() {
                        request_close.set(true);
                    }
                });
                return;
            }

            let n = preview.files.len();
            let sel = app.rewrite_selected_file_idx.min(n.saturating_sub(1));
            app.rewrite_selected_file_idx = sel;

            ui.horizontal(|ui| {
                ui.label(t.rewrite_file_list_label());
                egui::ScrollArea::horizontal()
                    .id_salt("rewrite_file_tabs")
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            for (i, f) in preview.files.iter().enumerate() {
                                let label = format!(
                                    "{} ({})",
                                    f.relative_path,
                                    f.replacement_count
                                );
                                if ui
                                    .selectable_label(sel == i, label)
                                    .clicked()
                                {
                                    app.rewrite_selected_file_idx = i;
                                }
                            }
                        });
                    });
            });

            let file = &preview.files[sel];
            ui.label(
                egui::RichText::new(format!(
                    "{} — {}",
                    file.relative_path,
                    file.source_language.combo_label(app.ui_lang())
                ))
                .small()
                .color(egui::Color32::DARK_GRAY),
            );
            ui.label(
                egui::RichText::new(t.rewrite_replacements_in_file(
                    file.replacement_count,
                ))
                .small()
                .color(egui::Color32::GRAY),
            );

            ui.separator();

            ui.label(
                egui::RichText::new(t.rewrite_compare_hint())
                    .small()
                    .color(egui::Color32::GRAY),
            );
            ui.add_space(4.0);

            let job = build_unified_diff_layout_job(
                &file.relative_path,
                &file.source_before,
                &file.source_after,
                REWRITE_PREVIEW_FONT_SIZE,
                REWRITE_UNIFIED_DIFF_MAX_LINES,
            );

            let scroll_h = (ui.available_height() - 72.0).max(160.0);

            egui::ScrollArea::vertical()
                .id_salt("rewrite_unified_diff")
                .auto_shrink([false, false])
                .max_height(scroll_h)
                .show(ui, |ui| {
                    let galley = ui.fonts(|f| f.layout_job(job));
                    ui.add(egui::Label::new(galley).selectable(true));
                });

            ui.separator();

            let applying = app.rewrite_phase == RewritePhase::Applying;
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!applying, egui::Button::new(t.rewrite_apply()))
                    .on_hover_text(t.rewrite_apply_tooltip())
                    .clicked()
                {
                    app.start_rewrite_apply();
                }
                if ui.button(t.rewrite_close()).clicked() {
                    request_close.set(true);
                }
            });
        });

    if !open || request_close.get() {
        app.show_rewrite_popup = false;
    }
}
