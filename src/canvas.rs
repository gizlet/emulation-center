//! Drawing on the 640×480 logical screen, letterboxed into the window, in
//! the launcher's dark theme.
// Drawing takes a position, a size and colours.
#![allow(clippy::too_many_arguments)]

use crate::text::{Face, Text};
use raylib_sys::{self as ffi, Color, Rectangle, Vector2};

pub const W: f32 = 640.0;
pub const H: f32 = 480.0;

/// Fraction of the font size from the top of a line to the middle of its glyphs.
const MIDLINE: f32 = 0.56;

pub const fn rgb(hex: u32) -> Color {
    Color { r: (hex >> 16) as u8, g: (hex >> 8) as u8, b: hex as u8, a: 255 }
}

pub fn alpha(c: Color, a: f32) -> Color {
    Color { a: (c.a as f32 * a.clamp(0.0, 1.0)) as u8, ..c }
}

pub fn mix(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color { r: l(a.r, b.r), g: l(a.g, b.g), b: l(a.b, b.b), a: l(a.a, b.a) }
}

// The launcher's dark theme.
pub const BG_TOP: Color = rgb(0x1d2027);
pub const BG_BOTTOM: Color = rgb(0x121418);
pub const SURFACE: Color = rgb(0x262a32);
pub const SURFACE_HI: Color = rgb(0x323743);
pub const LINE: Color = rgb(0x3a3f4b);
pub const TEXT: Color = rgb(0xf3f5f8);
pub const DIM: Color = rgb(0xa4abb8);
pub const FAINT: Color = rgb(0x6c7380);
pub const ACCENT: Color = rgb(0x34b6ff);
pub const ACCENT_HI: Color = rgb(0x9ee3ff);
pub const ON_ACCENT: Color = rgb(0x06121c);
pub const DANGER: Color = rgb(0xff5d63);
pub const SUCCESS: Color = rgb(0x3ddc97);
pub const WARM: Color = rgb(0xff9a3d);
pub const WHITE: Color = rgb(0xffffff);
pub const BLACK: Color = rgb(0x000000);

fn segments(radius_px: f32) -> i32 {
    ((radius_px / 2.0).ceil() as i32).clamp(4, 24)
}

/// Maps the 640×480 logical screen onto the window, letterboxed.
#[derive(Clone, Copy)]
pub struct Canvas {
    scale: f32,
    ox: f32,
    oy: f32,
}

impl Canvas {
    pub fn now() -> Canvas {
        let (sw, sh) = unsafe { (ffi::GetScreenWidth() as f32, ffi::GetScreenHeight() as f32) };
        let scale = (sw / W).min(sh / H);
        Canvas { scale, ox: (sw - W * scale) / 2.0, oy: (sh - H * scale) / 2.0 }
    }

    pub fn scale(&self) -> f32 {
        self.scale
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

    pub fn gradient_v(&self, x: f32, y: f32, w: f32, h: f32, top: Color, bottom: Color) {
        let r = self.rect(x, y, w, h);
        unsafe {
            ffi::DrawRectangleGradientV(
                r.x as i32,
                r.y as i32,
                r.width.ceil() as i32,
                r.height.ceil() as i32,
                top,
                bottom,
            )
        };
    }

    pub fn fill(&self, x: f32, y: f32, w: f32, h: f32, round: f32, color: Color) {
        if w <= 0.0 || h <= 0.0 || color.a == 0 {
            return;
        }
        let r = self.rect(x, y, w, h);
        unsafe {
            if round <= 0.0 {
                ffi::DrawRectangleRec(r, color);
            } else {
                let roundness = (2.0 * round / w.min(h)).min(1.0);
                ffi::DrawRectangleRounded(r, roundness, segments(round * self.scale), color);
            }
        }
    }

    /// An outline drawn inside the rectangle's bounds.
    pub fn outline(&self, x: f32, y: f32, w: f32, h: f32, round: f32, thick: f32, color: Color) {
        if color.a == 0 || w <= thick || h <= thick {
            return;
        }
        let t = thick;
        let (x, y, w, h, round) = (x + t / 2.0, y + t / 2.0, w - t, h - t, (round - t / 2.0).max(0.5));
        let roundness = (2.0 * round / w.min(h)).min(1.0);
        unsafe {
            ffi::DrawRectangleRoundedLinesEx(
                self.rect(x, y, w, h),
                roundness,
                segments(round * self.scale),
                t * self.scale,
                color,
            )
        };
    }

    pub fn circle(&self, cx: f32, cy: f32, r: f32, color: Color) {
        if color.a > 0 {
            unsafe { ffi::DrawCircleV(self.v(cx, cy), r * self.scale, color) };
        }
    }

    /// A disc fading from `inner` at the center to `outer` at the rim.
    pub fn circle_gradient(&self, cx: f32, cy: f32, r: f32, inner: Color, outer: Color) {
        unsafe { ffi::DrawCircleGradient(self.v(cx, cy), r * self.scale, inner, outer) };
    }

    /// An arc between angles in degrees, 0° pointing right and growing clockwise.
    pub fn ring(&self, cx: f32, cy: f32, inner: f32, outer: f32, start: f32, end: f32, color: Color) {
        if color.a == 0 {
            return;
        }
        let segs = (((end - start).abs() / 6.0).ceil() as i32).max(8);
        unsafe { ffi::DrawRing(self.v(cx, cy), inner * self.scale, outer * self.scale, start, end, segs, color) };
    }

    /// A line with round caps.
    pub fn line(&self, x1: f32, y1: f32, x2: f32, y2: f32, thick: f32, color: Color) {
        unsafe { ffi::DrawLineEx(self.v(x1, y1), self.v(x2, y2), thick * self.scale, color) };
        self.circle(x1, y1, thick / 2.0, color);
        self.circle(x2, y2, thick / 2.0, color);
    }

    /// A filled triangle; the vertex order does not matter.
    pub fn triangle(&self, a: (f32, f32), b: (f32, f32), c: (f32, f32), color: Color) {
        let cross = (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0);
        let (b, c) = if cross > 0.0 { (c, b) } else { (b, c) };
        unsafe { ffi::DrawTriangle(self.v(a.0, a.1), self.v(b.0, b.1), self.v(c.0, c.1), color) };
    }

    /// `s` with its top-left corner at (x, y).
    pub fn text(&self, t: &Text, face: Face, s: &str, x: f32, y: f32, size: f32, color: Color) {
        t.draw(face, s, self.v(x, y), size * self.scale, color);
    }

    /// `s` from `x`, centered vertically on `cy`. Returns its width.
    pub fn text_mid(&self, t: &Text, face: Face, s: &str, x: f32, cy: f32, size: f32, color: Color) -> f32 {
        self.text(t, face, s, x, cy - size * MIDLINE, size, color);
        t.measure(face, s, size)
    }

    /// `s` ending at `right`, centered vertically on `cy`. Returns its left edge.
    pub fn text_right(&self, t: &Text, face: Face, s: &str, right: f32, cy: f32, size: f32, color: Color) -> f32 {
        let x = right - t.measure(face, s, size);
        self.text(t, face, s, x, cy - size * MIDLINE, size, color);
        x
    }

    /// `s` centered on (cx, cy).
    pub fn text_center(&self, t: &Text, face: Face, s: &str, cx: f32, cy: f32, size: f32, color: Color) {
        let x = cx - t.measure(face, s, size) / 2.0;
        self.text(t, face, s, x, cy - size * MIDLINE, size, color);
    }
}
