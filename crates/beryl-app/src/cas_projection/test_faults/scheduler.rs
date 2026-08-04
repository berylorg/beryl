use std::{
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
        mpsc::{Receiver, SyncSender, sync_channel},
    },
    time::Duration,
};

use beryl_home_store::HomeGeneration;
use beryl_model::BerylHomeId;

use crate::cas_projection::ProjectionServiceGeneration;
#[cfg(test)]
use crate::cas_projection::{LoadedCasProjection, service_config::ProjectionWorkerPermit};

static NEXT_TOKEN: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Eq, PartialEq)]
struct SchedulerKey {
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    service_generation: ProjectionServiceGeneration,
}

struct SchedulerPanicHook {
    key: SchedulerKey,
    token: u64,
    panic_taken: bool,
    join_taken: bool,
    panic_started: SyncSender<()>,
    join_started: SyncSender<()>,
}

/// Controls one exact scheduler-main panic and observes its eventual owning join.
pub struct AcceptedInputSchedulerPanicController {
    key: SchedulerKey,
    token: u64,
    panic_started: Receiver<()>,
    join_started: Receiver<()>,
}

#[cfg(test)]
pub(in crate::cas_projection) struct AcceptedInputSchedulerWorkerRequest {
    pub(in crate::cas_projection) projection: LoadedCasProjection,
    pub(in crate::cas_projection) worker: ProjectionWorkerPermit,
    pub(in crate::cas_projection) owner: beryl_model::SyndicThreadId,
    pub(in crate::cas_projection) release: Receiver<()>,
    pub(in crate::cas_projection) registered: SyncSender<()>,
}

#[cfg(test)]
struct SchedulerWorkerHook {
    key: SchedulerKey,
    token: u64,
    request: Option<AcceptedInputSchedulerWorkerRequest>,
}

#[cfg(test)]
pub(in crate::cas_projection) struct AcceptedInputSchedulerWorkerController {
    key: SchedulerKey,
    token: u64,
    registered: Receiver<()>,
    release: Option<SyncSender<()>>,
}

pub(crate) fn install_accepted_input_scheduler_panic(
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    service_generation: ProjectionServiceGeneration,
) -> AcceptedInputSchedulerPanicController {
    let key = SchedulerKey {
        home_id,
        home_generation,
        service_generation,
    };
    let token = NEXT_TOKEN.fetch_add(1, Ordering::Relaxed);
    let (panic_started, panic_observation) = sync_channel(1);
    let (join_started, join_observation) = sync_channel(1);
    let mut hooks = hooks().lock().unwrap_or_else(|poison| poison.into_inner());
    assert!(
        !hooks.iter().any(|hook| hook.key == key),
        "one exact accepted-input scheduler may own only one panic hook"
    );
    hooks.push(SchedulerPanicHook {
        key,
        token,
        panic_taken: false,
        join_taken: false,
        panic_started,
        join_started,
    });
    AcceptedInputSchedulerPanicController {
        key,
        token,
        panic_started: panic_observation,
        join_started: join_observation,
    }
}

#[cfg(test)]
pub(in crate::cas_projection) fn install_accepted_input_scheduler_worker(
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    service_generation: ProjectionServiceGeneration,
    projection: LoadedCasProjection,
    worker: ProjectionWorkerPermit,
) -> AcceptedInputSchedulerWorkerController {
    let key = SchedulerKey {
        home_id,
        home_generation,
        service_generation,
    };
    let token = NEXT_TOKEN.fetch_add(1, Ordering::Relaxed);
    let (registered, registered_observation) = sync_channel(1);
    let (release, release_observation) = sync_channel(1);
    let owner = projection.syndic_thread_id();
    let request = AcceptedInputSchedulerWorkerRequest {
        projection,
        worker,
        owner,
        release: release_observation,
        registered,
    };
    let mut hooks = worker_hooks()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    assert!(
        !hooks.iter().any(|hook| hook.key == key),
        "one exact accepted-input scheduler may own only one injected worker"
    );
    hooks.push(SchedulerWorkerHook {
        key,
        token,
        request: Some(request),
    });
    AcceptedInputSchedulerWorkerController {
        key,
        token,
        registered: registered_observation,
        release: Some(release),
    }
}

#[cfg(test)]
pub(in crate::cas_projection) fn take_accepted_input_scheduler_worker(
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    service_generation: ProjectionServiceGeneration,
) -> Option<AcceptedInputSchedulerWorkerRequest> {
    let key = SchedulerKey {
        home_id,
        home_generation,
        service_generation,
    };
    let mut hooks = worker_hooks()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let index = hooks.iter().position(|hook| hook.key == key)?;
    hooks.swap_remove(index).request
}

pub(crate) fn panic_accepted_input_scheduler_main_if_requested(
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    service_generation: ProjectionServiceGeneration,
) {
    let key = SchedulerKey {
        home_id,
        home_generation,
        service_generation,
    };
    let observation = {
        let mut hooks = hooks().lock().unwrap_or_else(|poison| poison.into_inner());
        hooks
            .iter_mut()
            .find(|hook| hook.key == key)
            .and_then(|hook| {
                if hook.panic_taken {
                    None
                } else {
                    hook.panic_taken = true;
                    Some(hook.panic_started.clone())
                }
            })
    };
    if let Some(observation) = observation {
        let _ = observation.send(());
        panic!("injected accepted-input scheduler-main panic");
    }
}

pub(crate) fn observe_accepted_input_scheduler_join(
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    service_generation: ProjectionServiceGeneration,
) {
    let key = SchedulerKey {
        home_id,
        home_generation,
        service_generation,
    };
    let observation = {
        let mut hooks = hooks().lock().unwrap_or_else(|poison| poison.into_inner());
        hooks
            .iter_mut()
            .find(|hook| hook.key == key)
            .and_then(|hook| {
                if hook.join_taken {
                    None
                } else {
                    hook.join_taken = true;
                    Some(hook.join_started.clone())
                }
            })
    };
    if let Some(observation) = observation {
        let _ = observation.send(());
    }
}

impl AcceptedInputSchedulerPanicController {
    /// Waits until the exact scheduler has entered the injected unwind.
    #[must_use]
    pub fn wait_until_panicking(&self, timeout: Duration) -> bool {
        self.panic_started.recv_timeout(timeout).is_ok()
    }

    /// Waits until inventory conversion starts joining the panicked scheduler owner.
    #[must_use]
    pub fn wait_until_join_requested(&self, timeout: Duration) -> bool {
        self.join_started.recv_timeout(timeout).is_ok()
    }
}

impl Drop for AcceptedInputSchedulerPanicController {
    fn drop(&mut self) {
        let mut hooks = hooks().lock().unwrap_or_else(|poison| poison.into_inner());
        if let Some(index) = hooks
            .iter()
            .position(|hook| hook.key == self.key && hook.token == self.token)
        {
            hooks.swap_remove(index);
        }
    }
}

#[cfg(test)]
impl AcceptedInputSchedulerWorkerController {
    pub(in crate::cas_projection) fn wait_until_registered(&self, timeout: Duration) -> bool {
        self.registered.recv_timeout(timeout).is_ok()
    }

    pub(in crate::cas_projection) fn release(mut self) {
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
    }
}

#[cfg(test)]
impl Drop for AcceptedInputSchedulerWorkerController {
    fn drop(&mut self) {
        let mut hooks = worker_hooks()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if let Some(index) = hooks
            .iter()
            .position(|hook| hook.key == self.key && hook.token == self.token)
        {
            hooks.swap_remove(index);
        }
        drop(hooks);
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
    }
}

fn hooks() -> &'static Mutex<Vec<SchedulerPanicHook>> {
    static HOOKS: OnceLock<Mutex<Vec<SchedulerPanicHook>>> = OnceLock::new();
    HOOKS.get_or_init(|| Mutex::new(Vec::new()))
}

#[cfg(test)]
fn worker_hooks() -> &'static Mutex<Vec<SchedulerWorkerHook>> {
    static HOOKS: OnceLock<Mutex<Vec<SchedulerWorkerHook>>> = OnceLock::new();
    HOOKS.get_or_init(|| Mutex::new(Vec::new()))
}
