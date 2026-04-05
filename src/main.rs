#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod export;
mod file_encoding;
mod help_html;
mod highlight;
mod i18n;
mod lang;
mod pattern_assist;
mod receiver_hint;
mod search;
mod sg_command;
mod terminal;
mod ui;

use app::AstGrepApp;

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("ast-grep GUI")
            .with_inner_size([1024.0, 700.0])
            .with_min_inner_size([600.0, 400.0]),
        ..Default::default()
    };

    eframe::run_native(
        "ast-grep GUI",
        native_options,
        Box::new(|cc| {
            // 日本語フォントの設定
            setup_custom_fonts(&cc.egui_ctx);
            Ok(Box::new(AstGrepApp::new(cc)))
        }),
    )
}

fn setup_custom_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    // 日本語フォントをバンドル（assets/NotoSansJP-Regular.ttf）
    let font_data = include_bytes!("../assets/NotoSansJP-Regular.ttf");
    fonts.font_data.insert(
        "NotoSansJP".to_owned(),
        egui::FontData::from_static(font_data),
    );

    // 日本語フォントをプロポーショナルとモノスペースの両方に追加
    fonts
        .families
        .get_mut(&egui::FontFamily::Proportional)
        .unwrap()
        .push("NotoSansJP".to_owned());

    fonts
        .families
        .get_mut(&egui::FontFamily::Monospace)
        .unwrap()
        .push("NotoSansJP".to_owned());

    ctx.set_fonts(fonts);
}
