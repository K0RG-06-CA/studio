//! Studio's one theme: Adobe-style dark grey, small type, dense panels.
//!
//! Colour values come from LightCraft's `crates/ui-egui/src/theme.rs` (measured from black-box
//! observation of Lightroom Classic's dark theme; MIT, credited in `NOTICE`), so the embedded
//! LightCraft views in milestone 2 match the shell around them.

use egui::{Color32, CornerRadius, FontFamily, FontId, Margin, Stroke, TextStyle, Theme, Visuals};

/// Colour and size tokens. Widgets read [`TOKENS`] instead of hard-coding colours.
#[derive(Clone, Copy, Debug)]
pub struct Tokens {
    /// Top bar, options bar, side panels, tool strip, tab bars.
    pub chrome: Color32,
    /// Canvas surround and filmstrip.
    pub canvas: Color32,
    /// Lines between panels.
    pub divider: Color32,
    /// Panel bodies and inset areas.
    pub inset: Color32,
    pub field: Color32,
    pub field_border: Color32,
    pub button: Color32,
    pub hover: Color32,
    pub pressed: Color32,
    /// The active persona button and active tool.
    pub active: Color32,
    pub text: Color32,
    pub text_label: Color32,
    pub text_dim: Color32,
    pub text_disabled: Color32,
    pub accent: Color32,
    // metrics (points)
    pub top_bar_h: f32,
    pub options_bar_h: f32,
    pub tool_strip_w: f32,
    pub filmstrip_h: f32,
    /// Body text size: Adobe apps use small type.
    pub font_size: f32,
}

pub const TOKENS: Tokens = Tokens {
    chrome: Color32::from_rgb(0x2d, 0x2d, 0x2d),
    canvas: Color32::from_rgb(0x1c, 0x1c, 0x1c),
    divider: Color32::from_rgb(0x1c, 0x1c, 0x1c),
    inset: Color32::from_rgb(0x23, 0x23, 0x23),
    field: Color32::from_rgb(0x22, 0x22, 0x22),
    field_border: Color32::from_rgb(0x3c, 0x3c, 0x3c),
    button: Color32::from_rgb(0x24, 0x24, 0x24),
    hover: Color32::from_rgb(0x3a, 0x3a, 0x3a),
    pressed: Color32::from_rgb(0x3f, 0x3f, 0x3f),
    active: Color32::from_rgb(0x48, 0x48, 0x48),
    text: Color32::from_rgb(0xe2, 0xe2, 0xe2),
    text_label: Color32::from_rgb(0xbc, 0xbc, 0xbc),
    text_dim: Color32::from_rgb(0x8e, 0x8e, 0x8e),
    text_disabled: Color32::from_rgb(0x5c, 0x5c, 0x5c),
    accent: Color32::from_rgb(0x01, 0x65, 0xdd),
    top_bar_h: 34.0,
    options_bar_h: 30.0,
    tool_strip_w: 34.0,
    filmstrip_h: 96.0,
    font_size: 11.5,
};

/// Apply the theme to the whole app, whatever the OS light/dark setting.
pub fn apply(ctx: &egui::Context) {
    let t = TOKENS;
    ctx.set_theme(Theme::Dark);
    ctx.style_mut_of(Theme::Dark, |s| {
        s.visuals = visuals(&t);
        s.text_styles = [
            (TextStyle::Small, FontId::new(t.font_size - 2.0, FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(t.font_size, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(t.font_size, FontFamily::Proportional)),
            (TextStyle::Heading, FontId::new(t.font_size + 2.0, FontFamily::Proportional)),
            (TextStyle::Monospace, FontId::new(t.font_size, FontFamily::Monospace)),
        ]
        .into();
        s.spacing.item_spacing = egui::vec2(6.0, 3.0);
        s.spacing.button_padding = egui::vec2(6.0, 2.0);
        s.spacing.interact_size.y = 18.0;
    });
}

fn visuals(t: &Tokens) -> Visuals {
    let mut v = Visuals::dark();
    let r = CornerRadius::same(2);
    v.override_text_color = None;
    v.panel_fill = t.chrome;
    v.window_fill = t.chrome;
    v.window_stroke = Stroke::new(1.0, t.field_border);
    v.window_corner_radius = r;
    v.menu_corner_radius = r;
    v.faint_bg_color = t.inset;
    v.extreme_bg_color = t.field;
    v.selection.bg_fill = t.accent;
    v.selection.stroke = Stroke::new(1.0, t.text);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = t.chrome;
    w.noninteractive.weak_bg_fill = t.chrome;
    w.noninteractive.bg_stroke = Stroke::new(1.0, t.divider);
    w.noninteractive.fg_stroke = Stroke::new(1.0, t.text_label);
    for (s, fill) in [(&mut w.inactive, t.button), (&mut w.hovered, t.hover), (&mut w.active, t.pressed), (&mut w.open, t.pressed)] {
        s.bg_fill = fill;
        s.weak_bg_fill = fill;
        s.corner_radius = r;
    }
    w.inactive.bg_stroke = Stroke::new(1.0, t.field_border);
    w.inactive.fg_stroke = Stroke::new(1.0, t.text_label);
    w.hovered.fg_stroke = Stroke::new(1.0, t.text);
    w.active.fg_stroke = Stroke::new(1.0, t.text);
    v
}

/// Dim text for areas a later milestone fills.
pub fn placeholder(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text).color(TOKENS.text_disabled)
}

/// egui_dock's look: flat chrome-grey tab strips like Photoshop's panel groups.
pub fn dock_style(style: &egui::Style) -> egui_dock::Style {
    let t = TOKENS;
    let mut d = egui_dock::Style::from_egui(style);
    d.main_surface_border_stroke = Stroke::NONE;
    d.separator.width = 2.0;
    d.separator.color_idle = t.divider;
    d.separator.color_hovered = t.accent;
    d.separator.color_dragged = t.accent;
    d.tab_bar.bg_fill = t.chrome;
    d.tab_bar.height = 22.0;
    d.tab_bar.hline_color = t.divider;
    d.tab_bar.corner_radius = CornerRadius::ZERO;
    d.tab.tab_body.bg_fill = t.inset;
    d.tab.tab_body.stroke = Stroke::NONE;
    d.tab.tab_body.inner_margin = Margin::same(6);
    d.tab.hline_below_active_tab_name = false;
    for (s, fill, text) in [
        (&mut d.tab.active, t.inset, t.text),
        (&mut d.tab.focused, t.inset, t.text),
        (&mut d.tab.inactive, t.chrome, t.text_dim),
        (&mut d.tab.hovered, t.hover, t.text),
    ] {
        s.bg_fill = fill;
        s.text_color = text;
        s.outline_color = Color32::TRANSPARENT;
        s.corner_radius = CornerRadius::ZERO;
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_forces_dark_grey_whatever_the_os_theme() {
        let ctx = egui::Context::default();
        ctx.set_theme(Theme::Light);
        apply(&ctx);
        assert_eq!(ctx.theme(), Theme::Dark);
        assert_eq!(ctx.style_of(Theme::Dark).visuals.panel_fill, TOKENS.chrome);
        assert_eq!(ctx.style_of(Theme::Dark).text_styles[&TextStyle::Body].size, TOKENS.font_size);
    }

    #[test]
    fn dock_tabs_use_theme_colours() {
        let d = dock_style(&egui::Style::default());
        assert_eq!(d.tab_bar.bg_fill, TOKENS.chrome);
        assert_eq!(d.tab.active.bg_fill, TOKENS.inset);
        assert_eq!(d.tab.tab_body.bg_fill, TOKENS.inset);
    }
}
