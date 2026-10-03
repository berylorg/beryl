use super::*;
use crate::cas_projection::{
    ProjectionCancellationToken, RuntimeFailure, RuntimeFailureSnapshot,
    SelectedRuntimeFailureObservation, SelectedRuntimeRetryError,
};
use crate::main_window::{
    MainWindowComposerSelectionIdentity, NoticeCommand, NoticeConditionId, NoticeDismissal,
    NoticeKind, NoticeVariant,
};

type ServiceIdentity = (
    beryl_model::BerylHomeId,
    beryl_home_store::HomeGeneration,
    crate::cas_projection::ProjectionServiceGeneration,
);
type SelectionScope = (
    beryl_model::WindowId,
    beryl_state::WindowClaimSelection,
    ServiceIdentity,
);

#[derive(Default)]
pub(super) struct RuntimeNoticeContribution {
    scope: Option<SelectionScope>,
    failure: Option<(beryl_model::ExecutionBinding, RuntimeFailureSnapshot)>,
    condition: Option<NoticeConditionId>,
    token: Option<NoticeRecordToken>,
    content: Option<NoticeContent>,
    revision: u64,
    retry_eligible: bool,
    retry_feedback: Option<&'static str>,
    pending: Option<RuntimeRetryPresentation>,
    recovered: Option<(beryl_model::ExecutionBinding, RuntimeFailureSnapshot)>,
}

struct RuntimeRetryPresentation {
    selection: MainWindowComposerSelectionIdentity,
    requested: NoticeRecordToken,
    published: Option<NoticeRecordToken>,
    cancellation: ProjectionCancellationToken,
    _task: Option<gpui::Task<()>>,
}

impl Drop for RuntimeRetryPresentation {
    fn drop(&mut self) {
        self.cancellation.cancel();
    }
}

impl MainWindowShellRoot {
    #[cfg(feature = "test-faults")]
    pub fn test_use_runtime_retry_native_worker(
        &mut self,
        custody: std::sync::Weak<std::sync::Mutex<Option<std::thread::JoinHandle<()>>>>,
    ) {
        self.notices.runtime_retry_test_worker = Some(custody);
    }

    #[cfg(feature = "test-faults")]
    pub fn test_runtime_retry_diagnostics(&self) -> (bool, bool, Option<&'static str>) {
        (
            self.notices.runtime.pending.is_some(),
            self.notices.runtime.retry_eligible,
            self.notices.runtime.retry_feedback,
        )
    }

    #[cfg(feature = "test-faults")]
    pub fn test_runtime_failure_snapshot(&self) -> Option<RuntimeFailureSnapshot> {
        self.notices
            .runtime
            .failure
            .as_ref()
            .map(|(_, failure)| *failure)
    }

    pub(in crate::main_window::shell) fn retain_runtime_failure(
        &mut self,
        selection: MainWindowComposerSelectionIdentity,
        service: ServiceIdentity,
        observation: SelectedRuntimeFailureObservation,
    ) {
        let scope = Some((selection.window_id(), selection.claim(), service));
        if self.notices.runtime.scope != scope {
            self.clear_runtime_notice();
            self.notices.runtime.scope = scope;
        }
        let SelectedRuntimeFailureObservation::Unavailable { execution, failure } = observation
        else {
            return;
        };
        if !failure.belongs_to_service(service.2) || failure.runtime_id() != execution.runtime_id()
        {
            return;
        }
        if self
            .notices
            .runtime
            .recovered
            .as_ref()
            .is_some_and(|(binding, recovered)| {
                binding == &execution
                    && (failure.same_attempt(*recovered) || failure.precedes(*recovered))
            })
        {
            return;
        }
        if self
            .notices
            .runtime
            .failure
            .as_ref()
            .is_some_and(|(_, current)| failure.precedes(*current))
        {
            return;
        }
        if self
            .notices
            .runtime
            .failure
            .as_ref()
            .is_some_and(|(current, _)| current != &execution)
        {
            self.clear_runtime_notice();
            self.notices.runtime.scope = scope;
        }
        if self
            .notices
            .runtime
            .failure
            .as_ref()
            .is_none_or(|(_, current)| !failure.same_attempt(*current))
        {
            let retry_failed = self.notices.runtime.pending.is_some();
            self.notices.runtime.pending = None;
            self.notices.runtime.retry_eligible = false;
            if retry_failed {
                self.notices.runtime.retry_feedback =
                    Some("Retry failed because the runtime became unavailable again.");
            }
        }
        self.notices.runtime.failure = Some((execution, failure));
    }

    pub(in crate::main_window::shell) fn runtime_retry_observation_target(
        &self,
    ) -> Option<(beryl_model::ExecutionBinding, RuntimeFailureSnapshot)> {
        self.notices.runtime.failure.clone()
    }

    pub(in crate::main_window::shell) fn retain_runtime_retry_eligibility(
        &mut self,
        eligibility: Option<(beryl_model::ExecutionBinding, RuntimeFailureSnapshot, bool)>,
    ) {
        self.notices.runtime.retry_eligible =
            eligibility.is_some_and(|(execution, failure, eligible)| {
                eligible
                    && self
                        .notices
                        .runtime
                        .failure
                        .as_ref()
                        .is_some_and(|(current, observed)| {
                            current == &execution && observed == &failure
                        })
            });
    }

    pub(super) fn sync_runtime_notice(&mut self, cx: &Context<Self>) {
        let scope = self
            .status_selection(cx)
            .zip(self.runtime_notice_service_identity())
            .map(|(selection, service)| (selection.window_id(), selection.claim(), service));
        if self.notices.runtime.scope != scope {
            self.clear_runtime_notice();
            self.notices.runtime.scope = scope;
        }
        if self.runtime_retry_interaction_gated() || self.notices.inert {
            self.notices.runtime.pending = None;
            self.notices.runtime.retry_eligible = false;
        }
        if self
            .notices
            .runtime
            .pending
            .as_ref()
            .is_some_and(|pending| {
                self.status_selection(cx).is_none_or(|selection| {
                    selection.window_id() != pending.selection.window_id()
                        || selection.claim() != pending.selection.claim()
                }) || self
                    .notices
                    .runtime
                    .token
                    .as_ref()
                    .is_none_or(|token| !self.notices.arbiter.contains(token))
            })
        {
            self.notices.runtime.pending = None;
            self.notices.runtime.retry_eligible = false;
        }
        let Some((_, failure)) = self.notices.runtime.failure.as_ref() else {
            return;
        };
        let detail = match failure.failure() {
            RuntimeFailure::Launch => "The configured runtime could not be launched.",
            RuntimeFailure::Admission => {
                "The configured runtime did not satisfy exact backend release admission."
            }
            RuntimeFailure::ProcessExited => "The configured runtime process exited.",
            RuntimeFailure::ConnectionLost => "The configured runtime connection was lost.",
            RuntimeFailure::AppRetirement => {
                "The configured runtime could not retire its app connection cleanly."
            }
            RuntimeFailure::BackendDisposal => {
                "The configured runtime could not dispose its backend cleanly."
            }
            RuntimeFailure::WorkerPanicked => "The configured runtime worker failed.",
            RuntimeFailure::IdentityExhausted => {
                "The configured runtime identity capacity is exhausted."
            }
        };
        let reason = if self.notices.runtime.pending.is_some() {
            "Recovery of this exact runtime, root and thread binding is pending."
        } else if !failure.retry_ready() {
            "The exact runtime failure is awaiting cleanup or an already admitted retry."
        } else {
            "This exact runtime, root and thread binding does not currently qualify for recovery."
        };
        let command =
            if self.notices.runtime.retry_eligible && self.notices.runtime.pending.is_none() {
                NoticeCommand::enabled(NoticeCommandId::new(1), "Retry")
            } else {
                NoticeCommand::disabled(NoticeCommandId::new(1), "Retry", reason)
            };
        let detail = self.notices.runtime.retry_feedback.map_or_else(
            || detail.to_owned(),
            |feedback| format!("{detail} {feedback}"),
        );
        let title = format!("Runtime {} unavailable", failure.runtime_id());
        let Ok(content) = NoticeContent::new(
            NoticeVariant::Error,
            NoticeDismissal::Persistent,
            &title,
            &detail,
        )
        .with_commands(&[command]) else {
            return;
        };
        if self.notices.runtime.content.as_ref() == Some(&content)
            && self
                .notices
                .runtime
                .token
                .as_ref()
                .is_some_and(|token| self.notices.arbiter.contains(token))
        {
            return;
        }
        self.notices.runtime.revision = self.notices.runtime.revision.saturating_add(1);
        let condition = self
            .notices
            .runtime
            .condition
            .get_or_insert_with(NoticeConditionId::new)
            .clone();
        let record = NoticeRecord {
            window_id: self.notices.window_id,
            condition,
            revision: self.notices.runtime.revision,
            kind: NoticeKind::RuntimeUnavailable,
            content: content.clone(),
        };
        self.notices.runtime.token = match self.notices.arbiter.admit(record) {
            NoticeAdmission::Admitted(token) | NoticeAdmission::Updated(token) => Some(token),
            _ => None,
        };
        self.notices.runtime.content = Some(content);
    }

    pub(super) fn runtime_retry_duplicate(
        &self,
        token: &NoticeVisibleToken,
        command: NoticeCommandId,
    ) -> bool {
        command == NoticeCommandId::new(1)
            && self
                .notices
                .runtime
                .pending
                .as_ref()
                .is_some_and(|pending| {
                    token.record() == &pending.requested
                        || pending.published.as_ref() == Some(token.record())
                })
    }

    pub(super) fn activate_runtime_retry(
        &mut self,
        token: &NoticeVisibleToken,
        command: NoticeCommandId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if command != NoticeCommandId::new(1)
            || self.notices.runtime.token.as_ref() != Some(token.record())
        {
            return false;
        }
        if self.notices.runtime.pending.is_some()
            || !self.notices.runtime.retry_eligible
            || self.runtime_retry_interaction_gated()
        {
            return true;
        }
        let Some(selection) = self.status_selection(cx) else {
            return true;
        };
        let Some(worker) = self.runtime_notice_worker() else {
            return true;
        };
        let Some(scope) = self.notices.runtime.scope else {
            return true;
        };
        let Some((execution, failure)) = self.notices.runtime.failure.clone() else {
            return true;
        };
        let requested = token.record().clone();
        let cancellation = ProjectionCancellationToken::new();
        self.notices.runtime.pending = Some(RuntimeRetryPresentation {
            selection,
            requested: requested.clone(),
            published: None,
            cancellation: cancellation.clone(),
            _task: None,
        });
        self.notices.runtime.retry_eligible = false;
        self.notices.runtime.retry_feedback = None;
        self.sync_notices(window, cx);
        let published = self.notices.runtime.token.clone();
        let Some(pending) = self.notices.runtime.pending.as_mut() else {
            return true;
        };
        pending.published = published.clone();
        let lifetime = Rc::downgrade(&self.notices.lifetime);
        let completion_worker = worker.clone();
        let completion_execution = execution.clone();
        #[cfg(feature = "test-faults")]
        let native_worker = self
            .notices
            .runtime_retry_test_worker
            .as_ref()
            .and_then(std::sync::Weak::upgrade);
        let work = cx.background_executor().spawn(async move {
            let recover = move || {
                let result =
                    worker.retry_selected_runtime(selection, &execution, failure, &cancellation);
                let result = result.and_then(|proof| {
                    if worker.selected_runtime_usability_current(selection, &proof) {
                        Ok(proof)
                    } else {
                        Err(SelectedRuntimeRetryError::Revoked)
                    }
                });
                let eligible =
                    worker.selected_runtime_retry_eligible(selection, &execution, failure);
                (result, eligible)
            };
            #[cfg(feature = "test-faults")]
            if let Some(custody) = native_worker {
                let (sender, receiver) = futures_channel::oneshot::channel();
                {
                    let mut slot = custody.lock().expect("test Retry worker custody");
                    assert!(slot.is_none(), "test Retry admits one native worker");
                    let handle = std::thread::Builder::new()
                        .name("runtime-retry-test-worker".to_owned())
                        .spawn(move || {
                            let _ = sender.send(recover());
                        });
                    let Ok(handle) = handle else {
                        return (Err(SelectedRuntimeRetryError::Unavailable), false);
                    };
                    *slot = Some(handle);
                }
                let result = receiver
                    .await
                    .unwrap_or((Err(SelectedRuntimeRetryError::Failed), false));
                let handle = custody.lock().expect("test Retry worker custody").take();
                if handle.is_some_and(|handle| handle.join().is_err()) {
                    return (Err(SelectedRuntimeRetryError::Failed), false);
                }
                return result;
            }
            recover()
        });
        let task = cx.spawn_in(window, async move |this, cx| {
            let (result, eligible) = work.await;
            let _ = this.update_in(cx, |root, window, cx| {
                root.sync_notices(window, cx);
                if root.notices.retired
                    || root.notices.inert
                    || root.status_selection(cx).is_none_or(|current| {
                        current.window_id() != selection.window_id() || current.claim() != selection.claim()
                    })
                    || root.notices.runtime.scope != Some(scope)
                    || !lifetime.upgrade().is_some_and(|current| Rc::ptr_eq(&current, &root.notices.lifetime))
                    || root.notices.runtime.token != published
                    || !root.notices.runtime.pending.as_ref().is_some_and(|pending| pending.requested == requested)
                    || !root.notices.runtime.failure.as_ref().is_some_and(|(binding, current)| {
                        binding == &completion_execution && current.same_attempt(failure)
                    })
                {
                    return;
                }
                let recovered = result.as_ref().ok().is_some_and(|proof| {
                    completion_worker.with_selected_runtime_usability_publication(selection, proof, || {
                        root.notices.runtime.pending = None;
                        if let Some(token) = root.notices.runtime.token.take() {
                            let _ = root.notices.arbiter.remove(&token);
                        }
                        root.notices.runtime.failure = None;
                        root.notices.runtime.content = None;
                        root.notices.runtime.retry_feedback = None;
                        root.notices.runtime.retry_eligible = false;
                        root.notices.runtime.recovered = Some((completion_execution.clone(), failure));
                    }).is_some()
                });
                if !recovered {
                    root.notices.runtime.pending = None;
                    root.notices.runtime.retry_eligible = eligible;
                    root.notices.runtime.retry_feedback = Some(match result {
                        Err(SelectedRuntimeRetryError::Cancelled) => "Retry was cancelled before usability was confirmed.",
                        Err(SelectedRuntimeRetryError::Unavailable) => "Retry is unavailable for this exact binding.",
                        Err(SelectedRuntimeRetryError::Failed) => "Retry could not establish the selected thread's required backend operations.",
                        _ => "Retry no longer has current authority for this exact binding.",
                    });
                }
                root.sync_notices(window, cx);
            });
        });
        if let Some(pending) = self.notices.runtime.pending.as_mut() {
            pending._task = Some(task);
        }
        true
    }

    fn clear_runtime_notice(&mut self) {
        if let Some(token) = self.notices.runtime.token.take() {
            let _ = self.notices.arbiter.remove(&token);
        }
        self.notices.runtime = RuntimeNoticeContribution::default();
    }
}
