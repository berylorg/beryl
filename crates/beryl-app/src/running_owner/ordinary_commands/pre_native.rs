use super::*;

impl RunningProcessOwner {
    #[cfg(test)]
    pub(crate) fn test_cancelled_ordinary_close_outcomes(
        &self,
    ) -> Option<(Option<bool>, Option<bool>)> {
        self.cancelled_ordinary_close
            .as_ref()
            .map(|close| close.test_outcome_status())
    }

    #[cfg(test)]
    pub(crate) fn test_require_pre_native_close(
        &self,
        identity: &Rc<()>,
        app: &App,
    ) -> Result<(), String> {
        self.require_pre_native_close(identity, app)
    }
    fn pre_native_close_windows(
        &self,
        identity: &Rc<()>,
        app: &App,
    ) -> Result<Vec<gpui::WindowHandle<crate::main_window::MainWindowShellRoot>>, String> {
        if !self.process.commands.is_active_identity(identity) {
            return Err("pre-native close request is unavailable".into());
        }
        let attempt = self
            .shutdown
            .as_ref()
            .ok_or("pre-native close attempt is unavailable")?;
        attempt
            .lease
            .validate()
            .map_err(|error| error.to_string())?;
        let windows = match self.ordinary_close_window {
            Some(window) => vec![window],
            None => self
                .process
                .windows
                .shells()
                .iter()
                .map(|shell| shell.window())
                .collect(),
        };
        if windows.is_empty()
            || !windows.iter().any(|window| {
                window
                    .read(app)
                    .ok()
                    .and_then(|root| root.controller())
                    .is_some_and(|controller| {
                        controller.window_id() == self.shutdown_status().unwrap().0
                    })
            })
        {
            return Err("pre-native close invoking shell is unavailable".into());
        }
        Ok(windows)
    }

    pub(super) fn retain_pre_native_close(
        &mut self,
        identity: &Rc<()>,
        app: &App,
    ) -> Result<(), String> {
        let windows = self.pre_native_close_windows(identity, app)?;
        for window in &windows {
            let shell = self
                .process
                .windows
                .shells_mut()
                .iter_mut()
                .find(|shell| shell.window() == *window)
                .ok_or("pre-native close shell is unavailable")?;
            shell.release_settled_nonfinal_native_if_present()?;
        }
        for window in windows {
            let shell = self
                .process
                .windows
                .shells_mut()
                .iter_mut()
                .find(|shell| shell.window() == window)
                .unwrap();
            let id = shell.retained_window_id(app)?;
            shell.retain_pre_native_close(identity.clone(), id, app)?;
        }
        Ok(())
    }

    pub(super) fn require_pre_native_close(
        &self,
        identity: &Rc<()>,
        app: &App,
    ) -> Result<(), String> {
        for window in self.pre_native_close_windows(identity, app)? {
            self.process
                .windows
                .shells()
                .iter()
                .find(|shell| shell.window() == window)
                .ok_or("pre-native close shell is unavailable")?
                .require_pre_native_close(identity, app)?;
        }
        Ok(())
    }

    fn release_pre_native_close(&mut self, identity: &Rc<()>, app: &App) -> Result<(), String> {
        self.require_pre_native_close(identity, app)?;
        for window in self.pre_native_close_windows(identity, app)? {
            self.process
                .windows
                .shells_mut()
                .iter_mut()
                .find(|shell| shell.window() == window)
                .unwrap()
                .release_pre_native_close(identity, app)?;
        }
        Ok(())
    }

    pub(in crate::running_owner) fn release_pre_native_close_if_present(
        &mut self,
        identity: &Rc<()>,
        app: &App,
    ) -> Result<(), String> {
        let windows = self.pre_native_close_windows(identity, app)?;
        for window in windows {
            self.process
                .windows
                .shells_mut()
                .iter_mut()
                .find(|shell| shell.window() == window)
                .ok_or("pre-native close shell is unavailable")?
                .release_pre_native_close_if_present(identity, app)?;
        }
        Ok(())
    }

    pub(super) async fn restore_pre_native_close(
        owner: &Rc<RefCell<Self>>,
        identity: &Rc<()>,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        let (drafts, window) = cx
            .update(|app| {
                let retained = owner.borrow();
                retained.require_pre_native_close(identity, app)?;
                let windows = retained.pre_native_close_windows(identity, app)?;
                if windows.len() != 1 {
                    return Err(
                        "ordinary close restoration requires its exact one-window membership"
                            .into(),
                    );
                }
                let drafts = retained
                    .shutdown
                    .as_ref()
                    .unwrap()
                    .drafts
                    .as_ref()
                    .ok_or("pre-native close drafts are unavailable")?
                    .clone();
                drafts.borrow_mut().install_detached_sources(app)?;
                Ok::<_, String>((drafts, windows[0]))
            })
            .map_err(|error| error.to_string())??;
        loop {
            let drained = cx
                .update(|app| {
                    owner.borrow().require_pre_native_close(identity, app)?;
                    window
                        .update(app, |root, _, cx| root.drain_detached_shutdown_reads(cx))
                        .map_err(|error| error.to_string())
                })
                .map_err(|error| error.to_string())??;
            if drained {
                break;
            }
            cx.background_executor()
                .timer(Duration::from_millis(10))
                .await;
        }
        Self::restore_retained_pre_native_close(owner, identity, window, &drafts, cx).await?;
        cx.update(|app| {
            let mut retained = owner.borrow_mut();
            retained.require_pre_native_close(identity, app)?;
            if retained
                .process
                .services
                .as_ref()
                .and_then(|services| services.graph())
                .is_none_or(|graph| {
                    graph.home().health().state() != beryl_home_store::HomeHealthState::Healthy
                })
            {
                return Err("pre-native close home failed before cancellation".into());
            }
            retained.release_pre_native_close(identity, app)?;
            let Some(RunningShutdownSession::RemovedWindow(original)) =
                retained.shutdown.as_mut().unwrap().session.take()
            else {
                return Err("pre-native close original outcome is unavailable".into());
            };
            retained.cancelled_ordinary_close = Some(original);
            Ok(())
        })
        .map_err(|error| error.to_string())?
    }
}
