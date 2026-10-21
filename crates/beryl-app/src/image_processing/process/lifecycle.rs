use super::*;

impl ImageMediaAttachment {
    pub(crate) fn id(&self) -> u64 {
        self.inner.id
    }
    pub(crate) fn is_retired(&self) -> bool {
        self.inner.retired.load(Ordering::Acquire)
    }
    pub(crate) fn changed(&self) -> ImageMediaChanged {
        self.inner.event.changed(false)
    }
    pub(crate) fn retire(&self) -> ImageResourceDrain {
        retire_attachment(&self.inner)
    }
    pub(crate) fn poll_drain(&self) -> ImageResourceDrain {
        if !self.inner.retired.load(Ordering::Acquire) {
            return ImageResourceDrain::Pending;
        }
        if self.inner.active_workers.load(Ordering::Acquire) != 0 {
            return ImageResourceDrain::Pending;
        }
        if self.inner.consumer.domain().failure().is_some() {
            return ImageResourceDrain::Pending;
        }
        self.inner.consumer.retire()
    }
}

impl Drop for ImageMediaAttachment {
    fn drop(&mut self) {
        self.retire();
    }
}

pub(super) fn retire_attachment(attachment: &Arc<AttachmentState>) -> ImageResourceDrain {
    attachment.retired.store(true, Ordering::Release);
    if let Some(core) = attachment.core.upgrade() {
        let windows = {
            let mut state = lock(&core.state);
            state.queue.retain(|queued| {
                !queued
                    .attachment
                    .upgrade()
                    .is_some_and(|entry| entry.id == attachment.id)
            });
            let mut prepared = Vec::new();
            for (identity, flight) in &mut state.flights {
                if identity.attachment == attachment.id {
                    flight.cancellation.cancel();
                    if let Some(cancellation) = &flight.upload_cancellation {
                        cancellation.cancel();
                    }
                    if matches!(flight.phase, FlightPhase::Prepared(_)) {
                        prepared.push(*identity);
                    }
                }
            }
            for identity in prepared {
                state.flights.remove(&identity);
            }
            lock(&attachment.source).take();
            state
                .windows
                .values()
                .filter_map(Weak::upgrade)
                .filter(|window| {
                    window
                        .attachment
                        .upgrade()
                        .is_some_and(|entry| entry.id == attachment.id)
                })
                .collect::<Vec<_>>()
        };
        for window in windows {
            presentation::close_window(&window);
        }
        core.cpu.retire_attachment(attachment.id);
        core.work.notify_all();
        core.event.notify();
    }
    attachment.event.notify();
    let native = attachment.consumer.retire();
    if attachment.consumer.domain().failure().is_some() {
        return ImageResourceDrain::Pending;
    }
    if attachment.active_workers.load(Ordering::Acquire) == 0 {
        native
    } else {
        ImageResourceDrain::Pending
    }
}

impl ImageMediaProcess {
    pub(crate) fn begin_shutdown(&self) -> ImageResourceDrain {
        shutdown_core(&self.core);
        self.poll_shutdown()
    }

    pub(crate) fn poll_shutdown(&self) -> ImageResourceDrain {
        if self.core.domain.failure().is_some() {
            return ImageResourceDrain::Pending;
        }
        if !lock(&self.core.state).stopping {
            return ImageResourceDrain::Pending;
        }
        let mut workers = lock(&self.workers);
        let mut index = 0;
        while index < workers.len() {
            if workers[index].is_finished() {
                let handle = workers.swap_remove(index);
                if handle.join().is_err() {
                    let mut state = lock(&self.core.state);
                    state.failures = state.failures.saturating_add(1);
                }
            } else {
                index += 1;
            }
        }
        let state = lock(&self.core.state);
        if !workers.is_empty()
            || state.active_workers != 0
            || !state.queue.is_empty()
            || !state.flights.is_empty()
        {
            return ImageResourceDrain::Pending;
        }
        drop(state);
        self.core.domain.retire()
    }
}

pub(super) fn shutdown_core(core: &Arc<ProcessCore>) {
    let attachment = {
        let mut state = lock(&core.state);
        state.stopping = true;
        state.attachment.upgrade()
    };
    if let Some(attachment) = attachment {
        retire_attachment(&attachment);
    }
    core.work.notify_all();
    core.event.notify();
}

impl Drop for ImageMediaProcess {
    fn drop(&mut self) {
        shutdown_core(&self.core);
        let workers = self
            .workers
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for worker in workers.drain(..) {
            let _ = worker.join();
        }
    }
}
