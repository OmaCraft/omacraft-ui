//! Three-state appearance: Follow desktop / Light / Dark.
//!
//! Light/dark ([`Mode`]) is a separate setting from the colour palette. Picking Light or Dark never
//! turns off the desktop palette; the app keeps using the Omarchy colours when the theme has that
//! variant and falls back to its built-in palette when it does not.

use egui::{Color32, Pos2, Rect, Response, Sense, Shape, Stroke, Theme, Ui, Vec2, epaint::PathShape};

/// The light/dark setting.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Mode {
    /// Follow the desktop (the Omarchy theme, else the OS). The default in every app.
    #[default]
    Desktop,
    Light,
    Dark,
}

impl Mode {
    pub const ALL: [Mode; 3] = [Mode::Desktop, Mode::Light, Mode::Dark];

    /// The ◐ click order: Desktop → Light → Dark → Desktop.
    pub const fn next(self) -> Self {
        match self {
            Mode::Desktop => Mode::Light,
            Mode::Light => Mode::Dark,
            Mode::Dark => Mode::Desktop,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Mode::Desktop => "Follow desktop",
            Mode::Light => "Light",
            Mode::Dark => "Dark",
        }
    }

    /// The stored value and the `view.appearance` argument.
    pub const fn key(self) -> &'static str {
        match self {
            Mode::Desktop => "desktop",
            Mode::Light => "light",
            Mode::Dark => "dark",
        }
    }

    /// Read a stored or control-port value, including the older spellings the four apps used.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "desktop" | "system" | "follow" | "follow_desktop" | "auto" => Some(Mode::Desktop),
            "light" | "day" => Some(Mode::Light),
            "dark" | "night" => Some(Mode::Dark),
            _ => None,
        }
    }

    /// Migrate the older settings: a stored "dark mode" flag plus a separate "follow desktop" or
    /// "system theme" flag. Following the desktop wins, and an unset follow flag now means follow,
    /// so the new default applies to anyone who never picked Light or Dark on purpose.
    pub fn migrate(old_dark: Option<bool>, old_follow: Option<bool>) -> Self {
        match (old_follow, old_dark) {
            (Some(true), _) | (None, None) => Mode::Desktop,
            (_, Some(true)) => Mode::Dark,
            (_, Some(false)) => Mode::Light,
            (Some(false), None) => Mode::Desktop,
        }
    }

    /// The theme to paint. `desktop` is the desktop's own preference when known (Omarchy's light
    /// marker, else the OS), and `fallback` is used when it is not.
    pub fn theme(self, desktop: Option<Theme>, fallback: Theme) -> Theme {
        match self {
            Mode::Desktop => desktop.unwrap_or(fallback),
            Mode::Light => Theme::Light,
            Mode::Dark => Theme::Dark,
        }
    }
}

/// Size of the title-bar icon button (see `docs/ui-consistency.md`).
pub const BUTTON_SIZE: Vec2 = Vec2::new(28.0, 26.0);

/// The ◐ button. Click cycles; right-click lists the three modes as radio items, then `extra`
/// (Notes puts its accent swatches there). Returns true when the mode changed.
pub fn toggle(ui: &mut Ui, mode: &mut Mode, extra: impl FnOnce(&mut Ui)) -> bool {
    let before = *mode;
    let (rect, response) = ui.allocate_exact_size(BUTTON_SIZE, Sense::click());
    let response = response.on_hover_text(format!("Appearance: {}", mode.label()));
    paint_button(ui, rect, &response, *mode);
    if response.clicked() {
        *mode = mode.next();
    }
    response.context_menu(|ui| {
        for choice in Mode::ALL {
            if ui.radio_value(mode, choice, choice.label()).clicked() {
                ui.close();
            }
        }
        extra(ui);
    });
    *mode != before
}

fn paint_button(ui: &Ui, rect: Rect, response: &Response, mode: Mode) {
    let visuals = ui.style().interact(response);
    if response.hovered() || response.has_focus() {
        ui.painter().rect_filled(rect, 4.0, visuals.weak_bg_fill);
    }
    paint_icon(ui.painter(), rect.center(), 7.0, mode, visuals.fg_stroke.color);
}

/// Half circle for Desktop, sun for Light, moon for Dark. Drawn with shapes, so no icon font.
pub fn paint_icon(painter: &egui::Painter, center: Pos2, radius: f32, mode: Mode, color: Color32) {
    let stroke = Stroke::new(1.3, color);
    match mode {
        Mode::Desktop => {
            painter.circle_stroke(center, radius, stroke);
            // Left half filled.
            let points: Vec<Pos2> = (0..=24)
                .map(|i| {
                    let a = std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * i as f32 / 24.0;
                    center + radius * Vec2::angled(a)
                })
                .collect();
            painter.add(Shape::Path(PathShape::convex_polygon(points, color, Stroke::NONE)));
        }
        Mode::Light => {
            painter.circle_stroke(center, radius * 0.45, stroke);
            for i in 0..8 {
                let dir = Vec2::angled(std::f32::consts::TAU * i as f32 / 8.0);
                painter.line_segment([center + dir * radius * 0.7, center + dir * radius], stroke);
            }
        }
        Mode::Dark => {
            // A crescent: the outer circle minus a disc offset up and to the right.
            let inner = center + Vec2::new(radius * 0.5, -radius * 0.4);
            let r2 = radius * 0.8;
            if let Some((p1, p2)) = intersections(center, radius, inner, r2) {
                let away = (center - inner).angle();
                let mut points = arc(center, radius, p1, p2, away);
                points.extend(arc(inner, r2, p2, p1, away));
                painter.add(Shape::Path(PathShape::closed_line(points, stroke)));
            }
        }
    }
}

fn intersections(c0: Pos2, r0: f32, c1: Pos2, r1: f32) -> Option<(Pos2, Pos2)> {
    let d = (c1 - c0).length();
    if d <= f32::EPSILON || d > r0 + r1 || d < (r0 - r1).abs() {
        return None;
    }
    let u = (c1 - c0) / d;
    let a = (r0 * r0 - r1 * r1 + d * d) / (2.0 * d);
    let h = (r0 * r0 - a * a).max(0.0).sqrt();
    let p = c0 + u * a;
    let perp = Vec2::new(-u.y, u.x);
    Some((p + perp * h, p - perp * h))
}

/// Points on a circle from `from` to `to`, taking the way round that passes direction `through`.
fn arc(c: Pos2, r: f32, from: Pos2, to: Pos2, through: f32) -> Vec<Pos2> {
    use std::f32::consts::{FRAC_PI_2, PI, TAU};
    let a0 = (from - c).angle();
    let a1 = (to - c).angle();
    let mut sweep = (a1 - a0).rem_euclid(TAU);
    let diff = ((a0 + sweep / 2.0 - through + PI).rem_euclid(TAU) - PI).abs();
    if diff > FRAC_PI_2 {
        sweep -= TAU;
    }
    (0..=20).map(|i| c + r * Vec2::angled(a0 + sweep * i as f32 / 20.0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn click_cycles_through_all_three() {
        assert_eq!(Mode::Desktop.next(), Mode::Light);
        assert_eq!(Mode::Light.next(), Mode::Dark);
        assert_eq!(Mode::Dark.next(), Mode::Desktop);
    }

    #[test]
    fn default_follows_the_desktop() {
        assert_eq!(Mode::default(), Mode::Desktop);
    }

    #[test]
    fn old_values_migrate() {
        assert_eq!(Mode::parse("system"), Some(Mode::Desktop));
        assert_eq!(Mode::parse("Dark"), Some(Mode::Dark));
        assert_eq!(Mode::parse("bogus"), None);
        for mode in Mode::ALL {
            assert_eq!(Mode::parse(mode.key()), Some(mode));
        }
        // Deck's case: never touched the setting → now follows the desktop.
        assert_eq!(Mode::migrate(None, None), Mode::Desktop);
        assert_eq!(Mode::migrate(Some(true), Some(true)), Mode::Desktop);
        assert_eq!(Mode::migrate(Some(true), Some(false)), Mode::Dark);
        assert_eq!(Mode::migrate(Some(false), Some(false)), Mode::Light);
        assert_eq!(Mode::migrate(Some(false), None), Mode::Light);
    }

    #[test]
    fn crescent_arcs_meet() {
        let c = Pos2::new(10.0, 10.0);
        let inner = c + Vec2::new(3.5, -2.8);
        let (p1, p2) = intersections(c, 7.0, inner, 5.6).unwrap();
        let outer = arc(c, 7.0, p1, p2, (c - inner).angle());
        let back = arc(inner, 5.6, p2, p1, (c - inner).angle());
        assert!((outer[0] - p1).length() < 1e-3 && (*outer.last().unwrap() - p2).length() < 1e-3);
        assert!((back[0] - p2).length() < 1e-3 && (*back.last().unwrap() - p1).length() < 1e-3);
        // The outer arc goes round the far side, away from the cut-out.
        assert!((outer[10] - inner).length() > 7.0);
        // The inner arc stays inside the outer circle.
        assert!(back.iter().all(|p| (*p - c).length() <= 7.0 + 1e-3));
    }

    #[test]
    fn theme_resolution() {
        assert_eq!(Mode::Desktop.theme(Some(Theme::Light), Theme::Dark), Theme::Light);
        assert_eq!(Mode::Desktop.theme(None, Theme::Dark), Theme::Dark);
        assert_eq!(Mode::Light.theme(Some(Theme::Dark), Theme::Dark), Theme::Light);
    }
}
