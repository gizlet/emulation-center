//! Where the user's games are: the TF2 card where it is mounted, or a
//! folder the user picks. The TF2 card is a mounted filesystem on an MMC
//! disk other than the one holding the root filesystem (TF1, with the base,
//! boot and data partitions). Gizlet OS mounts it at /media/tf2, which wins
//! when several mounts qualify. The center only reads these folders.

use std::path::{Path, PathBuf};

pub const TF2_MOUNT: &str = "/media/tf2";

#[derive(Clone, Debug, PartialEq)]
pub struct Mount {
    /// `MAJOR:MINOR` of the mounted device.
    pub device: String,
    pub target: PathBuf,
}

/// Mount points in /proc/self/mountinfo escape space, tab, newline and
/// backslash as octal.
fn unescape(field: &str) -> String {
    let bytes = field.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 3 < bytes.len() && bytes[i + 1..i + 4].iter().all(|b| (b'0'..=b'7').contains(b)) {
            out.push(u8::from_str_radix(&field[i + 1..i + 4], 8).unwrap_or(b'?'));
            i += 4;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn parse_mountinfo(text: &str) -> Vec<Mount> {
    text.lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split(' ').collect();
            let device = fields.get(2)?;
            let target = fields.get(4)?;
            Some(Mount { device: device.to_string(), target: unescape(target).into() })
        })
        .collect()
}

/// Picks the TF2 mount. `disk_of` names the disk holding a `MAJOR:MINOR`
/// device (`mmcblk1` for `mmcblk1p1`), or nothing for virtual filesystems.
pub fn find_tf2(mounts: &[Mount], disk_of: impl Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let root = mounts.iter().find(|m| m.target == Path::new("/")).and_then(|m| disk_of(&m.device));
    let mut found: Option<PathBuf> = None;
    for m in mounts {
        let Some(disk) = disk_of(&m.device) else { continue };
        if !disk.starts_with("mmcblk") || Some(&disk) == root.as_ref() {
            continue;
        }
        if m.target == Path::new(TF2_MOUNT) {
            return Some(m.target.clone());
        }
        if found.is_none() {
            found = Some(m.target.clone());
        }
    }
    found
}

/// The disk holding a `MAJOR:MINOR` block device, from sysfs under `sys`.
pub fn sys_disk_of(sys: &Path, device: &str) -> Option<String> {
    let mut path = std::fs::canonicalize(sys.join("dev/block").join(device)).ok()?;
    if path.join("partition").is_file() {
        path.pop();
    }
    path.file_name().map(|n| n.to_string_lossy().into_owned())
}

/// The TF2 card's mount point on this device, if it is mounted. Tests point
/// `EMULATION_CENTER_MOUNTINFO` and `EMULATION_CENTER_SYSFS` at their own
/// mount table and sysfs to present a disk image as an MMC card.
pub fn detect_tf2() -> Option<PathBuf> {
    let mountinfo = std::env::var_os("EMULATION_CENTER_MOUNTINFO").unwrap_or_else(|| "/proc/self/mountinfo".into());
    let sys = PathBuf::from(std::env::var_os("EMULATION_CENTER_SYSFS").unwrap_or_else(|| "/sys".into()));
    let text = std::fs::read_to_string(mountinfo).ok()?;
    find_tf2(&parse_mountinfo(&text), |device| sys_disk_of(&sys, device)).filter(|p| p.is_dir())
}

/// The folder to open in the emulator: the one the user chose while it
/// exists, else the TF2 card, else /home.
pub fn games_folder(chosen: Option<&Path>, tf2: Option<&Path>) -> PathBuf {
    chosen.filter(|p| p.is_dir()).or(tf2).map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("/home"))
}

/// Subfolders of `dir`, without hidden ones, sorted case-insensitively. Names
/// with line breaks are left out: the hand-off to an emulator is line-based.
pub fn subfolders(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !n.starts_with('.') && n != "System Volume Information" && n != "lost+found")
        .filter(|n| !n.contains(['\n', '\r']))
        .collect();
    names.sort_by_key(|n| n.to_lowercase());
    names
}

/// Files in `dir`, for showing what a folder holds before choosing it.
pub fn file_count(dir: &Path) -> usize {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| e.path().is_file() && !e.file_name().to_string_lossy().starts_with('.'))
                .count()
        })
        .unwrap_or(0)
}

#[derive(Clone, Debug, PartialEq)]
pub enum PlaceKind {
    Tf2,
    Home,
    Media,
    Mnt,
}

/// Where the folder picker starts: the places the emulators can read.
pub fn places(tf2: Option<&Path>) -> Vec<(PlaceKind, PathBuf)> {
    let mut out = Vec::new();
    if let Some(tf2) = tf2 {
        out.push((PlaceKind::Tf2, tf2.to_path_buf()));
    }
    for (kind, path) in [(PlaceKind::Home, "/home"), (PlaceKind::Media, "/media"), (PlaceKind::Mnt, "/mnt")] {
        let path = PathBuf::from(path);
        if path.is_dir() && tf2 != Some(path.as_path()) {
            out.push((kind, path));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // As on an R36S with both cards: TF1 (mmcblk1 here) holds the base, the
    // boot partition and the data partition bound at /opt/gizlet and /home.
    const MOUNTINFO: &str = "\
21 1 179:2 / / ro,relatime shared:1 - squashfs /dev/root ro
22 21 0:5 / /dev rw shared:2 - devtmpfs devtmpfs rw
30 21 179:3 /gizlet /opt/gizlet rw,noatime shared:9 - ext4 /dev/mmcblk1p3 rw
31 21 179:3 /home /home rw,noatime shared:10 - ext4 /dev/mmcblk1p3 rw
32 21 179:1 / /boot rw shared:11 - vfat /dev/mmcblk1p1 rw
40 21 179:33 / /media/tf2 rw,nosuid,nodev,noexec shared:20 - exfat /dev/mmcblk0p1 rw
41 21 179:34 / /media/my\\040games rw shared:21 - vfat /dev/mmcblk0p2 rw
";

    fn disk_of(device: &str) -> Option<String> {
        match device {
            "179:1" | "179:2" | "179:3" => Some("mmcblk1".into()),
            "179:33" | "179:34" => Some("mmcblk0".into()),
            "8:1" => Some("sda".into()),
            _ => None,
        }
    }

    #[test]
    fn finds_tf2_and_not_the_system_card() {
        let mounts = parse_mountinfo(MOUNTINFO);
        assert_eq!(mounts[6].target, PathBuf::from("/media/my games"));
        assert_eq!(find_tf2(&mounts, disk_of), Some(PathBuf::from("/media/tf2")));
        let elsewhere: Vec<Mount> = mounts.iter().filter(|m| m.device != "179:33").cloned().collect();
        assert_eq!(find_tf2(&elsewhere, disk_of), Some(PathBuf::from("/media/my games")));
        let tf1_only: Vec<Mount> = mounts.iter().filter(|m| !m.device.starts_with("179:3")).cloned().collect();
        assert_eq!(find_tf2(&tf1_only, disk_of), None);
        let usb = parse_mountinfo(
            "21 1 179:2 / / ro - squashfs /dev/root ro\n50 21 8:1 / /media/usb rw - vfat /dev/sda1 rw\n",
        );
        assert_eq!(find_tf2(&usb, disk_of), None);
    }

    #[test]
    fn prefers_the_chosen_folder_while_it_exists() {
        let tmp = std::env::temp_dir();
        assert_eq!(games_folder(Some(&tmp), Some(Path::new("/media/tf2"))), tmp);
        let gone = Path::new("/nonexistent/emulation-center");
        assert_eq!(games_folder(Some(gone), Some(Path::new("/media/tf2"))), PathBuf::from("/media/tf2"));
        assert_eq!(games_folder(Some(gone), None), PathBuf::from("/home"));
    }

    #[test]
    fn lists_visible_subfolders_in_order() {
        let dir = std::env::temp_dir().join(format!("emulation-center-test-{}", std::process::id()));
        for name in ["psp", "NES", ".hidden", "gba"] {
            std::fs::create_dir_all(dir.join(name)).unwrap();
        }
        std::fs::write(dir.join("readme.txt"), "x").unwrap();
        assert_eq!(subfolders(&dir), ["gba", "NES", "psp"]);
        assert_eq!(file_count(&dir), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
