//! Where the user's games are: the TF2 card where it is mounted, or a
//! folder the user picks. The TF2 card is a mounted filesystem on an MMC
//! disk other than the one holding the root filesystem (TF1, with the base,
//! boot and data partitions). Gizlet OS mounts it at /media/tf2, which wins
//! when several mounts qualify. The center only reads these folders.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc};

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

/// Whether the picker shows an entry: not hidden, and not a filesystem's own folder.
fn listed(name: &str) -> bool {
    !name.starts_with('.') && name != "System Volume Information" && name != "lost+found"
}

/// Whether an entry is a folder and whether it is a file. The type comes
/// from the directory listing; a symbolic link counts as what it points to.
fn kind(entry: &std::fs::DirEntry) -> (bool, bool) {
    match entry.file_type() {
        Ok(t) if t.is_symlink() => {
            let path = entry.path();
            (path.is_dir(), path.is_file())
        }
        Ok(t) => (t.is_dir(), t.is_file()),
        Err(_) => (false, false),
    }
}

/// Subfolders of `dir`, without hidden ones, sorted case-insensitively. Names
/// with line breaks are left out: the hand-off to an emulator is line-based.
pub fn subfolders(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|e| kind(e).0)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| listed(n) && !n.contains(['\n', '\r']))
        .collect();
    names.sort_by_key(|n| n.to_lowercase());
    names
}

/// What a folder holds, for showing it before choosing it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Contents {
    /// Visible files.
    pub files: usize,
    /// The subfolders the picker lists.
    pub folders: usize,
}

/// The visible files and subfolders of `dir`. A folder of many games costs
/// no more than reading its listing.
pub fn contents(dir: &Path) -> Contents {
    let mut contents = Contents::default();
    let Ok(entries) = std::fs::read_dir(dir) else { return contents };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !listed(&name) {
            continue;
        }
        let (is_dir, is_file) = kind(&entry);
        if is_dir && !name.contains(['\n', '\r']) {
            contents.folders += 1;
        } else if is_file {
            contents.files += 1;
        }
    }
    contents
}

/// Reads what folders hold on a worker thread, so that a large folder on a
/// slow card never holds up the screen. Requests belong to a generation;
/// starting a new one drops the requests of the last that are still queued.
pub struct Scanner {
    requests: mpsc::Sender<(u64, PathBuf)>,
    results: mpsc::Receiver<(u64, PathBuf, Contents)>,
    generation: Arc<AtomicU64>,
}

impl Scanner {
    pub fn start() -> Scanner {
        let (requests, queue) = mpsc::channel::<(u64, PathBuf)>();
        let (done, results) = mpsc::channel();
        let generation = Arc::new(AtomicU64::new(0));
        let current = Arc::clone(&generation);
        std::thread::spawn(move || {
            for (asked, path) in queue {
                if asked != current.load(Ordering::Relaxed) {
                    continue;
                }
                let found = contents(&path);
                if done.send((asked, path, found)).is_err() {
                    break;
                }
            }
        });
        Scanner { requests, results, generation }
    }

    /// Starts a new generation, dropping the requests not yet read.
    pub fn restart(&self) {
        self.generation.fetch_add(1, Ordering::Relaxed);
    }

    pub fn request(&self, path: PathBuf) {
        let _ = self.requests.send((self.generation.load(Ordering::Relaxed), path));
    }

    /// The folders read since the last call, in this generation.
    pub fn results(&self) -> Vec<(PathBuf, Contents)> {
        let current = self.generation.load(Ordering::Relaxed);
        self.results.try_iter().filter(|(asked, ..)| *asked == current).map(|(_, path, c)| (path, c)).collect()
    }
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
        for name in ["psp", "NES", ".hidden", "gba", "lost+found"] {
            std::fs::create_dir_all(dir.join(name)).unwrap();
        }
        std::fs::write(dir.join("readme.txt"), "x").unwrap();
        std::fs::write(dir.join(".DS_Store"), "x").unwrap();
        assert_eq!(subfolders(&dir), ["gba", "NES", "psp"]);
        assert_eq!(contents(&dir), Contents { files: 1, folders: 3 });
        assert_eq!(contents(&dir.join("gba")), Contents::default());
        assert_eq!(contents(&dir.join("missing")), Contents::default());

        let scanner = Scanner::start();
        scanner.request(dir.clone());
        scanner.request(dir.join("gba"));
        let mut found = Vec::new();
        for _ in 0..200 {
            found.extend(scanner.results());
            if found.len() == 2 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(found, [(dir.clone(), Contents { files: 1, folders: 3 }), (dir.join("gba"), Contents::default())]);
        // A new generation drops what the last one asked for.
        scanner.request(dir.clone());
        scanner.restart();
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert_eq!(scanner.results(), []);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
