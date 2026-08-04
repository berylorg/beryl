use std::{
    collections::HashMap,
    sync::{
        Arc, Condvar, Mutex, OnceLock,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use beryl_backend::UserMessageEchoLifecycle;

use super::{
    AdmittedProjectionSession, NEXT_TOKEN, ProviderOperationCountSnapshot, ProviderTestKey,
};

/// Content-free checked-user publication activity and work.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CheckedUserPublicationSnapshot {
    activity: ProviderOperationCountSnapshot,
    publications: usize,
}

impl CheckedUserPublicationSnapshot {
    #[must_use]
    pub const fn activity(self) -> ProviderOperationCountSnapshot {
        self.activity
    }

    #[must_use]
    pub const fn publications(self) -> usize {
        self.publications
    }
}

#[derive(Debug, Default)]
pub(crate) struct CheckedUserPublicationMetrics {
    current: AtomicUsize,
    high_water: AtomicUsize,
    publications: AtomicUsize,
}

impl CheckedUserPublicationMetrics {
    pub(crate) fn begin(&self) -> CheckedUserPublicationGuard<'_> {
        let previous = self
            .current
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |current| {
                current.checked_add(1)
            })
            .expect("checked-user publication activity overflowed");
        self.high_water.fetch_max(previous + 1, Ordering::SeqCst);
        self.publications
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |count| {
                count.checked_add(1)
            })
            .expect("checked-user publication count overflowed");
        CheckedUserPublicationGuard { metrics: self }
    }

    pub(crate) fn snapshot(&self) -> CheckedUserPublicationSnapshot {
        CheckedUserPublicationSnapshot {
            activity: ProviderOperationCountSnapshot {
                current: self.current.load(Ordering::SeqCst),
                high_water: self.high_water.load(Ordering::SeqCst),
            },
            publications: self.publications.load(Ordering::SeqCst),
        }
    }
}

pub(crate) struct CheckedUserPublicationGuard<'a> {
    metrics: &'a CheckedUserPublicationMetrics,
}

impl Drop for CheckedUserPublicationGuard<'_> {
    fn drop(&mut self) {
        self.metrics
            .current
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |current| {
                current.checked_sub(1)
            })
            .expect("checked-user publication activity underflowed");
    }
}

static PUBLICATION_BARRIERS: OnceLock<
    Mutex<HashMap<ProviderTestKey, Arc<CheckedUserPublicationBarrier>>>,
> = OnceLock::new();

#[derive(Debug, Default)]
struct BarrierState {
    paused: bool,
    released: bool,
}

#[derive(Debug)]
struct CheckedUserPublicationBarrier {
    token: u64,
    lifecycle: UserMessageEchoLifecycle,
    state: Mutex<BarrierState>,
    changed: Condvar,
}

/// Controls one exact checked-user publication pause while its source permit is live.
pub struct CheckedUserPublicationBarrierController {
    key: ProviderTestKey,
    barrier: Arc<CheckedUserPublicationBarrier>,
}

/// Installs a pause for one lifecycle echo on this exact connection broker.
pub fn install_checked_user_publication_barrier(
    session: &AdmittedProjectionSession,
    lifecycle: UserMessageEchoLifecycle,
) -> CheckedUserPublicationBarrierController {
    install_checked_user_publication_barrier_for_key(session.provider_test_key(), lifecycle)
}

pub(crate) fn install_checked_user_publication_barrier_for_key(
    key: ProviderTestKey,
    lifecycle: UserMessageEchoLifecycle,
) -> CheckedUserPublicationBarrierController {
    let barrier = Arc::new(CheckedUserPublicationBarrier {
        token: NEXT_TOKEN.fetch_add(1, Ordering::Relaxed),
        lifecycle,
        state: Mutex::new(BarrierState::default()),
        changed: Condvar::new(),
    });
    let previous = publication_barriers()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .insert(key, Arc::clone(&barrier));
    if let Some(previous) = previous {
        release(&previous);
    }
    CheckedUserPublicationBarrierController { key, barrier }
}

impl CheckedUserPublicationBarrierController {
    /// Waits until the selected lifecycle owns the real source-publication permit.
    #[must_use]
    pub fn wait_until_paused(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut state = self
            .barrier
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        while !state.paused && !state.released {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                break;
            };
            let (next, timed) = self
                .barrier
                .changed
                .wait_timeout(state, remaining)
                .unwrap_or_else(|poison| poison.into_inner());
            state = next;
            if timed.timed_out() {
                break;
            }
        }
        state.paused
    }

    /// Releases the publication to durable activation and frame staging.
    pub fn release(&self) {
        release(&self.barrier);
    }
}

impl Drop for CheckedUserPublicationBarrierController {
    fn drop(&mut self) {
        release(&self.barrier);
        let mut barriers = publication_barriers()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if barriers
            .get(&self.key)
            .is_some_and(|barrier| barrier.token == self.barrier.token)
        {
            barriers.remove(&self.key);
        }
    }
}

pub(crate) fn pause_checked_user_publication(
    key: ProviderTestKey,
    lifecycle: UserMessageEchoLifecycle,
) {
    let barrier = publication_barriers()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .get(&key)
        .cloned();
    let Some(barrier) = barrier.filter(|barrier| barrier.lifecycle == lifecycle) else {
        return;
    };
    let mut state = barrier
        .state
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    if state.released {
        return;
    }
    state.paused = true;
    barrier.changed.notify_all();
    while !state.released {
        state = barrier
            .changed
            .wait(state)
            .unwrap_or_else(|poison| poison.into_inner());
    }
}

fn release(barrier: &CheckedUserPublicationBarrier) {
    barrier
        .state
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .released = true;
    barrier.changed.notify_all();
}

fn publication_barriers()
-> &'static Mutex<HashMap<ProviderTestKey, Arc<CheckedUserPublicationBarrier>>> {
    PUBLICATION_BARRIERS.get_or_init(|| Mutex::new(HashMap::new()))
}

static RECEIVER_LOSS: OnceLock<Mutex<HashMap<ProviderTestKey, u64>>> = OnceLock::new();

/// Keeps one exact broker's next checked-user submit armed for typed receiver loss.
pub struct CheckedUserSubmitReceiverLossController {
    key: ProviderTestKey,
    token: u64,
}

/// Makes this broker's next checked-user operation return ownership as `ReceiverLost`.
pub fn install_checked_user_submit_receiver_loss(
    session: &AdmittedProjectionSession,
) -> CheckedUserSubmitReceiverLossController {
    let key = session.provider_test_key();
    let token = NEXT_TOKEN.fetch_add(1, Ordering::Relaxed);
    receiver_loss()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .insert(key, token);
    CheckedUserSubmitReceiverLossController { key, token }
}

impl Drop for CheckedUserSubmitReceiverLossController {
    fn drop(&mut self) {
        let mut registry = receiver_loss()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if registry.get(&self.key) == Some(&self.token) {
            registry.remove(&self.key);
        }
    }
}

pub(crate) fn take_checked_user_submit_receiver_loss(key: ProviderTestKey) -> bool {
    receiver_loss()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .remove(&key)
        .is_some()
}

fn receiver_loss() -> &'static Mutex<HashMap<ProviderTestKey, u64>> {
    RECEIVER_LOSS.get_or_init(|| Mutex::new(HashMap::new()))
}
