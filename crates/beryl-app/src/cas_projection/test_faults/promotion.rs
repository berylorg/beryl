use std::{
    collections::HashMap,
    sync::{
        Arc, Condvar, Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use beryl_model::SyndicThreadId;

use crate::cas_projection::connection::ProjectionConnection;

static NEXT_TOKEN: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
enum PromotionBarrierStage {
    BeforeReservation,
    Reserved,
    BeforeReconciliation,
    GenerationInvalidated,
}

#[derive(Default)]
struct PromotionBarrierState {
    paused: bool,
    released: bool,
}

struct PromotionBarrier {
    token: u64,
    state: Mutex<PromotionBarrierState>,
    changed: Condvar,
}

/// Controls one exact pause after promotion authority is reserved and before publication.
pub struct ScheduledPromotionBarrierController {
    thread_id: SyndicThreadId,
    stage: PromotionBarrierStage,
    barrier: Arc<PromotionBarrier>,
}

/// Test-only authority for retiring one exact admitted projection connection.
#[derive(Clone)]
pub struct ProjectionConnectionRetirementHandle {
    connection: Arc<ProjectionConnection>,
}

/// Installs one exact-thread scheduled-promotion pause.
pub fn install_scheduled_promotion_barrier(
    thread_id: SyndicThreadId,
) -> ScheduledPromotionBarrierController {
    install_barrier(thread_id, PromotionBarrierStage::Reserved)
}

/// Installs one exact-thread pause immediately before promotion reservation.
pub fn install_scheduled_promotion_reservation_barrier(
    thread_id: SyndicThreadId,
) -> ScheduledPromotionBarrierController {
    install_barrier(thread_id, PromotionBarrierStage::BeforeReservation)
}

/// Installs one exact-thread pause after command outcome and before reconciliation.
pub fn install_scheduled_promotion_reconciliation_barrier(
    thread_id: SyndicThreadId,
) -> ScheduledPromotionBarrierController {
    install_barrier(thread_id, PromotionBarrierStage::BeforeReconciliation)
}

/// Installs one exact-thread pause after a worker proves its home generation obsolete.
pub fn install_scheduled_generation_invalidation_barrier(
    thread_id: SyndicThreadId,
) -> ScheduledPromotionBarrierController {
    install_barrier(thread_id, PromotionBarrierStage::GenerationInvalidated)
}

fn install_barrier(
    thread_id: SyndicThreadId,
    stage: PromotionBarrierStage,
) -> ScheduledPromotionBarrierController {
    let barrier = Arc::new(PromotionBarrier {
        token: NEXT_TOKEN.fetch_add(1, Ordering::Relaxed),
        state: Mutex::new(PromotionBarrierState::default()),
        changed: Condvar::new(),
    });
    let previous = barriers()
        .lock()
        .expect("scheduled-promotion barrier registry is usable")
        .insert((thread_id, stage), Arc::clone(&barrier));
    assert!(
        previous.is_none(),
        "one thread cannot install overlapping scheduled-promotion barriers"
    );
    ScheduledPromotionBarrierController {
        thread_id,
        stage,
        barrier,
    }
}

impl ScheduledPromotionBarrierController {
    /// Waits until the exact worker reaches this controller's installed promotion stage.
    #[must_use]
    pub fn wait_until_paused(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut state = self
            .barrier
            .state
            .lock()
            .expect("scheduled-promotion barrier is usable");
        while !state.paused && !state.released {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                return false;
            };
            let (next, result) = self
                .barrier
                .changed
                .wait_timeout(state, remaining)
                .expect("scheduled-promotion barrier is usable");
            state = next;
            if result.timed_out() && !state.paused {
                return false;
            }
        }
        state.paused
    }

    /// Releases the worker from this controller's installed promotion stage.
    pub fn release(&self) {
        let mut state = self
            .barrier
            .state
            .lock()
            .expect("scheduled-promotion barrier is usable");
        state.released = true;
        self.barrier.changed.notify_all();
    }
}

impl Drop for ScheduledPromotionBarrierController {
    fn drop(&mut self) {
        self.release();
        let mut barriers = barriers()
            .lock()
            .expect("scheduled-promotion barrier registry is usable");
        if barriers
            .get(&(self.thread_id, self.stage))
            .is_some_and(|barrier| barrier.token == self.barrier.token)
        {
            barriers.remove(&(self.thread_id, self.stage));
        }
    }
}

impl ProjectionConnectionRetirementHandle {
    pub(in crate::cas_projection) fn new(connection: Arc<ProjectionConnection>) -> Self {
        Self { connection }
    }

    /// Retires and joins the exact connection, waiting for reserved promotion to release.
    pub fn retire(&self) {
        self.connection.retire();
    }

    /// Returns whether retirement has fenced this connection.
    #[must_use]
    pub fn is_retired(&self) -> bool {
        self.connection.is_retired()
    }

    /// Returns whether terminal shutdown joined the driver and detached forwarding.
    #[must_use]
    pub fn is_detached(&self) -> bool {
        self.connection.is_detached()
    }

    /// Poisons the exact attachment mutex while leaving its ingester handle recoverable.
    pub fn poison_ingester_handle(&self) {
        self.connection.poison_ingester_handle_for_test();
    }

    /// Forces the exact provider ingester's next terminal receipt to be unclean.
    pub fn fail_next_ingester_join(&self) {
        self.connection.fail_next_ingester_join_for_test();
    }
}

pub(crate) fn pause_scheduled_promotion(thread_id: SyndicThreadId) {
    pause_scheduled_promotion_at(thread_id, PromotionBarrierStage::Reserved);
}

pub(crate) fn pause_scheduled_promotion_reservation(thread_id: SyndicThreadId) {
    pause_scheduled_promotion_at(thread_id, PromotionBarrierStage::BeforeReservation);
}

pub(crate) fn pause_scheduled_promotion_reconciliation(thread_id: SyndicThreadId) {
    pause_scheduled_promotion_at(thread_id, PromotionBarrierStage::BeforeReconciliation);
}

pub(crate) fn pause_scheduled_generation_invalidation(thread_id: SyndicThreadId) {
    pause_scheduled_promotion_at(thread_id, PromotionBarrierStage::GenerationInvalidated);
}

fn pause_scheduled_promotion_at(thread_id: SyndicThreadId, stage: PromotionBarrierStage) {
    let barrier = barriers()
        .lock()
        .expect("scheduled-promotion barrier registry is usable")
        .get(&(thread_id, stage))
        .cloned();
    let Some(barrier) = barrier else {
        return;
    };
    let mut state = barrier
        .state
        .lock()
        .expect("scheduled-promotion barrier is usable");
    state.paused = true;
    barrier.changed.notify_all();
    while !state.released {
        state = barrier
            .changed
            .wait(state)
            .expect("scheduled-promotion barrier is usable");
    }
}

type PromotionBarrierKey = (SyndicThreadId, PromotionBarrierStage);

fn barriers() -> &'static Mutex<HashMap<PromotionBarrierKey, Arc<PromotionBarrier>>> {
    static BARRIERS: OnceLock<Mutex<HashMap<PromotionBarrierKey, Arc<PromotionBarrier>>>> =
        OnceLock::new();
    BARRIERS.get_or_init(|| Mutex::new(HashMap::new()))
}
