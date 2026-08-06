use beryl_backend::{ThreadInfo, ThreadSessionMetadata};
use beryl_model::conversation::ConversationThreadId;

use super::{
    execution_detail::UserInputFragment,
    phase_thread_preparation_core::{
        PhaseThreadCleanupOutcome, PhaseThreadPreparationRequest, PhaseThreadPreparationResult,
    },
    phase_thread_transition::{
        PhaseThreadCompletionDecision, PhaseThreadCompletionNotice, PhaseThreadPreparationTask,
        PhaseThreadResultGuardFailure, bounded_phase_thread_notice_detail,
    },
    phase_thread_transition_deferred::PreparedPhaseThreadRegistration,
};

pub(crate) const PHASE_THREAD_SOURCE_QUEUE_FAILURE_MESSAGE: &str = "This accepted input was not delivered because the completed lifecycle phase moved continuation to a clean child thread.";

pub(crate) trait PhaseThreadSourceQueueHost {
    fn fail_source_pending_queue(&mut self, source_thread_id: &str, message: &str) -> bool;
}

pub(crate) fn fail_accepted_source_pending_input<H: PhaseThreadSourceQueueHost>(
    host: &mut H,
    source_thread_id: &str,
) -> bool {
    host.fail_source_pending_queue(source_thread_id, PHASE_THREAD_SOURCE_QUEUE_FAILURE_MESSAGE)
}

pub(crate) trait PhaseThreadCompletionHost {
    fn original_workspace_is_current(&self, request: &PhaseThreadPreparationRequest) -> bool;

    fn mark_inventory_refresh(&mut self);

    fn prepared_registration_validity(
        &self,
        request: &PhaseThreadPreparationRequest,
    ) -> Result<(), String>;

    fn register_prepared_child(
        &mut self,
        request: &PhaseThreadPreparationRequest,
        registration: &PreparedPhaseThreadRegistration,
        activate: bool,
    ) -> Result<(), String>;

    fn report_or_defer(
        &mut self,
        request: &PhaseThreadPreparationRequest,
        title: &'static str,
        detail: String,
        refresh_inventory: bool,
        prepared_registration: Option<PreparedPhaseThreadRegistration>,
    );
}

pub(crate) fn prepared_phase_thread_registration(
    child: &ThreadInfo,
) -> PreparedPhaseThreadRegistration {
    let summary = child.summary();
    PreparedPhaseThreadRegistration::new(
        ConversationThreadId::new(summary.id),
        summary.created_at,
        summary.updated_at,
    )
}

pub(crate) fn apply_deferred_prepared_registration<H: PhaseThreadCompletionHost>(
    host: &mut H,
    request: &PhaseThreadPreparationRequest,
    registration: &PreparedPhaseThreadRegistration,
) -> Result<(), String> {
    host.prepared_registration_validity(request)?;
    host.register_prepared_child(request, registration, false)
}

pub(crate) struct PreparedPhaseThreadActivation {
    pub(crate) request: PhaseThreadPreparationRequest,
    pub(crate) child: ThreadInfo,
    pub(crate) session_metadata: ThreadSessionMetadata,
    pub(crate) resume_fragment: UserInputFragment,
}

pub(crate) fn apply_phase_thread_completion<H: PhaseThreadCompletionHost>(
    host: &mut H,
    task: PhaseThreadPreparationTask,
    result: PhaseThreadPreparationResult,
    decision: PhaseThreadCompletionDecision,
    stale_reason: Option<PhaseThreadResultGuardFailure>,
) -> Option<PreparedPhaseThreadActivation> {
    let request = task.request().clone();
    if decision.refresh_original_workspace && host.original_workspace_is_current(&request) {
        host.mark_inventory_refresh();
    }

    match result {
        PhaseThreadPreparationResult::Prepared {
            child,
            session_metadata,
        } if decision.activate_prepared => {
            let child_id = child.summary().id;
            let registration = prepared_phase_thread_registration(&child);
            if let Err(error) = host
                .prepared_registration_validity(&request)
                .and_then(|()| host.register_prepared_child(&request, &registration, true))
            {
                host.mark_inventory_refresh();
                host.report_or_defer(
                    &request,
                    "Lifecycle phase thread registration failed",
                    bounded_phase_thread_notice_detail(format!(
                        "Prepared backend child {child_id} could not be registered locally and may remain orphaned. Inventory will be refreshed. {error}"
                    )),
                    true,
                    Some(registration),
                );
                return None;
            }
            Some(PreparedPhaseThreadActivation {
                request,
                child,
                session_metadata,
                resume_fragment: task.resume_fragment(),
            })
        }
        PhaseThreadPreparationResult::Prepared { child, .. } => {
            let child_id = child.summary().id;
            let prepared_registration = prepared_phase_thread_registration(&child);
            let registration = decision.register_prepared.then(|| {
                host.prepared_registration_validity(&request)
                    .and_then(|()| {
                        host.register_prepared_child(&request, &prepared_registration, false)
                    })
            });
            let retain_registration = !matches!(registration, Some(Ok(())));
            let detail = match registration {
                Some(Ok(())) => format!(
                    "Prepared backend child {child_id} was retained in the original workspace but was not activated because the request was cancelled or stale. Inventory will be refreshed."
                ),
                Some(Err(error)) => format!(
                    "Prepared backend child {child_id} could not yet be registered locally and remains retained for the original workspace without activation. Registration and inventory refresh will be retried when its exact member binding is available. {error}"
                ),
                None => format!(
                    "Prepared backend child {child_id} was not activated because the original request was cancelled, stale, or no longer owns the current workspace. Its exact Beryl registration provenance remains retained for that workspace."
                ),
            };
            host.report_or_defer(
                &request,
                "Lifecycle phase thread not activated",
                bounded_phase_thread_notice_detail(detail),
                decision.refresh_original_workspace,
                retain_registration.then_some(prepared_registration),
            );
            None
        }
        PhaseThreadPreparationResult::DefinitiveForkFailure { detail } => {
            if decision.notice != PhaseThreadCompletionNotice::None {
                let prefix = stale_reason
                    .map(|reason| format!("The result was stale ({reason:?}). "))
                    .unwrap_or_default();
                host.report_or_defer(
                    &request,
                    "Lifecycle phase thread failed",
                    bounded_phase_thread_notice_detail(format!("{prefix}{detail}")),
                    decision.refresh_original_workspace,
                    None,
                );
            }
            None
        }
        PhaseThreadPreparationResult::IndeterminateFork { detail } => {
            host.report_or_defer(
                &request,
                "Lifecycle phase thread outcome unknown",
                bounded_phase_thread_notice_detail(format!(
                    "The fork may have created an unidentified backend child. Beryl did not guess or select a thread; the original workspace inventory will be refreshed when it is current. {detail}"
                )),
                decision.refresh_original_workspace,
                None,
            );
            None
        }
        PhaseThreadPreparationResult::CancelledBeforeFork => {
            if decision.notice != PhaseThreadCompletionNotice::None {
                host.report_or_defer(
                    &request,
                    "Lifecycle phase thread cancelled",
                    "The clean phase-thread preparation was cancelled before it created a child."
                        .to_string(),
                    decision.refresh_original_workspace,
                    None,
                );
            }
            None
        }
        PhaseThreadPreparationResult::KnownChildFailure(failure) => {
            if decision.notice == PhaseThreadCompletionNotice::None {
                return None;
            }
            let orphaned = !matches!(failure.cleanup, PhaseThreadCleanupOutcome::Accepted);
            let orphan_detail = orphaned.then(|| {
                format!(
                    " Backend child {} may remain orphaned; the original workspace inventory will be refreshed when it is current.",
                    failure.child_id
                )
            });
            host.report_or_defer(
                &request,
                "Lifecycle phase thread failed",
                bounded_phase_thread_notice_detail(format!(
                    "Preparation failed during {:?}: {}{}",
                    failure.stage,
                    failure.detail,
                    orphan_detail.as_deref().unwrap_or_default()
                )),
                decision.refresh_original_workspace,
                None,
            );
            None
        }
    }
}
