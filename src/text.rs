//! Text in the launcher's fonts: Space Grotesk for titles, Inter for the
//! rest, and Droid Sans Fallback for every character those lack (Chinese,
//! and whatever a folder on the user's card is called). Each glyph is drawn
//! from the first font that has it. The fallback's atlas holds only the
//! characters seen so far and is rebuilt when new ones appear.

use raylib_sys::{self as ffi, Color, Font, Vector2};
use std::collections::BTreeSet;
use std::ffi::CString;
use std::path::Path;

#[derive(Clone, Copy, PartialEq)]
pub enum Face {
    Title,
    Body,
    Bold,
}

pub struct Text {
    title: Font,
    body: Font,
    bold: Font,
    fallback: Font,
    fallback_path: CString,
    fallback_chars: BTreeSet<char>,
    /// The size the fonts were rasterized at, per logical unit of size.
    scale: f32,
}

const SIZE: f32 = 32.0;

fn latin() -> Vec<i32> {
    let mut cps: Vec<i32> = (32..127).collect();
    cps.extend(160..256);
    cps.extend(['…', '·', '–', '—', '‘', '’', '“', '”', '•', '→', '←', '↑', '↓'].iter().map(|&c| c as i32));
    cps
}

unsafe fn load(path: &CString, size: i32, cps: &mut [i32]) -> Font {
    let font = ffi::LoadFontEx(path.as_ptr(), size, cps.as_mut_ptr(), cps.len() as i32);
    ffi::SetTextureFilter(font.texture, ffi::TextureFilter::TEXTURE_FILTER_BILINEAR as i32);
    font
}

fn has(font: &Font, c: char) -> bool {
    let i = unsafe { ffi::GetGlyphIndex(*font, c as i32) };
    i >= 0 && i < font.glyphCount && unsafe { (*font.glyphs.add(i as usize)).value } == c as i32
}

impl Text {
    /// Loads the fonts from `dir` for a screen `scale` times the logical size.
    pub fn load(dir: &Path, scale: f32, extra: &str) -> Text {
        let path = |name: &str| CString::new(dir.join(name).to_string_lossy().into_owned()).unwrap();
        let size = (SIZE * scale).round() as i32;
        let mut cps = latin();
        let (title, body, bold) = unsafe {
            (
                load(&path("SpaceGrotesk-Bold.ttf"), size, &mut cps),
                load(&path("Inter-Regular.ttf"), size, &mut cps),
                load(&path("Inter-SemiBold.ttf"), size, &mut cps),
            )
        };
        let mut text = Text {
            title,
            body,
            bold,
            fallback: Font {
                baseSize: 0,
                glyphCount: 0,
                glyphPadding: 0,
                texture: unsafe { std::mem::zeroed() },
                recs: std::ptr::null_mut(),
                glyphs: std::ptr::null_mut(),
            },
            fallback_path: path("DroidSansFallbackFull.ttf"),
            fallback_chars: BTreeSet::new(),
            scale,
        };
        text.need(extra);
        text
    }

    fn font(&self, face: Face) -> &Font {
        match face {
            Face::Title => &self.title,
            Face::Body => &self.body,
            Face::Bold => &self.bold,
        }
    }

    /// Makes sure every character of `s` can be drawn.
    pub fn need(&mut self, s: &str) {
        let missing: Vec<char> = s
            .chars()
            .filter(|&c| !c.is_control() && !self.fallback_chars.contains(&c) && !has(&self.body, c))
            .collect();
        if missing.is_empty() && self.fallback.glyphCount > 0 {
            return;
        }
        self.fallback_chars.extend(missing);
        self.fallback_chars.insert('?');
        let mut cps: Vec<i32> = self.fallback_chars.iter().map(|&c| c as i32).collect();
        unsafe {
            if self.fallback.glyphCount > 0 {
                ffi::UnloadFont(self.fallback);
            }
            self.fallback = load(&self.fallback_path, (SIZE * self.scale).round() as i32, &mut cps);
        }
    }

    fn glyph(&self, face: Face, c: char) -> (&Font, i32) {
        let primary = self.font(face);
        if has(primary, c) {
            return (primary, c as i32);
        }
        if face == Face::Title && has(&self.bold, c) {
            return (&self.bold, c as i32);
        }
        if has(&self.fallback, c) {
            return (&self.fallback, c as i32);
        }
        (&self.body, '?' as i32)
    }

    fn advance(font: &Font, cp: i32, size: f32) -> f32 {
        unsafe {
            let i = ffi::GetGlyphIndex(*font, cp) as usize;
            let g = *font.glyphs.add(i);
            let width = if g.advanceX > 0 { g.advanceX as f32 } else { (*font.recs.add(i)).width };
            width * size / font.baseSize as f32
        }
    }

    /// Width of `s` at `size` logical units, in logical units.
    pub fn measure(&self, face: Face, s: &str, size: f32) -> f32 {
        s.chars()
            .map(|c| {
                let (font, cp) = self.glyph(face, c);
                Self::advance(font, cp, size)
            })
            .sum()
    }

    /// Draws `s` with its top-left corner at the screen position `at`, `size`
    /// screen pixels high.
    pub fn draw(&self, face: Face, s: &str, at: Vector2, size: f32, color: Color) {
        let mut x = at.x;
        for c in s.chars() {
            let (font, cp) = self.glyph(face, c);
            if c != ' ' {
                unsafe { ffi::DrawTextCodepoint(*font, cp, Vector2 { x, y: at.y }, size, color) };
            }
            x += Self::advance(font, cp, size);
        }
    }

    /// `s` shortened with an ellipsis to fit `width`.
    pub fn fit(&self, face: Face, s: &str, size: f32, width: f32) -> String {
        if self.measure(face, s, size) <= width {
            return s.to_string();
        }
        let mut chars: Vec<char> = s.chars().collect();
        while !chars.is_empty() {
            chars.pop();
            let candidate: String = chars.iter().collect::<String>() + "…";
            if self.measure(face, &candidate, size) <= width {
                return candidate;
            }
        }
        "…".into()
    }

    /// `s` shortened from the start, for paths whose end matters most.
    pub fn fit_start(&self, face: Face, s: &str, size: f32, width: f32) -> String {
        if self.measure(face, s, size) <= width {
            return s.to_string();
        }
        let mut chars: Vec<char> = s.chars().collect();
        while !chars.is_empty() {
            chars.remove(0);
            let candidate = format!("…{}", chars.iter().collect::<String>());
            if self.measure(face, &candidate, size) <= width {
                return candidate;
            }
        }
        "…".into()
    }
}
