use std::{
    collections::{HashMap, VecDeque},
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex, Weak},
    thread,
    time::{Duration, SystemTime},
};

use crate::{HomeHealthState, HomeStore};
use sha2::Digest;

use super::{
    StableThemeFileId, ThemeWatchError, ThemeWatchHint, ThemeWatchLimits, ThemeWatchSubscription,
};

#[derive(Default)]
pub(crate) struct ThemeWatcherCoordinator {
    active: Mutex<Option<Weak<WatchShared>>>,
}

impl ThemeWatcherCoordinator {
    pub(crate) fn shutdown(&self) {
        if let Ok(mut active) = self.active.lock() {
            if let Some(shared) = active.take().and_then(|shared| shared.upgrade()) {
                shared.shutdown();
            }
        }
    }
}

struct QueueState {
    queue: VecDeque<ThemeWatchHint>,
    shutdown: bool,
}

pub(crate) struct WatchShared {
    state: Mutex<QueueState>,
    changed: Condvar,
    capacity: usize,
}

impl WatchShared {
    fn new(capacity: usize) -> Self {
        Self {
            state: Mutex::new(QueueState {
                queue: VecDeque::new(),
                shutdown: false,
            }),
            changed: Condvar::new(),
            capacity,
        }
    }

    fn push(&self, hint: ThemeWatchHint) {
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        if state.shutdown {
            return;
        }
        if state.queue.contains(&ThemeWatchHint::Overflow) {
            return;
        }
        if hint == ThemeWatchHint::Overflow || state.queue.len() == self.capacity {
            state.queue.clear();
            state.queue.push_back(ThemeWatchHint::Overflow);
        } else if !state.queue.contains(&hint) {
            state.queue.push_back(hint);
        }
        self.changed.notify_all();
    }

    pub(crate) fn shutdown(&self) {
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        state.shutdown = true;
        state.queue.clear();
        self.changed.notify_all();
    }

    pub(crate) fn try_recv(&self) -> Result<Option<ThemeWatchHint>, ThemeWatchError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| ThemeWatchError::LockPoisoned)?;
        if let Some(hint) = state.queue.pop_front() {
            return Ok(Some(hint));
        }
        if state.shutdown {
            return Err(ThemeWatchError::ShutDown);
        }
        Ok(None)
    }

    pub(crate) fn recv_timeout(
        &self,
        timeout: Duration,
    ) -> Result<Option<ThemeWatchHint>, ThemeWatchError> {
        let state = self
            .state
            .lock()
            .map_err(|_| ThemeWatchError::LockPoisoned)?;
        let (mut state, _) = self
            .changed
            .wait_timeout_while(state, timeout, |state| {
                state.queue.is_empty() && !state.shutdown
            })
            .map_err(|_| ThemeWatchError::LockPoisoned)?;
        if let Some(hint) = state.queue.pop_front() {
            return Ok(Some(hint));
        }
        if state.shutdown {
            return Err(ThemeWatchError::ShutDown);
        }
        Ok(None)
    }

    fn wait_interval(&self, interval: Duration) -> bool {
        let state = match self.state.lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        let (state, _) = match self
            .changed
            .wait_timeout_while(state, interval, |state| !state.shutdown)
        {
            Ok(result) => result,
            Err(poisoned) => poisoned.into_inner(),
        };
        state.shutdown
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Stamp {
    length: u64,
    modified: Option<SystemTime>,
    sha256: [u8; 32],
}

struct Observation {
    manifest: Option<Stamp>,
    documents: HashMap<StableThemeFileId, Stamp>,
}

impl HomeStore {
    pub fn subscribe_theme_changes(
        &self,
        limits: ThemeWatchLimits,
    ) -> Result<ThemeWatchSubscription, ThemeWatchError> {
        let admission = self.health.admit()?;
        let generation_guard = self
            .generation
            .read()
            .map_err(|_| ThemeWatchError::LockPoisoned)?;
        let generation = generation_guard
            .as_ref()
            .ok_or(ThemeWatchError::LockPoisoned)?;
        let database = generation.database.clone();
        let health_generation = admission.generation();
        admission.confirm_database(&database, |_| ThemeWatchError::LockPoisoned)?;
        drop(generation_guard);

        let shared = Arc::new(WatchShared::new(limits.queue_capacity().get()));
        {
            let mut active = self
                .theme_watcher
                .active
                .lock()
                .map_err(|_| ThemeWatchError::LockPoisoned)?;
            if active.as_ref().and_then(Weak::upgrade).is_some() {
                return Err(ThemeWatchError::AlreadySubscribed);
            }
            *active = Some(Arc::downgrade(&shared));
        }
        let root = self.canonical_path().join("themes");
        let previous = observe(&root, limits);
        if previous.is_err() {
            shared.push(ThemeWatchHint::Overflow);
        }
        let health = Arc::clone(&self.health);
        let worker_shared = Arc::clone(&shared);
        let worker = thread::Builder::new()
            .name("beryl-theme-watch".into())
            .spawn(move || {
                let mut previous = previous;
                loop {
                    if worker_shared.wait_interval(limits.interval()) {
                        break;
                    }
                    let current_health = health.snapshot();
                    if current_health.state() != HomeHealthState::Healthy
                        || current_health.generation() != Some(health_generation)
                    {
                        worker_shared.shutdown();
                        break;
                    }
                    let current = observe(&root, limits);
                    match (&previous, &current) {
                        (Ok(old), Ok(new)) => emit_changes(&worker_shared, old, new),
                        _ => worker_shared.push(ThemeWatchHint::Overflow),
                    }
                    previous = current;
                }
            })
            .map_err(|_| ThemeWatchError::ShutDown)?;
        Ok(ThemeWatchSubscription {
            shared,
            worker: Some(worker),
        })
    }
}

fn observe(root: &Path, limits: ThemeWatchLimits) -> Result<Observation, ()> {
    let maximum = limits.max_entries_per_poll().get();
    let manifest = stamp(&root.join("manifest.toml"), limits)?;
    let mut documents = HashMap::with_capacity(maximum.min(64));
    let installed = root.join("installed");
    let entries = match fs::read_dir(installed) {
        Ok(entries) => entries,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Observation {
                manifest,
                documents,
            });
        }
        Err(_) => return Err(()),
    };
    for (index, entry) in entries.enumerate() {
        if index >= maximum {
            return Err(());
        }
        let entry = entry.map_err(|_| ())?;
        let name = entry.file_name().into_string().map_err(|_| ())?;
        let Some(stem) = name.strip_suffix(".toml") else {
            continue;
        };
        let Ok(id) = StableThemeFileId::new(stem) else {
            continue;
        };
        if let Some(value) = stamp(&entry.path(), limits)? {
            documents.insert(id, value);
        }
    }
    Ok(Observation {
        manifest,
        documents,
    })
}

fn stamp(path: &PathBuf, limits: ThemeWatchLimits) -> Result<Option<Stamp>, ()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
            if metadata.len() > limits.max_file_bytes() {
                return Err(());
            }
            let mut file = fs::File::open(path).map_err(|_| ())?;
            let mut digest = sha2::Sha256::new();
            let mut actual = 0_u64;
            let mut buffer = vec![0_u8; limits.io_buffer_bytes().get()];
            loop {
                let read = file.read(&mut buffer).map_err(|_| ())?;
                if read == 0 {
                    break;
                }
                actual = actual.checked_add(read as u64).ok_or(())?;
                if actual > limits.max_file_bytes() {
                    return Err(());
                }
                sha2::Digest::update(&mut digest, &buffer[..read]);
            }
            if actual != metadata.len() {
                return Err(());
            }
            Ok(Some(Stamp {
                length: actual,
                modified: metadata.modified().ok(),
                sha256: sha2::Digest::finalize(digest).into(),
            }))
        }
        Ok(_) => Err(()),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(()),
    }
}

fn emit_changes(shared: &WatchShared, old: &Observation, new: &Observation) {
    if old.manifest != new.manifest {
        shared.push(ThemeWatchHint::ManifestChanged);
    }
    for id in old.documents.keys().chain(new.documents.keys()) {
        if old.documents.get(id) != new.documents.get(id) {
            shared.push(ThemeWatchHint::DocumentChanged(id.clone()));
        }
    }
}
