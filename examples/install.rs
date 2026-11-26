//! Installs or updates one emulator from the Gizlet Store the way the center
//! does, without a display: `cargo run --no-default-features --example
//! install -- retroarch` (or `ppsspp`). It uses `GIZLET_STORE_API`,
//! `GIZLET_CLI` and `GIZLET_SAVE_DIR` like the center, and prints what the
//! Store offers, what is installed and how the install went.

use emulation_center::catalog::EMULATORS;
use emulation_center::jobs::{Center, Task};
use emulation_center::{save_dir, store};
use std::time::{Duration, Instant};

fn main() {
    let name = std::env::args().nth(1).unwrap_or_default();
    let Some(index) = EMULATORS.iter().position(|e| e.repo.ends_with(&format!("/{name}"))) else {
        eprintln!("usage: install retroarch|ppsspp");
        std::process::exit(2);
    };
    let center = Center::start(store::api(), save_dir().join("downloads"));
    let settle = || {
        let deadline = Instant::now() + Duration::from_secs(1800);
        std::thread::sleep(Duration::from_millis(200));
        loop {
            let s = center.snapshot();
            if !s.busy && !s.entries.iter().any(|e| e.working()) {
                return s;
            }
            if Instant::now() > deadline {
                eprintln!("timed out");
                std::process::exit(1);
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    };
    let s = settle();
    let entry = &s.entries[index];
    println!("{}: installed {:?}, Store {:?}", EMULATORS[index].name, entry.installed, entry.store);
    let Some(offer) = entry.offer().cloned() else {
        println!("nothing to install");
        return;
    };
    println!("installing {} ({} bytes, sha256 {})", offer.version, offer.size, offer.sha256);
    center.install(index);
    let s = settle();
    match &s.entries[index].task {
        Task::Installed(v) => println!("installed {v}; gizlet list says {:?}", s.entries[index].installed),
        other => {
            eprintln!("install failed: {other:?}");
            std::process::exit(1);
        }
    }
}
