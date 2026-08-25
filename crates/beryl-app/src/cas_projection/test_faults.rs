use std::sync::{
    Arc, Condvar, Mutex, OnceLock,
    atomic::{AtomicU64, Ordering},
    mpsc::{Receiver, SyncSender, sync_channel},
};

mod promotion;
mod provider;
mod recovery;
mod scheduler;
mod target;
mod terminal_history;

pub use promotion::{
    ProjectionConnectionRetirementHandle, ScheduledPromotionBarrierController,
    install_scheduled_generation_invalidation_barrier, install_scheduled_promotion_barrier,
    install_scheduled_promotion_reconciliation_barrier,
    install_scheduled_promotion_reservation_barrier,
};
pub(crate) use promotion::{
    pause_scheduled_generation_invalidation, pause_scheduled_promotion,
    pause_scheduled_promotion_reconciliation, pause_scheduled_promotion_reservation,
};
pub use provider::*;
pub use recovery::{
    RecoverySourceBarrierController, install_recovery_cursor_open_barrier,
    install_recovery_page_handoff_barrier, install_recovery_source_barrier,
};
pub(crate) use recovery::{pause_recovery_page_handoff, pause_recovery_source};
pub use scheduler::AcceptedInputSchedulerPanicController;
#[cfg(test)]
pub(in crate::cas_projection) use scheduler::{
    AcceptedInputSchedulerWorkerController, AcceptedInputSchedulerWorkerRequest,
    install_accepted_input_scheduler_worker, take_accepted_input_scheduler_worker,
};
pub(crate) use scheduler::{
    install_accepted_input_scheduler_panic, observe_accepted_input_scheduler_join,
    panic_accepted_input_scheduler_main_if_requested,
};
pub(crate) use target::abandon_live_event_target_if_requested;
pub use target::{LiveEventTargetAbandonmentController, install_live_event_target_abandonment};
pub(crate) use terminal_history::pause_terminal_history;
pub use terminal_history::{
    TerminalHistoryBarrierController, TerminalHistoryBarrierStage, install_terminal_history_barrier,
};

pub fn signal_accepted_ready(service: &super::ProjectionConnectionService) {
    service.signal_accepted_ready_for_test();
}

static NEXT_TOKEN: AtomicU64 = AtomicU64::new(1);
static APPROVAL_INSTALL_BARRIER: OnceLock<Mutex<Option<ApprovalInstallBarrier>>> = OnceLock::new();
static APPROVAL_SUBMIT_BARRIER: OnceLock<Mutex<Option<ApprovalSubmitBarrier>>> = OnceLock::new();
static APPROVAL_SLOT_BARRIER: OnceLock<Mutex<Option<ApprovalSlotBarrier>>> = OnceLock::new();

struct ApprovalInstallBarrier {
    token: u64,
    thread_id: beryl_model::CasThreadId,
    slot_id: Option<usize>,
    arrived: SyncSender<()>,
    release: Arc<(Mutex<bool>, Condvar)>,
}

struct ApprovalSubmitBarrier {
    token: u64,
    thread_id: beryl_model::CasThreadId,
    broker_id: Option<usize>,
    arrived: SyncSender<()>,
    release: Arc<(Mutex<bool>, Condvar)>,
    cancellation_observed: SyncSender<()>,
}

struct ApprovalSlotBarrier {
    token: u64,
    thread_id: beryl_model::CasThreadId,
    arrived: SyncSender<()>,
    release: Receiver<()>,
}

/// Controls one exact pause after approval queue admission and before obligation installation.
pub struct ApprovalInstallBarrierController {
    token: u64,
    arrived: Receiver<()>,
    release: Arc<(Mutex<bool>, Condvar)>,
}

/// Controls one exact pause before an approval enters the ordered broker channel.
pub struct ApprovalSubmitBarrierController {
    token: u64,
    arrived: Receiver<()>,
    release: Arc<(Mutex<bool>, Condvar)>,
    cancellation_observed: Receiver<()>,
}

/// Controls one exact pause before permission-obligation slot admission.
pub struct ApprovalSlotBarrierController {
    token: u64,
    arrived: Receiver<()>,
    release: SyncSender<()>,
}

/// Installs one exact-thread approval-install pause for cancellation linearization tests.
pub fn install_approval_install_barrier(
    thread_id: beryl_model::CasThreadId,
) -> ApprovalInstallBarrierController {
    let token = NEXT_TOKEN.fetch_add(1, Ordering::Relaxed);
    let (arrived, arrival) = sync_channel(1);
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    *approval_barrier()
        .lock()
        .expect("approval fault barrier is usable") = Some(ApprovalInstallBarrier {
        token,
        thread_id,
        slot_id: None,
        arrived,
        release: Arc::clone(&release),
    });
    ApprovalInstallBarrierController {
        token,
        arrived: arrival,
        release,
    }
}

/// Installs one exact-thread pause before ordered approval delivery.
pub fn install_approval_submit_barrier(
    thread_id: beryl_model::CasThreadId,
) -> ApprovalSubmitBarrierController {
    let token = NEXT_TOKEN.fetch_add(1, Ordering::Relaxed);
    let (arrived, arrival) = sync_channel(1);
    let (cancellation_observed, cancellation) = sync_channel(1);
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    *approval_submit_barrier()
        .lock()
        .expect("approval submit fault barrier is usable") = Some(ApprovalSubmitBarrier {
        token,
        thread_id,
        broker_id: None,
        arrived,
        release: Arc::clone(&release),
        cancellation_observed,
    });
    ApprovalSubmitBarrierController {
        token,
        arrived: arrival,
        release,
        cancellation_observed: cancellation,
    }
}

/// Installs one exact-thread pause before permission-obligation slot admission.
pub fn install_approval_slot_barrier(
    thread_id: beryl_model::CasThreadId,
) -> ApprovalSlotBarrierController {
    let token = NEXT_TOKEN.fetch_add(1, Ordering::Relaxed);
    let (arrived, arrival) = sync_channel(1);
    let (release, released) = sync_channel(1);
    *approval_slot_barrier()
        .lock()
        .expect("approval slot fault barrier is usable") = Some(ApprovalSlotBarrier {
        token,
        thread_id,
        arrived,
        release: released,
    });
    ApprovalSlotBarrierController {
        token,
        arrived: arrival,
        release,
    }
}

/// Abandons only the target receiver while retaining its registered route for failure tests.
pub fn abandon_live_event_receiver(target: &mut super::LiveEventTarget) {
    target.abandon_receiver_for_test();
}

impl ApprovalInstallBarrierController {
    /// Waits until the request is queued before its interruption obligation enters the slot.
    pub fn wait_for_route(&self) {
        self.arrived
            .recv()
            .expect("approval ingestion stopped before the install barrier");
    }

    /// Releases the paused ingester to produce its typed completion.
    pub fn release(&self) {
        let (released, changed) = &*self.release;
        *released.lock().expect("approval fault barrier is usable") = true;
        changed.notify_all();
    }
}

impl ApprovalSubmitBarrierController {
    /// Waits until the request reaches the broker sink but has not entered its channel.
    pub fn wait_for_submit(&self) {
        self.arrived
            .recv()
            .expect("approval submission stopped before the submit barrier");
    }

    /// Waits until the exact paused broker observes cross-thread cancellation.
    pub fn wait_for_cancellation(&self) {
        self.cancellation_observed
            .recv()
            .expect("approval broker closed without reporting cancellation");
    }

    /// Releases the paused submission after the test changes connection state.
    pub fn release(&self) {
        let (released, changed) = &*self.release;
        *released
            .lock()
            .expect("approval submit fault barrier is usable") = true;
        changed.notify_all();
    }
}

impl ApprovalSlotBarrierController {
    /// Waits until the permission request is held before obligation slot admission.
    pub fn wait_for_slot(&self) {
        self.arrived
            .recv()
            .expect("approval ingestion stopped before the slot barrier");
    }

    /// Releases obligation-slot admission after the test changes connection state.
    pub fn release(&self) {
        self.release
            .send(())
            .expect("approval ingestion stopped while paused at the slot barrier");
    }
}

impl Drop for ApprovalInstallBarrierController {
    fn drop(&mut self) {
        let (released, changed) = &*self.release;
        *released.lock().expect("approval fault barrier is usable") = true;
        changed.notify_all();
        let mut slot = approval_barrier()
            .lock()
            .expect("approval fault barrier is usable");
        if slot.as_ref().is_some_and(|entry| entry.token == self.token) {
            *slot = None;
        }
    }
}

impl Drop for ApprovalSubmitBarrierController {
    fn drop(&mut self) {
        let (released, changed) = &*self.release;
        *released
            .lock()
            .expect("approval submit fault barrier is usable") = true;
        changed.notify_all();
        let mut slot = approval_submit_barrier()
            .lock()
            .expect("approval submit fault barrier is usable");
        if slot.as_ref().is_some_and(|entry| entry.token == self.token) {
            *slot = None;
        }
    }
}

impl Drop for ApprovalSlotBarrierController {
    fn drop(&mut self) {
        let _ = self.release.try_send(());
        let mut slot = approval_slot_barrier()
            .lock()
            .expect("approval slot fault barrier is usable");
        if slot.as_ref().is_some_and(|entry| entry.token == self.token) {
            *slot = None;
        }
    }
}

pub(crate) fn pause_approval_install(thread_id: &beryl_model::CasThreadId, slot_id: usize) {
    let pending = {
        let mut slot = approval_barrier()
            .lock()
            .expect("approval fault barrier is usable");
        let Some(entry) = slot.as_mut().filter(|entry| &entry.thread_id == thread_id) else {
            return;
        };
        entry.slot_id = Some(slot_id);
        (entry.arrived.clone(), Arc::clone(&entry.release))
    };
    let _ = pending.0.try_send(());
    let (released, changed) = &*pending.1;
    let mut released = released.lock().expect("approval fault barrier is usable");
    while !*released {
        released = changed
            .wait(released)
            .expect("approval fault barrier is usable");
    }
}

pub(crate) fn pause_approval_submit(thread_id: &beryl_model::CasThreadId, broker_id: usize) {
    let pending = {
        let mut slot = approval_submit_barrier()
            .lock()
            .expect("approval submit fault barrier is usable");
        let Some(entry) = slot.as_mut().filter(|entry| &entry.thread_id == thread_id) else {
            return;
        };
        entry.broker_id = Some(broker_id);
        (entry.arrived.clone(), Arc::clone(&entry.release))
    };
    let _ = pending.0.try_send(());
    let (released, changed) = &*pending.1;
    let mut released = released
        .lock()
        .expect("approval submit fault barrier is usable");
    while !*released {
        released = changed
            .wait(released)
            .expect("approval submit fault barrier is usable");
    }
}

pub(crate) fn pause_approval_slot_admission(thread_id: &beryl_model::CasThreadId) {
    let pending = {
        let mut slot = approval_slot_barrier()
            .lock()
            .expect("approval slot fault barrier is usable");
        if slot
            .as_ref()
            .is_some_and(|entry| &entry.thread_id == thread_id)
        {
            slot.take()
        } else {
            None
        }
    };
    if let Some(pending) = pending {
        let _ = pending.arrived.send(());
        let _ = pending.release.recv();
    }
}

pub(crate) fn observe_approval_submit_cancellation(broker_id: usize) {
    let observed = approval_submit_barrier()
        .lock()
        .expect("approval submit fault barrier is usable")
        .as_ref()
        .filter(|entry| entry.broker_id == Some(broker_id))
        .map(|entry| entry.cancellation_observed.clone());
    if let Some(observed) = observed {
        let _ = observed.try_send(());
    }
}

fn approval_barrier() -> &'static Mutex<Option<ApprovalInstallBarrier>> {
    APPROVAL_INSTALL_BARRIER.get_or_init(|| Mutex::new(None))
}

fn approval_submit_barrier() -> &'static Mutex<Option<ApprovalSubmitBarrier>> {
    APPROVAL_SUBMIT_BARRIER.get_or_init(|| Mutex::new(None))
}

fn approval_slot_barrier() -> &'static Mutex<Option<ApprovalSlotBarrier>> {
    APPROVAL_SLOT_BARRIER.get_or_init(|| Mutex::new(None))
}
