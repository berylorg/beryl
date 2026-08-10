use std::thread::JoinHandle;

use super::{ActiveSteeringRetryState, SchedulerFailure, SchedulerRuntime, WorkerDisposition};

pub(super) enum WorkerKind {
    Steering,
    RecoveredProjection(beryl_model::SyndicThreadId),
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

    pub(super) fn register_recovered_projection_worker(
        &mut self,
        handle: JoinHandle<WorkerDisposition>,
        syndic_thread_id: beryl_model::SyndicThreadId,
    ) {
        self.register_worker(handle, WorkerKind::RecoveredProjection(syndic_thread_id));
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
        self.workers.iter().any(|worker| {
            matches!(
                worker.kind,
                WorkerKind::Next(active) | WorkerKind::RecoveredProjection(active)
                    if active == syndic_thread_id
            )
        })
    }

    pub(super) fn mark_active_workers_verification_resumed(&mut self) {
        for worker in &self.workers {
            if !self
                .verification_resumed_workers
                .contains(&worker.thread_id)
            {
                self.verification_resumed_workers.push(worker.thread_id);
            }
        }
    }

    fn take_worker_verification_resume(&mut self, thread_id: std::thread::ThreadId) -> bool {
        let Some(index) = self
            .verification_resumed_workers
            .iter()
            .position(|covered| *covered == thread_id)
        else {
            return false;
        };
        self.verification_resumed_workers.swap_remove(index);
        true
    }

    fn exact_home_is_healthy(&self) -> bool {
        let health = self.context.home.health();
        self.context.home.home_id() == self.context.home_id
            && health.state() == beryl_home_store::HomeHealthState::Healthy
            && health.generation() == Some(self.context.home_generation)
    }

    pub(super) fn drain_completions(&mut self) -> (bool, bool, bool, bool, bool) {
        let mut recovered_projection_worker_ready = false;
        let mut recovered_pending_worker_ready = false;
        let mut next_worker_ready = false;
        let mut verification_pending = false;
        let mut late_verification_resumed = false;
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
            let verification_was_resumed =
                self.take_worker_verification_resume(completion.thread_id);
            let result = worker.handle.join();
            self.record_worker_join();
            match result {
                Ok(disposition) if disposition == completion.disposition => {
                    let (
                        recovered_projection_ready,
                        recovered_pending_ready,
                        next_ready,
                        worker_verification_pending,
                    ) = self.apply_worker_disposition(completion.disposition);
                    recovered_projection_worker_ready |= recovered_projection_ready;
                    recovered_pending_worker_ready |= recovered_pending_ready;
                    next_worker_ready |= next_ready;
                    if worker_verification_pending
                        && verification_was_resumed
                        && self.exact_home_is_healthy()
                    {
                        late_verification_resumed = true;
                    } else {
                        verification_pending |= worker_verification_pending;
                    }
                }
                Ok(_) | Err(_) => self.fail_closed(SchedulerFailure::Fatal),
            }
        }
        (
            recovered_projection_worker_ready,
            recovered_pending_worker_ready,
            next_worker_ready,
            verification_pending,
            late_verification_resumed,
        )
    }

    pub(super) fn join_all_workers(&mut self) {
        while let Some(worker) = self.workers.pop() {
            let _ = self.take_worker_verification_resume(worker.thread_id);
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

    fn apply_worker_disposition(
        &mut self,
        disposition: WorkerDisposition,
    ) -> (bool, bool, bool, bool) {
        match disposition {
            WorkerDisposition::Parked => {
                self.parked_retry = true;
                self.retry_pass_active = false;
                self.scan = None;
                self.context.signal.update_diagnostics(|diagnostics| {
                    diagnostics.retry_state = ActiveSteeringRetryState::Parked;
                });
                (false, false, false, false)
            }
            WorkerDisposition::Settled => (false, false, false, false),
            WorkerDisposition::VerificationPending => (false, false, false, true),
            WorkerDisposition::RecoveredProjectionContinue => (true, false, false, false),
            WorkerDisposition::RecoveredProjectionParked => (true, false, false, false),
            WorkerDisposition::RecoveredPendingContinue => (
                false,
                true,
                self.next_capacity_waiting || self.next_active_worker_waiting,
                false,
            ),
            WorkerDisposition::NextContinue => {
                (false, self.recovered_pending_capacity_waiting, true, false)
            }
            WorkerDisposition::NextParked => (false, false, false, false),
            WorkerDisposition::PersistentHomeFailure => {
                self.fail_closed(SchedulerFailure::PersistentHomeFailure);
                (false, false, false, false)
            }
            WorkerDisposition::Fatal => {
                self.fail_closed(SchedulerFailure::Fatal);
                (false, false, false, false)
            }
        }
    }
}
