use regex::Regex;

use crate::app::AstGrepApp;
use crate::regex_visualizer::{
    visualize_regex, RegexDiagram, RegexDiagramNode, RegexVisualKind,
};
use crate::search::SearchMode;

pub fn show(app: &mut AstGrepApp, ctx: &egui::Context) {
    if app.search_mode != SearchMode::Regex {
        app.show_regex_visualizer = false;
        return;
    }

    if !app.show_regex_visualizer {
        return;
    }

    let t = app.tr();
    let mut show = app.show_regex_visualizer;

    egui::Window::new(t.regex_visualizer_window_title())
        .open(&mut show)
        .resizable(true)
        .default_size([760.0, 560.0])
        .show(ctx, |ui| {
            regex_visualizer_content(ui, app);
        });

    app.show_regex_visualizer = show;
}

fn regex_visualizer_content(ui: &mut egui::Ui, app: &mut AstGrepApp) {
    let t = app.tr();

    ui.label(
        egui::RichText::new(t.regex_visualizer_intro())
            .small()
            .color(egui::Color32::GRAY),
    );
    ui.add_space(6.0);

    ui.label(t.regex_visualizer_pattern_label());
    ui.add(
        egui::TextEdit::multiline(&mut app.pattern)
            .desired_width(f32::INFINITY)
            .desired_rows(3)
            .font(egui::TextStyle::Monospace)
            .hint_text(t.pattern_hint_regex()),
    );

    ui.add_space(6.0);
    ui.label(t.regex_visualizer_test_label());
    ui.add(
        egui::TextEdit::multiline(&mut app.regex_visualizer_test_text)
            .desired_width(f32::INFINITY)
            .desired_rows(4)
            .font(egui::TextStyle::Monospace)
            .hint_text(t.regex_visualizer_test_hint()),
    );

    let vis = visualize_regex(&app.pattern, app.ui_lang());

    ui.add_space(8.0);

    let status_text = if vis.is_valid {
        t.regex_visualizer_status_ok()
    } else {
        t.regex_visualizer_status_error()
    };
    let status_color = if vis.is_valid {
        egui::Color32::from_rgb(100, 200, 120)
    } else {
        egui::Color32::from_rgb(220, 100, 100)
    };
    ui.label(egui::RichText::new(status_text).strong().color(status_color));
    if let Some(err) = &vis.compile_error {
        ui.label(
            egui::RichText::new(err)
                .small()
                .color(egui::Color32::from_rgb(220, 120, 120)),
        );
    }

    ui.label(
        egui::RichText::new(t.regex_visualizer_summary(
            vis.stats.groups,
            vis.stats.alternations,
            vis.stats.char_classes,
            vis.stats.quantifiers,
        ))
        .small()
        .color(egui::Color32::GRAY),
    );

    show_regex_test_matches(ui, app, vis.is_valid);

    if app.pattern.trim().is_empty() {
        ui.separator();
        ui.label(
            egui::RichText::new(t.regex_visualizer_empty())
                .small()
                .color(egui::Color32::GRAY),
        );
        return;
    }

    ui.separator();
    ui.heading(t.regex_visualizer_automaton_heading());
    ui.label(
        egui::RichText::new(&vis.diagram.note)
            .small()
            .color(egui::Color32::GRAY),
    );
    ui.add_space(6.0);
    draw_diagram(ui, &vis.diagram);
    ui.add_space(10.0);
    ui.separator();

    egui::ScrollArea::vertical()
        .id_salt("regex_visualizer_lines")
        .show(ui, |ui| {
            for line in vis.lines {
                ui.horizontal_wrapped(|ui| {
                    ui.add_space((line.depth as f32) * 18.0);
                    ui.label(
                        egui::RichText::new(line.label)
                            .small()
                            .color(kind_color(line.kind)),
                    );
                    ui.label(egui::RichText::new(line.token).monospace());
                    if !line.note.is_empty() {
                        ui.label(
                            egui::RichText::new(line.note)
                                .small()
                                .color(egui::Color32::GRAY),
                        );
                    }
                });
                ui.add_space(2.0);
            }
        });
}

const MAX_REGEX_TEST_MATCHES: usize = 100;

fn show_regex_test_matches(ui: &mut egui::Ui, app: &AstGrepApp, pattern_ok: bool) {
    let t = app.tr();
    let hay = app.regex_visualizer_test_text.as_str();
    if hay.trim().is_empty() {
        return;
    }
    if !pattern_ok || app.pattern.trim().is_empty() {
        return;
    }
    let Ok(re) = Regex::new(app.pattern.trim()) else {
        return;
    };

    ui.add_space(6.0);
    ui.separator();
    ui.heading(t.regex_visualizer_test_matches_heading());
    let matches: Vec<_> = re.find_iter(hay).take(MAX_REGEX_TEST_MATCHES).collect();
    if matches.is_empty() {
        ui.label(
            egui::RichText::new(t.regex_visualizer_test_no_matches())
                .small()
                .color(egui::Color32::from_rgb(180, 180, 180)),
        );
        return;
    }
    ui.label(
        egui::RichText::new(t.regex_visualizer_test_count(matches.len()))
            .small()
            .color(egui::Color32::GRAY),
    );
    if matches.len() == MAX_REGEX_TEST_MATCHES {
        ui.label(
            egui::RichText::new(t.regex_visualizer_test_match_truncated())
                .small()
                .color(egui::Color32::from_rgb(160, 160, 140)),
        );
    }

    egui::ScrollArea::vertical()
        .id_salt("regex_visualizer_test_matches")
        .max_height(160.0)
        .show(ui, |ui| {
            for (i, m) in matches.iter().enumerate() {
                let n = i + 1;
                let range = m.range();
                let text = m.as_str();
                let preview = if text.chars().count() > 120 {
                    let s: String = text.chars().take(120).collect();
                    format!("{s}…")
                } else {
                    text.to_string()
                };
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!("#{n}"))
                            .small()
                            .color(egui::Color32::from_rgb(140, 180, 220)),
                    );
                    ui.label(
                        egui::RichText::new(format!("{}..{}", range.start, range.end))
                            .small()
                            .monospace()
                            .color(egui::Color32::GRAY),
                    );
                });
                ui.label(
                    egui::RichText::new(preview)
                        .small()
                        .monospace()
                        .color(egui::Color32::LIGHT_GRAY),
                );
                ui.add_space(4.0);
            }
        });
}

fn draw_diagram(ui: &mut egui::Ui, diagram: &RegexDiagram) {
    let size = measure_node(&diagram.root);
    let canvas = egui::vec2(
        (size.x + 120.0).max(ui.available_width()),
        (size.y + 60.0).max(180.0),
    );

    egui::ScrollArea::horizontal()
        .id_salt("regex_visualizer_diagram_scroll")
        .show(ui, |ui| {
            let (rect, _) = ui.allocate_exact_size(canvas, egui::Sense::hover());
            let painter = ui.painter_at(rect);
            let start = egui::pos2(rect.min.x + 30.0, rect.center().y);
            let end = egui::pos2(start.x + size.x + 60.0, start.y);
            let root_top = rect.center().y - size.y * 0.5;

            painter.circle_filled(start, 5.0, egui::Color32::from_rgb(90, 130, 220));
            painter.circle_filled(end, 5.0, egui::Color32::from_rgb(90, 180, 120));

            let entry = egui::pos2(start.x + 20.0, start.y);
            let exit = egui::pos2(end.x - 20.0, end.y);
            painter.line_segment(
                [start, entry],
                egui::Stroke::new(2.0, egui::Color32::LIGHT_GRAY),
            );
            painter.line_segment(
                [exit, end],
                egui::Stroke::new(2.0, egui::Color32::LIGHT_GRAY),
            );
            let mut group_id = 0usize;
            let (node_entry, node_exit) = draw_node(
                ui,
                &painter,
                &diagram.root,
                entry.x,
                root_top,
                &mut group_id,
            );
            painter.line_segment(
                [entry, node_entry],
                egui::Stroke::new(2.0, egui::Color32::LIGHT_GRAY),
            );
            painter.line_segment(
                [node_exit, exit],
                egui::Stroke::new(2.0, egui::Color32::LIGHT_GRAY),
            );
        });
}

fn measure_node(node: &RegexDiagramNode) -> egui::Vec2 {
    match node {
        RegexDiagramNode::Empty => egui::vec2(40.0, 32.0),
        RegexDiagramNode::Token { label, .. } => {
            let width = (label.chars().count().max(1) as f32) * 9.0 + 28.0;
            egui::vec2(width.clamp(42.0, 240.0), 32.0)
        }
        RegexDiagramNode::Sequence(items) => {
            let gap = 18.0;
            let mut width: f32 = 0.0;
            let mut height: f32 = 0.0;
            for (idx, item) in items.iter().enumerate() {
                let size = measure_node(item);
                width += size.x;
                if idx > 0 {
                    width += gap;
                }
                height = height.max(size.y);
            }
            egui::vec2(width.max(40.0), height.max(32.0))
        }
        RegexDiagramNode::Alternation(branches) => {
            let lane_gap = 28.0;
            let mut width: f32 = 0.0;
            let mut height = 0.0;
            for (idx, branch) in branches.iter().enumerate() {
                let size = measure_node(branch);
                width = width.max(size.x);
                height += size.y;
                if idx > 0 {
                    height += lane_gap;
                }
            }
            egui::vec2(width + 80.0, height.max(32.0))
        }
        RegexDiagramNode::Repeat { child, .. } => {
            let size = measure_node(child);
            egui::vec2(size.x + 30.0, size.y + 56.0)
        }
        RegexDiagramNode::Group { child, .. } => {
            const PAD: f32 = 10.0;
            const TITLE_H: f32 = 22.0;
            let inner = measure_node(child);
            let min_w = (inner.x + PAD * 2.0).max(120.0);
            egui::vec2(min_w, inner.y + PAD * 2.0 + TITLE_H)
        }
    }
}

fn draw_node(
    ui: &mut egui::Ui,
    painter: &egui::Painter,
    node: &RegexDiagramNode,
    x: f32,
    y: f32,
    group_id: &mut usize,
) -> (egui::Pos2, egui::Pos2) {
    match node {
        RegexDiagramNode::Empty => {
            let h = 32.0;
            let start = egui::pos2(x, y + h * 0.5);
            let end = egui::pos2(x + 40.0, y + h * 0.5);
            painter.line_segment([start, end], egui::Stroke::new(1.5, egui::Color32::GRAY));
            painter.text(
                egui::pos2((start.x + end.x) * 0.5, start.y - 4.0),
                egui::Align2::CENTER_BOTTOM,
                "ε",
                egui::FontId::monospace(12.0),
                egui::Color32::GRAY,
            );
            (start, end)
        }
        RegexDiagramNode::Token { label, kind } => {
            let size = measure_node(node);
            let rect = egui::Rect::from_min_size(egui::pos2(x, y), size);
            painter.rect_filled(rect, 6.0, token_fill(*kind));
            painter.rect_stroke(
                rect,
                6.0,
                egui::Stroke::new(1.0, token_stroke(*kind)),
            );
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::monospace(12.0),
                egui::Color32::WHITE,
            );
            (
                egui::pos2(rect.left(), rect.center().y),
                egui::pos2(rect.right(), rect.center().y),
            )
        }
        RegexDiagramNode::Sequence(items) => {
            let total = measure_node(node);
            let mut cursor_x = x;
            let mut prev_exit = None;
            let gap = 18.0;
            let center_y = y + total.y * 0.5;
            let mut first_entry = egui::pos2(x, center_y);
            let mut last_exit = egui::pos2(x + total.x, center_y);

            for (idx, item) in items.iter().enumerate() {
                let size = measure_node(item);
                let child_y = y + (total.y - size.y) * 0.5;
                let (entry, exit) = draw_node(ui, painter, item, cursor_x, child_y, group_id);
                if idx == 0 {
                    first_entry = entry;
                }
                if let Some(prev) = prev_exit {
                    painter.line_segment(
                        [prev, entry],
                        egui::Stroke::new(2.0, egui::Color32::LIGHT_GRAY),
                    );
                }
                prev_exit = Some(exit);
                last_exit = exit;
                cursor_x += size.x + gap;
            }

            (first_entry, last_exit)
        }
        RegexDiagramNode::Alternation(branches) => {
            let total = measure_node(node);
            let split_x = x + 16.0;
            let merge_x = x + total.x - 16.0;
            let center_y = y + total.y * 0.5;
            let entry = egui::pos2(x, center_y);
            let exit = egui::pos2(x + total.x, center_y);
            let mut branch_centers = Vec::new();
            let mut offset_y = y;

            for (idx, branch) in branches.iter().enumerate() {
                let size = measure_node(branch);
                let (branch_entry, branch_exit) =
                    draw_node(ui, painter, branch, x + 40.0, offset_y, group_id);
                let cy = branch_entry.y;
                branch_centers.push(cy);
                painter.line_segment(
                    [egui::pos2(split_x, cy), branch_entry],
                    egui::Stroke::new(2.0, egui::Color32::LIGHT_GRAY),
                );
                painter.line_segment(
                    [branch_exit, egui::pos2(merge_x, cy)],
                    egui::Stroke::new(2.0, egui::Color32::LIGHT_GRAY),
                );
                offset_y += size.y;
                if idx + 1 < branches.len() {
                    offset_y += 28.0;
                }
            }

            if let (Some(min_y), Some(max_y)) = (
                branch_centers.iter().cloned().reduce(f32::min),
                branch_centers.iter().cloned().reduce(f32::max),
            ) {
                painter.line_segment(
                    [entry, egui::pos2(split_x, center_y)],
                    egui::Stroke::new(2.0, egui::Color32::LIGHT_GRAY),
                );
                painter.line_segment(
                    [egui::pos2(split_x, min_y), egui::pos2(split_x, max_y)],
                    egui::Stroke::new(2.0, egui::Color32::from_rgb(255, 170, 170)),
                );
                painter.line_segment(
                    [egui::pos2(merge_x, min_y), egui::pos2(merge_x, max_y)],
                    egui::Stroke::new(2.0, egui::Color32::from_rgb(255, 170, 170)),
                );
                painter.line_segment(
                    [egui::pos2(merge_x, center_y), exit],
                    egui::Stroke::new(2.0, egui::Color32::LIGHT_GRAY),
                );
            }

            (entry, exit)
        }
        RegexDiagramNode::Group { child, meta } => {
            const PAD: f32 = 10.0;
            const TITLE_H: f32 = 22.0;
            let this_group_id = *group_id;
            *group_id += 1;
            let total = measure_node(node);
            let inner_size = measure_node(child);
            let frame_rect = egui::Rect::from_min_size(egui::pos2(x, y), total);
            painter.rect_filled(
                frame_rect,
                4.0,
                egui::Color32::from_rgba_unmultiplied(45, 45, 40, 140),
            );
            painter.rect_stroke(
                frame_rect,
                4.0,
                egui::Stroke::new(1.5, egui::Color32::from_rgb(175, 165, 115)),
            );
            painter.text(
                egui::pos2(frame_rect.min.x + PAD, frame_rect.min.y + 4.0),
                egui::Align2::LEFT_TOP,
                &meta.title,
                egui::FontId::proportional(11.0),
                egui::Color32::from_rgb(210, 205, 175),
            );
            let inner_x = x + PAD;
            let inner_y = y + PAD + TITLE_H;
            let (child_entry, child_exit) =
                draw_node(ui, painter, child, inner_x, inner_y, group_id);
            let center_y = inner_y + inner_size.y * 0.5;
            let entry = egui::pos2(frame_rect.left(), center_y);
            let exit = egui::pos2(frame_rect.right(), center_y);
            painter.line_segment(
                [entry, child_entry],
                egui::Stroke::new(2.0, egui::Color32::LIGHT_GRAY),
            );
            painter.line_segment(
                [child_exit, exit],
                egui::Stroke::new(2.0, egui::Color32::LIGHT_GRAY),
            );

            let response = ui.interact(
                frame_rect,
                ui.id().with("regex_viz_group").with(this_group_id),
                egui::Sense::hover(),
            );
            response.on_hover_text(&meta.tooltip);

            (entry, exit)
        }
        RegexDiagramNode::Repeat { child, quantifier } => {
            let total = measure_node(node);
            let child_size = measure_node(child);
            let child_x = x + 15.0;
            let child_y = y + 24.0;
            let entry = egui::pos2(x, child_y + child_size.y * 0.5);
            let exit = egui::pos2(x + total.x, child_y + child_size.y * 0.5);
            let (child_entry, child_exit) =
                draw_node(ui, painter, child, child_x, child_y, group_id);
            painter.line_segment(
                [entry, child_entry],
                egui::Stroke::new(2.0, egui::Color32::LIGHT_GRAY),
            );
            painter.line_segment(
                [child_exit, exit],
                egui::Stroke::new(2.0, egui::Color32::LIGHT_GRAY),
            );

            if allows_zero(quantifier) {
                let bypass_y = y + total.y - 12.0;
                let stroke = egui::Stroke::new(1.6, egui::Color32::from_rgb(220, 200, 120));
                let left = egui::pos2(entry.x + 10.0, bypass_y);
                let right = egui::pos2(exit.x - 10.0, bypass_y);
                painter.line_segment([entry, left], stroke);
                painter.line_segment([left, right], stroke);
                painter.line_segment([right, exit], stroke);
                painter.text(
                    egui::pos2((left.x + right.x) * 0.5, bypass_y + 2.0),
                    egui::Align2::CENTER_TOP,
                    "skip",
                    egui::FontId::monospace(11.0),
                    egui::Color32::from_rgb(220, 200, 120),
                );
            }

            if repeats_more(quantifier) {
                let loop_y = y + 10.0;
                let stroke = egui::Stroke::new(1.6, egui::Color32::from_rgb(140, 220, 140));
                let left = egui::pos2(child_entry.x + 6.0, loop_y);
                let right = egui::pos2(child_exit.x - 6.0, loop_y);
                painter.line_segment([child_exit, right], stroke);
                painter.line_segment([right, left], stroke);
                painter.line_segment([left, child_entry], stroke);
                painter.text(
                    egui::pos2((left.x + right.x) * 0.5, loop_y - 2.0),
                    egui::Align2::CENTER_BOTTOM,
                    quantifier,
                    egui::FontId::monospace(11.0),
                    egui::Color32::from_rgb(140, 220, 140),
                );
            }

            (entry, exit)
        }
    }
}

fn allows_zero(quantifier: &str) -> bool {
    quantifier == "*" || quantifier == "?" || quantifier.starts_with("{0")
}

fn repeats_more(quantifier: &str) -> bool {
    quantifier == "*" || quantifier == "+" || quantifier.contains(',')
}

fn token_fill(kind: RegexVisualKind) -> egui::Color32 {
    match kind {
        RegexVisualKind::Anchor => egui::Color32::from_rgb(65, 80, 125),
        RegexVisualKind::Group | RegexVisualKind::GroupEnd => egui::Color32::from_rgb(110, 95, 55),
        RegexVisualKind::Alternation => egui::Color32::from_rgb(125, 70, 70),
        RegexVisualKind::Quantifier => egui::Color32::from_rgb(70, 110, 70),
        RegexVisualKind::CharClass => egui::Color32::from_rgb(60, 105, 120),
        RegexVisualKind::Escape => egui::Color32::from_rgb(95, 75, 130),
        RegexVisualKind::Wildcard => egui::Color32::from_rgb(120, 115, 60),
        RegexVisualKind::Literal => egui::Color32::from_rgb(70, 70, 75),
        RegexVisualKind::Error => egui::Color32::from_rgb(140, 60, 60),
    }
}

fn token_stroke(kind: RegexVisualKind) -> egui::Color32 {
    match kind {
        RegexVisualKind::Anchor => egui::Color32::from_rgb(170, 190, 255),
        RegexVisualKind::Group | RegexVisualKind::GroupEnd => egui::Color32::from_rgb(255, 210, 120),
        RegexVisualKind::Alternation => egui::Color32::from_rgb(255, 170, 170),
        RegexVisualKind::Quantifier => egui::Color32::from_rgb(160, 240, 160),
        RegexVisualKind::CharClass => egui::Color32::from_rgb(150, 230, 240),
        RegexVisualKind::Escape => egui::Color32::from_rgb(210, 180, 255),
        RegexVisualKind::Wildcard => egui::Color32::from_rgb(235, 220, 140),
        RegexVisualKind::Literal => egui::Color32::from_rgb(220, 220, 220),
        RegexVisualKind::Error => egui::Color32::from_rgb(255, 140, 140),
    }
}

fn kind_color(kind: RegexVisualKind) -> egui::Color32 {
    match kind {
        RegexVisualKind::Anchor => egui::Color32::from_rgb(180, 180, 255),
        RegexVisualKind::Group | RegexVisualKind::GroupEnd => {
            egui::Color32::from_rgb(255, 210, 120)
        }
        RegexVisualKind::Alternation => egui::Color32::from_rgb(255, 160, 160),
        RegexVisualKind::Quantifier => egui::Color32::from_rgb(140, 220, 140),
        RegexVisualKind::CharClass => egui::Color32::from_rgb(140, 210, 220),
        RegexVisualKind::Escape => egui::Color32::from_rgb(200, 180, 255),
        RegexVisualKind::Wildcard => egui::Color32::from_rgb(220, 220, 120),
        RegexVisualKind::Literal => egui::Color32::from_rgb(200, 200, 200),
        RegexVisualKind::Error => egui::Color32::from_rgb(240, 110, 110),
    }
}
