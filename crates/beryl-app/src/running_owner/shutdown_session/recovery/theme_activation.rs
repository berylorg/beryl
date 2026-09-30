use super::*;
use beryl_home_store::CommandCancellation;

impl RunningProcessOwner {
    pub(crate) fn activate_interrupted_exit_theme(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        appearance: &gpui::Entity<crate::theme_runtime::GpuiAppearanceWindowSet>,
        cancellation: CommandCancellation,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, Result<(), String>, &mut App) + 'static,
    ) -> Result<(), String> {
        Self::activate_interrupted_exit_theme_with(
            owner,
            request,
            appearance,
            cancellation,
            app,
            completed,
            |_| {},
            || {},
        )
    }

    fn activate_interrupted_exit_theme_with(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        appearance: &gpui::Entity<crate::theme_runtime::GpuiAppearanceWindowSet>,
        cancellation: CommandCancellation,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, Result<(), String>, &mut App) + 'static,
        before_activation: impl FnOnce(&mut ProcessServiceOwner) + Send + 'static,
        before_delivery: impl FnOnce() + Send + 'static,
    ) -> Result<(), String> {
        let (mut services, activation_slot) = {
            let mut owner = owner.borrow_mut();
            owner.interrupted_exit_publication_result(request)?;
            owner.validate_interrupted_exit_bindings(request, appearance, app)?;
            if cancellation.is_cancelled() {
                return Err("Interrupted Exit theme activation was cancelled".into());
            }
            let activation_slot = owner
                .interrupted_exit
                .as_ref()
                .unwrap()
                .theme_activation
                .clone();
            if activation_slot.borrow().is_some() {
                return Err("Interrupted Exit theme activation was already attempted".into());
            }
            (owner.process.services.take().unwrap(), activation_slot)
        };
        let identity = request.identity();
        let retained = owner.clone();
        let worker_cancellation = cancellation.clone();
        let work = app.background_executor().spawn(async move {
            let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
                before_activation(&mut services);
                if worker_cancellation.is_cancelled() {
                    return Err("Interrupted Exit theme activation was cancelled".into());
                }
                services
                    .graph_mut()
                    .ok_or("Published recovery graph is unavailable")?
                    .release_theme()
                    .map_err(|error| format!("Recovery theme activation failed: {error:?}"))
            }))
            .unwrap_or_else(|_| Err("Interrupted Exit theme activation unwound".into()));
            before_delivery();
            (services, result)
        });
        app.spawn(async move |cx| {
            let (services, result) = work.await;
            let mut delivered = result.clone();
            retained.borrow_mut().process.services = Some(services);
            *activation_slot.borrow_mut() = Some(result);
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
                    delivered = Err("Interrupted Exit theme activation was cancelled".into());
                }
            }
            let _ = cx.update(|app| completed(&retained, delivered, app));
        })
        .detach();
        Ok(())
    }

    pub(crate) fn interrupted_exit_theme_activation_result(
        &self,
        request: &RunningExitRequest,
    ) -> Result<(), String> {
        self.interrupted_exit_publication_result(request)?;
        self.interrupted_exit
            .as_ref()
            .unwrap()
            .theme_activation
            .borrow()
            .as_ref()
            .ok_or("Interrupted Exit theme activation has not settled")?
            .clone()
    }

    #[cfg(test)]
    pub(crate) fn test_activate_interrupted_exit_theme(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        appearance: &gpui::Entity<crate::theme_runtime::GpuiAppearanceWindowSet>,
        cancellation: CommandCancellation,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, Result<(), String>, &mut App) + 'static,
        before_activation: impl FnOnce(&mut ProcessServiceOwner) + Send + 'static,
        before_delivery: impl FnOnce() + Send + 'static,
    ) -> Result<(), String> {
        Self::activate_interrupted_exit_theme_with(
            owner,
            request,
            appearance,
            cancellation,
            app,
            completed,
            before_activation,
            before_delivery,
        )
    }
}
