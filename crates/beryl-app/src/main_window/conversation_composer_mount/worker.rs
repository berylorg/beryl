use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

#[derive(Clone, Default)]
pub(in crate::main_window) struct WorkerLifetime(Arc<()>);

impl WorkerLifetime {
    pub(in crate::main_window) fn retained(&self) -> usize {
        Arc::strong_count(&self.0) - 1
    }

    pub(in crate::main_window) fn track<F>(&self, job: F) -> ResourceWorker<F> {
        ResourceWorker {
            job: Some(job),
            _lifetime: self.0.clone(),
        }
    }

    pub(super) fn track_future<F: Future>(&self, future: F) -> ResourceWorker<Pin<Box<F>>> {
        self.track(Box::pin(future))
    }
}

pub(in crate::main_window) struct ResourceWorker<F> {
    // Captured resources drop before the lifetime witness, including unstarted work.
    job: Option<F>,
    _lifetime: Arc<()>,
}

impl<F: Future> Future for ResourceWorker<Pin<Box<F>>> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.get_mut().job.as_mut().unwrap().as_mut().poll(cx)
    }
}

impl<F> ResourceWorker<F> {
    pub(in crate::main_window) fn run_with<A, R>(mut self, argument: A) -> R
    where
        F: FnOnce(A) -> R,
    {
        self.job.take().unwrap()(argument)
    }

    pub(in crate::main_window) fn run<R>(mut self) -> R
    where
        F: FnOnce() -> R,
    {
        self.job.take().unwrap()()
    }
}
