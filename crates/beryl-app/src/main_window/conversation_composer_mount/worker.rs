use std::sync::Arc;

#[derive(Default)]
pub(super) struct WorkerLifetime(Arc<()>);

impl WorkerLifetime {
    pub(super) fn retained(&self) -> usize {
        Arc::strong_count(&self.0) - 1
    }

    pub(super) fn track<F>(&self, job: F) -> ResourceWorker<F> {
        ResourceWorker {
            job: Some(job),
            _lifetime: self.0.clone(),
        }
    }
}

pub(super) struct ResourceWorker<F> {
    // Captured resources drop before the lifetime witness, including unstarted work.
    job: Option<F>,
    _lifetime: Arc<()>,
}

impl<F> ResourceWorker<F> {
    pub(super) fn run<R>(mut self) -> R
    where
        F: FnOnce() -> R,
    {
        self.job.take().unwrap()()
    }
}
