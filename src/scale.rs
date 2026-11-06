//! One type scale for every chrome surface: main window, menus, popups, dialogs, settings, palette.
//!
//! Sizes are logical points before the Omarchy scale factor. Apps never call `FontId::proportional(15.0)`
//! for chrome text; they ask for a [`Role`] and get the face and size from here.

use crate::fonts::Face;
use egui::{FontId, Style, TextStyle};

/// A named text role. Each role has one face and one size.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Role {
    /// Menu and popup items, context menus. UI regular 13.
    Menu,
    /// Shortcut text in menus and the palette. UI regular 12, dim, right-aligned.
    Shortcut,
    /// Dialog title. UI bold 14.
    DialogTitle,
    /// Dialog body, settings rows, form labels. UI regular 13.
    Body,
    /// Settings section heading. UI bold 13.
    Section,
    /// Hints, captions, secondary text. UI regular 12, dim.
    Caption,
    /// Large display text: Home/start titles, welcome pages. Instrument Sans SemiBold 24.
    DisplayL,
    /// Medium display text: app names, File-view page headings. Instrument Sans SemiBold 18.
    DisplayM,
    /// Numbers that must line up: grid headers, status sums, rulers, counters. Inter Tabular 12.
    Numeric,
    /// Formula bar, code, cell editing. System mono 13.
    Mono,
}

impl Role {
    pub const ALL: [Role; 10] = [
        Role::Menu,
        Role::Shortcut,
        Role::DialogTitle,
        Role::Body,
        Role::Section,
        Role::Caption,
        Role::DisplayL,
        Role::DisplayM,
        Role::Numeric,
        Role::Mono,
    ];

    pub const fn face(self) -> Face {
        match self {
            Role::Menu | Role::Shortcut | Role::Body | Role::Caption => Face::Ui,
            Role::DialogTitle | Role::Section => Face::UiStrong,
            Role::DisplayL | Role::DisplayM => Face::Display,
            Role::Numeric => Face::Numeric,
            Role::Mono => Face::Mono,
        }
    }

    pub const fn size(self) -> f32 {
        match self {
            Role::Menu | Role::Body | Role::Section | Role::Mono => 13.0,
            Role::Shortcut | Role::Caption | Role::Numeric => 12.0,
            Role::DialogTitle => 14.0,
            Role::DisplayL => 24.0,
            Role::DisplayM => 18.0,
        }
    }

    /// Shortcut and caption text use the theme's weak text colour.
    pub const fn dim(self) -> bool {
        matches!(self, Role::Shortcut | Role::Caption)
    }

    pub fn font(self) -> FontId {
        FontId::new(self.size(), self.face().family())
    }

    /// The `TextStyle::Name` under which [`apply`] registers this role.
    pub const fn name(self) -> &'static str {
        match self {
            Role::Menu => "menu",
            Role::Shortcut => "shortcut",
            Role::DialogTitle => "dialog_title",
            Role::Body => "body",
            Role::Section => "section",
            Role::Caption => "caption",
            Role::DisplayL => "display_l",
            Role::DisplayM => "display_m",
            Role::Numeric => "numeric",
            Role::Mono => "mono",
        }
    }

    pub fn text_style(self) -> TextStyle {
        TextStyle::Name(self.name().into())
    }

    /// True when a font is exactly one of the roles. Used by the per-app audit tests. Several roles
    /// share a face and size (menu and body text), so this answers "on the scale", not "which role".
    pub fn on_scale(font: &FontId) -> bool {
        Role::ALL.into_iter().any(|role| role.font() == *font)
    }
}

/// Row heights shared by every surface.
pub mod rows {
    /// Menu, popup and palette rows.
    pub const MENU: f32 = 24.0;
    /// Settings rows.
    pub const SETTINGS: f32 = 28.0;
    /// Dialog buttons.
    pub const BUTTON: f32 = 28.0;
}

/// Install the scale into a style: the named roles plus egui's built-in text styles.
///
/// Built-in styles keep the chrome spec from `docs/ui-consistency.md`: Body/Button 13, Small 11,
/// Heading bold 18, Monospace 12.
pub fn apply(style: &mut Style) {
    for role in Role::ALL {
        style.text_styles.insert(role.text_style(), role.font());
    }
    style.text_styles.insert(TextStyle::Body, FontId::new(13.0, Face::Ui.family()));
    style.text_styles.insert(TextStyle::Button, FontId::new(13.0, Face::Ui.family()));
    style.text_styles.insert(TextStyle::Small, FontId::new(11.0, Face::Ui.family()));
    style.text_styles.insert(TextStyle::Heading, FontId::new(18.0, Face::UiStrong.family()));
    style.text_styles.insert(TextStyle::Monospace, FontId::new(12.0, Face::Mono.family()));
    style.spacing.interact_size.y = rows::MENU;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_is_never_below_16() {
        for role in Role::ALL {
            if role.face() == Face::Display {
                assert!(role.size() >= 16.0, "{role:?}");
            }
        }
    }

    #[test]
    fn every_role_is_registered_and_findable() {
        let mut style = Style::default();
        apply(&mut style);
        for role in Role::ALL {
            assert_eq!(style.text_styles.get(&role.text_style()), Some(&role.font()));
            assert!(Role::on_scale(&role.font()));
        }
    }

    #[test]
    fn ad_hoc_sizes_are_off_scale() {
        assert!(!Role::on_scale(&FontId::new(12.5, Face::Ui.family())));
        assert!(!Role::on_scale(&FontId::new(15.0, Face::Mono.family())));
    }

    #[test]
    fn mono_is_only_the_mono_role() {
        let mono: Vec<_> = Role::ALL.into_iter().filter(|r| r.face() == Face::Mono).collect();
        assert_eq!(mono, vec![Role::Mono]);
    }
}
