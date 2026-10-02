use super::*;
use crate::app_services::{
    AppServiceConfiguration, recovery_graph::RecoveryServicePreparationError,
};
use beryl_home_store::{CommandCancellation, HomeGeneration};
use gpui::AsyncApp;
use settlement::{CandidateSettlement, CandidateSettlementError};
use syndic_storage::SyndicTimestamp;

pub(crate) enum RecoveryPreparationFailure {
    Candidate(CandidateSettlementError),
    Services(RecoveryServicePreparationError),
    Resume(crate::exit_session::ResumeSessionOutcome),
    ResumeReconciliation(beryl_home_store::ReconciliationFailure),
}

impl RunningProcessOwner {
    pub(crate) async fn retry_interrupted_exit_preparation(
        owner: &impl RecoveryOwnerAccess,
        request: &RunningExitRequest,
        generation: HomeGeneration,
        configuration: AppServiceConfiguration,
        at: SyndicTimestamp,
        cancellation: CommandCancellation,
        failed: impl FnMut(RecoveryPreparationFailure),
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let _driver = owner
            .recovery_owner()?
            .borrow_mut()
            .reserve_interrupted_exit_driver(request)?;
        Self::retry_interrupted_exit_preparation_attempts(
            owner,
            request,
            generation,
            configuration,
            at,
            cancellation,
            failed,
            cx,
        )
        .await
    }

    pub(crate) async fn retire_and_retry_interrupted_exit_preparation(
        owner: &impl RecoveryOwnerAccess,
        request: &RunningExitRequest,
        generation: HomeGeneration,
        configuration: AppServiceConfiguration,
        at: SyndicTimestamp,
        cancellation: CommandCancellation,
        failed: impl FnMut(RecoveryPreparationFailure),
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let _driver = owner
            .recovery_owner()?
            .borrow_mut()
            .reserve_interrupted_exit_driver(request)?;
        Self::retire_interrupted_exit_for_preparation(
            owner,
            request,
            generation,
            cancellation.clone(),
            cx,
        )
        .await?;
        Self::retry_interrupted_exit_preparation_attempts(
            owner,
            request,
            generation,
            configuration,
            at,
            cancellation,
            failed,
            cx,
        )
        .await
    }

    pub(super) async fn retry_interrupted_exit_preparation_attempts(
        owner: &impl RecoveryOwnerAccess,
        request: &RunningExitRequest,
        generation: HomeGeneration,
        configuration: AppServiceConfiguration,
        at: SyndicTimestamp,
        cancellation: CommandCancellation,
        mut failed: impl FnMut(RecoveryPreparationFailure),
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        loop {
            owner
                .recovery_owner()?
                .borrow()
                .validate_interrupted_exit_preparation(request, generation)?;
            if cancellation.is_cancelled() {
                return Err("Interrupted Exit preparation was cancelled".into());
            }
            let result = Self::prepare_interrupted_exit_attempt(
                owner,
                request,
                generation,
                configuration.clone(),
                at,
                cancellation.clone(),
                cx,
            )
            .await;
            if cancellation.is_cancelled() {
                return result;
            }
            Self::hand_off_interrupted_exit_resume_failures(
                &owner.recovery_owner()?,
                request,
                &cancellation,
                &mut failed,
            )?;
            if cancellation.is_cancelled() {
                Self::dispose_cancelled_interrupted_exit_preparation(
                    owner, request, generation, cx,
                )
                .await?;
                return Err("Interrupted Exit preparation was cancelled".into());
            }
            if result.is_ok() {
                return result;
            }
            let failure = {
                let retained_owner = owner.recovery_owner()?;
                let mut owner = retained_owner.borrow_mut();
                owner.interrupted_exit_graph_retirement_result(request)?;
                let candidate = match owner
                    .interrupted_exit
                    .as_ref()
                    .unwrap()
                    .settlement
                    .borrow()
                    .as_ref()
                {
                    Some(CandidateSettlement::DisposedFailure(_)) => true,
                    Some(CandidateSettlement::DisposedPreparationFailure { .. }) => false,
                    _ => return result,
                };
                owner
                    .process
                    .services
                    .as_ref()
                    .ok_or("The complete service owner is on a worker")?
                    .validate_retired_service_home(generation)
                    .map_err(|error| error.to_string())?;
                if candidate {
                    RecoveryPreparationFailure::Candidate(
                        owner.take_interrupted_exit_candidate_failure(request)?,
                    )
                } else {
                    RecoveryPreparationFailure::Services(
                        owner.take_interrupted_exit_preparation_failure(request, generation)?,
                    )
                }
            };
            failed(failure);
        }
    }

    fn hand_off_interrupted_exit_resume_failures(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        cancellation: &CommandCancellation,
        failed: &mut impl FnMut(RecoveryPreparationFailure),
    ) -> Result<(), String> {
        loop {
            let failure = {
                let owner = owner.borrow();
                owner.interrupted_exit_graph_retirement_result(request)?;
                if cancellation.is_cancelled() {
                    return Ok(());
                }
                let recovery = owner.interrupted_exit.as_ref().unwrap();
                let mut session = recovery.session.borrow_mut();
                let session = session
                    .as_mut()
                    .ok_or("Interrupted Exit resume is on a worker")?;
                let previous = recovery.previous_resume.borrow_mut().take();
                if let Some(previous) = previous {
                    Some(RecoveryPreparationFailure::Resume(previous))
                } else if let RunningShutdownSession::Resuming(resume) = session {
                    resume
                        .take_previous_reconciliation()
                        .map(RecoveryPreparationFailure::ResumeReconciliation)
                } else {
                    None
                }
            };
            let Some(failure) = failure else {
                return Ok(());
            };
            failed(failure);
        }
    }
}
