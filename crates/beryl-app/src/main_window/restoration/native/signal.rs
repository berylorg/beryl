use std::{
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    task::{Poll, Waker},
};

#[derive(Default)]
pub(super) struct StartupWake {
    version: AtomicU64,
    waiter: Mutex<Option<Waker>>,
}

impl StartupWake {
    pub(super) fn version(&self) -> u64 {
        self.version.load(Ordering::Acquire)
    }

    pub(super) fn notify(&self) {
        self.version.fetch_add(1, Ordering::AcqRel);
        let waiter = self
            .waiter
            .lock()
            .expect("startup notification lock")
            .take();
        if let Some(waiter) = waiter {
            waiter.wake();
        }
    }

    pub(super) async fn wait(&self, observed: u64) {
        std::future::poll_fn(|cx| {
            let mut waiter = self.waiter.lock().expect("startup notification lock");
            if self.version() != observed {
                *waiter = None;
                Poll::Ready(())
            } else {
                *waiter = Some(cx.waker().clone());
                Poll::Pending
            }
        })
        .await
    }
}
