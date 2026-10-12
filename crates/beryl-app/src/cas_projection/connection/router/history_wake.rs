use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};

#[derive(Debug, Default)]
pub(super) struct HistoryWake {
    consumer: Mutex<Option<std::thread::Thread>>,
    changed: AtomicBool,
}

impl HistoryWake {
    pub(super) fn arm(&self) {
        *self
            .consumer
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Some(std::thread::current());
    }

    pub(super) fn notify(&self) {
        if let Some(consumer) = self
            .consumer
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .as_ref()
        {
            consumer.unpark();
        }
    }

    pub(super) fn published(&self) {
        self.changed.store(true, Ordering::Release);
        self.notify();
    }

    pub(super) fn take_changed(&self) -> bool {
        self.changed.swap(false, Ordering::AcqRel)
    }
}

impl super::LiveEventTarget {
    pub(in crate::cas_projection) fn arm_history_wake(&self) {
        self.registration().history_wake.arm();
    }

    pub(in crate::cas_projection) fn take_history_changed(&self) -> bool {
        self.registration().history_wake.take_changed()
    }
}
