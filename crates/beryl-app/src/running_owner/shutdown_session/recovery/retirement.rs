use super::*;
use beryl_home_store::HomeGeneration;

pub(super) enum GraphRetirement {
    Pending,
    Returned(Result<(), String>),
}

impl RunningProcessOwner {
    pub(crate) fn retire_interrupted_exit_graph(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        generation: HomeGeneration,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, &mut App) + 'static,
    ) -> Result<(), String> {
        Self::retire_interrupted_exit_graph_with(owner, request, generation, app, completed, || {})
    }

    fn retire_interrupted_exit_graph_with(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        generation: HomeGeneration,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, &mut App) + 'static,
        before_retire: impl FnOnce() + Send + 'static,
    ) -> Result<(), String> {
        let (mut services, result_slot) = {
            let mut owner = owner.borrow_mut();
            let recovery = owner
                .interrupted_exit
                .as_ref()
                .ok_or("No reported failed Exit")?;
            if !Rc::ptr_eq(&recovery.request, &request.identity())
                || !owner.process.commands.is_active(request)
            {
                return Err("Interrupted Exit request changed".into());
            }
            if recovery.session.borrow().is_none()
                || recovery.settlement.borrow().is_some()
                || recovery.resident.is_some()
            {
                return Err(
                    "Interrupted Exit session custody is unavailable or candidate work exists"
                        .into(),
                );
            }
            if recovery.retirement.borrow().is_some() {
                return Err("Interrupted Exit graph retirement is already retained".into());
            }
            let result_slot = recovery.retirement.clone();
            owner
                .process
                .services
                .as_ref()
                .ok_or("The complete service owner is on a worker")?
                .validate_failed_service_graph_retirement(generation)
                .map_err(|error| error.to_string())?;
            if !owner.retire_interrupted_exit_residents(request, app)? {
                return Err("Interrupted Exit resident retirement is not ready".into());
            }
            *result_slot.borrow_mut() = Some(GraphRetirement::Pending);
            (owner.process.services.take().unwrap(), result_slot)
        };
        let retained = owner.clone();
        let work = app.background_executor().spawn(async move {
            let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
                before_retire();
                services
                    .retire_failed_service_graph(generation)
                    .map_err(|error| error.to_string())
            }))
            .unwrap_or_else(|_| Err("Interrupted Exit graph retirement unwound".into()));
            (services, result)
        });
        app.spawn(async move |cx| {
            let (services, result) = work.await;
            retained.borrow_mut().process.services = Some(services);
            *result_slot.borrow_mut() = Some(GraphRetirement::Returned(result));
            let _ = cx.update(|app| completed(&retained, app));
        })
        .detach();
        Ok(())
    }

    pub(crate) fn interrupted_exit_graph_retirement_result(
        &self,
        request: &RunningExitRequest,
    ) -> Result<(), String> {
        let recovery = self
            .interrupted_exit
            .as_ref()
            .ok_or("No reported failed Exit")?;
        if !Rc::ptr_eq(&recovery.request, &request.identity())
            || !self.process.commands.is_active(request)
        {
            return Err("Interrupted Exit request changed".into());
        }
        match recovery.retirement.borrow().as_ref() {
            Some(GraphRetirement::Returned(result)) => result.clone(),
            _ => Err("Interrupted Exit graph retirement has not returned".into()),
        }
    }

    #[cfg(test)]
    pub(crate) fn test_retire_interrupted_exit_graph(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        generation: HomeGeneration,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, &mut App) + 'static,
        before_retire: impl FnOnce() + Send + 'static,
    ) -> Result<(), String> {
        Self::retire_interrupted_exit_graph_with(
            owner,
            request,
            generation,
            app,
            completed,
            before_retire,
        )
    }
}
