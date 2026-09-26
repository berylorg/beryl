use super::*;

impl NativeRestoreSetFlight {
    pub(super) async fn run(&mut self, cx: &mut AsyncApp) -> Result<Vec<MainWindowShell>, String> {
        for index in 0..self.members.len() {
            self.check_cancelled()?;
            let state =
                std::mem::replace(&mut self.members[index].state, NativeMemberState::InFlight);
            let NativeMemberState::Prepared {
                prepared,
                placement,
            } = state
            else {
                unreachable!("native startup constructs each fixed member once")
            };
            let construction = cx
                .update(|app| {
                    let mut host = GpuiMainWindowShellHost::new(app, self.appearance.clone())
                        .with_prepared_placement(placement);
                    #[cfg(feature = "test-faults")]
                    if let Some(hook) = &mut self.faults.before_construction {
                        hook(index, &mut host);
                    }
                    host.construct_startup_hidden(*prepared)
                })
                .expect("native startup retains a live GUI executor during construction");
            let mut shell = match construction {
                Ok(shell) => shell,
                Err(MainWindowStartupShellHostFailure::BeforeConstruction { prepared, error }) => {
                    // The placement snapshot is no longer needed for disposal of the original preparation.
                    self.members[index].state =
                        NativeMemberState::Unconstructed(Box::new(prepared));
                    return Err(error);
                }
                Err(MainWindowStartupShellHostFailure::Native { shell, error }) => {
                    self.members[index].state = NativeMemberState::Hidden(shell);
                    return Err(error);
                }
            };
            #[cfg(feature = "test-faults")]
            cx.update(|app| {
                if let Some(hook) = &mut self.faults.before_desktop {
                    hook(index, &mut shell, app);
                }
            })
            .expect("native startup retains a live GUI executor before desktop placement");
            let (sender, completed) = futures_channel::oneshot::channel();
            let admission = cx
                .update(|app| {
                    shell.start_desktop_placement(
                        self.cancellation.cancellation.clone(),
                        move |result, _| {
                            let _ = sender.send(result);
                        },
                        app,
                    )
                })
                .expect("native startup retains a live GUI executor during desktop admission");
            if let Err(failure) = admission {
                self.members[index].state = NativeMemberState::Hidden(failure.shell);
                return Err(format!(
                    "startup desktop admission failed: {:?}",
                    failure.reason
                ));
            }
            match completed
                .await
                .expect("desktop flight invokes its owned completion")
            {
                MainWindowDesktopPlacementCompletion::Ready { mut shell, .. } => {
                    #[cfg(feature = "test-faults")]
                    cx.update(|app| {
                        if let Some(hook) = &mut self.faults.after_desktop {
                            hook(index, &mut shell, app);
                        }
                    })
                    .expect("native startup retains a live GUI executor after desktop placement");
                    self.members[index].state = NativeMemberState::Hidden(shell);
                }
                MainWindowDesktopPlacementCompletion::Rejected { shell, reason } => {
                    self.members[index].state = NativeMemberState::Hidden(shell);
                    return Err(format!("startup desktop placement rejected: {reason:?}"));
                }
            }
        }
        self.wait_until_ready(cx).await?;
        self.final_validation(cx).await?;
        cx.update(|app| self.publish(app))
            .expect("native startup retains a live GUI executor during publication")
    }

    fn check_cancelled(&self) -> Result<(), String> {
        if self.cancellation.is_cancelled() {
            Err("native startup set was cancelled".to_owned())
        } else {
            Ok(())
        }
    }

    fn readiness(&self, app: &mut App) -> Result<bool, String> {
        self.check_cancelled()?;
        let owner = self
            .owner
            .as_ref()
            .expect("native startup retains its attempt");
        owner.attempt.validate_home(&owner.services.store)?;
        if self.members.len() != owner.expected_windows.len()
            || self.members.is_empty()
            || self.members.len() > MAX_RESTORABLE_WINDOWS
        {
            return Err("native startup membership is incomplete".to_owned());
        }
        let mut ready = true;
        for (index, (member, expected)) in
            self.members.iter().zip(&owner.expected_windows).enumerate()
        {
            if member.window_id != *expected {
                return Err("native startup member order or identity changed".to_owned());
            }
            let NativeMemberState::Hidden(shell) = &member.state else {
                return Err("native startup member has no settled hidden shell".to_owned());
            };
            if self.members[..index].iter().any(|previous| {
                matches!(&previous.state, NativeMemberState::Hidden(previous) if previous.window() == shell.window())
            }) {
                return Err("native startup contains duplicate native handles".to_owned());
            }
            ready &= shell.startup_readiness(*expected, app)?;
        }
        #[cfg(feature = "test-faults")]
        if ready && let Some(observed) = &self.faults.hold_readiness {
            observed();
            return Ok(false);
        }
        Ok(ready)
    }

    async fn wait_until_ready(&self, cx: &mut AsyncApp) -> Result<(), String> {
        let subscriptions = cx
            .update(|app| {
                let mut subscriptions = Vec::with_capacity(self.members.len() * 2 + 2);
                for member in &self.members {
                    let NativeMemberState::Hidden(shell) = &member.state else {
                        return Err("startup readiness lost a native member".to_owned());
                    };
                    let root = shell.window().entity(app).map_err(|e| e.to_string())?;
                    let wake = self.cancellation.wake.clone();
                    subscriptions.push(app.observe(&root, move |_, _| wake.notify()));
                    if let Some(composer) = shell.startup_composer_entity(app) {
                        let wake = self.cancellation.wake.clone();
                        subscriptions.push(app.observe(&composer, move |_, _| wake.notify()));
                    }
                }
                let wake = self.cancellation.wake.clone();
                subscriptions.push(app.observe(&self.appearance, move |_, _| wake.notify()));
                let wake = self.cancellation.wake.clone();
                subscriptions.push(app.on_window_closed(move |_| wake.notify()));
                Ok(subscriptions)
            })
            .expect("native startup retains a live GUI executor during observation")?;
        let result = loop {
            let observed = self.cancellation.wake.version();
            match cx
                .update(|app| self.readiness(app))
                .expect("native startup retains a live GUI executor during readiness")
            {
                Ok(true) => break Ok(()),
                Err(error) => break Err(error),
                Ok(false) => self.cancellation.wake.wait(observed).await,
            }
        };
        drop(subscriptions);
        result
    }

    async fn final_validation(&mut self, cx: &mut AsyncApp) -> Result<(), String> {
        self.check_cancelled()?;
        let owner = self
            .owner
            .take()
            .expect("native startup retains its attempt");
        let validation = std::mem::take(&mut self.validation);
        #[cfg(feature = "test-faults")]
        let hook = self.faults.before_final_validation.take();
        let task = cx.background_executor().spawn(async move {
            #[cfg(feature = "test-faults")]
            if let Some(hook) = hook {
                hook();
            }
            let result = owner.revalidate_members(validation.len(), |index, expected| {
                let member = &validation[index];
                if member.window_id != expected {
                    return Err("native validation member identity changed".to_owned());
                }
                (member.check)(&owner.attempt, &owner.services)
            });
            (owner, validation, result)
        });
        let (owner, validation, result) = task.await;
        self.owner = Some(owner);
        self.validation = validation;
        result?;
        self.check_cancelled()
    }

    fn publish(&mut self, app: &mut App) -> Result<Vec<MainWindowShell>, String> {
        if !self.readiness(app)? {
            return Err("startup member lost first-presentable readiness".to_owned());
        }
        let mut shells = Vec::with_capacity(self.members.len());
        for member in &mut self.members {
            let NativeMemberState::Hidden(shell) =
                std::mem::replace(&mut member.state, NativeMemberState::InFlight)
            else {
                unreachable!("whole-set admission proved every hidden shell")
            };
            shells.push(shell);
        }
        let result = self.publish_shells(&mut shells, app);
        match result {
            Ok(()) => Ok(shells),
            Err(error) => {
                for (member, shell) in self.members.iter_mut().zip(shells) {
                    member.state = NativeMemberState::Hidden(shell);
                }
                Err(error)
            }
        }
    }

    fn publish_shells(
        &mut self,
        shells: &mut [MainWindowShell],
        app: &mut App,
    ) -> Result<(), String> {
        for shell in shells.iter_mut() {
            shell.seal_startup_publication()?;
        }
        for (index, shell) in shells.iter_mut().enumerate() {
            self.check_cancelled()?;
            #[cfg(feature = "test-faults")]
            if let Some(hook) = &mut self.faults.before_publication {
                hook(index, shell, app)?;
            }
            self.check_cancelled()?;
            shell.publish(app)?;
        }
        self.check_cancelled()?;
        #[cfg(feature = "test-faults")]
        if let Some(hook) = self.faults.before_interaction_release.take() {
            hook(shells, app);
        }
        self.check_cancelled()?;
        MainWindowShell::release_startup_interaction(shells, app)
    }
}
