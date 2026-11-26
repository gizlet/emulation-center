//! Stands in for the center's UI in the end-to-end test (tests/e2e.sh): it
//! takes the steps a player would, through the same code the UI calls.
//!
//! 1. Installs RetroArch from the Store (the test's fixture Store).
//! 2. Has the test publish an update (`FLOW_PUBLISH_UPDATE`), sees it offered
//!    and updates.
//! 3. Tries PPSSPP, whose download does not match the Store's checksum, and
//!    checks that nothing is installed.
//! 4. Chooses the games folder `FLOW_GAMES`, as the folder picker does, and
//!    opens RetroArch with it.
//!
//! Run again after the emulator exits, it quits, as pressing B does.

use emulation_center::catalog::EMULATORS;
use emulation_center::jobs::{Center, Shared, StoreState, Task};
use emulation_center::settings::Settings;
use emulation_center::{cli, games, save_dir, store, write_open_request};
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn settle(center: &Center) -> Shared {
    let deadline = Instant::now() + Duration::from_secs(180);
    std::thread::sleep(Duration::from_millis(100));
    loop {
        let s = center.snapshot();
        if !s.busy && !s.entries.iter().any(|e| e.working()) {
            return s;
        }
        assert!(Instant::now() < deadline, "the center did not settle: {s:?}");
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn step(n: u32, what: &str) {
    println!("flow {n}: {what}");
}

fn main() {
    let save = save_dir();
    let done = save.join("flow-done");
    if done.exists() {
        println!("flow: quitting the center");
        return;
    }
    let center = Center::start(store::api(), save.join("downloads"));
    let retroarch = 0;
    let ppsspp = 1;
    assert_eq!(EMULATORS[retroarch].id, "io.gizlet.retroarch");

    let s = settle(&center);
    assert_eq!(s.entries[retroarch].installed, None);
    assert_eq!(s.entries[retroarch].offer().map(|r| r.version.as_str()), Some("1.0.0"), "{s:?}");
    step(1, "RetroArch 1.0.0 is offered; installing");
    assert!(center.install(retroarch));
    let s = settle(&center);
    assert_eq!(s.entries[retroarch].task, Task::Installed("1.0.0".into()), "{s:?}");
    assert_eq!(cli::installed().unwrap().get("io.gizlet.retroarch").map(String::as_str), Some("1.0.0"));

    let publish = std::env::var("FLOW_PUBLISH_UPDATE").expect("FLOW_PUBLISH_UPDATE");
    assert!(std::process::Command::new(&publish).status().unwrap().success());
    center.refresh();
    let s = settle(&center);
    let offer = s.entries[retroarch].offer().map(|r| r.version.clone());
    assert_eq!(offer.as_deref(), Some("1.0.1"), "{s:?}");
    step(2, "RetroArch 1.0.1 is offered as an update; updating");
    assert!(center.install(retroarch));
    let s = settle(&center);
    assert_eq!(s.entries[retroarch].task, Task::Installed("1.0.1".into()), "{s:?}");
    assert_eq!(s.entries[retroarch].installed.as_deref(), Some("1.0.1"));
    assert!(s.entries[retroarch].offer().is_none());

    assert!(matches!(s.entries[ppsspp].store, StoreState::Ready(_)), "{s:?}");
    step(3, "PPSSPP's download is corrupted; installing must fail");
    assert!(center.install(ppsspp));
    let s = settle(&center);
    match &s.entries[ppsspp].task {
        Task::Failed(e) if e.contains("checksum") => {}
        other => panic!("expected a checksum failure, got {other:?}"),
    }
    assert_eq!(s.entries[ppsspp].installed, None);
    assert!(!cli::installed().unwrap().contains_key("io.gizlet.ppsspp"));

    let chosen = PathBuf::from(std::env::var("FLOW_GAMES").expect("FLOW_GAMES"));
    Settings { folder: Some(chosen.clone()) }.store(&save).unwrap();
    let folder = games::games_folder(Settings::load(&save).folder.as_deref(), games::detect_tf2().as_deref());
    assert_eq!(folder, chosen);
    step(4, &format!("opening RetroArch on {}", folder.display()));
    write_open_request(&save, EMULATORS[retroarch].id, &folder).unwrap();
    std::fs::write(done, "").unwrap();
}
