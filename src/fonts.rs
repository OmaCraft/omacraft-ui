//! Font tokens and the resolver.
//!
//! - `Ui` is bundled Inter, `UiStrong` Inter SemiBold.
//! - `Numeric` is Inter Tabular: Inter with `tnum` frozen into the digits, because egui cannot switch
//!   OpenType features on at runtime.
//! - `Display` is Instrument Sans SemiBold, for headings and brand moments at 16 pt and up.
//! - `Mono` is the system monospace font (the user's terminal font on Omarchy), only for the formula
//!   bar, code and cell editing.
//!
//! After Inter comes the system UI stack (`system-ui, -apple-system, "Segoe UI Variable", "Segoe UI",
//! "Adwaita Sans", Cantarell, "Noto Sans", sans-serif`) as real files, then the app's bundled script
//! faces, so no script renders as boxes. "Use system font" puts the system stack in front of Inter for
//! `Ui`, `UiStrong` and `Numeric`. `Display`, `Mono` and document fonts never change.
//!
//! Detecting system fonts runs subprocesses, so [`SystemFaces::detect`] belongs on a worker thread.
//! [`apply`] is pure and cheap.

use egui::{FontData, FontDefinitions, FontFamily};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const INTER_REGULAR: &[u8] = include_bytes!("../assets/fonts/Inter-Regular.ttf");
pub const INTER_SEMIBOLD: &[u8] = include_bytes!("../assets/fonts/Inter-SemiBold.ttf");
pub const INTER_TABULAR_REGULAR: &[u8] = include_bytes!("../assets/fonts/InterTabular-Regular.ttf");
pub const INTER_TABULAR_SEMIBOLD: &[u8] = include_bytes!("../assets/fonts/InterTabular-SemiBold.ttf");
pub const INSTRUMENT_SANS_REGULAR: &[u8] = include_bytes!("../assets/fonts/InstrumentSans-Regular.ttf");
pub const INSTRUMENT_SANS_SEMIBOLD: &[u8] = include_bytes!("../assets/fonts/InstrumentSans-SemiBold.ttf");

/// Keys of the bundled faces in `FontDefinitions::font_data`.
pub mod keys {
    pub const INTER: &str = "omacraft/Inter-Regular";
    pub const INTER_STRONG: &str = "omacraft/Inter-SemiBold";
    pub const NUMERIC: &str = "omacraft/InterTabular-Regular";
    pub const NUMERIC_STRONG: &str = "omacraft/InterTabular-SemiBold";
    pub const DISPLAY: &str = "omacraft/InstrumentSans-SemiBold";
    pub const DISPLAY_REGULAR: &str = "omacraft/InstrumentSans-Regular";
}

const FONT_LIMIT: u64 = 32 * 1024 * 1024;

/// A font token. Chrome code picks a token, never a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Face {
    Ui,
    UiStrong,
    Numeric,
    Display,
    Mono,
}

impl Face {
    pub const ALL: [Face; 5] = [Face::Ui, Face::UiStrong, Face::Numeric, Face::Display, Face::Mono];

    /// `Ui` is egui's `Proportional`, so every widget without an explicit font gets Inter.
    /// `Mono` is egui's `Monospace`.
    pub fn family(self) -> FontFamily {
        match self {
            Face::Ui => FontFamily::Proportional,
            Face::Mono => FontFamily::Monospace,
            Face::UiStrong => FontFamily::Name("omacraft.ui.strong".into()),
            Face::Numeric => FontFamily::Name("omacraft.numeric".into()),
            Face::Display => FontFamily::Name("omacraft.display".into()),
        }
    }
}

/// User-facing font settings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FontOptions {
    /// "Use system font". Off by default.
    pub use_system_font: bool,
}

impl FontOptions {
    /// Migrate the old per-app options ("Omarchy font", "Follow desktop font"). Any of them set to
    /// on means the user wanted the desktop's font.
    pub fn migrate(old_omarchy_font: Option<bool>, old_follow_desktop_font: Option<bool>) -> Self {
        Self { use_system_font: old_omarchy_font.unwrap_or(false) || old_follow_desktop_font.unwrap_or(false) }
    }
}

/// A system font file that was found and loaded.
#[derive(Clone)]
pub struct SystemFace {
    /// The family name the system reported for this file.
    pub family: String,
    pub path: PathBuf,
    pub index: u32,
    pub data: Arc<Vec<u8>>,
}

impl std::fmt::Debug for SystemFace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SystemFace").field("family", &self.family).field("path", &self.path).finish()
    }
}

impl PartialEq for SystemFace {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path && self.index == other.index
    }
}

/// What the system offers. Detect once on a worker thread; compare snapshots to see if fonts changed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SystemFaces {
    /// The system UI stack in order, regular weight. Only exact family matches.
    pub ui: Vec<SystemFace>,
    /// Bold faces for the same stack, where the system has them.
    pub ui_bold: Vec<SystemFace>,
    /// The system monospace face, if any. Mono has one weight in chrome.
    pub mono: Option<SystemFace>,
}

/// Where a face comes from, before it is loaded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Candidate {
    /// A family name that must come back from the lookup exactly, e.g. `Cantarell`.
    Family(String),
    /// An alias whose answer is the user's choice, e.g. fontconfig's `monospace`.
    Alias(String),
    /// A known file, e.g. `C:\Windows\Fonts\segoeui.ttf`.
    File { family: String, path: PathBuf },
}

/// What a font lookup returned: the families of the matched face and its file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Matched {
    pub families: Vec<String>,
    pub path: PathBuf,
    pub index: u32,
}

/// A font lookup, e.g. `fc-match`. It may return a substitute; the resolver checks the family.
pub trait Lookup {
    fn find(&self, pattern: &str, bold: bool) -> Option<Matched>;
}

/// A lookup that finds nothing (macOS and Windows use known files instead).
pub struct NoLookup;

impl Lookup for NoLookup {
    fn find(&self, _: &str, _: bool) -> Option<Matched> {
        None
    }
}

impl SystemFaces {
    /// Detect the platform's faces. Blocking; each subprocess is bounded to one second.
    pub fn detect() -> Self {
        let (ui, mono) = platform::candidates();
        Self::resolve(&*platform::lookup(), &ui, &mono, &load_file)
    }

    /// The resolver itself, with the lookup and file reader injected for tests.
    pub fn resolve(lookup: &dyn Lookup, ui: &[Candidate], mono: &[Candidate], load: &dyn Fn(&Path) -> Option<Vec<u8>>) -> Self {
        let mut out = Self::default();
        for candidate in ui {
            if let Some(face) = resolve_one(lookup, candidate, false, load)
                && !out.ui.contains(&face)
            {
                out.ui.push(face);
            }
            if let Some(face) = resolve_one(lookup, candidate, true, load)
                && !out.ui_bold.contains(&face)
                && !out.ui.contains(&face)
            {
                out.ui_bold.push(face);
            }
        }
        out.mono = mono.iter().find_map(|c| resolve_one(lookup, c, false, load));
        out
    }
}

fn resolve_one(lookup: &dyn Lookup, candidate: &Candidate, bold: bool, load: &dyn Fn(&Path) -> Option<Vec<u8>>) -> Option<SystemFace> {
    let (family, path, index) = match candidate {
        Candidate::Family(want) => {
            let m = lookup.find(want, bold)?;
            // fontconfig answers every query with something; only an exact family counts.
            let got = m.families.iter().find(|f| f.eq_ignore_ascii_case(want))?;
            (got.clone(), m.path, m.index)
        }
        Candidate::Alias(alias) => {
            let m = lookup.find(alias, bold)?;
            (m.families.first()?.clone(), m.path, m.index)
        }
        Candidate::File { family, path } => {
            if bold {
                return None;
            }
            (family.clone(), path.clone(), 0)
        }
    };
    let data = load(&path)?;
    Some(SystemFace { family, path, index, data: Arc::new(data) })
}

fn load_file(path: &Path) -> Option<Vec<u8>> {
    use std::io::Read;
    let file = std::fs::File::open(path).ok()?;
    if file.metadata().ok()?.len() > FONT_LIMIT {
        return None;
    }
    let mut data = Vec::new();
    file.take(FONT_LIMIT).read_to_end(&mut data).ok()?;
    Some(data)
}

/// Install the bundled faces, the system faces and the app's script fallbacks into `defs`.
///
/// `script_fallbacks` are keys the app already put in `defs.font_data` (Noto Arabic, CJK and so on).
/// The app's existing `Proportional` and `Monospace` lists (egui's defaults, emoji) stay at the end.
pub fn apply(defs: &mut FontDefinitions, options: FontOptions, system: &SystemFaces, script_fallbacks: &[&str]) {
    let data = &mut defs.font_data;
    data.insert(keys::INTER.into(), Arc::new(FontData::from_static(INTER_REGULAR)));
    data.insert(keys::INTER_STRONG.into(), Arc::new(FontData::from_static(INTER_SEMIBOLD)));
    data.insert(keys::NUMERIC.into(), Arc::new(FontData::from_static(INTER_TABULAR_REGULAR)));
    data.insert(keys::NUMERIC_STRONG.into(), Arc::new(FontData::from_static(INTER_TABULAR_SEMIBOLD)));
    data.insert(keys::DISPLAY.into(), Arc::new(FontData::from_static(INSTRUMENT_SANS_SEMIBOLD)));
    data.insert(keys::DISPLAY_REGULAR.into(), Arc::new(FontData::from_static(INSTRUMENT_SANS_REGULAR)));

    let mut insert = |face: &SystemFace| {
        let key = format!("system/{}#{}", face.path.display(), face.index);
        let blob: Arc<dyn AsRef<[u8]> + Send + Sync> = Arc::new(ArcBytes(face.data.clone()));
        data.entry(key.clone()).or_insert_with(|| Arc::new(FontData::from_blob(blob, face.index)));
        key
    };
    let system_ui: Vec<String> = system.ui.iter().map(&mut insert).collect();
    let system_bold: Vec<String> = system.ui_bold.iter().map(&mut insert).collect();
    let system_mono: Vec<String> = system.mono.iter().map(&mut insert).collect();

    let scripts: Vec<String> = script_fallbacks.iter().map(|s| s.to_string()).collect();
    let old_proportional = defs.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let old_mono = defs.families.get(&FontFamily::Monospace).cloned().unwrap_or_default();

    let inter = vec![keys::INTER.to_string()];
    let ui_front = if options.use_system_font { [&system_ui[..], &inter].concat() } else { [&inter[..], &system_ui].concat() };
    let ui = chain(&[&ui_front, &scripts, &old_proportional]);

    let strong_front: Vec<String> = if options.use_system_font {
        [&system_bold[..], &system_ui, &[keys::INTER_STRONG.to_string()]].concat()
    } else {
        vec![keys::INTER_STRONG.to_string()]
    };
    let strong = chain(&[&strong_front, &ui]);

    let numeric_front = if options.use_system_font { system_ui.clone() } else { vec![keys::NUMERIC.to_string()] };
    let numeric = chain(&[&numeric_front, &ui]);

    let display = chain(&[&[keys::DISPLAY.to_string()], &ui]);
    let mono = chain(&[&system_mono, &old_mono, &ui]);

    let families = &mut defs.families;
    families.insert(Face::Ui.family(), ui);
    families.insert(Face::UiStrong.family(), strong);
    families.insert(Face::Numeric.family(), numeric);
    families.insert(Face::Display.family(), display);
    families.insert(Face::Mono.family(), mono);
}

/// Concatenate, keeping the first occurrence of each key.
fn chain(parts: &[&[String]]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for key in parts.iter().flat_map(|p| p.iter()) {
        if !out.contains(key) {
            out.push(key.clone());
        }
    }
    out
}

struct ArcBytes(Arc<Vec<u8>>);

impl AsRef<[u8]> for ArcBytes {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

/// Run a helper with no shell, no stdin and a hard deadline. A stuck helper is killed.
#[cfg(all(unix, not(target_os = "macos")))]
pub(crate) fn run_bounded(program: &str, args: &[&str], timeout: std::time::Duration) -> Option<String> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    let mut child = Command::new(program).args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break,
            Ok(Some(_)) | Err(_) => return None,
            Ok(None) if std::time::Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(10)),
        }
    }
    let mut out = String::new();
    child.stdout.take()?.take(64 * 1024).read_to_string(&mut out).ok()?;
    Some(out)
}

/// fontconfig's `fc-match`.
#[cfg(all(unix, not(target_os = "macos")))]
pub struct FcMatch;

#[cfg(all(unix, not(target_os = "macos")))]
impl Lookup for FcMatch {
    fn find(&self, pattern: &str, bold: bool) -> Option<Matched> {
        let query = if bold { format!("{pattern}:weight=bold") } else { pattern.to_string() };
        let out = run_bounded("fc-match", &["--format", "%{family}\n%{file}\n%{index}\n", &query], std::time::Duration::from_secs(1))?;
        parse_fc_match(&out)
    }
}

/// Parse `fc-match --format "%{family}\n%{file}\n%{index}\n"`.
pub fn parse_fc_match(out: &str) -> Option<Matched> {
    let mut lines = out.lines();
    let families = lines.next()?.split(',').map(|f| f.trim().to_string()).filter(|f| !f.is_empty()).collect::<Vec<_>>();
    let path = PathBuf::from(lines.next()?.trim());
    // fontconfig puts a variable font's named instance in the high 16 bits (Cantarell Bold is
    // 262144). egui wants the face index only, and draws the default instance.
    let index = lines.next().and_then(|i| i.trim().parse::<u32>().ok()).unwrap_or(0) & 0xFFFF;
    (!families.is_empty() && !path.as_os_str().is_empty()).then_some(Matched { families, path, index })
}

/// Parse `gsettings get org.gnome.desktop.interface font-name`, e.g. `'Adwaita Sans 11'`.
pub fn parse_gtk_font_name(out: &str) -> Option<String> {
    let name = out.trim().trim_matches('\'').trim();
    let family = match name.rsplit_once(' ') {
        Some((family, size)) if size.parse::<f32>().is_ok() => family,
        _ => name,
    };
    // Pango style words after the family ("Inter Bold 11") are not part of the family name.
    let family = ["Bold", "Italic", "Oblique", "Light", "Medium", "Semi-Bold", "Regular"]
        .iter()
        .fold(family.trim(), |f, word| f.strip_suffix(word).map(str::trim).unwrap_or(f));
    (!family.is_empty()).then(|| family.to_string())
}

mod platform {
    use super::*;

    #[cfg(all(unix, not(target_os = "macos")))]
    pub fn lookup() -> Box<dyn Lookup> {
        Box::new(FcMatch)
    }

    #[cfg(not(all(unix, not(target_os = "macos"))))]
    pub fn lookup() -> Box<dyn Lookup> {
        Box::new(NoLookup)
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    pub fn candidates() -> (Vec<Candidate>, Vec<Candidate>) {
        let mut ui = Vec::new();
        if let Some(gtk) = run_bounded("gsettings", &["get", "org.gnome.desktop.interface", "font-name"], std::time::Duration::from_secs(1))
            .as_deref()
            .and_then(parse_gtk_font_name)
        {
            ui.push(Candidate::Family(gtk));
        }
        for family in ["Adwaita Sans", "Cantarell", "Noto Sans"] {
            ui.push(Candidate::Family(family.into()));
        }
        (ui, vec![Candidate::Alias("monospace".into())])
    }

    #[cfg(target_os = "macos")]
    pub fn candidates() -> (Vec<Candidate>, Vec<Candidate>) {
        let file = |family: &str, path: &str| Candidate::File { family: family.into(), path: path.into() };
        (
            vec![file("SF Pro", "/System/Library/Fonts/SFNS.ttf")],
            vec![file("SF Mono", "/System/Library/Fonts/SFNSMono.ttf"), file("Menlo", "/System/Library/Fonts/Menlo.ttc")],
        )
    }

    #[cfg(windows)]
    pub fn candidates() -> (Vec<Candidate>, Vec<Candidate>) {
        let fonts = std::env::var_os("WINDIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows")).join("Fonts");
        let file = |family: &str, name: &str| Candidate::File { family: family.into(), path: fonts.join(name) };
        (
            vec![file("Segoe UI Variable", "SegUIVar.ttf"), file("Segoe UI", "segoeui.ttf")],
            vec![file("Cascadia Mono", "CascadiaMono.ttf"), file("Consolas", "consola.ttf")],
        )
    }

    #[cfg(not(any(unix, windows)))]
    pub fn candidates() -> (Vec<Candidate>, Vec<Candidate>) {
        (Vec::new(), Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Pretends to be fontconfig on the Nuc: unknown families come back as Liberation Sans.
    struct Fake(HashMap<(&'static str, bool), (&'static str, &'static str)>);

    impl Lookup for Fake {
        fn find(&self, pattern: &str, bold: bool) -> Option<Matched> {
            let (family, path) = self
                .0
                .iter()
                .find(|((p, b), _)| *p == pattern && *b == bold)
                .map(|(_, v)| *v)
                .unwrap_or(("Liberation Sans", "/usr/share/fonts/liberation/LiberationSans-Regular.ttf"));
            Some(Matched { families: vec![family.into()], path: path.into(), index: 0 })
        }
    }

    fn nuc() -> Fake {
        Fake(HashMap::from([
            (("Adwaita Sans", false), ("Adwaita Sans", "/usr/share/fonts/adwaita/AdwaitaSans-Regular.ttf")),
            (("Adwaita Sans", true), ("Adwaita Sans", "/usr/share/fonts/adwaita/AdwaitaSans-Regular.ttf")),
            (("Noto Sans", false), ("Noto Sans", "/usr/share/fonts/noto/NotoSans-Regular.ttf")),
            (("Noto Sans", true), ("Noto Sans", "/usr/share/fonts/noto/NotoSans-Bold.ttf")),
            (("monospace", false), ("JetBrainsMono Nerd Font", "/usr/share/fonts/JetBrainsMonoNerdFont-Regular.ttf")),
            (("monospace", true), ("JetBrainsMono Nerd Font", "/usr/share/fonts/JetBrainsMonoNerdFont-Bold.ttf")),
        ]))
    }

    fn any_file(_: &Path) -> Option<Vec<u8>> {
        Some(vec![0])
    }

    fn nuc_faces() -> SystemFaces {
        let ui: Vec<_> = ["Adwaita Sans", "Cantarell", "Noto Sans"].into_iter().map(|f| Candidate::Family(f.into())).collect();
        SystemFaces::resolve(&nuc(), &ui, &[Candidate::Alias("monospace".into())], &any_file)
    }

    #[test]
    fn substitutes_are_rejected() {
        let faces = nuc_faces();
        let families: Vec<_> = faces.ui.iter().map(|f| f.family.as_str()).collect();
        assert_eq!(families, ["Adwaita Sans", "Noto Sans"], "Cantarell -> Liberation Sans must not slip in");
        // Adwaita Sans has one file for both weights; bold must not duplicate the regular face.
        let bold: Vec<_> = faces.ui_bold.iter().map(|f| f.family.as_str()).collect();
        assert_eq!(bold, ["Noto Sans"]);
    }

    #[test]
    fn mono_alias_takes_the_users_choice() {
        let faces = nuc_faces();
        assert_eq!(faces.mono.unwrap().family, "JetBrainsMono Nerd Font");
    }

    #[test]
    fn missing_files_are_skipped() {
        let ui = [Candidate::File { family: "Segoe UI".into(), path: "/nope/segoeui.ttf".into() }];
        let faces = SystemFaces::resolve(&NoLookup, &ui, &[], &|_| None);
        assert!(faces.ui.is_empty());
    }

    fn families(options: FontOptions) -> FontDefinitions {
        let mut defs = FontDefinitions::default();
        defs.font_data.insert("noto-arabic".into(), Arc::new(FontData::from_static(INTER_REGULAR)));
        apply(&mut defs, options, &nuc_faces(), &["noto-arabic"]);
        defs
    }

    #[test]
    fn inter_first_by_default_then_system_then_scripts() {
        let defs = families(FontOptions::default());
        let ui = &defs.families[&Face::Ui.family()];
        assert_eq!(ui[0], keys::INTER);
        let adwaita = ui.iter().position(|k| k.contains("AdwaitaSans")).unwrap();
        let arabic = ui.iter().position(|k| k == "noto-arabic").unwrap();
        assert!(adwaita < arabic);
        assert_eq!(defs.families[&Face::Numeric.family()][0], keys::NUMERIC);
        assert_eq!(defs.families[&Face::UiStrong.family()][0], keys::INTER_STRONG);
        assert_eq!(defs.families[&Face::Display.family()][0], keys::DISPLAY);
        assert!(defs.families[&Face::Mono.family()][0].contains("JetBrainsMono"));
        // The UI never starts with the mono face.
        assert!(!ui[0].contains("Mono"));
    }

    #[test]
    fn use_system_font_moves_only_ui_tokens() {
        let defs = families(FontOptions { use_system_font: true });
        assert!(defs.families[&Face::Ui.family()][0].contains("AdwaitaSans"));
        assert!(defs.families[&Face::Numeric.family()][0].contains("AdwaitaSans"));
        assert!(defs.families[&Face::UiStrong.family()][0].contains("NotoSans-Bold"));
        assert_eq!(defs.families[&Face::Display.family()][0], keys::DISPLAY);
        assert!(defs.families[&Face::Mono.family()][0].contains("JetBrainsMono"));
    }

    #[test]
    fn every_family_ends_with_the_egui_defaults() {
        let defs = families(FontOptions::default());
        let defaults = FontDefinitions::default();
        for face in Face::ALL {
            let list = &defs.families[&face.family()];
            for key in &defaults.families[&FontFamily::Proportional] {
                assert!(list.contains(key), "{face:?} lacks {key}");
            }
            let mut seen = std::collections::HashSet::new();
            assert!(list.iter().all(|k| seen.insert(k)), "{face:?} has duplicates");
            assert!(list.iter().all(|k| defs.font_data.contains_key(k)), "{face:?} names a missing face");
        }
    }

    #[test]
    fn parses_fc_match_and_gtk() {
        let m = parse_fc_match("Adwaita Sans,Adwaita Sans Text\n/usr/share/fonts/a.ttf\n2\n").unwrap();
        assert_eq!(m.families, ["Adwaita Sans", "Adwaita Sans Text"]);
        assert_eq!(m.index, 2);
        let cantarell_bold = parse_fc_match("Cantarell\n/usr/share/fonts/Cantarell-VF.otf\n262144\n").unwrap();
        assert_eq!(cantarell_bold.index, 0);
        assert_eq!(parse_fc_match(""), None);
        assert_eq!(parse_gtk_font_name("'Adwaita Sans 11'\n").as_deref(), Some("Adwaita Sans"));
        assert_eq!(parse_gtk_font_name("'Inter Bold 10.5'").as_deref(), Some("Inter"));
        assert_eq!(parse_gtk_font_name("'Cantarell'").as_deref(), Some("Cantarell"));
        assert_eq!(parse_gtk_font_name("''"), None);
    }

    #[test]
    fn migration_keeps_an_explicit_choice() {
        assert!(!FontOptions::migrate(None, None).use_system_font);
        assert!(FontOptions::migrate(Some(true), None).use_system_font);
        assert!(FontOptions::migrate(None, Some(true)).use_system_font);
    }

    #[test]
    fn bundled_faces_are_truetype() {
        for (name, bytes) in [
            ("inter", INTER_REGULAR),
            ("inter-strong", INTER_SEMIBOLD),
            ("numeric", INTER_TABULAR_REGULAR),
            ("numeric-strong", INTER_TABULAR_SEMIBOLD),
            ("display", INSTRUMENT_SANS_SEMIBOLD),
            ("display-regular", INSTRUMENT_SANS_REGULAR),
        ] {
            assert!(bytes.len() > 10_000 && &bytes[..4] == b"\0\x01\0\0", "{name} is not a TrueType file");
        }
    }
}
