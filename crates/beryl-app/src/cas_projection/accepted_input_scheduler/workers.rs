use std::thread::JoinHandle;

use super::{ActiveSteeringRetryState, SchedulerFailure, SchedulerRuntime, WorkerDisposition};

pub(super) enum WorkerKind {
    Steering,
    Next(beryl_model::SyndicThreadId),
}

pub(super) struct WorkerRecord {
    pub(super) handle: JoinHandle<WorkerDisposition>,
    pub(super) thread_id: std::thread::ThreadId,
    kind: WorkerKind,
}

impl SchedulerRuntime {
    pub(super) fn register_steering_worker(&mut self, handle: JoinHandle<WorkerDisposition>) {
        self.register_worker(handle, WorkerKind::Steering);
    }

    pub(super) fn register_next_worker(
        &mut self,
        handle: JoinHandle<WorkerDisposition>,
        syndic_thread_id: beryl_model::SyndicThreadId,
    ) {
        self.register_worker(handle, WorkerKind::Next(syndic_thread_id));
    }

    fn register_worker(&mut self, handle: JoinHandle<WorkerDisposition>, kind: WorkerKind) {
        let thread_id = handle.thread().id();
        self.workers.push(WorkerRecord {
            handle,
            thread_id,
            kind,
        });
        let active = self.workers.len();
        self.context.signal.update_diagnostics(|diagnostics| {
            diagnostics.workers_started = diagnostics.workers_started.saturating_add(1);
            diagnostics.workers_active = active;
            diagnostics.workers_high_water = diagnostics.workers_high_water.max(active);
        });
    }

    pub(super) fn has_active_steering_worker(&self) -> bool {
        self.workers
            .iter()
            .any(|worker| matches!(worker.kind, WorkerKind::Steering))
    }

    pub(super) fn has_active_next_worker(
        &self,
        syndic_thread_id: beryl_model::SyndicThreadId,
    ) -> bool {
        self.workers.iter().any(
            |worker| matches!(worker.kind, WorkerKind::Next(active) if active == syndic_thread_id),
        )
    }

    pub(super) fn drain_completions(&mut self) -> (bool, bool) {
        let mut recovered_pending_worker_ready = false;
        let mut next_worker_ready = false;
        for completion in self.completions.drain() {
            let Some(index) = self
                .workers
                .iter()
                .position(|worker| worker.thread_id == completion.thread_id)
            else {
                self.fail_closed(SchedulerFailure::Fatal);
                continue;
            };
            let worker = self.workers.swap_remove(index);
            let result = worker.handle.join();
            self.record_worker_join();
            match result {
                Ok(disposition) => {
                    let (recovered_pending_ready, next_ready) =
                        self.apply_worker_disposition(disposition);
                    recovered_pending_worker_ready |= recovered_pending_ready;
                    next_worker_ready |= next_ready;
                }
                Ok(_) | Err(_) => self.fail_closed(SchedulerFailure::Fatal),
            }
        }
        (recovered_pending_worker_ready, next_worker_ready)
    }

    pub(super) fn join_all_workers(&mut self) {
        while let Some(worker) = self.workers.pop() {
            let result = worker.handle.join();
            self.record_worker_join();
            match result {
                Ok(disposition) => {
                    let _ = self.apply_worker_disposition(disposition);
                }
                Err(_) => self.fail_closed(SchedulerFailure::Fatal),
            }
        }
        let _ = self.completions.drain();
    }

    fn record_worker_join(&mut self) {
        self.context.signal.update_diagnostics(|diagnostics| {
            diagnostics.workers_joined = diagnostics.workers_joined.saturating_add(1);
            diagnostics.workers_active = diagnostics.workers_active.saturating_sub(1);
        });
    }

    fn apply_worker_disposition(&mut self, disposition: WorkerDisposition) -> (bool, bool) {
        match disposition {
            WorkerDisposition::Parked => {
                self.parked_retry = true;
                self.retry_pass_active = false;
                self.scan = None;
                self.context.signal.update_diagnostics(|diagnostics| {
                    diagnostics.retry_state = ActiveSteeringRetryState::Parked;
                });
                (false, false)
            }
            WorkerDisposition::Settled => (false, false),
            WorkerDisposition::RecoveredPendingContinue => (
                true,
                self.next_capacity_waiting || self.next_active_worker_waiting,
            ),
            WorkerDisposition::NextContinue => (self.recovered_pending_capacity_waiting, true),
            WorkerDisposition::NextParked => (false, false),
            WorkerDisposition::PersistentHomeFailure => {
                self.fail_closed(SchedulerFailure::PersistentHomeFailure);
                (false, false)
            }
            WorkerDisposition::CommandNotCommitted(_)
            | WorkerDisposition::CommandCommitted { .. }
            | WorkerDisposition::CommandIndeterminate { .. } => {
                self.fail_closed(SchedulerFailure::PersistentHomeFailure);
                (false, false)
            }
            WorkerDisposition::Fatal => {
                self.fail_closed(SchedulerFailure::Fatal);
                (false, false)
            }
        }
    }
}
