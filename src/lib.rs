//! Shared OmaCraft suite chrome for OmaGrid, OmaWord, OmaDeck and OmaNotes.
//!
//! - [`fonts`]: font tokens (Inter, Inter Tabular, Instrument Sans, system mono), the fallback
//!   resolver and the "Use system font" switch.
//! - [`scale`]: one type scale for every chrome surface.
//! - [`appearance`]: the three-state Follow desktop / Light / Dark control.
//! - [`palette`]: the command palette.
//!
//! Only chrome lives here. Document rendering, commands and file formats stay in each app.

pub mod appearance;
pub mod fonts;
pub mod palette;
pub mod scale;

pub use appearance::Mode;
pub use fonts::{Face, FontOptions, SystemFaces};
pub use palette::{Command, Palette, Picked};
pub use scale::Role;

/// Install fonts and the type scale in one call. `system` comes from [`SystemFaces::detect`] on a
/// worker thread (or `SystemFaces::default()` before it finishes).
pub fn install(ctx: &egui::Context, mut defs: egui::FontDefinitions, options: FontOptions, system: &SystemFaces, script_fallbacks: &[&str]) {
    fonts::apply(&mut defs, options, system, script_fallbacks);
    ctx.set_fonts(defs);
    ctx.all_styles_mut(scale::apply);
}
