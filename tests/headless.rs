//! End-to-end checks in a real egui context, without a window.

use egui::{Context, Event, FontDefinitions, Key, Modifiers, Pos2, RawInput, Rect, Vec2};
use omacraft_ui::{Command, Face, FontOptions, Mode, Palette, Picked, Role, SystemFaces, install};

fn context(options: FontOptions) -> Context {
    let ctx = Context::default();
    install(&ctx, FontDefinitions::default(), options, &SystemFaces::default(), &[]);
    // Fonts are applied at the start of the next frame.
    step(&ctx, vec![], |_| {});
    ctx
}

/// One frame. Textures are not uploaded anywhere, so the delta is dropped on purpose.
fn step(ctx: &Context, events: Vec<Event>, f: impl FnMut(&mut egui::Ui)) {
    let mut out = ctx.run_ui(input(events), f);
    out.textures_delta.clear();
}

fn input(events: Vec<Event>) -> RawInput {
    RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440.0, 900.0))), events, ..Default::default() }
}

fn key(key: Key) -> Event {
    Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }
}

fn width(ctx: &Context, text: &str, role: Role) -> f32 {
    ctx.fonts_mut(|f| f.layout_no_wrap(text.into(), role.font(), egui::Color32::WHITE).size().x)
}

#[test]
fn numeric_digits_line_up_and_ui_digits_do_not() {
    let ctx = context(FontOptions::default());
    let numeric: Vec<f32> = "0123456789".chars().map(|c| width(&ctx, &c.to_string(), Role::Numeric)).collect();
    assert!(numeric.windows(2).all(|w| (w[0] - w[1]).abs() < 0.01), "{numeric:?}");
    let ui: Vec<f32> = "0123456789".chars().map(|c| width(&ctx, &c.to_string(), Role::Caption)).collect();
    assert!(ui.windows(2).any(|w| (w[0] - w[1]).abs() > 0.1), "Inter's default digits are proportional");
    assert!((width(&ctx, "1111", Role::Numeric) - width(&ctx, "8888", Role::Numeric)).abs() < 0.01);
}

#[test]
fn ui_is_proportional_and_mono_is_not() {
    let ctx = context(FontOptions::default());
    assert!(width(&ctx, "iiii", Role::Body) < width(&ctx, "MMMM", Role::Body) * 0.6);
    assert!((width(&ctx, "iiii", Role::Mono) - width(&ctx, "MMMM", Role::Mono)).abs() < 0.01);
    // Every token resolves to a face that can draw Turkish text.
    for face in Face::ALL {
        let font = egui::FontId::new(13.0, face.family());
        let w = ctx.fonts_mut(|f| f.layout_no_wrap("Çğış ÖÜ".into(), font, egui::Color32::WHITE).size().x);
        assert!(w > 20.0, "{face:?}");
    }
}

fn commands() -> Vec<Command> {
    vec![
        Command::new("format.bold", "Bold", "Home › Font").shortcut("Ctrl+B"),
        Command::new("insert.table", "Insert Table", "Insert › Tables"),
        Command::new("view.appearance", "Appearance", "View"),
    ]
}

fn frame(ctx: &Context, palette: &mut Palette, events: Vec<Event>) -> Option<Picked> {
    let mut picked = None;
    let anchor = Rect::from_min_size(Pos2::new(880.0, 6.0), Vec2::new(260.0, 26.0));
    step(ctx, events, |ui| {
        palette.handle_shortcut(ui.ctx());
        picked = palette.show(ui.ctx(), anchor, &commands());
    });
    picked
}

#[test]
fn palette_types_navigates_and_runs() {
    let ctx = context(FontOptions::default());
    let mut palette = Palette::default();
    let open = Event::Key { key: Key::P, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::COMMAND | Modifiers::SHIFT };
    assert_eq!(frame(&ctx, &mut palette, vec![open]), None);
    assert!(palette.is_open());
    // Focus lands in the input on the next frame; then type.
    frame(&ctx, &mut palette, vec![]);
    frame(&ctx, &mut palette, vec![Event::Text("ta".into())]);
    assert_eq!(palette.query(), "ta");
    let picked = frame(&ctx, &mut palette, vec![key(Key::Enter)]);
    assert_eq!(picked, Some(Picked::Command("insert.table".into())));
    assert!(!palette.is_open());
    assert_eq!(palette.recent()[0], "insert.table");

    // Reopen: the empty query lists Recent first; Escape closes without running.
    palette.open();
    frame(&ctx, &mut palette, vec![]);
    assert_eq!(frame(&ctx, &mut palette, vec![key(Key::ArrowDown), key(Key::Escape)]), None);
    assert!(!palette.is_open());
}

#[test]
fn appearance_toggle_cycles_on_click() {
    let ctx = context(FontOptions::default());
    let mut mode = Mode::default();
    let run = |events: Vec<Event>, mode: &mut Mode| {
        let mut changed = false;
        let mut rect = Rect::NOTHING;
        step(&ctx, events, |ui| {
            changed = omacraft_ui::appearance::toggle(ui, mode, |_| {});
            rect = ui.min_rect();
        });
        (changed, rect)
    };
    let (_, rect) = run(vec![], &mut mode);
    let at = rect.left_top() + omacraft_ui::appearance::BUTTON_SIZE / 2.0;
    let click = |pressed| Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: Modifiers::NONE };
    run(vec![Event::PointerMoved(at)], &mut mode);
    run(vec![click(true)], &mut mode);
    assert!(run(vec![click(false)], &mut mode).0);
    assert_eq!(mode, Mode::Light);
}
