//! The center's one setting: the games folder the user chose, kept in
//! `settings.ini` in its save folder.

use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Settings {
    pub folder: Option<PathBuf>,
}

impl Settings {
    pub fn load(save: &Path) -> Settings {
        let text = std::fs::read_to_string(save.join("settings.ini")).unwrap_or_default();
        let folder = text
            .lines()
            .filter_map(|line| line.split_once('='))
            .find(|(key, _)| key.trim() == "folder")
            .map(|(_, value)| PathBuf::from(value.trim()))
            .filter(|p| p.is_absolute());
        Settings { folder }
    }

    pub fn store(&self, save: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(save)?;
        let mut text = String::new();
        if let Some(folder) = &self.folder {
            text.push_str(&format!("folder={}\n", folder.display()));
        }
        let tmp = save.join("settings.ini.new");
        std::fs::write(&tmp, text)?;
        std::fs::rename(tmp, save.join("settings.ini"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_chosen_folder() {
        let save = std::env::temp_dir().join(format!("emulation-center-settings-{}", std::process::id()));
        assert_eq!(Settings::load(&save), Settings::default());
        let chosen = Settings { folder: Some("/media/tf2/psp games".into()) };
        chosen.store(&save).unwrap();
        assert_eq!(Settings::load(&save), chosen);
        Settings::default().store(&save).unwrap();
        assert_eq!(Settings::load(&save).folder, None);
        std::fs::remove_dir_all(save).unwrap();
    }
}
