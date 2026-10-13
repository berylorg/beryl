use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc, Weak,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll},
};

#[derive(Clone, Default)]
pub(in crate::main_window) struct WorkerLifetime(Arc<()>, Arc<AtomicBool>);

#[derive(Clone, Default)]
pub(crate) struct WorkerAdmission {
    lifetime: Weak<()>,
    suspended: Weak<AtomicBool>,
    maximum: usize,
}

pub(crate) struct WorkerLease {
    _lifetime: Arc<()>,
}

impl WorkerAdmission {
    #[cfg(test)]
    pub(crate) fn owned_for_test(maximum: usize) -> (Self, TestWorkerOwner) {
        let owner = TestWorkerOwner(WorkerLifetime::default());
        (owner.0.admission(maximum), owner)
    }

    pub(crate) fn suspended(&self) -> Option<bool> {
        self.suspended
            .upgrade()
            .map(|suspended| suspended.load(Ordering::Acquire))
    }
    pub(crate) fn available(&self) -> bool {
        self.suspended() == Some(false)
            && self.lifetime.strong_count().saturating_sub(1) < self.maximum
    }

    pub(crate) fn acquire(&self) -> Option<WorkerLease> {
        if !self.available() {
            return None;
        }
        let lifetime = self.lifetime.upgrade()?;
        // Admissions run serially on the owning foreground thread.
        if Arc::strong_count(&lifetime).saturating_sub(1) > self.maximum {
            return None;
        }
        Some(WorkerLease {
            _lifetime: lifetime,
        })
    }
}

#[cfg(test)]
pub(crate) struct TestWorkerOwner(WorkerLifetime);

#[cfg(test)]
impl TestWorkerOwner {
    pub(crate) fn suspend(&self) {
        self.0.suspend(true);
    }

    pub(crate) fn retained(&self) -> usize {
        self.0.retained()
    }
}

#[cfg(test)]
impl Drop for TestWorkerOwner {
    fn drop(&mut self) {
        self.suspend();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_save_worker_admission_leaves_read_capacity_and_fences_new_work() {
        let lifetime = WorkerLifetime::default();
        let admission = lifetime.admission(7);
        let mut leases = Vec::new();
        for _ in 0..7 {
            leases.push(admission.acquire().unwrap());
        }
        assert_eq!(lifetime.retained(), 7);
        assert!(!admission.available());
        assert!(admission.acquire().is_none());
        leases.pop();
        assert!(admission.available());
        lifetime.suspend(true);
        assert!(admission.acquire().is_none());
        leases.clear();
        assert_eq!(lifetime.retained(), 0);
        lifetime.suspend(false);
        let lease = admission.acquire().unwrap();
        drop(lifetime);
        assert!(admission.acquire().is_none());
        drop(lease);
        assert_eq!(admission.lifetime.strong_count(), 0);
    }
}

impl WorkerLifetime {
    pub(in crate::main_window) fn admission(&self, maximum: usize) -> WorkerAdmission {
        WorkerAdmission {
            lifetime: Arc::downgrade(&self.0),
            suspended: Arc::downgrade(&self.1),
            maximum,
        }
    }

    pub(in crate::main_window) fn suspend(&self, suspended: bool) {
        self.1.store(suspended, Ordering::Release);
    }
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
