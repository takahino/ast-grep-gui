//! 検索ヒットの型バリエーション集計（受信・（任意で）メソッド・引数数・各引数の型）

use egui::{Label, RichText, Sense, Ui};

use crate::app::AstGrepApp;
use crate::search::build_match_variation_report;
use crate::type_hint_config::{
    PendingTypeHintRuleDraft, draft_from_hint_cell, enrich_method_draft_from_call_context,
    hint_rule_draft_source_from_display,
};
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

fn summary_arg_column_key(
    args_multi_metavar: Option<&str>,
    arg_single_metavars: &[String],
    arg_index: usize,
) -> String {
    if let Some(name) = args_multi_metavar {
        format!("{name}#{arg_index}")
    } else if let Some(name) = arg_single_metavars.get(arg_index) {
        name.clone()
    } else {
        format!("ARG#{arg_index}")
    }
}

fn hint_cell_with_context_menu(
    ui: &mut Ui,
    width: f32,
    row_h: f32,
    display: &str,
    column_key: &str,
    row_arity: usize,
    arg_displays: &[String],
    t: crate::i18n::Tr,
    open_type_hint_draft: &mut Option<PendingTypeHintRuleDraft>,
) {
    let response = ui.add_sized(
        [width, row_h],
        Label::new(display).truncate().sense(Sense::click()),
    );
    let Some(src) = hint_rule_draft_source_from_display(display) else {
        return;
    };
    let col_key = column_key.to_string();
    let kind_label = src.kind_label.clone();
    let snippet = src.snippet.clone();
    let arg_labels: Vec<String> = arg_displays.iter().take(row_arity).cloned().collect();
    response.context_menu(|ui| {
        if ui
            .button(t.table_add_type_hint_rule())
            .on_hover_text(t.table_add_type_hint_rule_tooltip())
            .clicked()
        {
            let mut draft = draft_from_hint_cell(&col_key, &kind_label, &snippet, "", 0);
            enrich_method_draft_from_call_context(&mut draft, row_arity, &arg_labels);
            *open_type_hint_draft = Some(draft);
            ui.close_menu();
        }
    });
}

pub fn show(app: &mut AstGrepApp, ui: &mut Ui) {
    let t = app.tr();
    if app.results.is_empty() {
        ui.label(t.summary_empty_results());
        return;
    }

    let Some(report) =
        build_match_variation_report(&app.pattern, &app.results, app.type_hints_enabled)
    else {
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

    let max_arg_cols = report.rows.iter().map(|r| r.arity).max().unwrap_or(0);

    let num_fixed = if show_method { 4 } else { 3 };
    let widths = summary_column_widths(show_method, max_arg_cols);
    let spacing_x = ui.spacing().item_spacing.x;
    let total_w: f32 =
        widths.iter().sum::<f32>() + spacing_x * (widths.len().saturating_sub(1)) as f32;

    let row_h = ui
        .text_style_height(&egui::TextStyle::Body)
        .max(ui.spacing().interact_size.y);
    let header_h = row_h.max(ui.spacing().interact_size.y);

    let receiver_key = report.receiver_metavar.clone();
    let method_key = report.method_metavar.clone();
    let args_multi = report.args_multi_metavar.clone();
    let arg_singles = report.arg_single_metavars.clone();

    let mut open_type_hint_draft: Option<PendingTypeHintRuleDraft> = None;

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
                                    hint_cell_with_context_menu(
                                        ui,
                                        widths[1],
                                        row_h,
                                        &row.receiver_display,
                                        &receiver_key,
                                        row.arity,
                                        &row.arg_displays,
                                        t,
                                        &mut open_type_hint_draft,
                                    );
                                    if show_method {
                                        if let Some(ref mk) = method_key {
                                            hint_cell_with_context_menu(
                                                ui,
                                                widths[2],
                                                row_h,
                                                &row.method_display,
                                                mk,
                                                row.arity,
                                                &row.arg_displays,
                                                t,
                                                &mut open_type_hint_draft,
                                            );
                                        } else {
                                            ui.add_sized(
                                                [widths[2], row_h],
                                                Label::new(&row.method_display).truncate(),
                                            );
                                        }
                                    }
                                    let arity_wi = if show_method { 3 } else { 2 };
                                    ui.add_sized(
                                        [widths[arity_wi], row_h],
                                        Label::new(row.arity.to_string()).truncate(),
                                    );
                                    for i in 0..max_arg_cols {
                                        let wi = num_fixed + i;
                                        if i < row.arity {
                                            let col_key = summary_arg_column_key(
                                                args_multi.as_deref(),
                                                &arg_singles,
                                                i,
                                            );
                                            hint_cell_with_context_menu(
                                                ui,
                                                widths[wi],
                                                row_h,
                                                &row.arg_displays[i],
                                                &col_key,
                                                row.arity,
                                                &row.arg_displays,
                                                t,
                                                &mut open_type_hint_draft,
                                            );
                                        } else {
                                            ui.add_sized(
                                                [widths[wi], row_h],
                                                Label::new("").truncate(),
                                            );
                                        }
                                    }
                                });
                            });
                        }
                    });
                scroll_keyboard::store_scroll_metrics(
                    ui_v.ctx(),
                    sid,
                    &scroll_out,
                    table_interact_rect,
                );
            });
        });
    scroll_keyboard::store_horizontal_scroll_metrics(
        &ctx_table,
        sid_h,
        &scroll_h_out,
        table_interact_rect,
    );

    if let Some(draft) = open_type_hint_draft {
        app.open_type_hint_config_with_draft(draft);
    }
}
