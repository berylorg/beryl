use std::sync::{
    Mutex, OnceLock,
    atomic::Ordering,
    mpsc::{Receiver, SyncSender, sync_channel},
};

use beryl_model::SyndicThreadId;

use super::NEXT_TOKEN;

static RECOVERY_SOURCE_BARRIER: OnceLock<Mutex<Option<RecoverySourceBarrier>>> = OnceLock::new();

struct RecoverySourceBarrier {
    token: u64,
    thread_id: SyndicThreadId,
    after_pages: u64,
    stage: RecoveryBarrierStage,
    arrived: SyncSender<()>,
    release: Receiver<()>,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum RecoveryBarrierStage {
    BeforeRead,
    PageHandoff,
}

/// One exact test-only pause in a recovery source after a chosen page count.
pub struct RecoverySourceBarrierController {
    token: u64,
    arrived: Receiver<()>,
    release: SyncSender<()>,
}

/// Installs one exact-thread recovery-source barrier for deterministic fault tests.
pub fn install_recovery_source_barrier(
    thread_id: SyndicThreadId,
    after_pages: u64,
) -> RecoverySourceBarrierController {
    install_barrier(thread_id, after_pages, RecoveryBarrierStage::BeforeRead)
}

/// Installs a pause after one exact page is filled and while its transferable lease is live.
pub fn install_recovery_page_handoff_barrier(
    thread_id: SyndicThreadId,
    after_pages: u64,
) -> RecoverySourceBarrierController {
    install_barrier(thread_id, after_pages, RecoveryBarrierStage::PageHandoff)
}

fn install_barrier(
    thread_id: SyndicThreadId,
    after_pages: u64,
    stage: RecoveryBarrierStage,
) -> RecoverySourceBarrierController {
    let token = NEXT_TOKEN.fetch_add(1, Ordering::Relaxed);
    let (arrived, arrival) = sync_channel(1);
    let (release, released) = sync_channel(1);
    *barrier().lock().expect("recovery fault barrier is usable") = Some(RecoverySourceBarrier {
        token,
        thread_id,
        after_pages,
        stage,
        arrived,
        release: released,
    });
    RecoverySourceBarrierController {
        token,
        arrived: arrival,
        release,
    }
}

/// Installs a pause after the fresh target is idle and before its cursor is opened.
pub fn install_recovery_cursor_open_barrier(
    thread_id: SyndicThreadId,
) -> RecoverySourceBarrierController {
    install_recovery_source_barrier(thread_id, u64::MAX)
}

impl RecoverySourceBarrierController {
    /// Waits until the exact source reaches the installed page boundary.
    pub fn wait(&self) {
        self.arrived
            .recv()
            .expect("recovery source stopped before the fault barrier");
    }

    /// Releases the paused source after the test changes cancellation or storage state.
    pub fn release(&self) {
        self.release
            .send(())
            .expect("recovery source stopped while paused at the fault barrier");
    }
}

impl Drop for RecoverySourceBarrierController {
    fn drop(&mut self) {
        let mut slot = barrier().lock().expect("recovery fault barrier is usable");
        if slot.as_ref().is_some_and(|entry| entry.token == self.token) {
            *slot = None;
        }
    }
}

pub(crate) fn pause_recovery_source(thread_id: SyndicThreadId, served_pages: u64) {
    pause_recovery_barrier(thread_id, served_pages, RecoveryBarrierStage::BeforeRead);
}

pub(crate) fn pause_recovery_page_handoff(thread_id: SyndicThreadId, served_pages: u64) {
    pause_recovery_barrier(thread_id, served_pages, RecoveryBarrierStage::PageHandoff);
}

fn pause_recovery_barrier(
    thread_id: SyndicThreadId,
    served_pages: u64,
    stage: RecoveryBarrierStage,
) {
    let pending = {
        let mut slot = barrier().lock().expect("recovery fault barrier is usable");
        if slot.as_ref().is_some_and(|entry| {
            entry.thread_id == thread_id
                && entry.after_pages == served_pages
                && entry.stage == stage
        }) {
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

fn barrier() -> &'static Mutex<Option<RecoverySourceBarrier>> {
    RECOVERY_SOURCE_BARRIER.get_or_init(|| Mutex::new(None))
}
