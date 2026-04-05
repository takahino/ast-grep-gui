//! ScrollArea に PageUp/PageDown/矢印キーでのスクロールを追加する。
//! egui の ScrollArea はホイール時のみオフセットを更新するため、永続 State を事前に書き換える。

use egui::containers::scroll_area::{ScrollAreaOutput, State};
use egui::{Context, Id, Rect, Ui, Vec2, Vec2b};

#[derive(Clone, Copy)]
struct ScrollMetrics {
    content_size: Vec2,
    /// ビューポートの高さ（1 ページ分の PageDown 量）
    inner_h: f32,
}

fn inner_height_or_fallback(inner_rect: Rect, fallback: Rect) -> f32 {
    let mut h = inner_rect.height();
    if h <= 0.0 {
        h = fallback.height().max(1.0);
    }
    h
}

pub fn scroll_area_persistent_id(ui: &Ui, id_salt: impl std::hash::Hash) -> Id {
    ui.make_persistent_id(egui::Id::new(id_salt))
}

pub fn apply_keyboard_scroll_before_show(
    ctx: &Context,
    ui: &Ui,
    scroll_area_id: Id,
    interaction_rect: Rect,
    scroll_enabled: Vec2b,
) {
    if !ui.rect_contains_pointer(interaction_rect) {
        return;
    }

    let metrics_id = scroll_area_id.with("scroll_kb_metrics");
    let Some(metrics) = ctx.data(|d| d.get_temp::<ScrollMetrics>(metrics_id)) else {
        return;
    };

    let inner_h = metrics.inner_h;
    let max_offset_y = (metrics.content_size.y - inner_h).max(0.0);

    let mut state = State::load(ctx, scroll_area_id).unwrap_or_default();

    let page_h = inner_h.max(1.0);
    let line_h = ui.text_style_height(&egui::TextStyle::Body).max(12.0);

    let mut dy = 0.0f32;

    ctx.input_mut(|i| {
        if i.consume_key(egui::Modifiers::NONE, egui::Key::PageDown) {
            dy += page_h;
        }
        if i.consume_key(egui::Modifiers::NONE, egui::Key::PageUp) {
            dy -= page_h;
        }
        if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
            dy += line_h;
        }
        if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
            dy -= line_h;
        }
    });

    if dy == 0.0 {
        return;
    }

    if scroll_enabled[1] {
        state.offset.y = (state.offset.y + dy).clamp(0.0, max_offset_y);
    }

    state.store(ctx, scroll_area_id);
}

pub fn store_scroll_metrics<R>(ctx: &Context, scroll_area_id: Id, output: &ScrollAreaOutput<R>, fallback_rect: Rect) {
    let metrics_id = scroll_area_id.with("scroll_kb_metrics");
    let metrics = ScrollMetrics {
        content_size: output.content_size,
        inner_h: inner_height_or_fallback(output.inner_rect, fallback_rect),
    };
    ctx.data_mut(|d| d.insert_temp(metrics_id, metrics));
}
