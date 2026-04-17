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

#[derive(Clone, Copy)]
struct HorizontalScrollMetrics {
    content_w: f32,
    /// ビューポートの幅（左右矢印の 1 ステップの基準にも使う）
    inner_w: f32,
}

fn inner_height_or_fallback(inner_rect: Rect, fallback: Rect) -> f32 {
    let mut h = inner_rect.height();
    if h <= 0.0 {
        h = fallback.height().max(1.0);
    }
    h
}

fn inner_width_or_fallback(inner_rect: Rect, fallback: Rect) -> f32 {
    let mut w = inner_rect.width();
    if w <= 0.0 {
        w = fallback.width().max(1.0);
    }
    w
}

pub fn scroll_area_persistent_id(ui: &Ui, id_salt: impl std::hash::Hash) -> Id {
    ui.make_persistent_id(egui::Id::new(id_salt))
}

/// `allow_keyboard_without_pointer` が true のときは、ポインタが矩形外でも矢印／PageUp/PageDown でスクロールする（コードビューでコードペインにフォーカスがある場合など）。
pub fn apply_keyboard_scroll_before_show(
    ctx: &Context,
    ui: &Ui,
    scroll_area_id: Id,
    interaction_rect: Rect,
    scroll_enabled: Vec2b,
    allow_keyboard_without_pointer: bool,
) {
    if !ui.rect_contains_pointer(interaction_rect) && !allow_keyboard_without_pointer {
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

/// 横スクロール用（表モードなど）。←/→ で `state.offset.x` を更新する。
pub fn apply_keyboard_horizontal_scroll_before_show(
    ctx: &Context,
    ui: &Ui,
    scroll_area_id: Id,
    interaction_rect: Rect,
    scroll_enabled: bool,
    allow_keyboard_without_pointer: bool,
) {
    if !scroll_enabled || (!ui.rect_contains_pointer(interaction_rect) && !allow_keyboard_without_pointer) {
        return;
    }

    let metrics_id = scroll_area_id.with("scroll_kb_metrics_h");
    let Some(metrics) = ctx.data(|d| d.get_temp::<HorizontalScrollMetrics>(metrics_id)) else {
        return;
    };

    let max_offset_x = (metrics.content_w - metrics.inner_w).max(0.0);
    let mut state = State::load(ctx, scroll_area_id).unwrap_or_default();

    let step = ui.text_style_height(&egui::TextStyle::Body).max(12.0);

    let mut dx = 0.0f32;

    ctx.input_mut(|i| {
        if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight) {
            dx += step;
        }
        if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft) {
            dx -= step;
        }
    });

    if dx == 0.0 {
        return;
    }

    state.offset.x = (state.offset.x + dx).clamp(0.0, max_offset_x);
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

pub fn store_horizontal_scroll_metrics<R>(
    ctx: &Context,
    scroll_area_id: Id,
    output: &ScrollAreaOutput<R>,
    fallback_rect: Rect,
) {
    let metrics_id = scroll_area_id.with("scroll_kb_metrics_h");
    let metrics = HorizontalScrollMetrics {
        content_w: output.content_size.x,
        inner_w: inner_width_or_fallback(output.inner_rect, fallback_rect),
    };
    ctx.data_mut(|d| d.insert_temp(metrics_id, metrics));
}
