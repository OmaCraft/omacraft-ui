//! The suite command palette. One widget, the same in all four apps.
//!
//! It opens over the title-bar search box: its input sits exactly on the box and the results drop
//! down under it, so the box seems to come alive. Ctrl+Shift+P (⇧⌘P on macOS) opens it too.
//! Each row is `label · menu path · shortcut`. Matched characters are bold, and the selected row
//! has the hover fill plus an accent bar. Notes adds a "Pages" section under the commands.

use crate::fonts::Face;
use crate::scale::{Role, rows};
use egui::text::{LayoutJob, TextFormat};
use egui::{Align, Align2, Area, Color32, Context, Frame, Id, Key, KeyboardShortcut, Modifiers, Order, Rect, Sense, TextEdit, Ui, Vec2};

/// One command, as the app supplies it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Command {
    /// Stable id, e.g. `view.appearance`. Run with this.
    pub id: String,
    pub label: String,
    /// Where it lives in the menus, e.g. `Home › Font`.
    pub path: String,
    /// Display text for the current platform, e.g. `Ctrl+B` or `⌘B`.
    pub shortcut: Option<String>,
    pub enabled: bool,
}

impl Command {
    pub fn new(id: impl Into<String>, label: impl Into<String>, path: impl Into<String>) -> Self {
        Self { id: id.into(), label: label.into(), path: path.into(), shortcut: None, enabled: true }
    }

    pub fn shortcut(mut self, shortcut: impl Into<String>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

/// A non-command result, e.g. a Notes page. The app finds these for the current query.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Extra {
    pub id: String,
    pub label: String,
    /// Dim text after the label, e.g. the notebook and section.
    pub detail: String,
}

/// What the user picked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Picked {
    Command(String),
    Extra(String),
}

/// The shortcut that opens the palette: Ctrl+Shift+P, ⇧⌘P on macOS.
pub const SHORTCUT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::P);

const RECENT_MAX: usize = 8;
const VISIBLE_ROWS: usize = 12;
const MIN_WIDTH: f32 = 420.0;
const PAGE: usize = 8;

/// A fuzzy match: higher score is better; `hits` are char indices into the label.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Match {
    pub score: i32,
    pub hits: Vec<usize>,
}

/// Fuzzy-match `query` against a command. The label is matched as a subsequence, with bonuses for
/// word starts and runs. If the label does not match, the menu path and id are tried as plain
/// substrings with a lower score and no highlights.
pub fn matches(query: &str, command: &Command) -> Option<Match> {
    let query: Vec<char> = query.chars().filter(|c| !c.is_whitespace()).flat_map(char::to_lowercase).collect();
    if query.is_empty() {
        return Some(Match { score: 0, hits: Vec::new() });
    }
    if let Some(m) = subsequence(&query, &command.label) {
        return Some(m);
    }
    // Menu separators ("›", "/") and spaces do not count, so "homefont" finds "Home › Font".
    let keep = |c: &char| c.is_alphanumeric() || *c == '.' || *c == '_';
    let needle: String = query.iter().filter(|c| keep(c)).collect();
    let haystack: String = format!("{}{}", command.path, command.id).to_lowercase().chars().filter(keep).collect();
    if needle.is_empty() {
        return None;
    }
    haystack.contains(&needle).then_some(Match { score: 1, hits: Vec::new() })
}

fn subsequence(query: &[char], label: &str) -> Option<Match> {
    let chars: Vec<char> = label.chars().collect();
    let lower: Vec<char> = chars.iter().map(|c| c.to_lowercase().next().unwrap_or(*c)).collect();
    let mut hits = Vec::with_capacity(query.len());
    let mut score = 0;
    let mut at = 0;
    for &q in query {
        // Prefer the next word start that matches, else the next plain match.
        let next_plain = (at..lower.len()).find(|&i| lower[i] == q)?;
        let next_word = (at..lower.len()).find(|&i| lower[i] == q && word_start(&chars, i));
        let consecutive = hits.last().is_some_and(|&last| last + 1 == next_plain);
        let i = match next_word {
            Some(w) if !consecutive => w,
            _ => next_plain,
        };
        score += 10;
        if word_start(&chars, i) {
            score += 15;
        }
        if hits.last().is_some_and(|&last| last + 1 == i) {
            score += 10;
        }
        if hits.is_empty() {
            score -= i as i32;
        }
        hits.push(i);
        at = i + 1;
    }
    score -= (chars.len() as i32 - query.len() as i32) / 4;
    Some(Match { score: score.max(2), hits })
}

fn word_start(chars: &[char], i: usize) -> bool {
    i == 0 || (!chars[i - 1].is_alphanumeric() && chars[i].is_alphanumeric()) || (chars[i].is_uppercase() && chars[i - 1].is_lowercase())
}

#[derive(Clone, Debug, PartialEq)]
enum Row {
    Header(&'static str),
    Command { index: usize, hits: Vec<usize> },
    Extra(usize),
}

impl Row {
    fn selectable(&self) -> bool {
        !matches!(self, Row::Header(_))
    }
}

/// Palette state. Keep one per window.
#[derive(Clone, Debug, Default)]
pub struct Palette {
    open: bool,
    query: String,
    selected: usize,
    recent: Vec<String>,
    focus: bool,
}

impl Palette {
    pub fn open(&mut self) {
        if !self.open {
            self.open = true;
            self.query.clear();
            self.selected = 0;
            self.focus = true;
        }
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// The current query, for apps that add [`Extra`] results (Notes page search).
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Recent command ids, newest first, for saving with the app settings.
    pub fn recent(&self) -> &[String] {
        &self.recent
    }

    pub fn set_recent(&mut self, recent: Vec<String>) {
        self.recent = recent;
        self.recent.truncate(RECENT_MAX);
    }

    /// Record a run, including runs from the menus, so "Recent" stays useful.
    pub fn note_run(&mut self, id: &str) {
        self.recent.retain(|r| r != id);
        self.recent.insert(0, id.to_string());
        self.recent.truncate(RECENT_MAX);
    }

    /// Opens on Ctrl+Shift+P / ⇧⌘P. Call once per frame before drawing the title bar.
    pub fn handle_shortcut(&mut self, ctx: &Context) {
        if ctx.input_mut(|i| i.consume_shortcut(&SHORTCUT)) {
            self.open();
        }
    }

    fn rows(&self, commands: &[Command], extra_title: &'static str, extras: &[Extra]) -> Vec<Row> {
        let mut rows = Vec::new();
        if self.query.trim().is_empty() {
            let recent: Vec<usize> = self.recent.iter().filter_map(|id| commands.iter().position(|c| &c.id == id && c.enabled)).collect();
            if !recent.is_empty() {
                rows.push(Row::Header("Recent"));
                rows.extend(recent.iter().map(|&index| Row::Command { index, hits: Vec::new() }));
                rows.push(Row::Header("All commands"));
            }
            let mut all: Vec<usize> = (0..commands.len()).filter(|i| !recent.contains(i)).collect();
            all.sort_by(|&a, &b| commands[a].label.to_lowercase().cmp(&commands[b].label.to_lowercase()));
            rows.extend(all.into_iter().map(|index| Row::Command { index, hits: Vec::new() }));
            return rows;
        }
        let mut hits: Vec<(usize, Match)> = commands.iter().enumerate().filter_map(|(i, c)| matches(&self.query, c).map(|m| (i, m))).collect();
        hits.sort_by(|(a, ma), (b, mb)| {
            commands[*b]
                .enabled
                .cmp(&commands[*a].enabled)
                .then(mb.score.cmp(&ma.score))
                .then(commands[*a].label.len().cmp(&commands[*b].label.len()))
        });
        rows.extend(hits.into_iter().map(|(index, m)| Row::Command { index, hits: m.hits }));
        if !extras.is_empty() {
            rows.push(Row::Header(extra_title));
            rows.extend((0..extras.len()).map(Row::Extra));
        }
        rows
    }

    /// Draw the palette over `anchor` (the title-bar search box) if open. Returns what was picked;
    /// the app runs it. A picked command is added to "Recent".
    pub fn show(&mut self, ctx: &Context, anchor: Rect, commands: &[Command]) -> Option<Picked> {
        self.show_with(ctx, anchor, commands, "Pages", &[])
    }

    /// As [`Palette::show`], with a section of app results under the commands (Notes pages).
    pub fn show_with(&mut self, ctx: &Context, anchor: Rect, commands: &[Command], extra_title: &'static str, extras: &[Extra]) -> Option<Picked> {
        if !self.open {
            return None;
        }
        // The click that opened the palette must not also close it.
        let just_opened = self.focus;
        let rows = self.rows(commands, extra_title, extras);
        let selectable: Vec<usize> = (0..rows.len()).filter(|&i| rows[i].selectable()).collect();
        self.selected = self.selected.min(selectable.len().saturating_sub(1));

        let (escape, enter, step) = ctx.input_mut(|i| {
            let escape = i.consume_key(Modifiers::NONE, Key::Escape);
            let enter = i.consume_key(Modifiers::NONE, Key::Enter);
            let mut step: isize = 0;
            for (key, delta) in [(Key::ArrowDown, 1), (Key::ArrowUp, -1), (Key::PageDown, PAGE as isize), (Key::PageUp, -(PAGE as isize))] {
                if i.consume_key(Modifiers::NONE, key) {
                    step += delta;
                }
            }
            if i.consume_key(Modifiers::NONE, Key::Home) {
                step = isize::MIN;
            }
            if i.consume_key(Modifiers::NONE, Key::End) {
                step = isize::MAX;
            }
            (escape, enter, step)
        });
        if escape {
            self.close();
            return None;
        }
        let last = selectable.len().saturating_sub(1);
        self.selected = match step {
            isize::MIN => 0,
            isize::MAX => last,
            s => (self.selected as isize + s).clamp(0, last as isize) as usize,
        };
        let scroll_to = step != 0;

        let mut picked = None;
        if enter {
            picked = selectable.get(self.selected).and_then(|&r| pick(&rows[r], commands, extras));
        }

        let width = anchor.width().max(MIN_WIDTH);
        let id = Id::unique("omacraft-ui/palette");
        let area = Area::new(id)
            .order(Order::Foreground)
            .pivot(Align2::LEFT_TOP)
            // The frame margin is 4, so the input lands exactly on the search box.
            .fixed_pos(anchor.left_top() - Vec2::splat(4.0))
            .show(ctx, |ui| {
                Frame::popup(ui.style()).inner_margin(4.0).show(ui, |ui| {
                    ui.set_width(width);
                    let edit = TextEdit::singleline(&mut self.query)
                        .font(Role::Menu.font())
                        .hint_text("Search commands")
                        .desired_width(f32::INFINITY)
                        .frame(egui::Frame::NONE);
                    let response = ui.add_sized([width, anchor.height().max(rows::MENU)], edit);
                    if self.focus {
                        response.request_focus();
                        self.focus = false;
                    }
                    if response.changed() {
                        self.selected = 0;
                    }
                    ui.separator();
                    let max_height = rows::MENU * VISIBLE_ROWS as f32;
                    egui::ScrollArea::vertical().max_height(max_height).auto_shrink([false, true]).show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        if rows.iter().all(|r| !r.selectable()) {
                            ui.add_space(4.0);
                            ui.label(egui::RichText::new("No matching commands").font(Role::Caption.font()).color(ui.visuals().weak_text_color()));
                            return;
                        }
                        let mut n = 0;
                        for row in &rows {
                            let selected = row.selectable() && n == self.selected;
                            let clicked = draw_row(ui, row, selected, scroll_to, commands, extras);
                            if row.selectable() {
                                if clicked.hovered && ui.input(|i| i.pointer.delta() != Vec2::ZERO) {
                                    self.selected = n;
                                }
                                if clicked.clicked {
                                    picked = pick(row, commands, extras);
                                }
                                n += 1;
                            }
                        }
                    });
                });
            });
        if !just_opened && area.response.clicked_elsewhere() && picked.is_none() {
            self.close();
        }
        if let Some(p) = &picked {
            if let Picked::Command(id) = p {
                self.note_run(id);
            }
            self.close();
        }
        picked
    }
}

fn pick(row: &Row, commands: &[Command], extras: &[Extra]) -> Option<Picked> {
    match row {
        Row::Command { index, .. } => {
            let c = &commands[*index];
            c.enabled.then(|| Picked::Command(c.id.clone()))
        }
        Row::Extra(i) => Some(Picked::Extra(extras[*i].id.clone())),
        Row::Header(_) => None,
    }
}

struct RowResponse {
    hovered: bool,
    clicked: bool,
}

fn draw_row(ui: &mut Ui, row: &Row, selected: bool, scroll_to: bool, commands: &[Command], extras: &[Extra]) -> RowResponse {
    let visuals = ui.visuals().clone();
    let width = ui.available_width();
    if let Row::Header(title) = row {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, rows::MENU), Sense::hover());
        ui.painter().text(rect.left_center() + Vec2::new(10.0, 0.0), Align2::LEFT_CENTER, *title, Role::Section.font(), visuals.weak_text_color());
        return RowResponse { hovered: false, clicked: false };
    }
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, rows::MENU), Sense::click());
    if selected {
        ui.painter().rect_filled(rect, 4.0, visuals.widgets.hovered.weak_bg_fill);
        let bar = Rect::from_min_size(rect.left_top() + Vec2::new(0.0, 4.0), Vec2::new(3.0, rect.height() - 8.0));
        ui.painter().rect_filled(bar, 1.5, visuals.selection.bg_fill);
        if scroll_to {
            response.scroll_to_me(Some(Align::Center));
        }
    }
    let (label, hits, detail, shortcut, enabled): (&str, &[usize], &str, Option<&str>, bool) = match row {
        Row::Command { index, hits } => {
            let c = &commands[*index];
            (&c.label, hits, &c.path, c.shortcut.as_deref(), c.enabled)
        }
        Row::Extra(i) => (&extras[*i].label, &[], &extras[*i].detail, None, true),
        Row::Header(_) => unreachable!(),
    };
    let text = if enabled { visuals.text_color() } else { visuals.weak_text_color() };
    let dim = visuals.weak_text_color();
    let mut x = rect.left() + 12.0;
    let right_limit = rect.right() - 10.0;
    let shortcut_width = shortcut.map_or(0.0, |s| {
        let galley = ui.painter().layout_no_wrap(s.to_string(), Role::Shortcut.font(), dim);
        let w = galley.size().x;
        ui.painter().galley(egui::pos2(right_limit - w, rect.center().y - galley.size().y / 2.0), galley, dim);
        w + 16.0
    });
    let label_galley = ui.fonts_mut(|f| f.layout_job(label_job(label, hits, text, rect.width() * 0.6)));
    let label_w = label_galley.size().x;
    ui.painter().galley(egui::pos2(x, rect.center().y - label_galley.size().y / 2.0), label_galley, text);
    x += label_w + 10.0;
    let detail_room = right_limit - shortcut_width - x;
    if !detail.is_empty() && detail_room > 30.0 {
        let mut job = LayoutJob::simple_singleline(detail.to_string(), Role::Caption.font(), dim);
        job.wrap.max_width = detail_room;
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        let galley = ui.fonts_mut(|f| f.layout_job(job));
        ui.painter().galley(egui::pos2(x, rect.center().y - galley.size().y / 2.0), galley, dim);
    }
    RowResponse { hovered: response.hovered(), clicked: response.clicked() }
}

/// The label with matched characters in the strong face.
pub fn label_job(label: &str, hits: &[usize], color: Color32, max_width: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    let normal = TextFormat { font_id: Role::Menu.font(), color, ..Default::default() };
    let strong = TextFormat { font_id: egui::FontId::new(Role::Menu.size(), Face::UiStrong.family()), color, ..Default::default() };
    let mut run = String::new();
    let mut run_strong = false;
    for (i, ch) in label.chars().enumerate() {
        let is_hit = hits.contains(&i);
        if is_hit != run_strong && !run.is_empty() {
            job.append(&run, 0.0, if run_strong { strong.clone() } else { normal.clone() });
            run.clear();
        }
        run_strong = is_hit;
        run.push(ch);
    }
    if !run.is_empty() {
        job.append(&run, 0.0, if run_strong { strong } else { normal });
    }
    job.wrap.max_width = max_width;
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    job
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commands() -> Vec<Command> {
        vec![
            Command::new("format.bold", "Bold", "Home › Font").shortcut("Ctrl+B"),
            Command::new("view.appearance", "Appearance", "View"),
            Command::new("insert.table", "Insert Table", "Insert › Tables"),
            Command::new("file.save_as", "Save As…", "File").shortcut("Ctrl+Shift+S"),
            Command::new("data.sort", "Sort Ascending", "Data › Sort").enabled(false),
            Command::new("table.style", "Table Styles", "Table Design"),
        ]
    }

    #[test]
    fn word_starts_beat_middles() {
        let c = commands();
        let it = matches("it", &c[2]).unwrap();
        assert_eq!(it.hits, vec![0, 7], "I-nsert T-able");
        assert!(matches("tab", &c[5]).unwrap().score > matches("tab", &c[2]).unwrap().score, "prefix wins");
    }

    #[test]
    fn path_and_id_match_without_highlights() {
        let c = commands();
        let m = matches("homefont", &c[0]).unwrap();
        assert!(m.hits.is_empty());
        assert!(matches("format.bold", &c[0]).is_some());
        assert!(matches("zzz", &c[0]).is_none());
    }

    #[test]
    fn empty_query_shows_recent_first() {
        let c = commands();
        let mut p = Palette::default();
        p.note_run("file.save_as");
        p.note_run("data.sort"); // disabled: not offered as recent
        let rows = p.rows(&c, "Pages", &[]);
        assert_eq!(rows[0], Row::Header("Recent"));
        assert_eq!(rows[1], Row::Command { index: 3, hits: vec![] });
        assert_eq!(rows[2], Row::Header("All commands"));
        assert_eq!(rows.len(), 3 + c.len() - 1);
    }

    #[test]
    fn disabled_commands_sort_last_and_cannot_run() {
        let c = commands();
        let p = Palette { query: "s".into(), ..Default::default() };
        let rows = p.rows(&c, "Pages", &[]);
        let Row::Command { index, .. } = rows.last().unwrap() else { panic!() };
        assert_eq!(c[*index].id, "data.sort");
        assert_eq!(pick(rows.last().unwrap(), &c, &[]), None);
    }

    #[test]
    fn pages_follow_commands() {
        let c = commands();
        let pages = [Extra { id: "p1".into(), label: "Table of contents".into(), detail: "Work › Notes".into() }];
        let p = Palette { query: "table".into(), ..Default::default() };
        let rows = p.rows(&c, "Pages", &pages);
        let header = rows.iter().position(|r| *r == Row::Header("Pages")).unwrap();
        assert!(rows[..header].iter().all(|r| matches!(r, Row::Command { .. })));
        assert_eq!(rows[header + 1], Row::Extra(0));
        assert_eq!(pick(&rows[header + 1], &c, &pages), Some(Picked::Extra("p1".into())));
    }

    #[test]
    fn recent_is_bounded_and_deduplicated() {
        let mut p = Palette::default();
        for i in 0..20 {
            p.note_run(&format!("c{i}"));
        }
        p.note_run("c15");
        assert_eq!(p.recent().len(), RECENT_MAX);
        assert_eq!(p.recent()[0], "c15");
        assert_eq!(p.recent().iter().filter(|r| *r == "c15").count(), 1);
    }

    #[test]
    fn label_runs_split_on_hits() {
        let job = label_job("Insert Table", &[0, 7], Color32::WHITE, 300.0);
        let runs: Vec<_> = job.sections.iter().map(|s| &job.text[s.byte_range.start.0..s.byte_range.end.0]).collect();
        assert_eq!(runs, ["I", "nsert ", "T", "able"]);
    }
}
