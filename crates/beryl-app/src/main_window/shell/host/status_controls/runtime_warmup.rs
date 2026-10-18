use super::*;
use crate::cas_projection::{ProjectionCancellationToken, RuntimeInterest, RuntimeInterestStatus};

#[derive(Clone, Copy, Eq, PartialEq)]
struct SelectedRuntimeScope {
    service: (
        beryl_model::BerylHomeId,
        beryl_home_store::HomeGeneration,
        crate::cas_projection::ProjectionServiceGeneration,
    ),
    window: beryl_model::WindowId,
    claim: beryl_state::WindowClaimSelection,
}

impl SelectedRuntimeScope {
    fn new(
        worker: &PublishedExactStopWorker,
        selection: MainWindowComposerSelectionIdentity,
    ) -> Self {
        Self {
            service: worker.worker_identity(),
            window: selection.window_id(),
            claim: selection.claim(),
        }
    }
}

struct PendingViewInterest {
    scope: SelectedRuntimeScope,
    cancel: ProjectionCancellationToken,
    _task: gpui::Task<()>,
}

struct RetainedViewInterest {
    scope: SelectedRuntimeScope,
    interest: RuntimeInterest,
    epoch: Arc<()>,
}

#[derive(Default)]
pub(super) struct SelectedRuntimeWarmup {
    retained: Option<RetainedViewInterest>,
    pending: Option<PendingViewInterest>,
    retired: bool,
}

impl SelectedRuntimeWarmup {
    fn retire(&mut self) {
        self.retired = true;
        self.retained = None;
        if let Some(pending) = &self.pending {
            pending.cancel.cancel();
        }
    }
}

impl Drop for SelectedRuntimeWarmup {
    fn drop(&mut self) {
        self.retire();
    }
}

impl MainWindowShellRoot {
    pub(in crate::main_window::shell) fn resume_selected_runtime_warmup(&mut self) {
        self.status_controls.warmup.retired = false;
    }

    #[cfg(feature = "test-faults")]
    pub fn test_selected_runtime_warmup_retained(&self) -> bool {
        self.status_controls.warmup.retained.is_some()
    }

    pub(super) fn selected_view_interest_observation_target(
        &self,
    ) -> Option<(beryl_model::ExecutionBinding, std::sync::Weak<()>)> {
        let retained = self.status_controls.warmup.retained.as_ref()?;
        Some((
            retained.interest.binding().clone(),
            Arc::downgrade(&retained.epoch),
        ))
    }

    pub(super) fn apply_selected_view_interest_observation(
        &mut self,
        worker: PublishedExactStopWorker,
        selection: MainWindowComposerSelectionIdentity,
        epoch: std::sync::Weak<()>,
        result: Result<
            (bool, beryl_home_store::HomeMutationObservation),
            crate::cas_projection::RuntimeInterestError,
        >,
        cx: &mut Context<Self>,
    ) {
        let scope = SelectedRuntimeScope::new(&worker, selection);
        if self
            .status_selection(cx)
            .is_none_or(|current| SelectedRuntimeScope::new(&worker, current) != scope)
            || self.status_controls.worker.as_ref().is_none_or(|current| {
                !current.publication_current() || current.worker_identity() != scope.service
            })
            || self
                .status_controls
                .warmup
                .retained
                .as_ref()
                .is_none_or(|retained| {
                    retained.scope != scope || !Arc::downgrade(&retained.epoch).ptr_eq(&epoch)
                })
        {
            return;
        }
        let Ok((current, observation)) = result else {
            return;
        };
        if !current {
            let _ = worker.elect_selected_view_observation(&observation, || {
                self.status_controls.warmup.retained = None;
                cx.notify();
            });
        }
    }

    pub(in crate::main_window::shell) fn retire_selected_runtime_warmup(&mut self) {
        self.status_controls.warmup.retire();
    }

    pub(super) fn sync_selected_runtime_warmup(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.status_controls.warmup.retired || self.running_threads.views_retired {
            self.status_controls.warmup.retire();
            return;
        }
        let source = self
            .status_controls
            .worker
            .clone()
            .zip(self.status_selection(cx))
            .filter(|(worker, _)| worker.publication_current());
        let scope = source
            .as_ref()
            .map(|(worker, selection)| SelectedRuntimeScope::new(worker, *selection));
        let retained_current =
            self.status_controls
                .warmup
                .retained
                .as_ref()
                .is_some_and(|retained| {
                    Some(retained.scope) == scope
                        && retained.interest.status() != RuntimeInterestStatus::Retired
                });
        if !retained_current {
            self.status_controls.warmup.retained = None;
        }
        if let Some(pending) = &self.status_controls.warmup.pending {
            if Some(pending.scope) != scope || self.status_controls.warmup.retired {
                pending.cancel.cancel();
            }
            return;
        }
        if self.status_controls.warmup.retired
            || self.status_controls.warmup.retained.is_some()
            || self.status_mutation_gate().is_some()
            || self.running_threads.views_retired
            || self.running_threads.reads_suspended
            || !self.running_read_capacity_available()
        {
            return;
        }
        let Some((worker, selection)) = source else {
            return;
        };
        let running_service = crate::running_owner::RunningProcessOwner::mounted_owner(cx)
            .and_then(|owner| owner.upgrade())
            .and_then(|owner| {
                owner
                    .try_borrow()
                    .ok()
                    .and_then(|owner| owner.status_stop_worker())
            })
            .filter(|running| running.publication_current())
            .map(|running| running.worker_identity());
        if running_service != Some(worker.worker_identity()) {
            return;
        }
        let scope = SelectedRuntimeScope::new(&worker, selection);
        let cancel = ProjectionCancellationToken::new();
        let work_cancel = cancel.clone();
        let release = self.running_threads.workers.track(|| ());
        let work = self.running_threads.workers.track(move || {
            let result = worker.acquire_selected_view_interest(selection, &work_cancel);
            (worker, result, release)
        });
        let job = cx.background_executor().spawn(async move { work.run() });
        let task = cx.spawn_in(window, async move |this, cx| {
            let (worker, result, _release) = job.await;
            let _ = this.update_in(cx, |root, _, cx| {
                let Some(pending) = root.status_controls.warmup.pending.take() else {
                    return;
                };
                let current = root.status_selection(cx);
                if pending.scope != scope
                    || pending.cancel.is_cancelled()
                    || root.status_controls.warmup.retired
                    || root.running_threads.views_retired
                    || root.status_controls.worker.as_ref().is_none_or(|mounted| {
                        !mounted.publication_current() || mounted.worker_identity() != scope.service
                    })
                    || current.is_none_or(|selection| {
                        SelectedRuntimeScope::new(&worker, selection) != scope
                    })
                {
                    return;
                }
                if let Ok((interest, observation)) = result {
                    let _ = worker.elect_selected_view_observation(&observation, || {
                        root.status_controls.warmup.retained = Some(RetainedViewInterest {
                            scope,
                            interest,
                            epoch: Arc::new(()),
                        });
                        cx.notify();
                    });
                }
            });
        });
        self.status_controls.warmup.pending = Some(PendingViewInterest {
            scope,
            cancel,
            _task: task,
        });
    }
}
