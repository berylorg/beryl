use std::sync::{
    Mutex, OnceLock,
    atomic::Ordering,
    mpsc::{Receiver, SyncSender, sync_channel},
};

use beryl_model::SyndicThreadId;

use super::NEXT_TOKEN;

pub fn converge_terminal_history_candidate(
    store: &beryl_home_store::HomeCandidateRecoveryAccess<'_>,
    storage: &syndic_storage::SyndicStorage,
    thread: beryl_model::SyndicThreadId,
    turn: beryl_model::SyndicTurnId,
    observed_at: syndic_storage::SyndicTimestamp,
    limit: syndic_storage::SyndicPointReadLimit,
) -> Result<(), crate::cas_projection::OrdinaryTurnExecutionError> {
    crate::cas_projection::ordinary::converge_terminal_history_candidate(
        store,
        storage,
        thread,
        turn,
        observed_at,
        limit,
    )
}

pub fn converge_terminal_history(
    store: &beryl_home_store::HomeStore,
    storage: &syndic_storage::SyndicStorage,
    thread: beryl_model::SyndicThreadId,
    turn: beryl_model::SyndicTurnId,
    observed_at: syndic_storage::SyndicTimestamp,
    limit: syndic_storage::SyndicPointReadLimit,
) -> Result<(), crate::cas_projection::OrdinaryTurnExecutionError> {
    crate::cas_projection::ordinary::converge_terminal_history(
        store,
        storage,
        thread,
        turn,
        observed_at,
        limit,
        None,
    )
}

pub struct TerminalCompletionProbe(
    pub(crate) crate::cas_projection::service::TerminalCompletionObserver,
);

impl TerminalCompletionProbe {
    pub fn turn_id(&self) -> beryl_model::SyndicTurnId {
        self.0.turn_id()
    }

    pub fn lifecycle(&self) -> Option<syndic_storage::TurnLifecycle> {
        self.0.completion().map(|proof| proof.lifecycle())
    }
}

static TERMINAL_HISTORY_BARRIER: OnceLock<Mutex<Option<TerminalHistoryBarrier>>> = OnceLock::new();

struct TerminalHistoryBarrier {
    token: u64,
    thread_id: SyndicThreadId,
    stage: TerminalHistoryBarrierStage,
    arrived: SyncSender<()>,
    release: Receiver<()>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalHistoryBarrierStage {
    AfterItems,
    BeforeGateRelease,
    AfterGateCommit,
    AfterGateRelease,
}

pub struct TerminalHistoryBarrierController {
    token: u64,
    arrived: Receiver<()>,
    release: SyncSender<()>,
}

pub fn install_terminal_history_barrier(
    thread_id: SyndicThreadId,
    stage: TerminalHistoryBarrierStage,
) -> TerminalHistoryBarrierController {
    let token = NEXT_TOKEN.fetch_add(1, Ordering::Relaxed);
    let (arrived, arrival) = sync_channel(1);
    let (release, released) = sync_channel(1);
    *barrier()
        .lock()
        .expect("terminal-history fault barrier is usable") = Some(TerminalHistoryBarrier {
        token,
        thread_id,
        stage,
        arrived,
        release: released,
    });
    TerminalHistoryBarrierController {
        token,
        arrived: arrival,
        release,
    }
}

impl TerminalHistoryBarrierController {
    pub fn wait(&self) {
        self.arrived
            .recv()
            .expect("terminal-history convergence stopped before the fault barrier");
    }

    pub fn release(&self) {
        self.release
            .send(())
            .expect("terminal-history convergence stopped while paused at the fault barrier");
    }
}

impl Drop for TerminalHistoryBarrierController {
    fn drop(&mut self) {
        let mut slot = barrier()
            .lock()
            .expect("terminal-history fault barrier is usable");
        if slot.as_ref().is_some_and(|entry| entry.token == self.token) {
            *slot = None;
        }
    }
}

pub(crate) fn pause_terminal_history(
    thread_id: SyndicThreadId,
    stage: TerminalHistoryBarrierStage,
) {
    let pending = {
        let mut slot = barrier()
            .lock()
            .expect("terminal-history fault barrier is usable");
        if slot
            .as_ref()
            .is_some_and(|entry| entry.thread_id == thread_id && entry.stage == stage)
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

fn barrier() -> &'static Mutex<Option<TerminalHistoryBarrier>> {
    TERMINAL_HISTORY_BARRIER.get_or_init(|| Mutex::new(None))
}
