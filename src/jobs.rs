//! The center's background work, one job at a time on a worker thread so
//! the UI keeps drawing: reading what is installed and what the Store
//! offers, and installing or updating an emulator.

use crate::catalog::EMULATORS;
use crate::cli::{self, Target};
use crate::download::{self, Progress, Stage};
use crate::store::{self, Lookup, Release};
use crate::version::is_newer;
use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, PartialEq)]
pub enum StoreState {
    Unknown,
    Checking,
    Ready(Release),
    NotReady,
    Missing,
    Offline(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Task {
    Idle,
    Waiting,
    Downloading,
    Verifying,
    Installing,
    Failed(String),
    Installed(String),
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub installed: Option<String>,
    pub store: StoreState,
    pub task: Task,
}

impl Entry {
    /// The release to install: any release when none is installed, else a
    /// newer one.
    pub fn offer(&self) -> Option<&Release> {
        match (&self.store, &self.installed) {
            (StoreState::Ready(r), Some(v)) if !is_newer(&r.version, v) => None,
            (StoreState::Ready(r), _) => Some(r),
            _ => None,
        }
    }

    pub fn working(&self) -> bool {
        matches!(self.task, Task::Waiting | Task::Downloading | Task::Verifying | Task::Installing)
    }
}

#[derive(Clone, Debug)]
pub struct Shared {
    pub entries: Vec<Entry>,
    pub target: Option<Target>,
    /// Why the installed apps could not be read.
    pub error: Option<String>,
    pub busy: bool,
}

enum Command {
    Refresh,
    Install(usize),
}

pub struct Center {
    shared: Arc<Mutex<Shared>>,
    pub progress: Arc<Progress>,
    tx: Sender<Command>,
}

impl Center {
    /// Starts the worker, which reads the installed versions and the Store
    /// at once. Packages are downloaded into `downloads`.
    pub fn start(api: String, downloads: PathBuf) -> Center {
        let shared = Arc::new(Mutex::new(Shared {
            entries: EMULATORS
                .iter()
                .map(|_| Entry { installed: None, store: StoreState::Unknown, task: Task::Idle })
                .collect(),
            target: None,
            error: None,
            busy: true,
        }));
        let progress = Arc::new(Progress::default());
        let (tx, rx) = mpsc::channel();
        let worker = Worker { shared: shared.clone(), progress: progress.clone(), api, downloads };
        std::thread::spawn(move || {
            for command in rx {
                worker.shared.lock().unwrap().busy = true;
                match command {
                    Command::Refresh => worker.refresh(),
                    Command::Install(i) => worker.install(i),
                }
                worker.shared.lock().unwrap().busy = false;
            }
        });
        let center = Center { shared, progress, tx };
        center.refresh();
        center
    }

    pub fn refresh(&self) {
        let _ = self.tx.send(Command::Refresh);
    }

    /// Queues installing or updating emulator `index`; false if it is
    /// already being worked on.
    pub fn install(&self, index: usize) -> bool {
        let mut shared = self.shared.lock().unwrap();
        let Some(entry) = shared.entries.get_mut(index) else { return false };
        if entry.working() {
            return false;
        }
        entry.task = Task::Waiting;
        let _ = self.tx.send(Command::Install(index));
        true
    }

    pub fn snapshot(&self) -> Shared {
        self.shared.lock().unwrap().clone()
    }
}

struct Worker {
    shared: Arc<Mutex<Shared>>,
    progress: Arc<Progress>,
    api: String,
    downloads: PathBuf,
}

impl Worker {
    fn set_task(&self, i: usize, task: Task) {
        self.shared.lock().unwrap().entries[i].task = task;
    }

    fn target(&self) -> Result<Target, String> {
        if let Some(t) = self.shared.lock().unwrap().target.clone() {
            return Ok(t);
        }
        let t = cli::target()?;
        self.shared.lock().unwrap().target = Some(t.clone());
        Ok(t)
    }

    fn read_installed(&self) {
        let installed = cli::installed();
        let mut shared = self.shared.lock().unwrap();
        match installed {
            Ok(list) => {
                shared.error = None;
                for (entry, emulator) in shared.entries.iter_mut().zip(EMULATORS.iter()) {
                    entry.installed = list.get(emulator.id).cloned();
                }
            }
            Err(e) => shared.error = Some(e),
        }
    }

    fn lookup(&self, i: usize) -> StoreState {
        let target = match self.target() {
            Ok(t) => t,
            Err(e) => return StoreState::Offline(e),
        };
        match store::lookup(&self.api, EMULATORS[i].repo, &target) {
            Lookup::Ready(r) => StoreState::Ready(r),
            Lookup::NotReady => StoreState::NotReady,
            Lookup::Missing => StoreState::Missing,
            Lookup::Error(e) => StoreState::Offline(e),
        }
    }

    fn refresh(&self) {
        {
            let mut shared = self.shared.lock().unwrap();
            for entry in shared.entries.iter_mut() {
                entry.store = StoreState::Checking;
            }
        }
        self.read_installed();
        for i in 0..EMULATORS.len() {
            let state = self.lookup(i);
            self.shared.lock().unwrap().entries[i].store = state;
        }
    }

    fn install(&self, i: usize) {
        let state = self.lookup(i);
        self.shared.lock().unwrap().entries[i].store = state.clone();
        let StoreState::Ready(release) = state else {
            let why = match state {
                StoreState::Offline(e) => e,
                StoreState::NotReady => "the Store is still checking this release".into(),
                _ => "no release for this device yet".into(),
            };
            return self.set_task(i, Task::Failed(why));
        };
        if let Some(free) = download::free_space(self.downloads.parent().unwrap_or(&self.downloads)) {
            let needed = download::space_needed(&release);
            if free < needed {
                return self
                    .set_task(i, Task::Failed(format!("needs {} MB free on the card", needed.div_ceil(1 << 20))));
            }
        }
        self.progress.done.store(0, std::sync::atomic::Ordering::Relaxed);
        let stage = |s: Stage| {
            self.set_task(
                i,
                match s {
                    Stage::Downloading => Task::Downloading,
                    Stage::Verifying => Task::Verifying,
                },
            )
        };
        let file = match download::fetch(&release, &self.downloads, &self.progress, &stage) {
            Ok(file) => file,
            Err(e) => return self.set_task(i, Task::Failed(e)),
        };
        self.set_task(i, Task::Installing);
        let result = cli::install(&file);
        let _ = std::fs::remove_file(&file);
        self.read_installed();
        match result {
            Ok(()) => self.set_task(i, Task::Installed(release.version)),
            Err(e) => self.set_task(i, Task::Failed(e)),
        }
    }
}
