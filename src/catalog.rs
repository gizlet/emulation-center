//! The emulators the center offers. Each is an ordinary gizlet with its own
//! repository and Store listing; the center installs it by that link.

/// What the center draws on an emulator's tile.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Icon {
    /// A controller, for home consoles.
    Gamepad,
    /// A wide handheld.
    Handheld,
}

pub struct Emulator {
    pub id: &'static str,
    /// `github.com/<owner>/<repo>`, the gizlet's Store link.
    pub repo: &'static str,
    pub name: &'static str,
    pub systems: &'static str,
    pub systems_zh: &'static str,
    pub color: [u8; 3],
    pub icon: Icon,
}

pub const EMULATORS: [Emulator; 2] = [
    Emulator {
        id: "io.gizlet.retroarch",
        repo: "github.com/gizlet/retroarch",
        name: "RetroArch",
        systems: "NES · SNES · Game Boy · GBA · PC Engine · WonderSwan",
        systems_zh: "NES · SNES · Game Boy · GBA · PC Engine · WonderSwan",
        color: [0x5B, 0x4B, 0xD6],
        icon: Icon::Gamepad,
    },
    Emulator {
        id: "io.gizlet.ppsspp",
        repo: "github.com/gizlet/ppsspp",
        name: "PPSSPP",
        systems: "PlayStation Portable",
        systems_zh: "PlayStation Portable（PSP）",
        color: [0x1D, 0x77, 0xC9],
        icon: Icon::Handheld,
    },
];
