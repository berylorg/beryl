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
}

impl RunningProcessOwner {
    pub(crate) async fn retry_interrupted_exit_preparation(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        generation: HomeGeneration,
        configuration: AppServiceConfiguration,
        at: SyndicTimestamp,
        cancellation: CommandCancellation,
        failed: impl FnMut(RecoveryPreparationFailure),
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let _driver = owner
            .borrow_mut()
            .reserve_interrupted_exit_preparation(request)?;
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
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        generation: HomeGeneration,
        configuration: AppServiceConfiguration,
        at: SyndicTimestamp,
        cancellation: CommandCancellation,
        failed: impl FnMut(RecoveryPreparationFailure),
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let _driver = owner
            .borrow_mut()
            .reserve_interrupted_exit_preparation(request)?;
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

    async fn retry_interrupted_exit_preparation_attempts(
        owner: &Rc<RefCell<Self>>,
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
            if result.is_ok() || cancellation.is_cancelled() {
                return result;
            }
            let failure = {
                let mut owner = owner.borrow_mut();
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
}
