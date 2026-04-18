use crate::app::AstGrepApp;
use crate::file_encoding::read_text_file_as;
use crate::highlight::{build_layout_job, build_layout_job_with_in_view_find};
use crate::ui::in_view_find;

pub fn show(app: &mut AstGrepApp, ctx: &egui::Context) {
    if app.table_preview.is_none() {
        return;
    }

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
        .constrain_to(ctx.screen_rect())
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

            let source = match read_text_file_as(&path, text_encoding) {
                Ok(s) => s,
                Err(e) => {
                    ui.label(t.code_read_error_fmt(e));
                    return;
                }
            };

            let mut find_scroll: Option<usize> = None;
            in_view_find::show_bar_preview(app, ui, source.as_str(), &mut |ln| {
                find_scroll = Some(ln);
            });
            ui.add_space(4.0);

            let dbl_scroll = app
                .table_preview
                .as_mut()
                .and_then(|s| s.pending_scroll_line.take());
            let scroll_line = find_scroll.or(dbl_scroll);

            const FONT_SIZE: f32 = 13.0;
            let line_height = ui.fonts(|f| f.row_height(&egui::FontId::monospace(FONT_SIZE)));
            let highlighted = app
                .highlighter
                .highlight_source(&relative_path, &source, lang);
            let job = if app.in_view_find.open && !app.in_view_find.query.is_empty() {
                let spans = in_view_find::find_byte_spans(
                    source.as_str(),
                    &app.in_view_find.query,
                    app.in_view_find.case_sensitive,
                );
                build_layout_job_with_in_view_find(
                    highlighted,
                    &matches,
                    FONT_SIZE,
                    1,
                    source.as_str(),
                    &spans,
                    app.in_view_find.current,
                )
            } else {
                build_layout_job(highlighted, &matches, FONT_SIZE)
            };

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
