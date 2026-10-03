use super::*;
use crate::cas_projection::{
    RuntimeFailure, RuntimeFailureSnapshot, SelectedRuntimeFailureObservation,
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
}

impl MainWindowShellRoot {
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
        self.notices.runtime.failure = Some((execution, failure));
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
        let reason = if !failure.retry_ready() {
            "The exact runtime failure is awaiting cleanup or an already admitted retry."
        } else {
            "Recovery of this exact runtime, root and thread binding is not available yet."
        };
        let title = format!("Runtime {} unavailable", failure.runtime_id());
        let Ok(content) = NoticeContent::new(
            NoticeVariant::Error,
            NoticeDismissal::Persistent,
            &title,
            detail,
        )
        .with_commands(&[NoticeCommand::disabled(
            NoticeCommandId::new(1),
            "Retry",
            reason,
        )]) else {
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

    fn clear_runtime_notice(&mut self) {
        if let Some(token) = self.notices.runtime.token.take() {
            let _ = self.notices.arbiter.remove(&token);
        }
        self.notices.runtime = RuntimeNoticeContribution::default();
    }
}
