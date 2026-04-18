use egui::Ui;

use crate::app::AstGrepApp;
use crate::help_html;
use crate::lang::presets_for;

pub fn show(app: &mut AstGrepApp, ctx: &egui::Context) {
    if !app.show_help {
        return;
    }

    let t = app.tr();
    // show_help を一時変数で取り出して借用競合を回避
    let mut show = app.show_help;
    let mut selected_pattern: Option<String> = None;

    egui::Window::new(t.help_window_title())
        .open(&mut show)
        .resizable(true)
        .default_width(520.0)
        .constrain_to(ctx.screen_rect())
        .show(ctx, |ui| {
            pattern_help_content(ui, app, &mut selected_pattern);
        });

    app.show_help = show;

    // パターンが選択された場合は適用して閉じる
    if let Some(pat) = selected_pattern {
        app.pattern = pat;
        app.show_help = false;
    }
}

fn pattern_help_content(ui: &mut Ui, app: &AstGrepApp, selected_pattern: &mut Option<String>) {
    let t = app.tr();
    ui.label(
        egui::RichText::new(t.help_popup_browser_blurb())
            .small()
            .color(ui.visuals().weak_text_color()),
    );
    ui.horizontal(|ui| {
        if ui
            .button(t.help_open_browser_btn())
            .on_hover_text(t.help_open_browser_tooltip())
            .clicked()
        {
            help_html::open_pattern_help_in_browser(app.ui_lang());
        }
    });
    ui.add_space(6.0);

    ui.heading(t.help_tips_heading());
    ui.separator();
    for tip in [t.help_tip_1(), t.help_tip_2(), t.help_tip_3()] {
        ui.horizontal(|ui| {
            ui.label("•");
            ui.label(egui::RichText::new(tip).small());
        });
    }
    ui.add_space(10.0);

    ui.heading(t.help_meta_heading());
    ui.separator();

    egui::Grid::new("help_grid")
        .num_columns(2)
        .striped(true)
        .spacing([20.0, 4.0])
        .show(ui, |ui| {
            ui.label(egui::RichText::new("$VAR").monospace().color(egui::Color32::YELLOW));
            ui.label(t.help_meta_var_single());
            ui.end_row();

            ui.label(egui::RichText::new("$$$ARGS").monospace().color(egui::Color32::YELLOW));
            ui.label(t.help_meta_multi());
            ui.end_row();

            ui.label(egui::RichText::new("$_").monospace().color(egui::Color32::YELLOW));
            ui.label(t.help_meta_ignore());
            ui.end_row();

            ui.label(egui::RichText::new(t.help_meta_same_var_key()).monospace().color(egui::Color32::YELLOW));
            ui.label(t.help_meta_same());
            ui.end_row();
        });

    ui.add_space(8.0);
    ui.heading(t.help_presets_heading());
    ui.separator();

    let presets = presets_for(app.selected_lang, app.ui_lang());
    for preset in &presets {
        ui.horizontal(|ui| {
            if ui.button(&preset.label).clicked() {
                *selected_pattern = Some(preset.pattern.to_string());
            }
            ui.label(
                egui::RichText::new(&preset.description)
                    .small()
                    .color(egui::Color32::GRAY),
            );
        });
    }

    ui.add_space(8.0);
    ui.heading(t.help_examples_heading());
    ui.separator();

    let examples = [
        ("fn $NAME($$$ARGS)", t.help_example_1_desc()),
        ("$VAR.unwrap()", t.help_example_2_desc()),
        ("console.log($$$ARGS)", t.help_example_3_desc()),
    ];

    for (pattern, desc) in &examples {
        ui.horizontal(|ui| {
            if ui
                .add(
                    egui::Button::new(egui::RichText::new(*pattern).monospace()).small(),
                )
                .clicked()
            {
                *selected_pattern = Some(pattern.to_string());
            }
            ui.label(egui::RichText::new(*desc).small().color(egui::Color32::GRAY));
        });
    }
}
