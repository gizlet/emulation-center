//! Emulation Center for Gizlet OS: install, update and open RetroArch and
//! PPSSPP, and point them at the games already on the TF2 card.
//!
//! Opening an emulator writes `open-request` in the save folder and exits;
//! the package's entry script (`app/emulation-center`) then runs it with
//! `gizlet run` and brings the center back when it exits.
//!
//! Environment:
//! - `GIZLET_SAVE_DIR`, `GIZLET_LANG`, `GIZLET_SWAP_AB`: as `gizlet run` sets them.
//! - `GIZLET_STORE_API`: the Gizlet Store API (https://api.gizlet.io).
//! - `GIZLET_CLI`: the `gizlet` command (`gizlet` on the PATH).
//! - `EMULATION_CENTER_WINDOW=WxH`: the desktop window size (1280x960).
//! - `EMULATION_CENTER_CAPTURE=FILE.png`: render the screen once the Store has
//!   answered, save it and exit; `EMULATION_CENTER_SCENE=picker` captures the
//!   folder picker (`picker:DIR` inside DIR) and `EMULATION_CENTER_FOCUS=N`
//!   focuses row N.

mod canvas;
mod icons;
mod input;
mod text;

use canvas::{
    alpha, mix, rgb, Canvas, ACCENT, ACCENT_HI, BG_BOTTOM, BG_TOP, BLACK, DANGER, DIM, FAINT, H, LINE, ON_ACCENT,
    SUCCESS, SURFACE, SURFACE_HI, TEXT, W, WARM, WHITE,
};
use emulation_center::catalog::{Emulator, Icon, EMULATORS};
use emulation_center::games::{self, Contents, PlaceKind, Scanner};
use emulation_center::i18n::{self, fill, Words};
use emulation_center::jobs::{Center, Entry, StoreState, Task};
use emulation_center::settings::Settings;
use emulation_center::{save_dir, store, version, write_open_request};
use input::{Button, Controls};
use raylib_sys::{self as ffi, Color, ConfigFlags as F};
use std::ffi::CString;
use std::path::{Path, PathBuf};
use text::{Face, Text};

/// Without a window system: raylib's DRM platform on the whole panel.
const KMS: bool = cfg!(feature = "gizlet-os");

const MARGIN: f32 = 24.0;
/// The home screen's cards.
const CARD_TOP: f32 = 80.0;
const CARD_H: f32 = 98.0;
const CARD_GAP: f32 = 10.0;
const CARD_ROUND: f32 = 16.0;
/// The divider above the button hints.
const FOOTER: f32 = 434.0;
/// The folder picker's list.
const LIST_TOP: f32 = 82.0;
const ROW_H: f32 = 42.0;
const ROWS: usize = 8;

const FOLDER: Color = rgb(0xe8a23a);
const GAMES_TILE: Color = rgb(0xd98324);

fn ease_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

fn place_color(kind: &PlaceKind) -> Color {
    match kind {
        PlaceKind::Tf2 => rgb(0x1f9d6e),
        PlaceKind::Home => rgb(0x6a55d8),
        PlaceKind::Media => rgb(0x2f74c0),
        PlaceKind::Mnt => rgb(0x5a6475),
    }
}

fn place_icon(cv: &Canvas, kind: &PlaceKind, cx: f32, cy: f32, s: f32, c: Color, hole: Color) {
    match kind {
        PlaceKind::Tf2 => icons::card(cv, cx, cy, s, c, hole),
        PlaceKind::Home => icons::house(cv, cx, cy, s, c, hole),
        PlaceKind::Media => icons::drive(cv, cx, cy, s, c, hole),
        PlaceKind::Mnt => icons::mounts(cv, cx, cy, s, c, hole),
    }
}

fn emulator_color(e: &Emulator) -> Color {
    let [r, g, b] = e.color;
    Color { r, g, b, a: 255 }
}

/// An emulator's tile with its icon.
fn emulator_tile(cv: &Canvas, e: &Emulator, x: f32, y: f32, size: f32) {
    let color = emulator_color(e);
    icons::tile(cv, x, y, size, size * 0.27, color);
    let (cx, cy, s) = (x + size / 2.0, y + size / 2.0, size * 0.66);
    match e.icon {
        Icon::Gamepad => icons::gamepad(cv, cx, cy, s, WHITE, color),
        Icon::Handheld => icons::handheld(cv, cx, cy, s, WHITE, color),
    }
}

fn count(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        one.to_string()
    } else {
        fill(many, &[("n", &n.to_string())])
    }
}

enum Row {
    Place(PlaceKind, PathBuf),
    Up,
    Folder(String),
}

struct Picker {
    /// None: the list of places.
    dir: Option<PathBuf>,
    rows: Vec<Row>,
    /// What each row's folder holds, read in the background as the row comes into view.
    contents: Vec<Option<Contents>>,
    asked: Vec<bool>,
    focus: usize,
    /// The first row shown.
    first: usize,
    here: Option<Contents>,
}

impl Picker {
    fn path(&self, row: &Row) -> Option<PathBuf> {
        match (row, &self.dir) {
            (Row::Place(_, path), _) => Some(path.clone()),
            (Row::Folder(name), Some(d)) => Some(d.join(name)),
            _ => None,
        }
    }

    /// Scrolls just enough to show the focused row.
    fn reveal(&mut self) {
        if self.focus < self.first {
            self.first = self.focus;
        } else if self.focus >= self.first + ROWS {
            self.first = self.focus + 1 - ROWS;
        }
    }
}

enum Screen {
    Home,
    Picker(Picker),
    /// Opening the emulator at this index.
    Opening(usize),
}

struct App {
    words: &'static Words,
    center: Center,
    scanner: Scanner,
    save: PathBuf,
    settings: Settings,
    tf2: Option<PathBuf>,
    tf2_checked: f64,
    focus: usize,
    screen: Screen,
    notice: Option<(String, f64)>,
    swap_ab: bool,
    /// The time of this frame, and of the last button press.
    now: f64,
    moved: f64,
}

const GAMES_ROW: usize = EMULATORS.len();

impl App {
    fn games_folder(&self) -> PathBuf {
        games::games_folder(self.settings.folder.as_deref(), self.tf2.as_deref())
    }

    fn notice(&mut self, s: String, time: f64) {
        self.notice = Some((s, time + 3.0));
    }

    fn place_name(&self, kind: &PlaceKind) -> &'static str {
        match kind {
            PlaceKind::Tf2 => self.words.tf2,
            PlaceKind::Home => self.words.home,
            PlaceKind::Media => self.words.media,
            PlaceKind::Mnt => self.words.mounts,
        }
    }

    /// The picker in `dir`, focused on `from` (the folder just left) when it is
    /// listed. Counting what folders hold starts over.
    fn open_picker(&self, dir: Option<PathBuf>, from: Option<&Path>) -> Picker {
        self.scanner.restart();
        let mut rows = Vec::new();
        match &dir {
            None => rows.extend(games::places(self.tf2.as_deref()).into_iter().map(|(k, p)| Row::Place(k, p))),
            Some(d) => {
                rows.push(Row::Up);
                rows.extend(games::subfolders(d).into_iter().map(Row::Folder));
                self.scanner.request(d.clone());
            }
        }
        let (contents, asked) = (vec![None; rows.len()], vec![false; rows.len()]);
        let mut picker = Picker { dir, rows, contents, asked, focus: 0, first: 0, here: None };
        if let Some(from) = from {
            if let Some(i) = picker.rows.iter().position(|row| picker.path(row).as_deref() == Some(from)) {
                picker.focus = i;
                picker.reveal();
            }
        }
        picker
    }

    /// Whether `dir` is one of the places the picker starts from.
    fn is_place(&self, dir: &Path) -> bool {
        games::places(self.tf2.as_deref()).iter().any(|(_, p)| p == dir)
    }

    fn update(&mut self, c: &Controls, text: &mut Text, time: f64) -> bool {
        self.now = time;
        if c.any() {
            self.moved = time;
        }
        if time - self.tf2_checked > 2.0 {
            self.tf2_checked = time;
            self.tf2 = games::detect_tf2();
        }
        // The games folder's name may have characters the fonts have not loaded yet.
        for path in [self.settings.folder.as_deref(), self.tf2.as_deref()].into_iter().flatten() {
            text.need(&path.to_string_lossy());
        }
        if let Some((_, until)) = &self.notice {
            if time > *until {
                self.notice = None;
            }
        }
        if matches!(self.screen, Screen::Picker(_)) {
            let Screen::Picker(p) = std::mem::replace(&mut self.screen, Screen::Home) else { unreachable!() };
            self.screen = self.update_picker(p, c, time);
            if let Screen::Picker(p) = &mut self.screen {
                for row in &p.rows {
                    if let Row::Folder(name) = row {
                        text.need(name);
                    }
                }
                if let Some(d) = &p.dir {
                    text.need(&d.to_string_lossy());
                }
                for (path, found) in self.scanner.results() {
                    if p.dir.as_ref() == Some(&path) {
                        p.here = Some(found);
                    } else if let Some(i) = p.rows.iter().position(|row| p.path(row).as_ref() == Some(&path)) {
                        p.contents[i] = Some(found);
                    }
                }
                for i in p.first..(p.first + ROWS).min(p.rows.len()) {
                    if !p.asked[i] && matches!(p.rows[i], Row::Folder(_)) {
                        if let Some(path) = p.path(&p.rows[i]) {
                            self.scanner.request(path);
                            p.asked[i] = true;
                        }
                    }
                }
            }
            return true;
        }
        let state = self.center.snapshot();
        if matches!(self.screen, Screen::Opening(_)) {
            return false;
        }
        if c.pressed(Button::Up) {
            self.focus = self.focus.saturating_sub(1);
        }
        if c.pressed(Button::Down) {
            self.focus = (self.focus + 1).min(GAMES_ROW);
        }
        if c.pressed(Button::Y) {
            self.center.refresh();
        }
        if c.pressed(Button::B) {
            // An install in progress finishes first; a download is resumed next time.
            if state.entries.iter().any(|e| e.task == Task::Installing) {
                self.notice(self.words.wait_install.to_string(), time);
            } else {
                return false;
            }
        }
        if self.focus == GAMES_ROW {
            if c.pressed(Button::A) {
                let picker = self.open_picker(None, None);
                self.screen = Screen::Picker(picker);
            } else if c.pressed(Button::X) && self.settings.folder.is_some() && self.tf2.is_some() {
                self.settings.folder = None;
                let _ = self.settings.store(&self.save);
            }
            return true;
        }
        let i = self.focus;
        let entry = &state.entries[i];
        if c.pressed(Button::A) {
            if entry.installed.is_some() && !entry.working() {
                if state.entries.iter().any(Entry::working) {
                    self.notice(fill(self.words.waiting, &[]), time);
                } else {
                    let games = self.games_folder();
                    match write_open_request(&self.save, EMULATORS[i].id, &games) {
                        Ok(()) => self.screen = Screen::Opening(i),
                        Err(e) => self.notice(fill(self.words.failed, &[("e", &e.to_string())]), time),
                    }
                }
            } else if entry.offer().is_some() {
                self.center.install(i);
            } else if matches!(entry.store, StoreState::Offline(_) | StoreState::NotReady | StoreState::Missing) {
                self.center.refresh();
            }
        } else if c.pressed(Button::X) && entry.offer().is_some() {
            self.center.install(i);
        }
        true
    }

    /// The parent to go back to from `dir`, or the places from a place.
    fn up_from(&self, dir: &Path) -> Option<PathBuf> {
        if self.is_place(dir) {
            None
        } else {
            dir.parent().map(Path::to_path_buf)
        }
    }

    fn update_picker(&mut self, mut p: Picker, c: &Controls, time: f64) -> Screen {
        let n = p.rows.len();
        if n > 0 && c.pressed(Button::Up) {
            p.focus = (p.focus + n - 1) % n;
        }
        if n > 0 && c.pressed(Button::Down) {
            p.focus = (p.focus + 1) % n;
        }
        if n > 0 && c.pressed(Button::L) {
            p.focus = p.focus.saturating_sub(ROWS);
        }
        if n > 0 && c.pressed(Button::R) {
            p.focus = (p.focus + ROWS).min(n - 1);
        }
        p.reveal();
        if c.pressed(Button::B) {
            return match &p.dir {
                None => Screen::Home,
                Some(d) => Screen::Picker(self.open_picker(self.up_from(d), Some(d))),
            };
        }
        if c.pressed(Button::A) && n > 0 {
            return match (&p.rows[p.focus], &p.dir) {
                (Row::Up, Some(d)) => Screen::Picker(self.open_picker(self.up_from(d), Some(d))),
                (row, _) => Screen::Picker(self.open_picker(p.path(row), None)),
            };
        }
        if c.pressed(Button::X) {
            let chosen = match (&p.dir, p.rows.get(p.focus)) {
                (Some(d), _) => Some(d.clone()),
                (None, Some(Row::Place(_, path))) => Some(path.clone()),
                _ => None,
            };
            if let Some(chosen) = chosen {
                // Choosing the TF2 card itself follows the card wherever it is mounted.
                self.settings.folder = (self.tf2.as_deref() != Some(chosen.as_path())).then_some(chosen);
                if let Err(e) = self.settings.store(&self.save) {
                    self.notice(fill(self.words.failed, &[("e", &e.to_string())]), time);
                }
                return Screen::Home;
            }
        }
        Screen::Picker(p)
    }

    fn status(&self, e: &Entry, progress: f32) -> (String, Color) {
        let w = self.words;
        let pct = format!("{}", (progress * 100.0).round() as u32);
        match &e.task {
            Task::Waiting => return (w.waiting.into(), ACCENT),
            Task::Downloading => return (fill(w.downloading, &[("p", &pct)]), ACCENT),
            Task::Verifying => return (fill(w.verifying, &[("p", &pct)]), ACCENT),
            Task::Installing => return (w.installing.into(), ACCENT),
            Task::Failed(err) => return (fill(w.failed, &[("e", err)]), DANGER),
            Task::Installed(_) | Task::Idle => {}
        }
        match (&e.installed, &e.store) {
            (Some(v), StoreState::Ready(r)) if version::is_newer(&r.version, v) => {
                (format!("{} · {}", fill(w.installed, &[("v", v)]), fill(w.update_to, &[("v", &r.version)])), WARM)
            }
            (Some(v), StoreState::Ready(_)) => {
                (format!("{} · {}", fill(w.installed, &[("v", v)]), w.up_to_date), SUCCESS)
            }
            (Some(v), StoreState::Checking | StoreState::Unknown) => {
                (format!("{} · {}", fill(w.installed, &[("v", v)]), w.checking), SUCCESS)
            }
            (Some(v), _) => (fill(w.installed, &[("v", v)]), SUCCESS),
            (None, StoreState::Ready(r)) => (format!("{} · {}", w.not_installed, r.version), DIM),
            (None, StoreState::Checking | StoreState::Unknown) => (w.checking.into(), DIM),
            (None, StoreState::NotReady) => (w.not_ready.into(), DIM),
            (None, StoreState::Missing) => (w.missing.into(), DIM),
            (None, StoreState::Offline(err)) => (fill(w.offline, &[("e", err)]), DANGER),
        }
    }

    /// What a folder holds, in a few words.
    fn describe(&self, c: Contents) -> String {
        let w = self.words;
        let folders = count(c.folders, w.one_folder, w.folders);
        let files = count(c.files, w.one_file, w.files);
        match (c.folders, c.files) {
            (0, 0) => w.empty.to_string(),
            (0, _) => files,
            (_, 0) => folders,
            _ => format!("{folders} · {files}"),
        }
    }

    /// The controller button for a hint: with A and B swapped, the confirm
    /// and back buttons are the other physical buttons.
    fn glyph<'a>(&self, button: &'a str) -> &'a str {
        match (self.swap_ab, button) {
            (true, "A") => "B",
            (true, "B") => "A",
            _ => button,
        }
    }

    /// The focus colour, pulsing for a few seconds after a button press.
    fn focus_color(&self) -> Color {
        let since = (self.now - self.moved) as f32;
        let pulse = (3.0 - since).clamp(0.0, 1.0);
        let wave = (self.now * 3.2).sin() as f32 / 2.0;
        mix(ACCENT, ACCENT_HI, (0.5 + wave * pulse) * 0.6)
    }

    /// The focus ring around a rounded rectangle, with a short pop when focus moves.
    fn focus_ring(&self, cv: &Canvas, x: f32, y: f32, w: f32, h: f32, round: f32) {
        let pop = 1.0 - ease_out(((self.now - self.moved) / 0.22) as f32);
        let (gap, thick) = (3.0 + pop * 3.0, 3.0);
        let c = self.focus_color();
        let o = gap + thick;
        let (x, y, w, h, r) = (x - o, y - o, w + 2.0 * o, h + 2.0 * o, round + o);
        cv.outline(x - 3.0, y - 3.0, w + 6.0, h + 6.0, r + 3.0, 4.0, alpha(c, 0.18));
        cv.outline(x, y, w, h, r, thick, c);
    }

    fn card(&self, cv: &Canvas, y: f32, focused: bool) {
        let (x, w) = (MARGIN, W - 2.0 * MARGIN);
        if focused {
            cv.fill(x, y, w, CARD_H, CARD_ROUND, SURFACE_HI);
            self.focus_ring(cv, x, y, w, CARD_H, CARD_ROUND);
        } else {
            cv.fill(x, y, w, CARD_H, CARD_ROUND, alpha(SURFACE, 0.92));
            cv.outline(x, y, w, CARD_H, CARD_ROUND, 1.0, alpha(WHITE, 0.04));
        }
    }

    /// An action button inside a card, ending at `x_right`. Returns the next one's right edge.
    #[allow(clippy::too_many_arguments)]
    fn chip(&self, cv: &Canvas, t: &Text, x_right: f32, cy: f32, button: &str, label: &str, primary: bool) -> f32 {
        let h = 30.0;
        let w = 34.0 + t.measure(Face::Bold, label, 14.0) + 14.0;
        let x = x_right - w;
        let (bg, fg, letter) = if primary { (ACCENT, ON_ACCENT, ACCENT) } else { (alpha(TEXT, 0.1), TEXT, SURFACE_HI) };
        cv.fill(x, cy - h / 2.0, w, h, h / 2.0, bg);
        icons::button(cv, t, self.glyph(button), x + 16.0, cy, 10.0, fg, letter);
        cv.text_mid(t, Face::Bold, label, x + 34.0, cy, 14.0, fg);
        x - 8.0
    }

    /// A status line with its dot, or a spinner while `busy`.
    #[allow(clippy::too_many_arguments)]
    fn status_line(&self, cv: &Canvas, t: &Text, s: &str, x: f32, cy: f32, width: f32, color: Color, busy: bool) {
        if busy {
            icons::spinner(cv, x + 4.0, cy, 5.5, 2.0, self.now, color);
        } else {
            cv.circle(x + 4.0, cy, 3.5, color);
        }
        let s = t.fit(Face::Body, s, 13.0, width - 16.0);
        cv.text_mid(t, Face::Body, &s, x + 16.0, cy, 13.0, color);
    }

    /// The divider and the button hints along the bottom, listed left to right.
    /// Returns the left edge of the first hint.
    fn footer(&self, cv: &Canvas, t: &Text, items: &[(&str, &str)]) -> f32 {
        cv.fill(MARGIN, FOOTER, W - 2.0 * MARGIN, 1.0, 0.0, mix(LINE, BG_BOTTOM, 0.2));
        let cy = (FOOTER + H) / 2.0;
        let r = 9.5;
        let mut x = W - MARGIN - 4.0;
        for (button, label) in items.iter().rev() {
            x = cv.text_right(t, Face::Body, label, x, cy, 14.0, TEXT) - 7.0;
            icons::button(cv, t, self.glyph(button), x - r, cy, r, TEXT, BG_BOTTOM);
            x -= 2.0 * r + 20.0;
        }
        x + 20.0
    }

    fn draw(&self, cv: Canvas, t: &Text) {
        unsafe { ffi::ClearBackground(BG_BOTTOM) };
        cv.gradient_v(0.0, 0.0, W, H, BG_TOP, BG_BOTTOM);
        // A faint glow behind the content keeps the dark background from looking flat.
        cv.circle_gradient(W * 0.5, -H * 0.15, H * 0.95, alpha(ACCENT, 0.08), alpha(ACCENT, 0.0));
        match &self.screen {
            Screen::Home | Screen::Opening(_) => self.draw_home(&cv, t),
            Screen::Picker(p) => self.draw_picker(&cv, t, p),
        }
        if let Screen::Opening(i) = &self.screen {
            self.draw_opening(&cv, t, &EMULATORS[*i]);
        }
        if let Some((s, _)) = &self.notice {
            let s = t.fit(Face::Body, s, 14.0, W - 100.0);
            let w = t.measure(Face::Body, &s, 14.0) + 36.0;
            let (x, y, h) = ((W - w) / 2.0, FOOTER - 46.0, 34.0);
            cv.fill(x - 1.0, y + 2.0, w + 2.0, h + 2.0, h / 2.0 + 1.0, alpha(BLACK, 0.35));
            cv.fill(x, y, w, h, h / 2.0, SURFACE_HI);
            cv.outline(x, y, w, h, h / 2.0, 1.0, alpha(WHITE, 0.08));
            cv.text_mid(t, Face::Body, &s, x + 18.0, y + h / 2.0, 14.0, TEXT);
        }
    }

    fn draw_opening(&self, cv: &Canvas, t: &Text, e: &Emulator) {
        cv.fill(0.0, 0.0, W, H, 0.0, alpha(BG_BOTTOM, 0.82));
        let (w, h) = (300.0, 168.0);
        let (x, y) = ((W - w) / 2.0, (H - h) / 2.0);
        cv.fill(x, y, w, h, 20.0, SURFACE);
        cv.outline(x, y, w, h, 20.0, 1.0, alpha(WHITE, 0.06));
        emulator_tile(cv, e, W / 2.0 - 30.0, y + 24.0, 60.0);
        let s = fill(self.words.opening, &[("name", e.name)]);
        cv.text_center(t, Face::Bold, &s, W / 2.0, y + 112.0, 18.0, TEXT);
        icons::spinner(cv, W / 2.0, y + 142.0, 8.0, 2.5, self.now, ACCENT);
    }

    fn draw_home(&self, cv: &Canvas, t: &Text) {
        let w = self.words;
        let state = self.center.snapshot();
        cv.text(t, Face::Title, w.title, MARGIN + 4.0, 16.0, 26.0, TEXT);
        let subtitle = match &state.error {
            Some(e) => fill(w.cannot_read_apps, &[("e", e)]),
            None => w.subtitle.to_string(),
        };
        let subtitle = t.fit(Face::Body, &subtitle, 13.0, W - 2.0 * MARGIN - 8.0);
        let color = if state.error.is_some() { DANGER } else { DIM };
        cv.text_mid(t, Face::Body, &subtitle, MARGIN + 4.0, 60.0, 13.0, color);

        let (left, right) = (MARGIN + 94.0, W - MARGIN - 18.0);
        let progress = self.center.progress.fraction();
        for (i, (e, entry)) in EMULATORS.iter().zip(&state.entries).enumerate() {
            let y = CARD_TOP + i as f32 * (CARD_H + CARD_GAP);
            let focused = self.focus == i;
            self.card(cv, y, focused);
            emulator_tile(cv, e, MARGIN + 18.0, y + 19.0, 60.0);

            let mut name_right = right;
            if focused && !entry.working() {
                let offer = entry.offer();
                let primary = if entry.installed.is_some() {
                    Some(w.open)
                } else if offer.is_some() {
                    Some(w.install)
                } else if matches!(entry.store, StoreState::Offline(_) | StoreState::NotReady | StoreState::Missing) {
                    Some(w.refresh)
                } else {
                    None
                };
                let mut x = right;
                if let Some(label) = primary {
                    x = self.chip(cv, t, x, y + 30.0, "A", label, true);
                }
                if entry.installed.is_some() && offer.is_some() {
                    x = self.chip(cv, t, x, y + 30.0, "X", w.update, false);
                }
                name_right = x;
            }
            let name = t.fit(Face::Bold, e.name, 19.0, name_right - 8.0 - left);
            cv.text_mid(t, Face::Bold, &name, left, y + 28.0, 19.0, TEXT);
            let systems = if std::ptr::eq(w, &i18n::ZH) { e.systems_zh } else { e.systems };
            let systems = t.fit(Face::Body, systems, 13.0, right - left);
            cv.text_mid(t, Face::Body, &systems, left, y + 54.0, 13.0, DIM);
            let (status, color) = self.status(entry, progress);
            self.status_line(cv, t, &status, left, y + 76.0, right - left, color, entry.working());
            if matches!(entry.task, Task::Downloading | Task::Verifying) {
                cv.fill(left, y + 88.0, right - left, 4.0, 2.0, alpha(LINE, 0.9));
                cv.fill(left, y + 88.0, ((right - left) * progress).max(4.0), 4.0, 2.0, ACCENT);
            }
        }

        let y = CARD_TOP + GAMES_ROW as f32 * (CARD_H + CARD_GAP);
        let focused = self.focus == GAMES_ROW;
        self.card(cv, y, focused);
        icons::tile(cv, MARGIN + 18.0, y + 19.0, 60.0, 16.0, GAMES_TILE);
        icons::folder(cv, MARGIN + 48.0, y + 50.0, 40.0, WHITE, mix(WHITE, GAMES_TILE, 0.4));
        let mut name_right = right;
        if focused {
            let mut x = self.chip(cv, t, right, y + 30.0, "A", w.change_folder, true);
            if self.settings.folder.is_some() && self.tf2.is_some() {
                x = self.chip(cv, t, x, y + 30.0, "X", w.use_tf2, false);
            }
            name_right = x;
        }
        let name = t.fit(Face::Bold, w.games, 19.0, name_right - 8.0 - left);
        cv.text_mid(t, Face::Bold, &name, left, y + 28.0, 19.0, TEXT);
        let note = t.fit(Face::Body, w.open_note, 13.0, right - left);
        cv.text_mid(t, Face::Body, &note, left, y + 54.0, 13.0, DIM);
        let (line, color) = match (&self.settings.folder, &self.tf2) {
            (Some(chosen), _) if chosen.is_dir() => (chosen.display().to_string(), TEXT),
            (_, Some(tf2)) => (fill(w.tf2_found, &[("p", &tf2.display().to_string())]), SUCCESS),
            _ => (w.no_tf2.to_string(), WARM),
        };
        let line = t.fit_start(Face::Body, &line, 13.0, right - left - 16.0);
        self.status_line(cv, t, &line, left, y + 76.0, right - left, color, false);

        let entry = state.entries.get(self.focus);
        let primary = match entry {
            Some(e) if e.installed.is_some() => w.open,
            Some(_) => w.install,
            None => w.choose,
        };
        let hints_left = self.footer(cv, t, &[("B", w.quit), ("Y", w.refresh), ("A", primary)]);
        if state.busy
            && entry.is_some_and(|e| !e.working())
            && matches!(entry.map(|e| &e.store), Some(StoreState::Checking))
        {
            let cy = (FOOTER + H) / 2.0;
            icons::spinner(cv, MARGIN + 10.0, cy, 6.0, 2.0, self.now, FAINT);
            let s = t.fit(Face::Body, w.checking, 13.0, hints_left - MARGIN - 40.0);
            cv.text_mid(t, Face::Body, &s, MARGIN + 24.0, cy, 13.0, FAINT);
        }
    }

    /// Where the picker is, as the place's name and the folders below it.
    fn crumbs(&self, dir: &Path) -> (Option<PlaceKind>, Vec<String>) {
        let place = games::places(self.tf2.as_deref())
            .into_iter()
            .filter(|(_, path)| dir.starts_with(path))
            .max_by_key(|(_, path)| path.components().count());
        match place {
            Some((kind, path)) => {
                let mut parts = vec![self.place_name(&kind).to_string()];
                if let Ok(rest) = dir.strip_prefix(&path) {
                    parts.extend(rest.iter().map(|c| c.to_string_lossy().into_owned()));
                }
                (Some(kind), parts)
            }
            None => (None, vec![dir.display().to_string()]),
        }
    }

    fn draw_crumbs(&self, cv: &Canvas, t: &Text, p: &Picker, cy: f32) {
        let mut x = MARGIN + 4.0;
        let (kind, mut parts) = match &p.dir {
            None => (None, vec![self.words.places.to_string()]),
            Some(d) => self.crumbs(d),
        };
        if let Some(kind) = &kind {
            let color = place_color(kind);
            cv.fill(x, cy - 10.0, 20.0, 20.0, 6.0, color);
            place_icon(cv, kind, x + 10.0, cy, 14.0, WHITE, color);
            x += 28.0;
        }
        let sep = 16.0;
        let room = W - MARGIN - 4.0 - x;
        let width = |parts: &[String]| -> f32 {
            parts.iter().map(|s| t.measure(Face::Bold, s, 14.0)).sum::<f32>() + sep * (parts.len() as f32 - 1.0)
        };
        // Too long: keep the place and the last folders, with an ellipsis between.
        while parts.len() > 2 && width(&parts) > room {
            if parts[1] == "…" {
                if parts.len() == 3 {
                    break;
                }
                parts.remove(2);
            } else {
                parts[1] = "…".into();
            }
        }
        let last = parts.len() - 1;
        for (i, part) in parts.iter().enumerate() {
            let s = t.fit_start(Face::Bold, part, 14.0, (W - MARGIN - 4.0 - x).max(20.0));
            let color = if i == last { TEXT } else { DIM };
            x += cv.text_mid(t, Face::Bold, &s, x, cy, 14.0, color);
            if i < last {
                icons::chevron(cv, x + sep / 2.0, cy, 11.0, FAINT);
                x += sep;
            }
        }
    }

    fn draw_picker(&self, cv: &Canvas, t: &Text, p: &Picker) {
        let w = self.words;
        cv.text(t, Face::Title, w.picker_title, MARGIN + 4.0, 16.0, 24.0, TEXT);
        self.draw_crumbs(cv, t, p, 58.0);

        let current = self.games_folder();
        let end = (p.first + ROWS).min(p.rows.len());
        let order = (p.first..end).filter(|&n| n != p.focus).chain((p.first..end).filter(|&n| n == p.focus));
        for n in order {
            self.draw_row(cv, t, p, n, LIST_TOP + (n - p.first) as f32 * ROW_H, n + 1 == end, &current);
        }
        if p.dir.is_some() && p.rows.len() == 1 {
            let cy = LIST_TOP + ROW_H + 110.0;
            icons::folder(cv, W / 2.0, cy - 30.0, 64.0, alpha(FAINT, 0.5), alpha(FAINT, 0.3));
            cv.text_center(t, Face::Body, w.no_folders, W / 2.0, cy + 18.0, 15.0, DIM);
        }
        if p.rows.len() > ROWS {
            let (x, top, h) = (W - 12.0, LIST_TOP + 4.0, ROWS as f32 * ROW_H - 8.0);
            cv.fill(x, top, 3.0, h, 1.5, alpha(LINE, 0.7));
            let thumb = (h * ROWS as f32 / p.rows.len() as f32).max(18.0);
            let at = (h - thumb) * p.first as f32 / (p.rows.len() - ROWS) as f32;
            cv.fill(x, top + at, 3.0, thumb, 1.5, alpha(DIM, 0.8));
        }

        let open = if p.dir.is_none() { w.choose } else { w.open };
        let hints_left = self.footer(cv, t, &[("B", w.back), ("X", w.use_this), ("A", open)]);
        if let Some(here) = p.here {
            let s = count(here.files, w.one_file_here, w.files_here);
            let s = t.fit(Face::Body, &s, 13.0, hints_left - MARGIN - 24.0);
            cv.text_mid(t, Face::Body, &s, MARGIN + 4.0, (FOOTER + H) / 2.0, 13.0, FAINT);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_row(&self, cv: &Canvas, t: &Text, p: &Picker, n: usize, y: f32, last: bool, current: &Path) {
        let w = self.words;
        let row = &p.rows[n];
        let focused = n == p.focus;
        let (x, width) = (MARGIN, W - 2.0 * MARGIN);
        if focused {
            cv.fill(x, y + 3.0, width, ROW_H - 6.0, 12.0, SURFACE_HI);
            self.focus_ring(cv, x, y + 3.0, width, ROW_H - 6.0, 12.0);
        } else if !last {
            cv.fill(x + 52.0, y + ROW_H - 0.5, width - 66.0, 1.0, 0.0, alpha(LINE, 0.55));
        }
        let cy = y + ROW_H / 2.0;
        let icon_x = x + 26.0;
        match row {
            Row::Place(kind, _) => {
                let color = place_color(kind);
                icons::tile(cv, icon_x - 14.0, cy - 14.0, 28.0, 8.0, color);
                place_icon(cv, kind, icon_x, cy, 19.0, WHITE, color);
            }
            Row::Up => {
                cv.circle(icon_x, cy, 13.0, alpha(TEXT, 0.08));
                icons::arrow_up(cv, icon_x, cy, 15.0, DIM);
            }
            Row::Folder(_) => icons::folder(cv, icon_x, cy, 28.0, FOLDER, icons::folder_back(FOLDER)),
        }

        let label_x = x + 52.0;
        let mut right = x + width - 14.0;
        if focused {
            icons::chevron(cv, right - 4.0, cy, 15.0, DIM);
            right -= 22.0;
        }
        if p.path(row).as_deref() == Some(current) {
            let bw = t.measure(Face::Bold, w.current, 12.0) + 30.0;
            cv.fill(right - bw, cy - 11.0, bw, 22.0, 11.0, alpha(SUCCESS, 0.14));
            icons::check(cv, right - bw + 12.0, cy, 11.0, SUCCESS);
            cv.text_mid(t, Face::Bold, w.current, right - bw + 21.0, cy, 12.0, SUCCESS);
            right -= bw + 12.0;
        }
        let (label, face, color) = match row {
            Row::Place(kind, _) => (self.place_name(kind).to_string(), Face::Bold, TEXT),
            Row::Up => (w.up.to_string(), Face::Body, DIM),
            Row::Folder(name) => (name.clone(), Face::Body, TEXT),
        };
        let label_w = t.measure(face, &label, 15.0).min(right - label_x - 60.0);
        let detail = match (row, &p.dir) {
            (Row::Place(_, path), _) => path.display().to_string(),
            (Row::Up, Some(d)) => match self.up_from(d) {
                Some(parent) => self.crumbs(&parent).1.last().cloned().unwrap_or_default(),
                None => w.places.to_string(),
            },
            (Row::Folder(_), _) => p.contents[n].map(|c| self.describe(c)).unwrap_or_default(),
            _ => String::new(),
        };
        if !detail.is_empty() {
            let detail = t.fit_start(Face::Body, &detail, 13.0, (right - label_x - label_w - 24.0).max(40.0));
            right = cv.text_right(t, Face::Body, &detail, right, cy, 13.0, if focused { DIM } else { FAINT }) - 16.0;
        }
        let label = t.fit(face, &label, 15.0, right - label_x);
        cv.text_mid(t, face, &label, label_x, cy, 15.0, color);
    }
}

fn env_size(name: &str) -> Option<(i32, i32)> {
    let v = std::env::var(name).ok()?;
    let (w, h) = v.split_once('x')?;
    Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
}

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--version") {
        println!("emulation-center {}", option_env!("GIZLET_APP_VERSION").unwrap_or(env!("CARGO_PKG_VERSION")));
        return;
    }
    let capture = std::env::var("EMULATION_CENTER_CAPTURE").ok();
    let save = save_dir();
    let lang = std::env::var("GIZLET_LANG").unwrap_or_else(|_| "en".into());
    let (w, h) = if KMS { (0, 0) } else { env_size("EMULATION_CENTER_WINDOW").unwrap_or((1280, 960)) };
    let mut flags = F::FLAG_VSYNC_HINT as u32;
    if !KMS {
        flags |= F::FLAG_MSAA_4X_HINT as u32 | F::FLAG_WINDOW_RESIZABLE as u32;
    }
    if capture.is_some() {
        flags |= F::FLAG_WINDOW_HIDDEN as u32;
    }
    unsafe {
        ffi::SetTraceLogLevel(if KMS { ffi::TraceLogLevel::LOG_WARNING } else { ffi::TraceLogLevel::LOG_ERROR } as i32);
        ffi::SetConfigFlags(flags);
        ffi::InitWindow(w, h, c"Emulation Center".as_ptr());
        if !ffi::IsWindowReady() {
            eprintln!("emulation-center: the display could not be opened");
            std::process::exit(1);
        }
        ffi::SetExitKey(0);
        ffi::SetTargetFPS(60);
    }
    let fonts =
        std::env::var_os("EMULATION_CENTER_ASSETS").map(PathBuf::from).unwrap_or_else(|| "assets".into()).join("fonts");
    let mut text = Text::load(&fonts, Canvas::now().scale(), &i18n::all_text());
    let mut app = App {
        words: i18n::words(&lang),
        center: Center::start(store::api(), save.join("downloads")),
        scanner: Scanner::start(),
        settings: Settings::load(&save),
        save,
        tf2: games::detect_tf2(),
        tf2_checked: 0.0,
        focus: std::env::var("EMULATION_CENTER_FOCUS").ok().and_then(|f| f.parse().ok()).unwrap_or(0).min(GAMES_ROW),
        screen: Screen::Home,
        notice: None,
        swap_ab: std::env::var("GIZLET_SWAP_AB").is_ok_and(|v| v == "1"),
        now: 0.0,
        moved: -10.0,
    };
    match std::env::var("EMULATION_CENTER_SCENE").as_deref() {
        Ok("picker") => app.screen = Screen::Picker(app.open_picker(None, None)),
        Ok(scene) if scene.starts_with("picker:") => {
            let dir = PathBuf::from(&scene["picker:".len()..]);
            let mut picker = app.open_picker(Some(dir), None);
            if let Some(focus) = std::env::var("EMULATION_CENTER_FOCUS").ok().and_then(|f| f.parse().ok()) {
                picker.focus = usize::min(focus, picker.rows.len().saturating_sub(1));
                picker.reveal();
            }
            app.screen = Screen::Picker(picker);
        }
        _ => {}
    }
    let mut controls = Controls::new();
    let mut frames = 0u32;
    let started = unsafe { ffi::GetTime() };
    loop {
        if unsafe { ffi::WindowShouldClose() } {
            break;
        }
        let (dt, time) = unsafe { (ffi::GetFrameTime(), ffi::GetTime()) };
        controls.poll(dt, time);
        let keep = app.update(&controls, &mut text, time);
        unsafe { ffi::BeginDrawing() };
        app.draw(Canvas::now(), &text);
        unsafe { ffi::EndDrawing() };
        frames += 1;
        if let Some(path) = &capture {
            let settled = !app.center.snapshot().busy && frames > 10;
            if settled || time - started > 20.0 {
                unsafe {
                    let image = ffi::LoadImageFromScreen();
                    let c = CString::new(path.as_str()).unwrap();
                    ffi::ExportImage(image, c.as_ptr());
                    ffi::UnloadImage(image);
                }
                break;
            }
        }
        if !keep {
            break;
        }
    }
    // Quitting stops a download in progress (the next install resumes it) and
    // lets an install that has started finish.
    app.center.shutdown();
    unsafe { ffi::CloseWindow() };
}
