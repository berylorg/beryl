use beryl_model::SyndicThreadId;
use std::{
    sync::{
        Mutex, OnceLock,
        mpsc::{Receiver, SyncSender, sync_channel},
    },
    time::Duration,
};

struct Pending {
    cleanup: bool,
    thread: SyndicThreadId,
    arrived: SyncSender<()>,
    release: Receiver<bool>,
}

static PENDING: OnceLock<Mutex<Option<Pending>>> = OnceLock::new();

pub struct StopHandoffBarrierController {
    thread: SyndicThreadId,
    arrived: Receiver<()>,
    release: SyncSender<bool>,
}

pub fn install_stop_handoff_barrier(thread: SyndicThreadId) -> StopHandoffBarrierController {
    install(thread, false)
}

pub fn install_stop_cleanup_barrier(thread: SyndicThreadId) -> StopHandoffBarrierController {
    install(thread, true)
}

fn install(thread: SyndicThreadId, cleanup: bool) -> StopHandoffBarrierController {
    let (arrived, arrival) = sync_channel(1);
    let (release, released) = sync_channel(1);
    let mut pending = PENDING.get_or_init(|| Mutex::new(None)).lock().unwrap();
    assert!(pending.is_none());
    *pending = Some(Pending {
        cleanup,
        thread,
        arrived,
        release: released,
    });
    StopHandoffBarrierController {
        thread,
        arrived: arrival,
        release,
    }
}

impl StopHandoffBarrierController {
    pub fn wait(&self) {
        self.arrived.recv_timeout(Duration::from_secs(10)).unwrap();
    }

    pub fn release(&self, unwind: bool) {
        let _ = self.release.try_send(unwind);
    }
}

impl Drop for StopHandoffBarrierController {
    fn drop(&mut self) {
        self.release(false);
        let mut pending = PENDING.get_or_init(|| Mutex::new(None)).lock().unwrap();
        if pending
            .as_ref()
            .is_some_and(|pending| pending.thread == self.thread)
        {
            *pending = None;
        }
    }
}

pub(crate) fn pause_stop_handoff(thread: SyndicThreadId) {
    pause(thread, false);
}

pub(crate) fn pause_stop_cleanup(thread: SyndicThreadId) {
    pause(thread, true);
}

fn pause(thread: SyndicThreadId, cleanup: bool) {
    let pending = {
        let mut pending = PENDING.get_or_init(|| Mutex::new(None)).lock().unwrap();
        if pending
            .as_ref()
            .is_some_and(|pending| pending.thread == thread && pending.cleanup == cleanup)
        {
            pending.take()
        } else {
            None
        }
    };
    if let Some(pending) = pending {
        let _ = pending.arrived.send(());
        if pending.release.recv_timeout(Duration::from_secs(15)) == Ok(true) {
            panic!("requested admitted stop handoff unwind");
        }
    }
}
