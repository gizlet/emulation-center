//! Emulation Center's logic, kept apart from the UI so it can be tested
//! without a display: the emulators it offers, the Gizlet Store lookups,
//! downloads, installation through the `gizlet` command, the TF2 card and the
//! hand-off to an emulator.

pub mod catalog;
pub mod cli;
pub mod download;
pub mod games;
pub mod i18n;
pub mod jobs;
pub mod settings;
pub mod store;
pub mod version;

use std::path::{Path, PathBuf};

/// Where the center keeps its own files: `GIZLET_SAVE_DIR`, or `./save`.
pub fn save_dir() -> PathBuf {
    std::env::var_os("GIZLET_SAVE_DIR").map(Into::into).unwrap_or_else(|| "save".into())
}

/// The file the entry script reads after the UI exits: the emulator to open
/// and the folder for its file browser, one per line.
pub fn open_request_path(save: &Path) -> PathBuf {
    save.join("open-request")
}

pub fn write_open_request(save: &Path, id: &str, games: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(save)?;
    let path = open_request_path(save);
    let tmp = path.with_extension("new");
    std::fs::write(&tmp, format!("{id}\n{}\n", games.display()))?;
    std::fs::rename(tmp, path)
}
