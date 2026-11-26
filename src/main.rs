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

mod input;
mod text;

use emulation_center::catalog::EMULATORS;
use emulation_center::games::{self, PlaceKind};
use emulation_center::i18n::{self, fill, Words};
use emulation_center::jobs::{Center, Entry, StoreState, Task};
use emulation_center::settings::Settings;
use emulation_center::{save_dir, store, version, write_open_request};
use input::{Button, Controls};
use raylib_sys::{self as ffi, Color, ConfigFlags as F, Rectangle, Vector2};
use std::ffi::CString;
use std::path::{Path, PathBuf};
use text::{Face, Text};

/// Without a window system: raylib's DRM platform on the whole panel.
const KMS: bool = cfg!(feature = "gizlet-os");
const W: f32 = 640.0;
const H: f32 = 480.0;

const fn rgb(hex: u32) -> Color {
    Color { r: (hex >> 16) as u8, g: (hex >> 8) as u8, b: hex as u8, a: 255 }
}

fn alpha(c: Color, a: f32) -> Color {
    Color { a: (a * 255.0) as u8, ..c }
}

// The launcher's dark theme.
const BG_TOP: Color = rgb(0x1d2027);
const BG_BOTTOM: Color = rgb(0x121418);
const SURFACE: Color = rgb(0x262a32);
const SURFACE_HI: Color = rgb(0x323743);
const LINE: Color = rgb(0x3a3f4b);
const TEXT: Color = rgb(0xf3f5f8);
const DIM: Color = rgb(0xa4abb8);
const FAINT: Color = rgb(0x6c7380);
const ACCENT: Color = rgb(0x34b6ff);
const ON_ACCENT: Color = rgb(0x06121c);
const DANGER: Color = rgb(0xff5d63);
const SUCCESS: Color = rgb(0x3ddc97);
const WARM: Color = rgb(0xff9a3d);

/// Maps the 640×480 logical screen onto the window, letterboxed.
#[derive(Clone, Copy)]
struct Canvas {
    scale: f32,
    ox: f32,
    oy: f32,
}

impl Canvas {
    fn now() -> Canvas {
        let (sw, sh) = unsafe { (ffi::GetScreenWidth() as f32, ffi::GetScreenHeight() as f32) };
        let scale = (sw / W).min(sh / H);
        Canvas { scale, ox: (sw - W * scale) / 2.0, oy: (sh - H * scale) / 2.0 }
    }
    fn v(&self, x: f32, y: f32) -> Vector2 {
        Vector2 { x: self.ox + x * self.scale, y: self.oy + y * self.scale }
    }
    fn rect(&self, x: f32, y: f32, w: f32, h: f32) -> Rectangle {
        Rectangle {
            x: self.ox + x * self.scale,
            y: self.oy + y * self.scale,
            width: w * self.scale,
            height: h * self.scale,
        }
    }
    fn fill(&self, x: f32, y: f32, w: f32, h: f32, round: f32, color: Color) {
        let r = self.rect(x, y, w, h);
        let roundness = if round <= 0.0 { 0.0 } else { (2.0 * round / w.min(h)).min(1.0) };
        unsafe { ffi::DrawRectangleRounded(r, roundness, 8, color) };
    }
    fn outline(&self, x: f32, y: f32, w: f32, h: f32, round: f32, thick: f32, color: Color) {
        let r = self.rect(x, y, w, h);
        let roundness = (2.0 * round / w.min(h)).min(1.0);
        unsafe { ffi::DrawRectangleRoundedLinesEx(r, roundness, 8, thick * self.scale, color) };
    }
    fn text(&self, t: &Text, face: Face, s: &str, x: f32, y: f32, size: f32, color: Color) {
        t.draw(face, s, self.v(x, y), size * self.scale, color);
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
    focus: usize,
    files: usize,
}

enum Screen {
    Home,
    Picker(Picker),
    Opening(String),
}

struct App {
    words: &'static Words,
    center: Center,
    save: PathBuf,
    settings: Settings,
    tf2: Option<PathBuf>,
    tf2_checked: f64,
    focus: usize,
    screen: Screen,
    notice: Option<(String, f64)>,
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

    fn open_picker(&self, dir: Option<PathBuf>) -> Picker {
        let mut rows = Vec::new();
        let mut files = 0;
        match &dir {
            None => rows.extend(games::places(self.tf2.as_deref()).into_iter().map(|(k, p)| Row::Place(k, p))),
            Some(d) => {
                rows.push(Row::Up);
                rows.extend(games::subfolders(d).into_iter().map(Row::Folder));
                files = games::file_count(d);
            }
        }
        Picker { dir, rows, focus: 0, files }
    }

    /// Whether `dir` is one of the places the picker starts from.
    fn is_place(&self, dir: &Path) -> bool {
        games::places(self.tf2.as_deref()).iter().any(|(_, p)| p == dir)
    }

    fn update(&mut self, c: &Controls, text: &mut Text, time: f64) -> bool {
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
            if let Screen::Picker(p) = &self.screen {
                for row in &p.rows {
                    if let Row::Folder(name) = row {
                        text.need(name);
                    }
                }
                if let Some(d) = &p.dir {
                    text.need(&d.to_string_lossy());
                }
            }
            return true;
        }
        let state = self.center.snapshot();
        if matches!(self.screen, Screen::Opening(_)) {
            return false;
        }
        {
            {
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
                        let picker = self.open_picker(None);
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
                                Ok(()) => self.screen = Screen::Opening(EMULATORS[i].name.to_string()),
                                Err(e) => self.notice(fill(self.words.failed, &[("e", &e.to_string())]), time),
                            }
                        }
                    } else if entry.offer().is_some() {
                        self.center.install(i);
                    } else if matches!(entry.store, StoreState::Offline(_) | StoreState::NotReady | StoreState::Missing)
                    {
                        self.center.refresh();
                    }
                } else if c.pressed(Button::X) && entry.offer().is_some() {
                    self.center.install(i);
                }
            }
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
            p.focus = p.focus.saturating_sub(8);
        }
        if n > 0 && c.pressed(Button::R) {
            p.focus = (p.focus + 8).min(n - 1);
        }
        if c.pressed(Button::B) {
            return match &p.dir {
                None => Screen::Home,
                Some(d) => Screen::Picker(self.open_picker(self.up_from(d))),
            };
        }
        if c.pressed(Button::A) && n > 0 {
            let next = match (&p.rows[p.focus], &p.dir) {
                (Row::Place(_, path), _) => Some(path.clone()),
                (Row::Up, Some(d)) => self.up_from(d),
                (Row::Folder(name), Some(d)) => Some(d.join(name)),
                _ => None,
            };
            return Screen::Picker(self.open_picker(next));
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

    fn draw(&self, cv: Canvas, t: &Text) {
        unsafe {
            ffi::ClearBackground(BG_BOTTOM);
            let full = cv.rect(0.0, 0.0, W, H);
            ffi::DrawRectangleGradientV(
                full.x as i32,
                full.y as i32,
                full.width as i32,
                full.height as i32,
                BG_TOP,
                BG_BOTTOM,
            );
        }
        match &self.screen {
            Screen::Home | Screen::Opening(_) => self.draw_home(cv, t),
            Screen::Picker(p) => self.draw_picker(cv, t, p),
        }
        if let Screen::Opening(name) = &self.screen {
            cv.fill(0.0, 0.0, W, H, 0.0, alpha(BG_BOTTOM, 0.85));
            let s = fill(self.words.opening, &[("name", name)]);
            let w = t.measure(Face::Bold, &s, 22.0);
            cv.text(t, Face::Bold, &s, (W - w) / 2.0, H / 2.0 - 14.0, 22.0, TEXT);
        }
        if let Some((s, _)) = &self.notice {
            let w = t.measure(Face::Body, s, 15.0).min(W - 60.0) + 32.0;
            cv.fill((W - w) / 2.0, 392.0, w, 34.0, 17.0, SURFACE_HI);
            let s = t.fit(Face::Body, s, 15.0, W - 60.0);
            cv.text(t, Face::Body, &s, (W - w) / 2.0 + 16.0, 401.0, 15.0, TEXT);
        }
    }

    fn chip(&self, cv: Canvas, t: &Text, x_right: f32, y: f32, button: &str, label: &str, primary: bool) -> f32 {
        let lw = t.measure(Face::Bold, label, 14.0);
        let w = lw + 40.0;
        let x = x_right - w;
        cv.fill(x, y, w, 26.0, 13.0, if primary { ACCENT } else { SURFACE_HI });
        let fg = if primary { ON_ACCENT } else { TEXT };
        cv.fill(x + 5.0, y + 4.0, 18.0, 18.0, 9.0, alpha(fg, 0.18));
        let bw = t.measure(Face::Bold, button, 12.0);
        cv.text(t, Face::Bold, button, x + 14.0 - bw / 2.0, y + 5.0, 12.0, fg);
        cv.text(t, Face::Bold, label, x + 28.0, y + 5.0, 14.0, fg);
        x - 8.0
    }

    fn hints(&self, cv: Canvas, t: &Text, items: &[(&str, &str)]) {
        let mut x = 28.0;
        for (button, label) in items {
            cv.fill(x, 449.0, 18.0, 18.0, 9.0, SURFACE_HI);
            let bw = t.measure(Face::Bold, button, 11.0);
            cv.text(t, Face::Bold, button, x + 9.0 - bw / 2.0, 452.0, 11.0, DIM);
            cv.text(t, Face::Body, label, x + 24.0, 450.0, 13.0, DIM);
            x += 24.0 + t.measure(Face::Body, label, 13.0) + 18.0;
        }
    }

    fn draw_home(&self, cv: Canvas, t: &Text) {
        let w = self.words;
        let state = self.center.snapshot();
        cv.text(t, Face::Title, w.title, 28.0, 18.0, 28.0, TEXT);
        let subtitle = match &state.error {
            Some(e) => fill(w.cannot_read_apps, &[("e", e)]),
            None => w.subtitle.to_string(),
        };
        let subtitle = t.fit(Face::Body, &subtitle, 14.0, W - 56.0);
        cv.text(t, Face::Body, &subtitle, 28.0, 56.0, 14.0, if state.error.is_some() { DANGER } else { DIM });

        let progress = self.center.progress.fraction();
        for (i, (e, entry)) in EMULATORS.iter().zip(&state.entries).enumerate() {
            let y = 90.0 + i as f32 * 114.0;
            let focused = self.focus == i;
            cv.fill(20.0, y, W - 40.0, 104.0, 14.0, if focused { SURFACE_HI } else { SURFACE });
            if focused {
                cv.outline(20.0, y, W - 40.0, 104.0, 14.0, 2.0, ACCENT);
            }
            let [r, g, b] = e.color;
            cv.fill(36.0, y + 18.0, 52.0, 52.0, 12.0, Color { r, g, b, a: 255 });
            let initial = &e.name[..1];
            let iw = t.measure(Face::Title, initial, 28.0);
            cv.text(t, Face::Title, initial, 62.0 - iw / 2.0, y + 26.0, 28.0, TEXT);
            cv.text(t, Face::Bold, e.name, 104.0, y + 14.0, 21.0, TEXT);
            let systems = if std::ptr::eq(w, &i18n::ZH) { e.systems_zh } else { e.systems };
            let systems = t.fit(Face::Body, systems, 13.0, W - 150.0);
            cv.text(t, Face::Body, &systems, 104.0, y + 42.0, 13.0, DIM);
            let (status, color) = self.status(entry, progress);
            let status = t.fit(Face::Body, &status, 14.0, if focused { 250.0 } else { W - 150.0 });
            cv.text(t, Face::Body, &status, 104.0, y + 70.0, 14.0, color);
            if matches!(entry.task, Task::Downloading | Task::Verifying) {
                cv.fill(104.0, y + 92.0, W - 148.0, 4.0, 2.0, LINE);
                cv.fill(104.0, y + 92.0, (W - 148.0) * progress, 4.0, 2.0, ACCENT);
            }
            if focused && !entry.working() {
                let mut x = W - 36.0;
                let offer = entry.offer();
                let update = entry.installed.is_some() && offer.is_some();
                if update {
                    x = self.chip(cv, t, x, y + 66.0, "X", w.update, false);
                }
                let primary = if entry.installed.is_some() {
                    Some(w.open)
                } else if offer.is_some() {
                    Some(w.install)
                } else if matches!(entry.store, StoreState::Offline(_) | StoreState::NotReady | StoreState::Missing) {
                    Some(w.refresh)
                } else {
                    None
                };
                if let Some(label) = primary {
                    self.chip(cv, t, x, y + 66.0, "A", label, true);
                }
            }
        }

        let y = 318.0;
        let focused = self.focus == GAMES_ROW;
        cv.fill(20.0, y, W - 40.0, 92.0, 14.0, if focused { SURFACE_HI } else { SURFACE });
        if focused {
            cv.outline(20.0, y, W - 40.0, 92.0, 14.0, 2.0, ACCENT);
        }
        cv.text(t, Face::Bold, w.games, 36.0, y + 12.0, 17.0, TEXT);
        let folder = self.games_folder();
        let (line, color) = match (&self.settings.folder, &self.tf2) {
            (Some(chosen), _) if chosen.is_dir() => (chosen.display().to_string(), TEXT),
            (_, Some(tf2)) => (fill(w.tf2_found, &[("p", &tf2.display().to_string())]), SUCCESS),
            _ => (w.no_tf2.to_string(), WARM),
        };
        let line = t.fit_start(Face::Body, &line, 14.0, W - 76.0);
        cv.text(t, Face::Body, &line, 36.0, y + 38.0, 14.0, color);
        let note = t.fit(Face::Body, w.open_note, 12.0, if focused { 300.0 } else { W - 76.0 });
        cv.text(t, Face::Body, &note, 36.0, y + 64.0, 12.0, FAINT);
        let _ = folder;
        if focused {
            let mut x = W - 36.0;
            if self.settings.folder.is_some() && self.tf2.is_some() {
                x = self.chip(cv, t, x, y + 58.0, "X", w.use_tf2, false);
            }
            self.chip(cv, t, x, y + 58.0, "A", w.change_folder, true);
        }

        let entry = state.entries.get(self.focus);
        let primary = match entry {
            Some(e) if e.installed.is_some() => w.open,
            Some(_) => w.install,
            None => w.choose,
        };
        self.hints(cv, t, &[("A", primary), ("Y", w.refresh), ("B", w.quit)]);
        if state.busy
            && entry.is_some_and(|e| !e.working())
            && matches!(entry.map(|e| &e.store), Some(StoreState::Checking))
        {
            let s = w.checking;
            let sw = t.measure(Face::Body, s, 13.0);
            cv.text(t, Face::Body, s, W - 28.0 - sw, 450.0, 13.0, FAINT);
        }
    }

    fn draw_picker(&self, cv: Canvas, t: &Text, p: &Picker) {
        let w = self.words;
        cv.text(t, Face::Title, w.picker_title, 28.0, 18.0, 26.0, TEXT);
        let here = match &p.dir {
            None => w.places.to_string(),
            Some(d) => d.display().to_string(),
        };
        let here = t.fit_start(Face::Body, &here, 14.0, W - 56.0);
        cv.text(t, Face::Body, &here, 28.0, 54.0, 14.0, DIM);

        const TOP: f32 = 84.0;
        const ROW: f32 = 34.0;
        const ROWS: usize = 9;
        let first = p.focus.saturating_sub(ROWS - 1).min(p.rows.len().saturating_sub(ROWS));
        if p.rows.is_empty() || (p.rows.len() == 1 && p.dir.is_some()) {
            cv.text(t, Face::Body, w.no_folders, 44.0, TOP + ROW + 8.0, 14.0, FAINT);
        }
        for (n, row) in p.rows.iter().enumerate().skip(first).take(ROWS) {
            let y = TOP + (n - first) as f32 * ROW;
            let focused = n == p.focus;
            if focused {
                cv.fill(20.0, y, W - 40.0, ROW - 4.0, 10.0, SURFACE_HI);
                cv.outline(20.0, y, W - 40.0, ROW - 4.0, 10.0, 2.0, ACCENT);
            }
            let (label, detail) = match row {
                Row::Place(kind, path) => (self.place_name(kind).to_string(), path.display().to_string()),
                Row::Up => (format!("↑  {}", w.back), String::new()),
                Row::Folder(name) => (name.clone(), String::new()),
            };
            let face = if matches!(row, Row::Place(..)) { Face::Bold } else { Face::Body };
            let label = t.fit(face, &label, 15.0, if detail.is_empty() { W - 90.0 } else { 220.0 });
            cv.text(t, face, &label, 40.0, y + 7.0, 15.0, if matches!(row, Row::Up) { DIM } else { TEXT });
            if !detail.is_empty() {
                let detail = t.fit_start(Face::Body, &detail, 13.0, W - 330.0);
                let dw = t.measure(Face::Body, &detail, 13.0);
                cv.text(t, Face::Body, &detail, W - 40.0 - dw, y + 9.0, 13.0, DIM);
            }
        }
        if p.dir.is_some() {
            let s = fill(w.files_here, &[("n", &p.files.to_string())]);
            cv.text(t, Face::Body, &s, 28.0, 400.0, 13.0, FAINT);
        }
        let open = if p.dir.is_none() { w.choose } else { w.open };
        self.hints(cv, t, &[("A", open), ("X", w.use_this), ("B", w.back)]);
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
    let mut text = Text::load(&fonts, Canvas::now().scale, &i18n::all_text());
    let mut app = App {
        words: i18n::words(&lang),
        center: Center::start(store::api(), save.join("downloads")),
        settings: Settings::load(&save),
        save,
        tf2: games::detect_tf2(),
        tf2_checked: 0.0,
        focus: std::env::var("EMULATION_CENTER_FOCUS").ok().and_then(|f| f.parse().ok()).unwrap_or(0).min(GAMES_ROW),
        screen: Screen::Home,
        notice: None,
    };
    match std::env::var("EMULATION_CENTER_SCENE").as_deref() {
        Ok("picker") => app.screen = Screen::Picker(app.open_picker(None)),
        Ok(scene) if scene.starts_with("picker:") => {
            app.screen = Screen::Picker(app.open_picker(Some(PathBuf::from(&scene["picker:".len()..]))));
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
    // Quitting stops a download in progress; the next install resumes it.
    emulation_center::download::stop_active();
    unsafe { ffi::CloseWindow() };
}
