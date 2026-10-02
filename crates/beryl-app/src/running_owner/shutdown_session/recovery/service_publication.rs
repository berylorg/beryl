use super::*;
use beryl_home_store::{CommandCancellation, HomeGeneration};
use settlement::CandidateSettlement;

impl RunningProcessOwner {
    pub(crate) fn publish_interrupted_exit_services(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        expected: HomeGeneration,
        generation: HomeGeneration,
        appearance: &gpui::Entity<crate::theme_runtime::GpuiAppearanceWindowSet>,
        cancellation: CommandCancellation,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, Result<(), String>, &mut App) + 'static,
    ) -> Result<(), String> {
        #[cfg(test)]
        let before_validate = {
            let cancel = std::mem::take(
                &mut owner
                    .borrow_mut()
                    .cancel_recovery_before_publication_validation,
            );
            let cancellation = cancellation.clone();
            move || {
                if cancel {
                    cancellation.cancel();
                }
            }
        };
        #[cfg(not(test))]
        let before_validate = || {};
        #[cfg(test)]
        let before_delivery = {
            let cancel = std::mem::take(&mut owner.borrow_mut().cancel_recovery_after_publication);
            let cancellation = cancellation.clone();
            move || {
                if cancel {
                    cancellation.cancel();
                }
            }
        };
        #[cfg(not(test))]
        let before_delivery = || {};
        Self::publish_interrupted_exit_services_with(
            owner,
            request,
            expected,
            generation,
            appearance,
            cancellation,
            app,
            completed,
            before_validate,
            before_delivery,
        )
    }

    fn publish_interrupted_exit_services_with(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        expected: HomeGeneration,
        generation: HomeGeneration,
        appearance: &gpui::Entity<crate::theme_runtime::GpuiAppearanceWindowSet>,
        cancellation: CommandCancellation,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, Result<(), String>, &mut App) + 'static,
        before_validate: impl FnOnce() + Send + 'static,
        before_delivery: impl FnOnce() + Send + 'static,
    ) -> Result<(), String> {
        let (mut services, session_slot, settlement_slot, publication_slot, original, graph, home) = {
            let mut owner = owner.borrow_mut();
            owner.interrupted_exit_services_result(request)?;
            owner.validate_interrupted_exit_bindings(request, appearance, app)?;
            if cancellation.is_cancelled() {
                return Err("Interrupted Exit publication was cancelled".into());
            }
            let recovery = owner.interrupted_exit.as_ref().unwrap();
            let settlement_slot = recovery.settlement.clone();
            let mut slot = settlement_slot.borrow_mut();
            let Some(CandidateSettlement::Services(Ok(graph))) = slot.as_mut() else {
                unreachable!("validated prepared graph bindings")
            };
            let home = graph.appearance().prepared().home().home_id();
            if generation == expected || !graph.matches_candidate(home, generation) {
                return Err("publication belongs to another recovery candidate".into());
            }
            owner
                .process
                .services
                .as_ref()
                .ok_or("The complete service owner is on a worker")?
                .validate_retired_service_home_return(expected, Some(home))
                .map_err(|error| error.to_string())?;
            drop(slot);
            if !owner.release_interrupted_exit_drafts(request, appearance, app)? {
                return Err("Interrupted Exit draft settlement is pending".into());
            }
            let original = recovery.session.borrow_mut().take().unwrap();
            let session_slot = recovery.session.clone();
            let publication_slot = recovery.publication.clone();
            *publication_slot.borrow_mut() = None;
            *recovery.service_validation.borrow_mut() = None;
            let Some(CandidateSettlement::Services(Ok(graph))) = settlement_slot
                .borrow_mut()
                .replace(CandidateSettlement::Pending)
            else {
                unreachable!("exclusive prepared graph custody")
            };
            (
                owner.process.services.take().unwrap(),
                session_slot,
                settlement_slot,
                publication_slot,
                original,
                graph,
                home,
            )
        };
        let identity = request.identity();
        let retained = owner.clone();
        let worker_cancellation = cancellation.clone();
        let work = app.background_executor().spawn(async move {
            let mut prepared = Some(graph);
            let validation = std::panic::catch_unwind(AssertUnwindSafe(|| {
                before_validate();
                if worker_cancellation.is_cancelled() {
                    return Err("Interrupted Exit publication was cancelled".into());
                }
                prepared
                    .as_mut()
                    .unwrap()
                    .revalidate_interrupted_exit_session(home, generation, &original)
            }))
            .unwrap_or_else(|_| Err("Interrupted Exit publication validation unwound".into()));
            let result = validation.and_then(|()| {
                services.publish_recovery_service_graph(
                    expected,
                    generation,
                    &mut prepared,
                    &worker_cancellation,
                )
            });
            before_delivery();
            (services, original, prepared, result)
        });
        app.spawn(async move |cx| {
            let (services, original, prepared, result) = work.await;
            let mut delivered = result.as_ref().map(|_| ()).map_err(Clone::clone);
            retained.borrow_mut().process.services = Some(services);
            *session_slot.borrow_mut() = Some(original);
            *settlement_slot.borrow_mut() = Some(match prepared {
                Some(graph) => CandidateSettlement::Services(Ok(graph)),
                None => CandidateSettlement::Published,
            });
            *publication_slot.borrow_mut() = Some(result);
            {
                let owner = retained.borrow();
                if !owner.process.commands.is_active_identity(&identity)
                    || !owner
                        .interrupted_exit
                        .as_ref()
                        .is_some_and(|recovery| Rc::ptr_eq(&recovery.request, &identity))
                {
                    delivered = Err("Interrupted Exit request changed".into());
                } else if cancellation.is_cancelled() {
                    delivered = Err("Interrupted Exit publication was cancelled".into());
                }
            }
            let _ = cx.update(|app| completed(&retained, delivered, app));
        })
        .detach();
        Ok(())
    }

    pub(crate) fn interrupted_exit_publication_result(
        &self,
        request: &RunningExitRequest,
    ) -> Result<(), String> {
        self.interrupted_exit_graph_retirement_result(request)?;
        self.interrupted_exit
            .as_ref()
            .unwrap()
            .publication
            .borrow()
            .as_ref()
            .ok_or("Interrupted Exit publication has not settled")?
            .as_ref()
            .map(|_| ())
            .map_err(Clone::clone)
    }

    #[cfg(test)]
    pub(crate) fn test_publish_interrupted_exit_services(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        expected: HomeGeneration,
        generation: HomeGeneration,
        appearance: &gpui::Entity<crate::theme_runtime::GpuiAppearanceWindowSet>,
        cancellation: CommandCancellation,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, Result<(), String>, &mut App) + 'static,
        before_validate: impl FnOnce() + Send + 'static,
        before_delivery: impl FnOnce() + Send + 'static,
    ) -> Result<(), String> {
        Self::publish_interrupted_exit_services_with(
            owner,
            request,
            expected,
            generation,
            appearance,
            cancellation,
            app,
            completed,
            before_validate,
            before_delivery,
        )
    }

    #[cfg(test)]
    pub(crate) fn test_replace_interrupted_exit_session(
        &self,
        session: RunningShutdownSession,
    ) -> RunningShutdownSession {
        self.interrupted_exit
            .as_ref()
            .unwrap()
            .session
            .borrow_mut()
            .replace(session)
            .unwrap()
    }

    #[cfg(test)]
    pub(crate) fn test_take_interrupted_exit_start(
        &self,
    ) -> crate::cas_projection::initial_start::InitialStartOwner {
        self.interrupted_exit
            .as_ref()
            .unwrap()
            .publication
            .borrow_mut()
            .take()
            .unwrap()
            .unwrap()
    }
}
