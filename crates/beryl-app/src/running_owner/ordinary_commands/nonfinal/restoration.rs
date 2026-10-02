use super::*;

#[derive(Clone, Copy)]
enum CloseRestorationAuthority {
    NativeSurvival,
    PreNative,
}

impl CloseRestorationAuthority {
    fn reattach(
        self,
        owner: &Rc<RefCell<RunningProcessOwner>>,
        identity: &Rc<()>,
        window: gpui::WindowHandle<crate::main_window::MainWindowShellRoot>,
        drafts: &Rc<RefCell<crate::running_owner::shutdown_drafts::RunningShutdownDrafts>>,
        restored: Option<(
            &beryl_state::SessionWindowRecord,
            Option<crate::main_window::MainWindowConversationComposerCloseTicket>,
        )>,
        app: &mut App,
    ) -> Result<(), String> {
        match self {
            Self::NativeSurvival => drafts
                .borrow_mut()
                .reattach_nonfinal_native(window, restored, app),
            Self::PreNative => {
                let mut retained = owner.borrow_mut();
                retained.require_pre_native_close(identity, app)?;
                let shell = retained
                    .process
                    .windows
                    .shells_mut()
                    .iter_mut()
                    .find(|shell| shell.window() == window)
                    .ok_or("pre-native recovery shell is unavailable")?;
                drafts
                    .borrow_mut()
                    .reattach_pre_native_close(shell, identity, restored, app)
            }
        }
    }
    fn validate(
        self,
        owner: &RunningProcessOwner,
        identity: &Rc<()>,
        window: gpui::WindowHandle<crate::main_window::MainWindowShellRoot>,
        app: &App,
    ) -> Result<(), String> {
        if !owner.process.commands.is_active_identity(identity) {
            return Err("retained close request changed".into());
        }
        match self {
            Self::NativeSurvival => owner
                .process
                .windows
                .shells()
                .iter()
                .find(|shell| shell.window() == window)
                .ok_or("surviving nonfinal shell is unavailable")?
                .require_nonfinal_native_survival(),
            Self::PreNative => owner.require_pre_native_close(identity, app),
        }
    }
}

impl RunningProcessOwner {
    #[cfg(test)]
    pub(crate) fn test_fail_next_nonfinal_native_close(
        &mut self,
        window: gpui::WindowHandle<crate::main_window::MainWindowShellRoot>,
        fault: gpui::WindowsNativeWindowDestructionTestFault,
    ) {
        self.process
            .windows
            .shells_mut()
            .iter_mut()
            .find(|shell| shell.window() == window)
            .unwrap()
            .test_fail_next_nonfinal_native_close(fault);
    }

    #[cfg(test)]
    pub(crate) fn test_before_next_native_close_restoration(
        &mut self,
        hook: impl FnOnce(&beryl_home_store::HomeStore) + Send + 'static,
    ) {
        self.before_native_close_restoration = Some(Box::new(hook));
    }

    pub(super) async fn restore_surviving_nonfinal_close(
        owner: &Rc<RefCell<Self>>,
        identity: &Rc<()>,
        window: gpui::WindowHandle<crate::main_window::MainWindowShellRoot>,
        drafts: &Rc<RefCell<crate::running_owner::shutdown_drafts::RunningShutdownDrafts>>,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        Self::restore_retained_close(
            owner,
            identity,
            window,
            drafts,
            CloseRestorationAuthority::NativeSurvival,
            cx,
        )
        .await
    }

    pub(in crate::running_owner::ordinary_commands) async fn restore_retained_pre_native_close(
        owner: &Rc<RefCell<Self>>,
        identity: &Rc<()>,
        window: gpui::WindowHandle<crate::main_window::MainWindowShellRoot>,
        drafts: &Rc<RefCell<crate::running_owner::shutdown_drafts::RunningShutdownDrafts>>,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        Self::restore_retained_close(
            owner,
            identity,
            window,
            drafts,
            CloseRestorationAuthority::PreNative,
            cx,
        )
        .await
    }

    async fn restore_retained_close(
        owner: &Rc<RefCell<Self>>,
        identity: &Rc<()>,
        window: gpui::WindowHandle<crate::main_window::MainWindowShellRoot>,
        drafts: &Rc<RefCell<crate::running_owner::shutdown_drafts::RunningShutdownDrafts>>,
        authority: CloseRestorationAuthority,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let (mut original, services, service) = cx
            .update(|app| {
                let service = drafts.borrow().retained_native_close_service(window)?;
                let mut retained = owner.borrow_mut();
                authority.validate(&retained, identity, window, app)?;
                if !retained.process.commands.is_active_identity(identity) {
                    return Err("surviving nonfinal request is unavailable".into());
                }
                if retained.process.services.is_none() {
                    return Err("surviving nonfinal graph is unavailable".into());
                }
                let original = match retained.shutdown.as_mut().unwrap().session.take() {
                    Some(RunningShutdownSession::RemovedWindow(original)) => original,
                    other => {
                        retained.shutdown.as_mut().unwrap().session = other;
                        return Err("surviving nonfinal removal outcome is unavailable".into());
                    }
                };
                retained.shutdown.as_mut().unwrap().session = Some(RunningShutdownSession::Pending);
                let services = retained
                    .process
                    .services
                    .take()
                    .ok_or("surviving nonfinal graph is unavailable")?;
                let _ = app;
                Ok::<_, String>((original, services, service))
            })
            .map_err(|e| e.to_string())??;
        #[cfg(test)]
        let before_restoration = owner.borrow_mut().before_native_close_restoration.take();
        let work = cx.background_executor().spawn(async move {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let graph = services
                    .graph()
                    .ok_or("surviving nonfinal graph is unavailable")?;
                #[cfg(test)]
                if let Some(hook) = before_restoration {
                    hook(graph.home());
                }
                let record = original.restore_healthy(graph.home(), &graph.state().session())?;
                let fresh = match service.as_ref() {
                    Some((service, close)) => {
                        let (fresh, authenticated) = service.rebind_surviving_native_close(
                            graph.state(),
                            original.removal_evidence(),
                            *close,
                        )?;
                        if authenticated != record {
                            return Err(
                                "surviving nonfinal resident and window reads differ".into()
                            );
                        }
                        Some(fresh)
                    }
                    None if record.selected_thread().is_none() => None,
                    None => {
                        return Err("surviving nonfinal selected resident is unavailable".into());
                    }
                };
                Ok::<_, String>((record, fresh))
            }))
            .unwrap_or_else(|_| Err("surviving nonfinal restoration worker unwound".into()));
            drop(service);
            (original, services, result)
        });
        let (original, services, result) = work.await;
        cx.update(|app| {
            {
                let mut retained = owner.borrow_mut();
                retained.process.services = Some(services);
                retained
                    .shutdown
                    .as_mut()
                    .ok_or("surviving nonfinal attempt disappeared")?
                    .session = Some(RunningShutdownSession::RemovedWindow(original));
                if !retained.process.commands.is_active_identity(identity) {
                    return Err("surviving nonfinal request changed".into());
                }
                authority.validate(&retained, identity, window, app)?;
            }
            match result {
                Ok((record, fresh)) => {
                    authority.reattach(
                        owner,
                        identity,
                        window,
                        drafts,
                        Some((&record, fresh)),
                        app,
                    )?;
                    if Self::ordinary_close_home_unavailable(owner) {
                        return Err("surviving nonfinal home failed before reopening".into());
                    }
                    Ok(())
                }
                Err(error) => {
                    if Self::ordinary_close_home_unavailable(owner) {
                        authority.reattach(owner, identity, window, drafts, None, app)?;
                    }
                    Err(error)
                }
            }
        })
        .map_err(|e| e.to_string())??;
        loop {
            let released = cx
                .update(|app| {
                    if Self::ordinary_close_home_unavailable(owner) {
                        return Err("surviving nonfinal home failed during reopening".into());
                    }
                    {
                        let retained = owner.borrow();
                        if !retained.process.commands.is_active_identity(identity) {
                            return Err("surviving nonfinal release request changed".into());
                        }
                        authority.validate(&retained, identity, window, app)?;
                    }
                    if !drafts
                        .borrow_mut()
                        .release_restored_native_close(window, app)?
                    {
                        return Ok::<_, String>(false);
                    }
                    if matches!(authority, CloseRestorationAuthority::PreNative) {
                        return Ok(true);
                    }
                    window
                        .update(app, |root, _, cx| {
                            root.set_shutdown_interaction_gated(false, cx)?;
                            root.set_ordinary_close_interaction_gated(false, cx);
                            Ok::<_, String>(())
                        })
                        .map_err(|e| e.to_string())??;
                    let mut retained = owner.borrow_mut();
                    retained
                        .process
                        .windows
                        .shells_mut()
                        .iter_mut()
                        .find(|shell| shell.window() == window)
                        .ok_or("surviving nonfinal shell disappeared")?
                        .release_nonfinal_native_survival()?;
                    Ok(true)
                })
                .map_err(|e| e.to_string())??;
            if released {
                return Ok(());
            }
            cx.background_executor()
                .timer(Duration::from_millis(10))
                .await;
        }
    }
}
