use super::*;

impl RunningProcessOwner {
    pub(in crate::running_owner) fn ordinary_close_home_unavailable(
        owner: &Rc<RefCell<Self>>,
    ) -> bool {
        owner
            .borrow()
            .process
            .services
            .as_ref()
            .and_then(|s| s.graph())
            .is_none_or(|graph| {
                graph.home().health().state() != beryl_home_store::HomeHealthState::Healthy
            })
    }

    pub(in crate::running_owner) fn prepare_ordinary_close_recovery(
        owner: &Rc<RefCell<Self>>,
        app: &mut App,
    ) -> Result<(), String> {
        Self::install_shutdown_interaction_gate(owner, app)?;
        let (windows, drafts) = {
            let owner = owner.borrow();
            (
                owner
                    .process
                    .windows
                    .shells()
                    .iter()
                    .map(|s| s.window())
                    .collect::<Vec<_>>(),
                owner.shutdown.as_ref().and_then(|a| a.drafts.clone()),
            )
        };
        if owner.borrow().shutdown_session().is_none() {
            let members = windows
                .iter()
                .map(|window| {
                    let controller = window
                        .read(app)
                        .map_err(|e| e.to_string())?
                        .controller()
                        .ok_or("surviving window controller is unavailable")?;
                    let claim = controller
                        .composer_mount()
                        .map(|mount| {
                            mount
                                .read(app)
                                .contribution()
                                .ok_or("surviving selected resident is unavailable")
                                .map(|resident| resident.read(app).selection_identity().claim())
                        })
                        .transpose()?;
                    Ok((controller.window_id(), claim))
                })
                .collect::<Result<Vec<_>, String>>()?;
            let mut retained = owner.borrow_mut();
            let graph = retained
                .process
                .services
                .as_ref()
                .and_then(|s| s.graph())
                .ok_or("surviving home graph is unavailable")?;
            let windows = super::unremoved_windows::UnremovedWindows::new(
                graph.home().home_id(),
                graph.home().canonical_path().to_owned(),
                members,
            );
            retained
                .shutdown
                .as_mut()
                .ok_or("ordinary close attempt is unavailable")?
                .session = Some(RunningShutdownSession::UnremovedWindows(windows));
        }
        if let Some(drafts) = drafts {
            for window in windows {
                drafts.borrow_mut().add_recovery_window(window, app);
            }
        }
        owner.borrow_mut().ordinary_close_window = None;
        Ok(())
    }
}
