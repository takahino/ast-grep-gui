//! 検索ヒットの型バリエーション集計（受信・（任意で）メソッド・引数数・各引数の型）

use egui::{Label, RichText, Ui};

use crate::app::AstGrepApp;
use crate::search::build_match_variation_report;
use crate::ui::scroll_keyboard;

fn summary_column_widths(has_method: bool, max_arg_cols: usize) -> Vec<f32> {
    let mut v = if has_method {
        vec![72.0, 200.0, 200.0, 56.0]
    } else {
        vec![72.0, 240.0, 56.0]
    };
    v.extend(std::iter::repeat(140.0).take(max_arg_cols));
    v
}

pub fn show(app: &mut AstGrepApp, ui: &mut Ui) {
    let t = app.tr();
    if app.results.is_empty() {
        ui.label(t.summary_empty_results());
        return;
    }

    let Some(report) = build_match_variation_report(&app.pattern, &app.results) else {
        ui.label(t.summary_pattern_ineligible());
        return;
    };

    let show_method = report.method_metavar.is_some();

    ui.heading(t.summary_title());
    ui.label(t.summary_keys_explanation(
        &report.receiver_metavar,
        report.method_metavar.as_deref(),
        report.args_multi_metavar.as_deref(),
        &report.arg_single_metavars,
    ));
    ui.add_space(8.0);

    if report.rows.is_empty() {
        ui.label(t.summary_no_match_rows());
        return;
    }

    let max_arg_cols = report
        .rows
        .iter()
        .map(|r| r.arity)
        .max()
        .unwrap_or(0);

    let num_fixed = if show_method { 4 } else { 3 };
    let widths = summary_column_widths(show_method, max_arg_cols);
    let spacing_x = ui.spacing().item_spacing.x;
    let total_w: f32 =
        widths.iter().sum::<f32>() + spacing_x * (widths.len().saturating_sub(1)) as f32;

    let row_h = ui.text_style_height(&egui::TextStyle::Body).max(ui.spacing().interact_size.y);
    let header_h = row_h.max(ui.spacing().interact_size.y);

    let table_interact_rect = ui.available_rect_before_wrap();
    let ctx_table = ui.ctx().clone();
    let sid_h = scroll_keyboard::scroll_area_persistent_id(ui, "summary_view_h");
    scroll_keyboard::apply_keyboard_horizontal_scroll_before_show(
        &ctx_table,
        ui,
        sid_h,
        table_interact_rect,
        true,
        false,
    );

    let scroll_h_out = egui::ScrollArea::horizontal()
        .id_salt("summary_view_h")
        .max_height(table_interact_rect.height())
        .auto_shrink([false, false])
        .show(ui, |ui_h| {
            ui_h.vertical(|ui_v| {
                let ctx = ui_v.ctx().clone();
                let sid = scroll_keyboard::scroll_area_persistent_id(ui_v, "summary_view");
                scroll_keyboard::apply_keyboard_scroll_before_show(
                    &ctx,
                    ui_v,
                    sid,
                    table_interact_rect,
                    egui::Vec2b::from([true, true]),
                    false,
                );

                ui_v.horizontal(|ui| {
                    ui.set_min_width(total_w);
                    ui.add_sized(
                        [widths[0], header_h],
                        Label::new(RichText::new(t.summary_col_count()).strong()).truncate(),
                    );
                    ui.add_sized(
                        [widths[1], header_h],
                        Label::new(RichText::new(t.summary_col_receiver()).strong()).truncate(),
                    );
                    if show_method {
                        ui.add_sized(
                            [widths[2], header_h],
                            Label::new(RichText::new(t.summary_col_method()).strong()).truncate(),
                        );
                    }
                    let arity_wi = if show_method { 3 } else { 2 };
                    ui.add_sized(
                        [widths[arity_wi], header_h],
                        Label::new(RichText::new(t.summary_col_arity()).strong()).truncate(),
                    );
                    for i in 0..max_arg_cols {
                        let wi = num_fixed + i;
                        ui.add_sized(
                            [widths[wi], header_h],
                            Label::new(RichText::new(t.summary_col_arg(i)).strong()).truncate(),
                        );
                    }
                });

                ui_v.separator();

                let scroll_out = egui::ScrollArea::vertical()
                    .id_salt("summary_view")
                    .min_scrolled_height(8.0)
                    .max_height(ui_v.available_height())
                    .auto_shrink([false, false])
                    .show(ui_v, |ui| {
                        ui.set_min_width(total_w);
                        // ヘッダと同じ固定幅でセルを並べる（Grid だと列幅が一致せずずれる）
                        for (ri, row) in report.rows.iter().enumerate() {
                            let frame = if ri % 2 == 1 {
                                egui::Frame::none()
                                    .fill(ui.visuals().faint_bg_color)
                                    .inner_margin(0.0)
                            } else {
                                egui::Frame::none().inner_margin(0.0)
                            };
                            frame.show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.set_min_width(total_w);
                                    ui.add_sized(
                                        [widths[0], row_h],
                                        Label::new(RichText::new(row.count.to_string()).strong())
                                            .truncate(),
                                    );
                                    ui.add_sized(
                                        [widths[1], row_h],
                                        Label::new(&row.receiver_display).truncate(),
                                    );
                                    if show_method {
                                        ui.add_sized(
                                            [widths[2], row_h],
                                            Label::new(&row.method_display).truncate(),
                                        );
                                    }
                                    let arity_wi = if show_method { 3 } else { 2 };
                                    ui.add_sized(
                                        [widths[arity_wi], row_h],
                                        Label::new(row.arity.to_string()).truncate(),
                                    );
                                    for i in 0..max_arg_cols {
                                        let wi = num_fixed + i;
                                        let cell = if i < row.arity {
                                            row.arg_displays[i].as_str()
                                        } else {
                                            ""
                                        };
                                        ui.add_sized(
                                            [widths[wi], row_h],
                                            Label::new(cell).truncate(),
                                        );
                                    }
                                });
                            });
                        }
                    });
                scroll_keyboard::store_scroll_metrics(ui_v.ctx(), sid, &scroll_out, table_interact_rect);
            });
        });
    scroll_keyboard::store_horizontal_scroll_metrics(&ctx_table, sid_h, &scroll_h_out, table_interact_rect);
}
