//! Buttons from the handheld's own evdev devices on Gizlet OS (raylib's DRM
//! platform only knows a fixed gamepad mapping), and from the keyboard on a
//! computer. Linux's gamepad codes: BTN_EAST is A, BTN_SOUTH B, BTN_NORTH X,
//! BTN_WEST Y. `GIZLET_SWAP_AB=1` swaps A and B, as the launcher does.

use raylib_sys::{self as ffi, KeyboardKey as K};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Button {
    Up,
    Down,
    Left,
    Right,
    A,
    B,
    X,
    Y,
    L,
    R,
}

const ALL: [Button; 10] = [
    Button::Up,
    Button::Down,
    Button::Left,
    Button::Right,
    Button::A,
    Button::B,
    Button::X,
    Button::Y,
    Button::L,
    Button::R,
];

pub struct Controls {
    swap_ab: bool,
    held: [f32; 10],
    pressed: [bool; 10],
    #[cfg(all(target_os = "linux", feature = "gizlet-os"))]
    evdev: evdev::Evdev,
}

fn index(b: Button) -> usize {
    ALL.iter().position(|&x| x == b).unwrap()
}

impl Controls {
    pub fn new() -> Controls {
        Controls {
            swap_ab: std::env::var("GIZLET_SWAP_AB").is_ok_and(|v| v == "1"),
            held: [0.0; 10],
            pressed: [false; 10],
            #[cfg(all(target_os = "linux", feature = "gizlet-os"))]
            evdev: evdev::Evdev::default(),
        }
    }

    fn keyboard(b: Button) -> bool {
        let keys: &[K] = match b {
            Button::Up => &[K::KEY_UP, K::KEY_W],
            Button::Down => &[K::KEY_DOWN, K::KEY_S],
            Button::Left => &[K::KEY_LEFT, K::KEY_A],
            Button::Right => &[K::KEY_RIGHT, K::KEY_D],
            Button::A => &[K::KEY_ENTER, K::KEY_K, K::KEY_SPACE],
            Button::B => &[K::KEY_ESCAPE, K::KEY_J, K::KEY_BACKSPACE],
            Button::X => &[K::KEY_I],
            Button::Y => &[K::KEY_U],
            Button::L => &[K::KEY_Q, K::KEY_PAGE_UP],
            Button::R => &[K::KEY_E, K::KEY_PAGE_DOWN],
        };
        keys.iter().any(|&k| unsafe { ffi::IsKeyDown(k as i32) })
    }

    /// Reads the controls once per frame.
    pub fn poll(&mut self, dt: f32, time: f64) {
        let _ = time;
        #[cfg(all(target_os = "linux", feature = "gizlet-os"))]
        self.evdev.poll(time);
        for (i, &b) in ALL.iter().enumerate() {
            let physical = match (b, self.swap_ab) {
                (Button::A, true) => Button::B,
                (Button::B, true) => Button::A,
                _ => b,
            };
            #[allow(unused_mut)]
            let mut down = Self::keyboard(b);
            #[cfg(all(target_os = "linux", feature = "gizlet-os"))]
            {
                down |= self.evdev.down(physical);
            }
            let _ = physical;
            let was = self.held[i];
            self.held[i] = if down { was + dt.max(1e-4) } else { 0.0 };
            let first = down && was == 0.0;
            // Directions repeat while held: after 0.35 s, eight times a second.
            let repeat = matches!(b, Button::Up | Button::Down | Button::Left | Button::Right | Button::L | Button::R)
                && was > 0.35
                && ((was - 0.35) / 0.125).floor() != ((self.held[i] - 0.35) / 0.125).floor();
            self.pressed[i] = first || repeat;
        }
    }

    pub fn pressed(&self, b: Button) -> bool {
        self.pressed[index(b)]
    }

    /// Whether any button was pressed this frame.
    pub fn any(&self) -> bool {
        self.pressed.iter().any(|&p| p)
    }
}

#[cfg(all(target_os = "linux", feature = "gizlet-os"))]
mod evdev {
    use super::Button;
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

    const EV_KEY: u16 = 0x01;
    const EV_ABS: u16 = 0x03;
    const KEY_CNT: usize = 0x300;
    const KEY_A: usize = 30;
    const BTN_SOUTH: usize = 0x130;
    const BTN_EAST: usize = 0x131;
    const BTN_NORTH: usize = 0x133;
    const BTN_WEST: usize = 0x134;
    const BTN_TL: usize = 0x136;
    const BTN_TR: usize = 0x137;
    const BTN_THUMBR: usize = 0x13e;
    const BTN_DPAD_UP: usize = 0x220;
    const BTN_DPAD_DOWN: usize = 0x221;
    const BTN_DPAD_LEFT: usize = 0x222;
    const BTN_DPAD_RIGHT: usize = 0x223;
    const ABS_X: usize = 0x00;
    const ABS_Y: usize = 0x01;
    const ABS_HAT0X: usize = 0x10;
    const ABS_HAT0Y: usize = 0x11;
    const ABS_CNT: usize = 0x40;

    /// `_IOC(_IOC_READ, 'E', nr, size)`.
    fn eviocg(nr: u64, size: usize) -> u64 {
        (2 << 30) | ((size as u64) << 16) | ((b'E' as u64) << 8) | nr
    }

    fn bit(bits: &[u8], n: usize) -> bool {
        bits.get(n / 8).is_some_and(|b| b & (1 << (n % 8)) != 0)
    }

    #[derive(Clone, Copy, Default)]
    struct Abs {
        value: i32,
        min: i32,
        max: i32,
    }

    impl Abs {
        /// -1, 0 or 1: past half way from the middle.
        fn direction(self) -> i32 {
            let half = (self.max - self.min) / 2;
            if half <= 0 {
                return 0;
            }
            let v = self.value - (self.min + half);
            if v < -half / 2 {
                -1
            } else if v > half / 2 {
                1
            } else {
                0
            }
        }
    }

    struct Device {
        path: String,
        fd: OwnedFd,
        keys: Vec<bool>,
        abs: [Abs; ABS_CNT],
        has_abs: [bool; ABS_CNT],
    }

    impl Device {
        fn open(path: &str) -> Option<Device> {
            let c = std::ffi::CString::new(path).ok()?;
            let raw = unsafe { libc::open(c.as_ptr(), libc::O_RDONLY | libc::O_NONBLOCK | libc::O_CLOEXEC) };
            if raw < 0 {
                return None;
            }
            let fd = unsafe { OwnedFd::from_raw_fd(raw) };
            let mut keys = [0u8; KEY_CNT / 8];
            if unsafe { libc::ioctl(raw, eviocg(0x20 + EV_KEY as u64, keys.len()) as _, keys.as_mut_ptr()) } < 0 {
                return None;
            }
            let gamepad = (BTN_SOUTH..=BTN_THUMBR).any(|b| bit(&keys, b))
                || (BTN_DPAD_UP..=BTN_DPAD_RIGHT).any(|b| bit(&keys, b));
            // Keyboards are raylib's.
            if bit(&keys, KEY_A) || !gamepad {
                return None;
            }
            let mut abs_bits = [0u8; ABS_CNT / 8];
            unsafe { libc::ioctl(raw, eviocg(0x20 + EV_ABS as u64, abs_bits.len()) as _, abs_bits.as_mut_ptr()) };
            let mut abs = [Abs::default(); ABS_CNT];
            let mut has_abs = [false; ABS_CNT];
            for code in [ABS_X, ABS_Y, ABS_HAT0X, ABS_HAT0Y] {
                if !bit(&abs_bits, code) {
                    continue;
                }
                let mut info: libc::input_absinfo = unsafe { std::mem::zeroed() };
                let size = std::mem::size_of::<libc::input_absinfo>();
                if unsafe { libc::ioctl(raw, eviocg(0x40 + code as u64, size) as _, &mut info) } == 0 {
                    abs[code] = Abs { value: info.value, min: info.minimum, max: info.maximum };
                    has_abs[code] = true;
                }
            }
            // Buttons still held from the app that came before start released,
            // so they do not act here until pressed again.
            Some(Device { path: path.to_string(), fd, keys: vec![false; KEY_CNT], abs, has_abs })
        }

        /// Applies pending events. False when the device is gone.
        fn read(&mut self) -> bool {
            let mut events: [libc::input_event; 64] = unsafe { std::mem::zeroed() };
            loop {
                let size = std::mem::size_of_val(&events);
                let n = unsafe { libc::read(self.fd.as_raw_fd(), events.as_mut_ptr().cast(), size) };
                if n < 0 {
                    return std::io::Error::last_os_error().raw_os_error() == Some(libc::EAGAIN);
                }
                if n == 0 {
                    return false;
                }
                let count = n as usize / std::mem::size_of::<libc::input_event>();
                for e in &events[..count] {
                    let code = e.code as usize;
                    match e.type_ {
                        EV_KEY if code < KEY_CNT && e.value != 2 => self.keys[code] = e.value != 0,
                        EV_ABS if code < ABS_CNT => self.abs[code].value = e.value,
                        _ => {}
                    }
                }
                if count < events.len() {
                    return true;
                }
            }
        }

        fn down(&self, b: Button) -> bool {
            let key = |code: usize| self.keys[code];
            let axis = |code: usize, sign: i32| {
                if !self.has_abs[code] {
                    return false;
                }
                if code == ABS_HAT0X || code == ABS_HAT0Y {
                    self.abs[code].value.signum() == sign
                } else {
                    self.abs[code].direction() == sign
                }
            };
            match b {
                Button::Up => key(BTN_DPAD_UP) || axis(ABS_HAT0Y, -1) || axis(ABS_Y, -1),
                Button::Down => key(BTN_DPAD_DOWN) || axis(ABS_HAT0Y, 1) || axis(ABS_Y, 1),
                Button::Left => key(BTN_DPAD_LEFT) || axis(ABS_HAT0X, -1) || axis(ABS_X, -1),
                Button::Right => key(BTN_DPAD_RIGHT) || axis(ABS_HAT0X, 1) || axis(ABS_X, 1),
                Button::A => key(BTN_EAST),
                Button::B => key(BTN_SOUTH),
                Button::X => key(BTN_NORTH),
                Button::Y => key(BTN_WEST),
                Button::L => key(BTN_TL),
                Button::R => key(BTN_TR),
            }
        }
    }

    #[derive(Default)]
    pub struct Evdev {
        devices: Vec<Device>,
        scanned: Option<f64>,
    }

    impl Evdev {
        pub fn poll(&mut self, time: f64) {
            if self.scanned.is_none_or(|t| time - t > 2.0) {
                self.scanned = Some(time);
                self.scan();
            }
            self.devices.retain_mut(Device::read);
        }

        fn scan(&mut self) {
            let Ok(dir) = std::fs::read_dir("/dev/input") else { return };
            let mut paths: Vec<String> = dir
                .flatten()
                .map(|e| e.path().to_string_lossy().into_owned())
                .filter(|p| p.rsplit('/').next().is_some_and(|n| n.starts_with("event")))
                .collect();
            paths.sort();
            for path in paths {
                if !self.devices.iter().any(|d| d.path == path) {
                    if let Some(d) = Device::open(&path) {
                        self.devices.push(d);
                    }
                }
            }
        }

        pub fn down(&self, b: Button) -> bool {
            self.devices.iter().any(|d| d.down(b))
        }
    }
}
