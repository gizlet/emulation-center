//! Vector icons drawn from primitives, in the launcher's style. Each fits a
//! square of side `s` centered on (cx, cy); `hole` is the colour behind it,
//! for cut-outs.
// Drawing takes a position, a size and colours.
#![allow(clippy::too_many_arguments)]

use crate::canvas::{alpha, mix, Canvas, BLACK, WHITE};
use crate::text::{Face, Text};
use raylib_sys::Color;

/// A rounded app tile with a soft shadow and a faint rim.
pub fn tile(cv: &Canvas, x: f32, y: f32, size: f32, round: f32, color: Color) {
    cv.fill(x - 1.0, y + 1.5, size + 2.0, size + 2.0, round + 1.0, alpha(BLACK, 0.28));
    cv.fill(x, y, size, size, round, color);
    cv.outline(x, y, size, size, round, 1.0, alpha(WHITE, 0.10));
}

/// A controller, for RetroArch's home consoles.
pub fn gamepad(cv: &Canvas, cx: f32, cy: f32, s: f32, c: Color, hole: Color) {
    let w = s * 0.92;
    let h = s * 0.5;
    cv.circle(cx - w * 0.3, cy + h * 0.28, s * 0.2, c);
    cv.circle(cx + w * 0.3, cy + h * 0.28, s * 0.2, c);
    cv.fill(cx - w / 2.0, cy - h / 2.0, w, h, h * 0.45, c);
    let (dx, dy) = (cx - w * 0.25, cy - h * 0.02);
    let (a, b) = (s * 0.17, s * 0.06);
    cv.fill(dx - a / 2.0, dy - b / 2.0, a, b, b * 0.3, hole);
    cv.fill(dx - b / 2.0, dy - a / 2.0, b, a, b * 0.3, hole);
    cv.circle(cx + w * 0.2, cy - h * 0.12, s * 0.048, hole);
    cv.circle(cx + w * 0.31, cy + h * 0.06, s * 0.048, hole);
}

/// A wide handheld with its screen between the D-pad and the buttons, for PPSSPP.
pub fn handheld(cv: &Canvas, cx: f32, cy: f32, s: f32, c: Color, hole: Color) {
    let (w, h) = (s * 0.98, s * 0.46);
    cv.fill(cx - w / 2.0, cy - h / 2.0, w, h, h * 0.5, c);
    let (sw, sh) = (s * 0.42, s * 0.28);
    cv.fill(cx - sw / 2.0, cy - sh / 2.0, sw, sh, s * 0.035, hole);
    let (dx, a, b) = (cx - s * 0.335, s * 0.13, s * 0.045);
    cv.fill(dx - a / 2.0, cy - b / 2.0, a, b, b * 0.3, hole);
    cv.fill(dx - b / 2.0, cy - a / 2.0, b, a, b * 0.3, hole);
    let (bx, d, r) = (cx + s * 0.335, s * 0.052, s * 0.026);
    for (ox, oy) in [(0.0, -d), (d, 0.0), (0.0, d), (-d, 0.0)] {
        cv.circle(bx + ox, cy + oy, r, hole);
    }
}

/// A folder: its back with the tab in `back`, its front in `c`.
pub fn folder(cv: &Canvas, cx: f32, cy: f32, s: f32, c: Color, back: Color) {
    let (w, h) = (s * 0.86, s * 0.66);
    let (x, y) = (cx - w / 2.0, cy - h / 2.0);
    let r = s * 0.07;
    cv.fill(x, y, w * 0.44, h * 0.3, r, back);
    cv.fill(x, y + h * 0.12, w, h * 0.88, r, back);
    cv.fill(x, y + h * 0.27, w, h * 0.73, r, c);
}

/// A memory card with its contacts and a cut corner, for the TF2 card.
pub fn card(cv: &Canvas, cx: f32, cy: f32, s: f32, c: Color, hole: Color) {
    let (w, h) = (s * 0.6, s * 0.78);
    let (x, y) = (cx - w / 2.0, cy - h / 2.0);
    cv.fill(x, y, w, h, s * 0.08, c);
    let cut = s * 0.2;
    cv.triangle((x + w - cut, y - 0.5), (x + w + 0.5, y - 0.5), (x + w + 0.5, y + cut), hole);
    for i in 0..3 {
        let px = x + s * 0.09 + i as f32 * s * 0.1;
        cv.fill(px, y + s * 0.08, s * 0.05, s * 0.16, s * 0.02, hole);
    }
}

pub fn house(cv: &Canvas, cx: f32, cy: f32, s: f32, c: Color, hole: Color) {
    cv.triangle((cx, cy - s * 0.4), (cx - s * 0.42, cy - s * 0.02), (cx + s * 0.42, cy - s * 0.02), c);
    cv.fill(cx - s * 0.3, cy - s * 0.08, s * 0.6, s * 0.44, s * 0.05, c);
    cv.fill(cx - s * 0.08, cy + s * 0.12, s * 0.16, s * 0.24, s * 0.03, hole);
}

/// A drive with its light, for cards and drives under /media.
pub fn drive(cv: &Canvas, cx: f32, cy: f32, s: f32, c: Color, hole: Color) {
    let (w, h) = (s * 0.86, s * 0.42);
    cv.fill(cx - w / 2.0, cy - h / 2.0, w, h, s * 0.1, c);
    cv.fill(cx - s * 0.3, cy - s * 0.03, s * 0.34, s * 0.06, s * 0.03, hole);
    cv.circle(cx + s * 0.25, cy, s * 0.055, hole);
}

/// Two stacked drives, for /mnt.
pub fn mounts(cv: &Canvas, cx: f32, cy: f32, s: f32, c: Color, hole: Color) {
    for oy in [-0.19, 0.19] {
        let y = cy + s * oy;
        let (w, h) = (s * 0.82, s * 0.3);
        cv.fill(cx - w / 2.0, y - h / 2.0, w, h, s * 0.08, c);
        cv.circle(cx + s * 0.24, y, s * 0.045, hole);
        cv.fill(cx - s * 0.28, y - s * 0.025, s * 0.28, s * 0.05, s * 0.025, hole);
    }
}

pub fn arrow_up(cv: &Canvas, cx: f32, cy: f32, s: f32, c: Color) {
    let t = s * 0.12;
    cv.line(cx, cy + s * 0.3, cx, cy - s * 0.12, t, c);
    cv.triangle((cx, cy - s * 0.36), (cx - s * 0.25, cy - s * 0.06), (cx + s * 0.25, cy - s * 0.06), c);
}

pub fn chevron(cv: &Canvas, cx: f32, cy: f32, s: f32, c: Color) {
    let t = s * 0.13;
    cv.line(cx - s * 0.12, cy - s * 0.26, cx + s * 0.14, cy, t, c);
    cv.line(cx + s * 0.14, cy, cx - s * 0.12, cy + s * 0.26, t, c);
}

pub fn check(cv: &Canvas, cx: f32, cy: f32, s: f32, c: Color) {
    let t = s * 0.14;
    cv.line(cx - s * 0.32, cy + s * 0.02, cx - s * 0.1, cy + s * 0.24, t, c);
    cv.line(cx - s * 0.1, cy + s * 0.24, cx + s * 0.34, cy - s * 0.24, t, c);
}

/// A turning arc; `time` in seconds.
pub fn spinner(cv: &Canvas, cx: f32, cy: f32, r: f32, thick: f32, time: f64, c: Color) {
    let start = (time * 300.0 % 360.0) as f32;
    cv.ring(cx, cy, r - thick, r, 0.0, 360.0, alpha(c, 0.22));
    cv.ring(cx, cy, r - thick, r, start, start + 100.0, c);
}

/// A face button: a filled circle with its letter.
pub fn button(cv: &Canvas, t: &Text, label: &str, cx: f32, cy: f32, r: f32, bg: Color, fg: Color) {
    cv.circle(cx, cy, r, bg);
    cv.text_center(t, Face::Bold, label, cx, cy + r * 0.06, r * 1.3, fg);
}

/// The back panel of a folder in colour `c`.
pub fn folder_back(c: Color) -> Color {
    mix(c, BLACK, 0.28)
}
