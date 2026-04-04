use crate::app::AstGrepApp;
use crate::export::file_to_ast_grep_console;
use crate::file_encoding::read_text_file_as;
use crate::highlight::build_layout_job;
use crate::search::{FileResult, SearchMode};

pub fn show(app: &mut AstGrepApp, ctx: &egui::Context) {
    let scroll_line = match &mut app.table_preview {
        Some(s) => s.pending_scroll_line.take(),
        None => return,
    };

    let (path, relative_path, matches, lang, text_encoding, line, col) = {
        let s = match &app.table_preview {
            Some(s) => s,
            None => return,
        };
        (
            s.path.clone(),
            s.relative_path.clone(),
            s.matches.clone(),
            s.source_language,
            s.text_encoding.clone(),
            s.line,
            s.col,
        )
    };

    let mut open = true;
    let t = app.tr();

    egui::Window::new(t.table_preview_window_title())
        .open(&mut open)
        .resizable(true)
        .default_size([720.0, 520.0])
        .show(ctx, |ui| {
            ui.label(
                egui::RichText::new(t.table_preview_subtitle(&relative_path, line, col)).strong(),
            );
            ui.label(
                egui::RichText::new(text_encoding.detail_text(app.ui_lang()))
                    .small()
                    .color(egui::Color32::GRAY),
            );
            ui.add_space(4.0);

            let file_result = FileResult {
                path: path.clone(),
                relative_path: relative_path.clone(),
                source_language: lang,
                text_encoding: text_encoding.clone(),
                matches: matches.clone(),
            };

            if app.search_mode == SearchMode::AstGrepRaw {
                let mut console_text = file_to_ast_grep_console(&file_result);
                egui::ScrollArea::both()
                    .id_salt("table_preview_raw")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut console_text)
                                .font(egui::TextStyle::Monospace)
                                .desired_width(f32::INFINITY)
                                .interactive(false),
                        );
                    });
                return;
            }

            let source = match read_text_file_as(&path, text_encoding) {
                Ok(s) => s,
                Err(e) => {
                    ui.label(t.code_read_error_fmt(e));
                    return;
                }
            };

            const FONT_SIZE: f32 = 13.0;
            let line_height = ui.fonts(|f| f.row_height(&egui::FontId::monospace(FONT_SIZE)));
            let highlighted = app
                .highlighter
                .highlight_source(&relative_path, &source, lang);
            let job = build_layout_job(highlighted, &matches, FONT_SIZE);

            let mut scroll = egui::ScrollArea::both()
                .id_salt("table_preview_code")
                .auto_shrink([false, false]);

            if let Some(ln) = scroll_line {
                let target_line = ln.saturating_sub(3) as f32;
                scroll = scroll.scroll_offset(egui::vec2(0.0, target_line * line_height));
            }

            scroll.show(ui, |ui| {
                let galley = ui.fonts(|f| f.layout_job(job));
                ui.add(egui::Label::new(galley).selectable(true));
            });
        });

    if !open {
        app.table_preview = None;
    }
}
