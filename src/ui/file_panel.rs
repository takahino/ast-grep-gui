use egui::Ui;

use crate::app::{AstGrepApp, CodeViewPaneFocus};

fn apply_file_selection(app: &mut AstGrepApp, idx: usize) {
    let Some(file) = app.results.get(idx) else {
        return;
    };
    let changed = app.selected_file_idx != Some(idx);
    app.selected_file_idx = Some(idx);
    app.code_view_pane_focus = CodeViewPaneFocus::FileList;
    if changed {
        if let Some(first_match) = file.matches.first() {
            app.pending_scroll_line = Some(first_match.line_start);
        }
    }
}

pub fn show(app: &mut AstGrepApp, ui: &mut Ui) {
    let t = app.tr();
    ui.heading(t.file_list_heading());
    ui.separator();

    if app.results.is_empty() {
        app.code_view_pointer_on_list = false;
        ui.label(
            egui::RichText::new(t.file_list_empty())
                .color(egui::Color32::GRAY),
        );
        return;
    }

    let n = app.results.len();
    let list_rect = ui.available_rect_before_wrap();
    let pointer_on_list = ui.rect_contains_pointer(list_rect);
    let pointer_on_code = app.code_view_pointer_on_code;
    let file_list_keys = pointer_on_list
        || (matches!(app.code_view_pane_focus, CodeViewPaneFocus::FileList) && !pointer_on_code);
    if file_list_keys {
        ui.input_mut(|i| {
            let down = i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown);
            let up = i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp);
            if !down && !up {
                return;
            }
            let cur = app.selected_file_idx;
            let next = if down {
                match cur {
                    None => Some(0),
                    Some(i) if i + 1 < n => Some(i + 1),
                    Some(i) => Some(i),
                }
            } else {
                match cur {
                    None => Some(n.saturating_sub(1)),
                    Some(i) if i > 0 => Some(i - 1),
                    Some(i) => Some(i),
                }
            };
            if let Some(idx) = next {
                apply_file_selection(app, idx);
            }
        });
    }

    let scroll_out = egui::ScrollArea::vertical()
        .id_salt("file_list")
        .show(ui, |ui| {
            let ui_lang = app.ui_lang();
            for idx in 0..app.results.len() {
                let (label, hover_tip) = {
                    let file = &app.results[idx];
                    (
                        format!("{} ({})", file.relative_path, file.matches.len()),
                        file.text_encoding.detail_text(ui_lang),
                    )
                };
                let is_selected = app.selected_file_idx == Some(idx);

                let response = ui
                    .selectable_label(is_selected, &label)
                    .on_hover_text(hover_tip);
                if response.clicked() {
                    apply_file_selection(app, idx);
                }
                if is_selected {
                    response.scroll_to_me(None);
                }
            }
        });
    app.code_view_pointer_on_list = ui.rect_contains_pointer(scroll_out.inner_rect);
}
