use super::*;
use beryl_home_store::HomeGeneration;
use gpui::AsyncApp;
use settlement::CandidateSettlement;

enum CancelledPreparation {
    Constructed,
    Settled,
    Services,
    FailedServices,
}

impl RunningProcessOwner {
    #[cfg(all(test, feature = "test-faults", target_os = "windows"))]
    pub(crate) fn test_retain_recovery_provider(&self) -> impl Send + 'static + use<> {
        let recovery = self.interrupted_exit.as_ref().unwrap();
        let settlement = recovery.settlement.borrow();
        let Some(CandidateSettlement::Services(Ok(graph))) = settlement.as_ref() else {
            panic!("prepared graph is absent")
        };
        graph.test_retain_recovery_provider()
    }

    #[cfg(all(test, feature = "test-faults", target_os = "windows"))]
    pub(crate) fn test_take_unreturned_service_failure(
        &self,
    ) -> crate::app_services::recovery_graph::RecoveryServicePreparationError {
        let Some(CandidateSettlement::Services(Err(failure))) = self
            .interrupted_exit
            .as_ref()
            .unwrap()
            .settlement
            .borrow_mut()
            .take()
        else {
            panic!("unreturned service failure is absent")
        };
        failure
    }

    #[cfg(test)]
    pub(crate) async fn test_settle_automatic_exit_cancellation(
        owner: &Rc<RefCell<Self>>,
        request: &impl RecoveryIdentity,
        retired: HomeGeneration,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        Self::settle_automatic_interrupted_exit_cancellation(owner, request, retired, cx).await
    }

    pub(super) async fn settle_automatic_interrupted_exit_cancellation(
        owner: &impl RecoveryOwnerAccess,
        request: &impl RecoveryIdentity,
        retired: HomeGeneration,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let (_driver, selected, threadless, candidate) = {
            let retained_owner = owner.recovery_owner()?;
            let mut retained = retained_owner.borrow_mut();
            let driver = retained.reserve_interrupted_exit_driver(request)?;
            let recovery = retained.interrupted_exit.as_ref().unwrap();
            if matches!(
                recovery.settlement.borrow().as_ref(),
                Some(CandidateSettlement::Published)
            ) || recovery
                .publication
                .borrow()
                .as_ref()
                .is_some_and(|result| result.is_ok())
            {
                return Err("Interrupted Exit cancellation followed publication; published custody remains fenced".into());
            }
            if recovery.retirement.borrow().is_none() {
                return Ok(());
            }
            retained.interrupted_exit_graph_retirement_result(request)?;
            let candidate = match recovery.settlement.borrow().as_ref() {
                Some(CandidateSettlement::Services(Ok(graph))) => {
                    let appearance = graph.appearance();
                    let home = appearance.prepared().home();
                    Some((home.home_id(), home.home_generation()))
                }
                _ => recovery
                    .resident
                    .as_ref()
                    .map(|flight| (flight.home, flight.generation)),
            };
            (
                driver,
                recovery.selected_windows.clone(),
                recovery.threadless_appearance.clone(),
                candidate,
            )
        };
        if let Some((home, generation)) = candidate {
            if let Some(selected) = selected {
                selected
                    .try_borrow_mut()
                    .map_err(|_| "Interrupted Exit selected cleanup is busy")?
                    .detach_unpublished(owner, home, generation, cx)
                    .await?;
            }
            if let Some(appearance) = threadless {
                cx.update(|app| appearance.update(app, |appearance, _| appearance.retire()))
                    .map_err(|error| error.to_string())?;
                owner
                    .recovery_owner()?
                    .borrow_mut()
                    .interrupted_exit
                    .as_mut()
                    .unwrap()
                    .threadless_appearance = None;
            }
        }
        Self::dispose_cancelled_interrupted_exit_preparation(owner, request, retired, cx).await
    }

    pub(super) async fn dispose_cancelled_interrupted_exit_preparation(
        owner: &impl RecoveryOwnerAccess,
        request: &impl RecoveryIdentity,
        generation: HomeGeneration,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let stage = {
            let retained_owner = owner.recovery_owner()?;
            let owner = retained_owner.borrow();
            owner.interrupted_exit_graph_retirement_result(request)?;
            let recovery = owner.interrupted_exit.as_ref().unwrap();
            match recovery.settlement.borrow().as_ref() {
                Some(CandidateSettlement::Constructed(Ok(_))) => CancelledPreparation::Constructed,
                Some(CandidateSettlement::Returned { .. }) => CancelledPreparation::Settled,
                Some(CandidateSettlement::Services(Ok(_))) => CancelledPreparation::Services,
                Some(CandidateSettlement::Services(Err(_))) => CancelledPreparation::FailedServices,
                _ => return Ok(()),
            }
        };
        if matches!(stage, CancelledPreparation::FailedServices) {
            return owner
                .recovery_owner()?
                .borrow_mut()
                .return_interrupted_exit_preparation_home(request, generation);
        }
        let (sender, receiver) = futures_channel::oneshot::channel();
        cx.update(|app| {
            let completed = move |_: &Rc<RefCell<Self>>, _: &mut App| {
                let _ = sender.send(());
            };
            match stage {
                CancelledPreparation::Constructed => Self::abort_constructed_exit_candidate(
                    &owner.recovery_owner()?,
                    request,
                    generation,
                    app,
                    completed,
                ),
                CancelledPreparation::Settled => Self::dispose_settled_interrupted_exit_candidate(
                    &owner.recovery_owner()?,
                    request,
                    generation,
                    true,
                    app,
                    completed,
                ),
                CancelledPreparation::Services => Self::cancel_interrupted_exit_services(
                    &owner.recovery_owner()?,
                    request,
                    app,
                    completed,
                ),
                CancelledPreparation::FailedServices => unreachable!(),
            }
        })
        .map_err(|error| error.to_string())??;
        receiver
            .await
            .map_err(|_| "Interrupted Exit cancellation disposal delivery is unavailable")?;
        if matches!(stage, CancelledPreparation::Services) {
            owner
                .recovery_owner()?
                .borrow_mut()
                .return_interrupted_exit_preparation_home(request, generation)?;
        }
        Ok(())
    }
}
