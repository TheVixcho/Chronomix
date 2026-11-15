use std::f32::consts::PI;

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Id, Margin, Pos2, Rect, Response, RichText, Sense,
    Shape, Stroke, StrokeKind, Ui, Vec2, epaint::Shadow,
};

/// Studio dark color palette inspired by modern pro-audio DAWs.
pub struct StudioPalette;

impl StudioPalette {
    pub const BG_DEEP: Color32 = Color32::from_rgb(10, 10, 12);
    pub const BG_PANEL: Color32 = Color32::from_rgb(18, 18, 22);
    pub const BG_CARD: Color32 = Color32::from_rgb(24, 24, 28);
    pub const BG_ELEVATED: Color32 = Color32::from_rgb(34, 34, 40);
    pub const BG_INPUT: Color32 = Color32::from_rgb(14, 14, 17);

    pub const BORDER_SUBTLE: Color32 = Color32::from_rgb(46, 46, 54);
    pub const BORDER_BRIGHT: Color32 = Color32::from_rgb(76, 76, 88);

    pub const ACCENT_CYAN: Color32 = Color32::from_rgb(230, 234, 242);
    pub const ACCENT_BLUE: Color32 = Color32::from_rgb(56, 58, 68);
    pub const ACCENT_PURPLE: Color32 = Color32::from_rgb(195, 198, 208);
    pub const ACCENT_EMERALD: Color32 = Color32::from_rgb(215, 220, 228);
    pub const ACCENT_AMBER: Color32 = Color32::from_rgb(200, 204, 214);
    pub const ACCENT_CRIMSON: Color32 = Color32::from_rgb(215, 45, 45);

    pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(255, 255, 255);
    pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(190, 195, 205);
    pub const TEXT_MUTED: Color32 = Color32::from_rgb(125, 130, 140);
}

/// Vector icons painted directly with `egui::Painter` so they never depend on emoji fonts or show tofu `□`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StudioIcon {
    Sparkle,
    Mic,
    Headphones,
    Sliders,
    Gear,
    Folder,
    File,
    Play,
    Stop,
    Record,
    Refresh,
    Window,
    Search,
    Close,
    Bolt,
    Piano,
}

/// Draws a crisp vector icon centered at `center` within a square of `size`.
pub fn draw_vector_icon(
    painter: &egui::Painter,
    center: Pos2,
    size: f32,
    icon: StudioIcon,
    color: Color32,
) {
    let r = size * 0.5;
    let stroke = Stroke::new(1.6_f32, color);

    match icon {
        StudioIcon::Sparkle => {
            let pts = vec![
                Pos2::new(center.x, center.y - r),
                Pos2::new(center.x + r * 0.35, center.y - r * 0.35),
                Pos2::new(center.x + r, center.y),
                Pos2::new(center.x + r * 0.35, center.y + r * 0.35),
                Pos2::new(center.x, center.y + r),
                Pos2::new(center.x - r * 0.35, center.y + r * 0.35),
                Pos2::new(center.x - r, center.y),
                Pos2::new(center.x - r * 0.35, center.y - r * 0.35),
            ];
            painter.add(Shape::convex_polygon(pts, color, Stroke::NONE));
        }
        StudioIcon::Mic => {
            let cap = Rect::from_center_size(
                Pos2::new(center.x, center.y - r * 0.22),
                Vec2::new(r * 0.72, r * 1.05),
            );
            painter.rect_stroke(cap, CornerRadius::same(3), stroke, StrokeKind::Inside);
            draw_arc(
                painter,
                Pos2::new(center.x, center.y - r * 0.05),
                r * 0.65,
                0.15,
                PI - 0.15,
                stroke,
            );
            painter.line_segment(
                [
                    Pos2::new(center.x, center.y + r * 0.58),
                    Pos2::new(center.x, center.y + r * 0.92),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    Pos2::new(center.x - r * 0.42, center.y + r * 0.92),
                    Pos2::new(center.x + r * 0.42, center.y + r * 0.92),
                ],
                stroke,
            );
        }
        StudioIcon::Headphones => {
            draw_arc(
                painter,
                Pos2::new(center.x, center.y + r * 0.1),
                r * 0.78,
                PI,
                PI * 2.0,
                Stroke::new(1.8_f32, color),
            );
            let left_cup = Rect::from_center_size(
                Pos2::new(center.x - r * 0.68, center.y + r * 0.32),
                Vec2::new(r * 0.42, r * 0.72),
            );
            let right_cup = Rect::from_center_size(
                Pos2::new(center.x + r * 0.68, center.y + r * 0.32),
                Vec2::new(r * 0.42, r * 0.72),
            );
            painter.rect_filled(left_cup, CornerRadius::same(2), color);
            painter.rect_filled(right_cup, CornerRadius::same(2), color);
        }
        StudioIcon::Sliders => {
            for (idx, knob_pos) in [(-0.58_f32, -0.25_f32), (0.0, 0.30), (0.58, -0.10)] {
                let x = center.x + idx * r;
                painter.line_segment(
                    [Pos2::new(x, center.y - r * 0.8), Pos2::new(x, center.y + r * 0.8)],
                    Stroke::new(1.4_f32, color),
                );
                let ky = center.y + knob_pos * r;
                let krect = Rect::from_center_size(Pos2::new(x, ky), Vec2::new(r * 0.42, r * 0.32));
                painter.rect_filled(krect, CornerRadius::same(1), color);
            }
        }
        StudioIcon::Gear => {
            let inner_r = r * 0.38;
            let outer_r = r * 0.72;
            painter.circle_stroke(center, outer_r, Stroke::new(1.8_f32, color));
            painter.circle_filled(center, inner_r * 0.7, color);
            for i in 0..6 {
                let ang = i as f32 * (PI / 3.0);
                let dir = Vec2::new(ang.cos(), ang.sin());
                painter.line_segment(
                    [center + dir * (outer_r - 1.0), center + dir * (r * 0.96)],
                    Stroke::new(2.0_f32, color),
                );
            }
        }
        StudioIcon::Folder => {
            let body = Rect::from_min_max(
                Pos2::new(center.x - r * 0.88, center.y - r * 0.42),
                Pos2::new(center.x + r * 0.88, center.y + r * 0.72),
            );
            let tab = Rect::from_min_max(
                Pos2::new(center.x - r * 0.88, center.y - r * 0.72),
                Pos2::new(center.x - r * 0.10, center.y - r * 0.35),
            );
            painter.rect_filled(tab, CornerRadius::same(2), color);
            painter.rect_stroke(body, CornerRadius::same(2), stroke, StrokeKind::Inside);
        }
        StudioIcon::File => {
            let body = Rect::from_center_size(center, Vec2::new(r * 1.25, r * 1.6));
            painter.rect_stroke(body, CornerRadius::same(2), stroke, StrokeKind::Inside);
            painter.line_segment(
                [
                    Pos2::new(body.left() + 3.0, center.y - 2.0),
                    Pos2::new(body.right() - 3.0, center.y - 2.0),
                ],
                Stroke::new(1.2_f32, color),
            );
            painter.line_segment(
                [
                    Pos2::new(body.left() + 3.0, center.y + 2.5),
                    Pos2::new(body.right() - 3.0, center.y + 2.5),
                ],
                Stroke::new(1.2_f32, color),
            );
        }
        StudioIcon::Play => {
            let pts = vec![
                Pos2::new(center.x - r * 0.45, center.y - r * 0.68),
                Pos2::new(center.x + r * 0.68, center.y),
                Pos2::new(center.x - r * 0.45, center.y + r * 0.68),
            ];
            painter.add(Shape::convex_polygon(pts, color, Stroke::NONE));
        }
        StudioIcon::Stop => {
            let sq = Rect::from_center_size(center, Vec2::splat(r * 1.15));
            painter.rect_filled(sq, CornerRadius::same(2), color);
        }
        StudioIcon::Record => {
            painter.circle_filled(center, r * 0.62, color);
            painter.circle_stroke(center, r * 0.88, Stroke::new(1.2_f32, color));
        }
        StudioIcon::Refresh => {
            draw_arc(painter, center, r * 0.72, 0.4, PI * 1.75, stroke);
            let tip = center + Vec2::new((PI * 1.75).cos(), (PI * 1.75).sin()) * (r * 0.72);
            painter.line_segment(
                [tip, Pos2::new(tip.x - 3.5, tip.y - 1.5)],
                stroke,
            );
            painter.line_segment(
                [tip, Pos2::new(tip.x + 1.0, tip.y - 4.0)],
                stroke,
            );
        }
        StudioIcon::Window => {
            let wrect = Rect::from_center_size(center, Vec2::new(r * 1.65, r * 1.35));
            painter.rect_stroke(wrect, CornerRadius::same(2), stroke, StrokeKind::Inside);
            let top_y = wrect.top() + r * 0.42;
            painter.line_segment(
                [Pos2::new(wrect.left(), top_y), Pos2::new(wrect.right(), top_y)],
                stroke,
            );
        }
        StudioIcon::Search => {
            let c = Pos2::new(center.x - r * 0.15, center.y - r * 0.15);
            painter.circle_stroke(c, r * 0.55, stroke);
            painter.line_segment(
                [
                    Pos2::new(c.x + r * 0.38, c.y + r * 0.38),
                    Pos2::new(center.x + r * 0.78, center.y + r * 0.78),
                ],
                Stroke::new(2.0_f32, color),
            );
        }
        StudioIcon::Close => {
            let d = r * 0.58;
            painter.line_segment(
                [Pos2::new(center.x - d, center.y - d), Pos2::new(center.x + d, center.y + d)],
                Stroke::new(1.8_f32, color),
            );
            painter.line_segment(
                [Pos2::new(center.x + d, center.y - d), Pos2::new(center.x - d, center.y + d)],
                Stroke::new(1.8_f32, color),
            );
        }
        StudioIcon::Bolt => {
            let pts = vec![
                Pos2::new(center.x + r * 0.15, center.y - r * 0.85),
                Pos2::new(center.x - r * 0.55, center.y + r * 0.08),
                Pos2::new(center.x - r * 0.05, center.y + r * 0.08),
                Pos2::new(center.x - r * 0.15, center.y + r * 0.85),
                Pos2::new(center.x + r * 0.55, center.y - r * 0.08),
                Pos2::new(center.x + r * 0.05, center.y - r * 0.08),
            ];
            painter.add(Shape::convex_polygon(pts, color, Stroke::NONE));
        }
        StudioIcon::Piano => {
            let box_rect = Rect::from_center_size(center, Vec2::new(size * 1.0, size * 0.75));
            painter.rect_stroke(box_rect, CornerRadius::same(2), stroke, StrokeKind::Inside);
            let kw = box_rect.width() / 4.0;
            for i in 1..4 {
                let x = box_rect.min.x + i as f32 * kw;
                painter.line_segment([Pos2::new(x, box_rect.min.y), Pos2::new(x, box_rect.max.y)], stroke);
            }
            let bw = kw * 0.55;
            let bh = box_rect.height() * 0.55;
            for i in [1, 2] {
                let bx = box_rect.min.x + i as f32 * kw - bw * 0.5;
                let b_rect = Rect::from_min_size(Pos2::new(bx, box_rect.min.y), Vec2::new(bw, bh));
                painter.rect_filled(b_rect, CornerRadius::same(1), color);
            }
        }
    }
}

/// Applies the studio dark theme, squared industrial DAW geometry, and compact spacing to `egui::Context`.
pub fn apply_studio_theme(ctx: &egui::Context) {
    let mut style = (*ctx.global_style()).clone();

    style.spacing.item_spacing = Vec2::new(8.0, 6.0);
    style.spacing.window_margin = Margin::same(10);
    style.spacing.button_padding = Vec2::new(10.0, 5.0);
    style.spacing.interact_size = Vec2::new(36.0, 26.0);
    style.spacing.slider_width = 130.0;
    style.spacing.combo_width = 140.0;
    style.animation_time = 0.14;

    let mut visuals = egui::Visuals::dark();
    visuals.dark_mode = true;
    visuals.panel_fill = StudioPalette::BG_DEEP;
    visuals.window_fill = StudioPalette::BG_CARD;
    visuals.extreme_bg_color = StudioPalette::BG_INPUT;
    visuals.faint_bg_color = StudioPalette::BG_PANEL;
    visuals.code_bg_color = StudioPalette::BG_INPUT;
    visuals.override_text_color = Some(StudioPalette::TEXT_PRIMARY);

    let rounding = CornerRadius::same(2);
    visuals.window_corner_radius = CornerRadius::same(3);
    visuals.menu_corner_radius = CornerRadius::same(2);

    visuals.widgets.noninteractive.bg_fill = StudioPalette::BG_CARD;
    visuals.widgets.noninteractive.weak_bg_fill = StudioPalette::BG_PANEL;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, StudioPalette::BORDER_SUBTLE);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, StudioPalette::TEXT_SECONDARY);
    visuals.widgets.noninteractive.corner_radius = rounding;

    visuals.widgets.inactive.bg_fill = StudioPalette::BG_ELEVATED;
    visuals.widgets.inactive.weak_bg_fill = StudioPalette::BG_ELEVATED;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, StudioPalette::BORDER_SUBTLE);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, StudioPalette::TEXT_PRIMARY);
    visuals.widgets.inactive.corner_radius = rounding;

    visuals.widgets.hovered.bg_fill = Color32::from_rgb(40, 42, 50);
    visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(40, 42, 50);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.2_f32, Color32::from_rgb(180, 185, 195));
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.5_f32, Color32::WHITE);
    visuals.widgets.hovered.corner_radius = rounding;
    visuals.widgets.hovered.expansion = 0.0;

    visuals.widgets.active.bg_fill = Color32::from_rgb(56, 58, 68);
    visuals.widgets.active.weak_bg_fill = Color32::from_rgb(56, 58, 68);
    visuals.widgets.active.bg_stroke = Stroke::new(1.5_f32, Color32::WHITE);
    visuals.widgets.active.fg_stroke = Stroke::new(2.0_f32, Color32::WHITE);
    visuals.widgets.active.corner_radius = rounding;
    visuals.widgets.active.expansion = 0.0;

    visuals.widgets.open.bg_fill = StudioPalette::BG_ELEVATED;
    visuals.widgets.open.weak_bg_fill = StudioPalette::BG_ELEVATED;
    visuals.widgets.open.bg_stroke = Stroke::new(1.2_f32, Color32::from_rgb(180, 185, 195));
    visuals.widgets.open.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);
    visuals.widgets.open.corner_radius = rounding;
    visuals.widgets.open.expansion = 0.0;

    visuals.selection.bg_fill = Color32::from_rgba_unmultiplied(220, 225, 235, 40);
    visuals.selection.stroke = Stroke::new(1.2_f32, Color32::from_rgb(220, 225, 235));

    visuals.popup_shadow = Shadow {
        offset: [0, 3],
        blur: 8,
        spread: 0,
        color: Color32::from_black_alpha(110),
    };

    style.visuals = visuals;
    ctx.set_global_style(style);
}

/// Linearly interpolates between two `Color32` values (`t` in `0.0..=1.0`).
pub fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let inv = 1.0 - t;
    Color32::from_rgba_unmultiplied(
        (a.r() as f32 * inv + b.r() as f32 * t).round() as u8,
        (a.g() as f32 * inv + b.g() as f32 * t).round() as u8,
        (a.b() as f32 * inv + b.b() as f32 * t).round() as u8,
        (a.a() as f32 * inv + b.a() as f32 * t).round() as u8,
    )
}

/// Renders a studio card container with squared corners and subtle border.
pub fn studio_card<R>(
    ui: &mut Ui,
    accent: Option<Color32>,
    inner_margin: Margin,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> egui::InnerResponse<R> {
    let stroke_color = accent
        .map(|c| lerp_color(StudioPalette::BORDER_SUBTLE, c, 0.42))
        .unwrap_or(StudioPalette::BORDER_SUBTLE);

    egui::Frame::new()
        .fill(StudioPalette::BG_CARD)
        .stroke(Stroke::new(1.0_f32, stroke_color))
        .corner_radius(CornerRadius::same(3))
        .inner_margin(inner_margin)
        .show(ui, add_contents)
}

/// Renders a squared industrial action button with a vector `StudioIcon` that never overflows adjacent widgets.
pub fn animated_action_button(
    ui: &mut Ui,
    id_salt: &str,
    icon: Option<StudioIcon>,
    label: &str,
    fill_color: Color32,
    active: bool,
    min_size: Vec2,
) -> Response {
    let id = ui.make_persistent_id(id_salt);
    let icon_pad = if icon.is_some() { 18.0 } else { 0.0 };
    let text_w = ui
        .painter()
        .layout_no_wrap(
            label.to_string(),
            FontId::proportional(12.5),
            Color32::WHITE,
        )
        .size()
        .x;
    let avail_w = ui.available_width().max(40.0);
    let natural_w = text_w + icon_pad + 20.0;
    let desired = Vec2::new(
        natural_w.max(min_size.x.min(natural_w + 10.0)).min(avail_w),
        min_size.y.max(28.0),
    );
    let (rect, resp) = ui.allocate_exact_size(desired, Sense::click());
    if ui.is_rect_visible(rect) {
        let hover_t = ui
            .ctx()
            .animate_bool_responsive(id.with("hover"), resp.hovered());
        let active_t = ui.ctx().animate_bool_responsive(id.with("active"), active);

        let base = if active {
            fill_color
        } else {
            lerp_color(StudioPalette::BG_ELEVATED, fill_color, 0.72)
        };
        let bg = lerp_color(base, Color32::WHITE, hover_t * 0.14);

        let painter = ui.painter();
        painter.rect_filled(rect, CornerRadius::same(2), bg);
        let stroke_col = lerp_color(
            Color32::from_white_alpha(40),
            Color32::from_white_alpha(165),
            hover_t.max(active_t * 0.6),
        );
        painter.rect_stroke(
            rect,
            CornerRadius::same(2),
            Stroke::new(1.0_f32, stroke_col),
            StrokeKind::Inside,
        );

        let clip_painter = painter.with_clip_rect(rect.shrink(2.0));
        if let Some(ic) = icon {
            let total_w = 13.0 + 6.0 + text_w;
            let start_x = (rect.center().x - total_w * 0.5).max(rect.left() + 6.0);
            draw_vector_icon(
                &clip_painter,
                Pos2::new(start_x + 6.5, rect.center().y),
                12.5,
                ic,
                Color32::WHITE,
            );
            clip_painter.text(
                Pos2::new(start_x + 19.0, rect.center().y),
                Align2::LEFT_CENTER,
                label,
                FontId::proportional(12.5),
                Color32::WHITE,
            );
        } else {
            clip_painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                label,
                FontId::proportional(12.5),
                Color32::WHITE,
            );
        }
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// Interactive hardware-style rotary knob with animated arc, tick marks, drag & double-click reset.
pub fn draw_rotary_knob(
    ui: &mut Ui,
    id_salt: &str,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    default_value: f32,
    unit_suffix: &str,
    accent: Color32,
) -> bool {
    let mut changed = false;
    let id = Id::new(id_salt);
    let min = *range.start();
    let max = *range.end();
    let span = (max - min).max(1e-5);

    ui.allocate_ui_with_layout(
        Vec2::new(72.0, 96.0),
        egui::Layout::top_down(egui::Align::Center),
        |ui| {
            let knob_size = Vec2::new(66.0, 66.0);
        let (rect, resp) = ui.allocate_exact_size(knob_size, Sense::click_and_drag());

        if resp.double_clicked() {
            *value = default_value.clamp(min, max);
            changed = true;
        } else if resp.dragged() {
            let delta = -resp.drag_delta().y + resp.drag_delta().x * 0.35;
            if delta.abs() > 0.0 {
                let step = (delta / 135.0) * span;
                *value = (*value + step).clamp(min, max);
                changed = true;
            }
        }

        if resp.hovered() || resp.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
        }

        let norm = ((*value - min) / span).clamp(0.0, 1.0);
        let anim_norm = ui.ctx().animate_value_with_time(id.with("norm"), norm, 0.06);
        let hover_t = ui
            .ctx()
            .animate_bool_responsive(id.with("hover"), resp.hovered() || resp.dragged());

        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            let center = rect.center();
            let outer_r = 23.0;
            let track_r = outer_r + 4.5;

            let start_angle = PI * 0.75; // 135 deg (bottom-left)
            let total_sweep = PI * 1.5; // 270 deg sweep

            // Subtle outer ticks
            for i in 0..=10 {
                let tick_t = i as f32 / 10.0;
                let ang = start_angle + tick_t * total_sweep;
                let dir = Vec2::new(ang.cos(), ang.sin());
                let p1 = center + dir * (track_r + 2.0);
                let p2 = center + dir * (track_r + 4.2);
                let tick_col = if tick_t <= anim_norm + 0.01 {
                    Color32::WHITE
                } else {
                    Color32::from_rgb(44, 46, 54)
                };
                painter.line_segment([p1, p2], Stroke::new(1.2_f32, tick_col));
            }

            draw_arc(
                painter,
                center,
                track_r,
                start_angle,
                start_angle + total_sweep,
                Stroke::new(3.0_f32, Color32::from_rgb(30, 31, 38)),
            );

            if anim_norm > 0.005 {
                let end_angle = start_angle + anim_norm * total_sweep;
                draw_arc(
                    painter,
                    center,
                    track_r,
                    start_angle,
                    end_angle,
                    Stroke::new(3.2_f32, Color32::from_rgb(230, 234, 242)),
                );
            }

            painter.circle_filled(
                center,
                outer_r,
                lerp_color(
                    Color32::from_rgb(24, 25, 30),
                    Color32::from_rgb(36, 38, 46),
                    hover_t,
                ),
            );
            painter.circle_stroke(
                center,
                outer_r,
                Stroke::new(
                    1.2_f32,
                    lerp_color(StudioPalette::BORDER_SUBTLE, Color32::WHITE, hover_t * 0.7),
                ),
            );

            // Indicator needle
            let val_angle = start_angle + anim_norm * total_sweep;
            let dir = Vec2::new(val_angle.cos(), val_angle.sin());
            let p_inner = center + dir * (outer_r * 0.35);
            let p_outer = center + dir * (outer_r * 0.82);
            painter.line_segment([p_inner, p_outer], Stroke::new(2.2_f32, Color32::WHITE));
            painter.circle_filled(p_outer, 1.8, accent);
        }

        // Value badge + label
        let formatted = if unit_suffix == "%" {
            format!("{:.0}%", *value)
        } else {
            format!("{:+.1} {unit_suffix}", *value)
        };
        ui.label(
            RichText::new(formatted)
                .monospace()
                .small()
                .strong()
                .color(StudioPalette::TEXT_PRIMARY),
        );
        ui.label(
            RichText::new(label)
                .small()
                .color(StudioPalette::TEXT_SECONDARY),
        );
    });

    changed
}

pub fn draw_arc(
    painter: &egui::Painter,
    center: Pos2,
    radius: f32,
    start_angle: f32,
    end_angle: f32,
    stroke: Stroke,
) {
    let steps = 24usize;
    let sweep = end_angle - start_angle;
    if sweep.abs() < 1e-4 {
        return;
    }
    let points: Vec<Pos2> = (0..=steps)
        .map(|i| {
            let t = i as f32 / steps as f32;
            let ang = start_angle + t * sweep;
            center + Vec2::new(ang.cos() * radius, ang.sin() * radius)
        })
        .collect();
    painter.add(Shape::line(points, stroke));
}

/// Studio segmented stereo LED VU meter with peak hold and dB markings that scales to narrow widths.
pub fn draw_studio_vu_meter(
    ui: &mut Ui,
    label: &str,
    left_amp: f32,
    right_amp: f32,
    left_peak: f32,
    right_peak: f32,
) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 4.0;
        for (ch, amp, peak) in [("L", left_amp, left_peak), ("R", right_amp, right_peak)] {
            ui.horizontal(|ui| {
                let total_w = ui.available_width();
                let label_w = if total_w < 220.0 { 36.0 } else { 44.0 };
                ui.add_sized(
                    [label_w, 14.0],
                    egui::Label::new(
                        RichText::new(format!("{label} {ch}"))
                            .small()
                            .monospace()
                            .strong()
                            .color(StudioPalette::TEXT_SECONDARY),
                    )
                    .truncate(),
                );

                let show_db = ui.available_width() > 130.0;
                let avail_w = if show_db {
                    (ui.available_width() - 56.0).max(40.0)
                } else {
                    (ui.available_width() - 4.0).max(32.0)
                };
                let segments = ((avail_w / 7.5).floor() as usize).clamp(8, 28);
                let (rect, _) = ui.allocate_exact_size(Vec2::new(avail_w, 13.0), Sense::hover());

                if ui.is_rect_visible(rect) {
                    let painter = ui.painter();
                    painter.rect_filled(rect, CornerRadius::same(2), StudioPalette::BG_INPUT);
                    painter.rect_stroke(
                        rect,
                        CornerRadius::same(2),
                        Stroke::new(1.0_f32, StudioPalette::BORDER_SUBTLE),
                        StrokeKind::Inside,
                    );

                    let inner = rect.shrink2(Vec2::new(3.0, 2.5));
                    let norm = amp_to_meter_norm(amp);
                    let peak_norm = amp_to_meter_norm(peak);
                    let peak_seg = ((peak_norm * segments as f32).round() as usize)
                        .saturating_sub(1)
                        .min(segments - 1);

                    let gap = 1.6_f32;
                    let seg_w =
                        ((inner.width() - gap * (segments as f32 - 1.0)) / segments as f32).max(1.5);

                    for i in 0..segments {
                        let seg_t = (i + 1) as f32 / segments as f32;
                        let x0 = inner.left() + i as f32 * (seg_w + gap);
                        let seg_rect = Rect::from_min_size(
                            Pos2::new(x0, inner.top()),
                            Vec2::new(seg_w, inner.height()),
                        );

                        let base_col = if seg_t > 0.90 {
                            Color32::from_rgb(225, 45, 45)
                        } else if seg_t > 0.72 {
                            Color32::from_rgb(215, 218, 226)
                        } else {
                            Color32::from_rgb(175, 180, 192)
                        };

                        let is_lit = norm >= seg_t - (0.5 / segments as f32);
                        let is_peak = peak_norm > 0.03 && i == peak_seg;

                        let col = if is_lit {
                            base_col
                        } else if is_peak {
                            lerp_color(base_col, Color32::WHITE, 0.35)
                        } else {
                            Color32::from_rgba_unmultiplied(
                                base_col.r(),
                                base_col.g(),
                                base_col.b(),
                                26,
                            )
                        };

                        painter.rect_filled(seg_rect, CornerRadius::same(1), col);
                    }
                }

                if show_db {
                    let db_str = if amp <= 1e-4 {
                        "-60 dB".to_string()
                    } else {
                        format!("{:+.1} dB", (20.0 * amp.log10()).max(-60.0))
                    };
                    ui.add_sized(
                        [50.0, 14.0],
                        egui::Label::new(
                            RichText::new(db_str)
                                .small()
                                .monospace()
                                .color(if amp >= 0.98 {
                                    StudioPalette::ACCENT_CRIMSON
                                } else {
                                    StudioPalette::TEXT_MUTED
                                }),
                        ),
                    );
                }
            });
        }
    });
}

pub fn amp_to_meter_norm(amp: f32) -> f32 {
    if amp <= 1e-4 {
        return 0.0;
    }
    let db = 20.0 * amp.log10();
    ((db + 60.0) / 60.0).clamp(0.0, 1.0)
}

/// Squared toggle switch with LED indicator.
pub fn animated_led_toggle(
    ui: &mut Ui,
    id_salt: &str,
    active: &mut bool,
    label: &str,
    active_color: Color32,
) -> bool {
    let id = ui.make_persistent_id(id_salt);
    let text_galley = ui.painter().layout_no_wrap(
        label.to_string(),
        FontId::proportional(12.5),
        StudioPalette::TEXT_PRIMARY,
    );
    let avail_w = ui.available_width().max(60.0);
    let desired = Vec2::new((text_galley.size().x + 44.0).min(avail_w), 28.0);
    let (rect, resp) = ui.allocate_exact_size(desired, Sense::click());
    let mut changed = false;
    if resp.clicked() {
        *active = !*active;
        changed = true;
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    if ui.is_rect_visible(rect) {
        let t = ui.ctx().animate_bool_responsive(id.with("on"), *active);
        let hover_t = ui
            .ctx()
            .animate_bool_responsive(id.with("hover"), resp.hovered());

        let painter = ui.painter();
        let bg = lerp_color(
            StudioPalette::BG_ELEVATED,
            Color32::from_rgba_unmultiplied(
                active_color.r(),
                active_color.g(),
                active_color.b(),
                45,
            ),
            t,
        );
        let stroke_col = lerp_color(
            StudioPalette::BORDER_SUBTLE,
            active_color,
            t.max(hover_t * 0.5),
        );
        painter.rect_filled(rect, CornerRadius::same(2), bg);
        painter.rect_stroke(
            rect,
            CornerRadius::same(2),
            Stroke::new(1.0_f32, stroke_col),
            StrokeKind::Inside,
        );

        // Squared switch track
        let sw_rect = Rect::from_min_size(
            Pos2::new(rect.left() + 7.0, rect.center().y - 6.0),
            Vec2::new(24.0, 12.0),
        );
        let track_col = lerp_color(Color32::from_rgb(18, 22, 32), active_color, t);
        painter.rect_filled(sw_rect, CornerRadius::same(2), track_col);

        let knob_x = sw_rect.left() + 2.0 + t * 11.0;
        let thumb_rect = Rect::from_min_size(
            Pos2::new(knob_x, sw_rect.top() + 2.0),
            Vec2::new(9.0, 8.0),
        );
        painter.rect_filled(thumb_rect, CornerRadius::same(1), Color32::WHITE);

        let clip_painter = painter.with_clip_rect(rect.shrink(2.0));
        clip_painter.text(
            Pos2::new(sw_rect.right() + 7.0, rect.center().y),
            Align2::LEFT_CENTER,
            label,
            FontId::proportional(12.5),
            lerp_color(StudioPalette::TEXT_SECONDARY, Color32::WHITE, t.max(hover_t)),
        );
    }

    changed
}

/// Draws a section heading with a painted vector icon so no emoji fonts are needed.
pub fn icon_heading(ui: &mut Ui, icon: StudioIcon, color: Color32, text: &str, size: f32) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(size + 4.0), Sense::hover());
        if ui.is_rect_visible(rect) {
            draw_vector_icon(ui.painter(), rect.center(), size, icon, color);
        }
        ui.label(RichText::new(text).size(size).strong().color(Color32::WHITE));
    });
}

/// Draws an animated signal connector with flowing particles when `active` is true.
pub fn draw_animated_signal_pipe(
    ui: &mut Ui,
    active: bool,
    time: f64,
    color: Color32,
) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(34.0, 44.0), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter();
    let y = rect.center().y;
    let p1 = Pos2::new(rect.left() + 2.0, y);
    let p2 = Pos2::new(rect.right() - 2.0, y);

    let base_col = if active {
        Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 100)
    } else {
        StudioPalette::BORDER_SUBTLE
    };
    painter.line_segment([p1, p2], Stroke::new(2.0_f32, base_col));

    // Arrowhead
    let tip = p2;
    painter.line_segment(
        [tip, Pos2::new(tip.x - 5.0, tip.y - 4.0)],
        Stroke::new(2.0_f32, if active { color } else { StudioPalette::BORDER_SUBTLE }),
    );
    painter.line_segment(
        [tip, Pos2::new(tip.x - 5.0, tip.y + 4.0)],
        Stroke::new(2.0_f32, if active { color } else { StudioPalette::BORDER_SUBTLE }),
    );

    if active {
        for offset in [0.0_f64, 0.5_f64] {
            let phase = ((time * 1.4 + offset).fract()) as f32;
            let x = p1.x + phase * (p2.x - p1.x);
            painter.circle_filled(Pos2::new(x, y), 3.2, color);
            painter.circle_filled(Pos2::new(x, y), 1.6, Color32::WHITE);
        }
    }
}
