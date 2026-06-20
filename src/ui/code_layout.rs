use egui::{Color32, Sense, Ui};

use crate::highlight::CodeLayoutJobs;

/// 行番号ガター（非選択）とソース本文（選択可能）を横並びで表示する。
pub fn show_selectable_code(ui: &mut Ui, jobs: CodeLayoutJobs) -> egui::Response {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let gutter_galley = ui.fonts(|f| f.layout_job(jobs.gutter));
        ui.add(egui::Label::new(gutter_galley).selectable(false));
        let code_galley = ui.fonts(|f| f.layout_job(jobs.code));
        ui.add(egui::Label::new(code_galley).selectable(true))
    })
    .inner
}

/// テーブル行など、カスタム painter 描画向けの分離レイアウト表示。
pub fn paint_split_layout_job_cell(
    ui: &mut Ui,
    width: f32,
    height: f32,
    jobs: CodeLayoutJobs,
    fallback_color: Color32,
    sense: Sense,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::Vec2::new(width, height), sense);
    let gutter_galley = ui.fonts(|fonts| fonts.layout_job(jobs.gutter));
    let code_galley = ui.fonts(|fonts| fonts.layout_job(jobs.code));
    let pos = rect.min + egui::vec2(4.0, 0.0);
    let gutter_width = gutter_galley.size().x;
    ui.painter()
        .with_clip_rect(rect)
        .galley(pos, gutter_galley, fallback_color);
    ui.painter().with_clip_rect(rect).galley(
        pos + egui::vec2(gutter_width, 0.0),
        code_galley,
        fallback_color,
    );
    response
}
