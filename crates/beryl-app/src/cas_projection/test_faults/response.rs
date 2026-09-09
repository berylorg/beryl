use beryl_model::SyndicThreadId;
use std::sync::{
    Mutex, OnceLock,
    mpsc::{Receiver, SyncSender, sync_channel},
};
use std::time::Duration;

struct Pending {
    thread: SyndicThreadId,
    arrived: SyncSender<()>,
    released: Receiver<()>,
}

static PENDING: OnceLock<Mutex<Option<Pending>>> = OnceLock::new();

pub struct ResponseWriteBarrierController {
    thread: SyndicThreadId,
    arrived: Receiver<()>,
    release: SyncSender<()>,
}

pub fn install_response_write_barrier(thread: SyndicThreadId) -> ResponseWriteBarrierController {
    let (arrived, arrival) = sync_channel(1);
    let (release, released) = sync_channel(1);
    let mut pending = PENDING.get_or_init(|| Mutex::new(None)).lock().unwrap();
    assert!(pending.is_none());
    *pending = Some(Pending {
        thread,
        arrived,
        released,
    });
    ResponseWriteBarrierController {
        thread,
        arrived: arrival,
        release,
    }
}

impl ResponseWriteBarrierController {
    pub fn wait(&self) {
        self.arrived.recv_timeout(Duration::from_secs(10)).unwrap();
    }
    pub fn release(&self) {
        let _ = self.release.try_send(());
    }
}

impl Drop for ResponseWriteBarrierController {
    fn drop(&mut self) {
        self.release();
        let mut pending = PENDING.get_or_init(|| Mutex::new(None)).lock().unwrap();
        if pending
            .as_ref()
            .is_some_and(|pending| pending.thread == self.thread)
        {
            *pending = None;
        }
    }
}

pub(crate) fn pause_response_write(thread: SyndicThreadId) {
    let pending = {
        let mut slot = PENDING.get_or_init(|| Mutex::new(None)).lock().unwrap();
        if slot
            .as_ref()
            .is_some_and(|pending| pending.thread == thread)
        {
            slot.take()
        } else {
            None
        }
    };
    if let Some(pending) = pending {
        let _ = pending.arrived.send(());
        let _ = pending.released.recv_timeout(Duration::from_secs(15));
    }
}
