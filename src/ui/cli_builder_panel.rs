//! コマンドライン一括検索の組み立て補助画面

use std::path::PathBuf;

use eframe::egui;

use crate::app::AstGrepApp;
use crate::cli_config::{default_cli_exe_name, format_cli_command, BatchRunRequest};
use crate::export::{OutputFormat, OutputView, OutputViewSet};

#[derive(Debug, Clone)]
pub struct CliBuilderState {
    pub patterns_file: String,
    pub output_path: String,
    pub view_code: bool,
    pub view_table: bool,
    pub view_summary: bool,
    pub format: OutputFormat,
    pub status_note: Option<String>,
}

impl Default for CliBuilderState {
    fn default() -> Self {
        Self {
            patterns_file: String::new(),
            output_path: String::new(),
            view_code: false,
            view_table: true,
            view_summary: false,
            format: OutputFormat::Json,
            status_note: None,
        }
    }
}

impl CliBuilderState {
    pub fn views(&self) -> OutputViewSet {
        let mut set = OutputViewSet::new();
        if self.view_code {
            set.insert(OutputView::Code);
        }
        if self.view_table {
            set.insert(OutputView::Table);
        }
        if self.view_summary {
            set.insert(OutputView::Summary);
        }
        if set.is_empty() {
            set.insert(OutputView::Table);
        }
        set
    }

    pub fn to_request(&self, app: &AstGrepApp) -> BatchRunRequest {
        use crate::cli_config::BatchCommonOptions;
        use crate::search::SearchMode;
        BatchRunRequest {
            patterns_file: PathBuf::from(&self.patterns_file),
            common: BatchCommonOptions {
                search_dir: app.search_dir.clone(),
                search_target_mode: app.search_target_mode,
                remote_target: app.remote_target.clone(),
                selected_lang: app.selected_lang,
                context_lines: app.context_lines,
                file_filter: app.file_filter.clone(),
                file_encoding_preference: app.file_encoding_preference,
                max_file_size_mb: app.max_file_size_mb,
                max_search_hits: app.max_search_hits,
                skip_dirs: app.skip_dirs.clone(),
                search_mode: SearchMode::AstGrep,
                plain_text_options: app.plain_text_options,
                cpp_include_dirs: app.cpp_include_dirs.clone(),
                type_hints_enabled: app.type_hints_enabled,
            },
            views: self.views(),
            format: self.format,
            output: if self.output_path.trim().is_empty() {
                None
            } else {
                Some(PathBuf::from(self.output_path.trim()))
            },
            ui_lang: app.ui_lang(),
        }
    }
}

pub fn show(app: &mut AstGrepApp, ctx: &egui::Context) {
    if !app.show_cli_builder {
        return;
    }

    let t = app.tr();
    let mut open = app.show_cli_builder;
    let mut run_clicked = false;
    let mut copy_clicked = false;

    egui::Window::new(t.cli_builder_title())
        .open(&mut open)
        .default_width(640.0)
        .show(ctx, |ui| {
            ui.label(t.cli_builder_description());

            ui.horizontal(|ui| {
                ui.label(t.cli_builder_patterns_file());
                ui.add(
                    egui::TextEdit::singleline(&mut app.cli_builder.patterns_file)
                        .desired_width(360.0)
                        .hint_text(t.cli_builder_patterns_hint()),
                );
                if ui.button(t.browse()).clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Text", &["txt", "patterns", ""])
                        .pick_file()
                    {
                        app.cli_builder.patterns_file = path.to_string_lossy().to_string();
                    }
                }
            });

            ui.horizontal(|ui| {
                ui.label(t.cli_builder_output_file());
                ui.add(
                    egui::TextEdit::singleline(&mut app.cli_builder.output_path)
                        .desired_width(360.0)
                        .hint_text(t.cli_builder_output_hint()),
                );
                if ui.button(t.browse()).clicked() {
                    let mut dlg = rfd::FileDialog::new();
                    dlg = match app.cli_builder.format {
                        OutputFormat::Xlsx => dlg.add_filter("Excel", &["xlsx"]),
                        OutputFormat::Json => dlg.add_filter("JSON", &["json"]),
                        OutputFormat::Markdown => dlg.add_filter("Markdown", &["md", "markdown"]),
                        OutputFormat::Html => dlg.add_filter("HTML", &["html"]),
                        OutputFormat::Text => dlg.add_filter("Text", &["txt"]),
                    };
                    if let Some(path) = dlg.save_file() {
                        app.cli_builder.output_path = path.to_string_lossy().to_string();
                    }
                }
            });

            ui.separator();
            ui.label(t.cli_builder_views_label());
            ui.horizontal(|ui| {
                ui.checkbox(&mut app.cli_builder.view_code, t.view_code());
                ui.checkbox(&mut app.cli_builder.view_table, t.view_table());
                ui.checkbox(&mut app.cli_builder.view_summary, t.view_summary());
            });

            ui.horizontal(|ui| {
                ui.label(t.cli_builder_format_label());
                egui::ComboBox::from_id_salt("cli_builder_format")
                    .selected_text(format_label(app.cli_builder.format))
                    .show_ui(ui, |ui| {
                        for fmt in [
                            OutputFormat::Json,
                            OutputFormat::Text,
                            OutputFormat::Markdown,
                            OutputFormat::Html,
                            OutputFormat::Xlsx,
                        ] {
                            ui.selectable_value(
                                &mut app.cli_builder.format,
                                fmt,
                                format_label(fmt),
                            );
                        }
                    });
            });

            ui.separator();
            ui.label(t.cli_builder_inherited_settings());
            ui.label(format!(
                "{}: {}",
                t.directory_label(),
                if app.search_dir.is_empty() {
                    "—"
                } else {
                    app.search_dir.as_str()
                }
            ));
            ui.label(format!(
                "{}: {}",
                t.search_lang_label(),
                app.selected_lang.combo_label(app.ui_lang())
            ));

            let req = app.cli_builder.to_request(app);
            let cmd = format_cli_command(&req, default_cli_exe_name());

            ui.separator();
            ui.label(t.cli_builder_command_preview());
            ui.add(
                egui::Label::new(egui::RichText::new(&cmd).monospace().size(12.0)).wrap(),
            );

            if let Some(note) = &app.cli_builder.status_note {
                ui.colored_label(egui::Color32::LIGHT_GREEN, note);
            }

            ui.horizontal(|ui| {
                if ui.button(t.cli_builder_copy()).clicked() {
                    copy_clicked = true;
                }
                let can_run = app.batch_runner.is_none()
                    && !matches!(
                        app.search_state,
                        crate::app::SearchState::Running | crate::app::SearchState::FetchingRemote(_)
                    )
                    && !app.cli_builder.patterns_file.trim().is_empty()
                    && !app.search_dir.trim().is_empty()
                    && !(app.cli_builder.format.requires_output_file()
                        && app.cli_builder.output_path.trim().is_empty());
                if ui
                    .add_enabled(can_run, egui::Button::new(t.cli_builder_run()))
                    .on_hover_text(t.cli_builder_run_tooltip())
                    .clicked()
                {
                    run_clicked = true;
                }
            });
        });

    app.show_cli_builder = open;

    if copy_clicked {
        if let Err(e) = crate::export::copy_to_clipboard(&format_cli_command(
            &app.cli_builder.to_request(app),
            default_cli_exe_name(),
        )) {
            app.cli_builder.status_note = Some(format!("{} {e}", app.tr().err_clipboard()));
        } else {
            app.cli_builder.status_note = Some(app.tr().cli_builder_copied().to_string());
        }
    }

    if run_clicked {
        let req = app.cli_builder.to_request(app);
        match app.start_batch_from_cli_builder(req) {
            Ok(()) => {
                app.show_cli_builder = false;
            }
            Err(e) => {
                app.cli_builder.status_note = Some(e);
            }
        }
    }
}

fn format_label(fmt: OutputFormat) -> &'static str {
    match fmt {
        OutputFormat::Text => "Text",
        OutputFormat::Json => "JSON",
        OutputFormat::Markdown => "Markdown",
        OutputFormat::Html => "HTML",
        OutputFormat::Xlsx => "Excel (.xlsx)",
    }
}
