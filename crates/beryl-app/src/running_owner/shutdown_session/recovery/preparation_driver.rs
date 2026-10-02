use super::*;
use crate::app_services::AppServiceConfiguration;
use beryl_home_store::{CommandCancellation, HomeGeneration};
use gpui::AsyncApp;
use settlement::CandidateSettlement;
use syndic_storage::SyndicTimestamp;

impl RunningProcessOwner {
    pub(super) fn reserve_interrupted_exit_driver(
        &mut self,
        request: &RunningExitRequest,
    ) -> Result<Rc<()>, String> {
        if !self.process.commands.is_active(request) {
            return Err("Interrupted Exit request changed".into());
        }
        let recovery = self
            .interrupted_exit
            .as_mut()
            .ok_or("No reported failed Exit")?;
        if !Rc::ptr_eq(&recovery.request, &request.identity()) {
            return Err("Interrupted Exit request changed".into());
        }
        if recovery.driver.upgrade().is_some() {
            return Err("Interrupted Exit recovery is already being driven".into());
        }
        let driver = Rc::new(());
        recovery.driver = Rc::downgrade(&driver);
        Ok(driver)
    }

    pub(crate) async fn prepare_retired_interrupted_exit(
        owner: &impl RecoveryOwnerAccess,
        request: &RunningExitRequest,
        generation: HomeGeneration,
        configuration: AppServiceConfiguration,
        at: SyndicTimestamp,
        cancellation: CommandCancellation,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let _driver = owner
            .recovery_owner()?
            .borrow_mut()
            .reserve_interrupted_exit_driver(request)?;
        owner
            .recovery_owner()?
            .borrow()
            .validate_interrupted_exit_preparation(request, generation)?;
        Self::prepare_interrupted_exit_attempt(
            owner,
            request,
            generation,
            configuration,
            at,
            cancellation,
            cx,
        )
        .await
    }

    pub(super) fn validate_interrupted_exit_preparation(
        &self,
        request: &RunningExitRequest,
        generation: HomeGeneration,
    ) -> Result<(), String> {
        self.interrupted_exit_graph_retirement_result(request)?;
        let recovery = self.interrupted_exit.as_ref().unwrap();
        if recovery.session.borrow().is_none()
            || recovery.resident.is_some()
            || recovery
                .pending_resident_frame
                .as_ref()
                .is_some_and(|wake| wake.strong_count() != 0)
        {
            return Err("Interrupted Exit recovery custody is unavailable".into());
        }
        if !matches!(
            recovery.settlement.borrow().as_ref(),
            None | Some(CandidateSettlement::Constructed(Err(_)))
        ) {
            return Err("Interrupted Exit candidate work is already retained".into());
        }
        self.process
            .services
            .as_ref()
            .ok_or("The complete service owner is on a worker")?
            .validate_retired_service_home(generation)
            .map_err(|error| error.to_string())
    }

    pub(super) async fn prepare_interrupted_exit_attempt(
        owner: &impl RecoveryOwnerAccess,
        request: &RunningExitRequest,
        generation: HomeGeneration,
        configuration: AppServiceConfiguration,
        at: SyndicTimestamp,
        cancellation: CommandCancellation,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        if let Err(error) = Self::construct_and_settle_interrupted_exit(
            owner,
            request,
            generation,
            cancellation.clone(),
            cx,
        )
        .await
        {
            if cancellation.is_cancelled() {
                Self::dispose_cancelled_interrupted_exit_preparation(
                    owner, request, generation, cx,
                )
                .await?;
            } else {
                Self::dispose_returned_interrupted_exit_failure(owner, request, generation, cx)
                    .await?;
            }
            return Err(error);
        }
        let result = Self::prepare_interrupted_exit_service_graph(
            owner,
            request,
            generation,
            configuration,
            at,
            cancellation.clone(),
            cx,
        )
        .await;
        if result.is_err() && cancellation.is_cancelled() {
            Self::dispose_cancelled_interrupted_exit_preparation(owner, request, generation, cx)
                .await?;
        }
        result
    }
}
