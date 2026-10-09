use super::*;
use beryl_home_store::CommandCancellation;

impl RunningProcessOwner {
    pub(in crate::running_owner) fn prepare_detached_shutdown_sources(
        owner: &Rc<RefCell<Self>>,
        cancellation: crate::cas_projection::ProjectionCancellationToken,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, Result<(), String>, &mut App) + 'static,
    ) {
        let drafts = match Self::shutdown_drafts(owner, false) {
            Ok(drafts) => drafts,
            Err(error) => {
                completed(owner, Err(error), app);
                return;
            }
        };
        let work = (|| {
            let mut drafts = drafts.borrow_mut();
            if !drafts.ready() || drafts.detached_preparing {
                return Err("detached source preparation is unavailable".to_owned());
            }
            if drafts.detached_prepared {
                return Ok(None);
            }
            let mut work = Vec::with_capacity(drafts.windows.len());
            for (window, draft) in &drafts.windows {
                let draft = draft.as_ref().map_err(Clone::clone)?;
                work.push(
                    window
                        .update(app, |root, _, cx| root.detached_export_service(draft, cx))
                        .map_err(|error| error.to_string())??,
                );
            }
            drafts.detached_preparing = true;
            Ok(Some(work))
        })();
        let work = match work {
            Ok(Some(work)) => work,
            Ok(None) => {
                completed(owner, Ok(()), app);
                return;
            }
            Err(error) => {
                completed(owner, Err(error), app);
                return;
            }
        };
        let pool = owner.borrow().detached_read_pool.clone();
        let retained = owner.clone();
        let command_cancellation = CommandCancellation::new();
        if cancellation.is_cancelled() {
            command_cancellation.cancel();
        }
        let signal = command_cancellation.clone();
        let requested = cancellation.clone();
        let monitor = app.spawn(async move |cx| {
            loop {
                if requested.is_cancelled() {
                    signal.cancel();
                    break;
                }
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(10))
                    .await;
            }
        });
        let task = app.background_executor().spawn(async move {
            let cancellation = command_cancellation;
            let mut sources = Vec::with_capacity(work.len());
            for resident in work {
                sources.push(match resident {
                    Some((service, close, selection)) => Some(service.export_detached_source(
                        close,
                        selection,
                        &pool,
                        &cancellation,
                    )?),
                    None => None,
                });
            }
            Ok::<_, String>(sources)
        });
        app.spawn(async move |cx| {
            let result = task.await;
            drop(monitor);
            let result = if cancellation.is_cancelled() {
                Err("detached source preparation cancelled".into())
            } else {
                result
            };
            cx.update(|app| {
                let result = result.and_then(|sources| {
                    let mut drafts = drafts.borrow_mut();
                    if !drafts.ready() || sources.len() != drafts.windows.len() {
                        return Err(
                            "detached source preparation lost its complete draft set".into()
                        );
                    }
                    for ((window, draft), source) in drafts.windows.iter_mut().zip(sources) {
                        let draft = draft.as_mut().map_err(|error| error.clone())?;
                        window
                            .update(app, |root, _, cx| {
                                root.set_detached_shutdown_source(draft, source, cx)
                            })
                            .map_err(|error| error.to_string())??;
                    }
                    drafts.detached_prepared = true;
                    Ok(())
                });
                {
                    let mut drafts = drafts.borrow_mut();
                    drafts.detached_preparing = false;
                    if result.is_err() {
                        drafts.discard_detached_sources();
                    }
                }
                completed(&retained, result, app);
            })
            .expect("detached source preparation retains GUI delivery");
        })
        .detach();
    }
}

impl RunningShutdownDrafts {
    pub(in crate::running_owner) fn settle_destroyed_final_resident(
        &mut self,
        shell: &mut crate::main_window::MainWindowShell,
        app: &mut App,
    ) -> Result<(), String> {
        let (_, draft) = self
            .windows
            .iter_mut()
            .find(|(window, _)| *window == shell.window())
            .ok_or("destroyed final shell has no original draft")?;
        shell.settle_destroyed_final_shutdown_resident(
            draft.as_mut().map_err(|error| error.clone())?,
            app,
        )
    }

    pub(in crate::running_owner) fn advance_destroyed_final_cleanup(
        &self,
        native_window: WindowHandle<MainWindowShellRoot>,
        window: beryl_model::WindowId,
    ) -> Result<bool, String> {
        let (_, draft) = self
            .windows
            .iter()
            .find(|(current, _)| *current == native_window)
            .ok_or("destroyed final cleanup has no original draft")?;
        draft
            .as_ref()
            .map_err(Clone::clone)?
            .advance_destroyed_final_cleanup(window)
    }

    pub(super) fn discard_detached_sources(&mut self) {
        for (_, draft) in &mut self.windows {
            if let Ok(draft) = draft {
                draft.discard_detached_source();
            }
        }
        self.detached_prepared = false;
    }

    pub(in crate::running_owner) fn install_detached_sources(
        &mut self,
        app: &mut App,
    ) -> Result<(), String> {
        if !self.ready() || !self.detached_prepared || self.detached_preparing {
            return Err("final shutdown requires the complete prepared detached set".into());
        }
        for (window, draft) in &self.windows {
            let draft = draft.as_ref().map_err(Clone::clone)?;
            window
                .update(app, |root, _, cx| {
                    root.validate_detached_shutdown_install(draft, cx)
                })
                .map_err(|error| error.to_string())??;
        }
        for (window, draft) in &mut self.windows {
            let draft = draft.as_mut().unwrap();
            window
                .update(app, |root, _, cx| {
                    root.install_detached_shutdown_source(draft, cx)
                })
                .expect("preflighted native shutdown window remains in the same GUI cut");
        }
        Ok(())
    }

    pub(in crate::running_owner) fn retire_final_residents(
        &mut self,
        app: &mut App,
    ) -> Result<bool, String> {
        let mut ready = true;
        #[cfg(test)]
        let mut last_poll = Vec::with_capacity(self.windows.len());
        for (window, draft) in &mut self.windows {
            let draft = draft.as_mut().map_err(|error| error.clone())?;
            #[cfg(test)]
            let mut diagnostic = None;
            let retired = window
                .update(app, |root, _, cx| {
                    let result = root.retire_final_shutdown_draft(draft, cx);
                    #[cfg(test)]
                    if matches!(&result, Ok(false)) {
                        diagnostic = Some(draft.test_final_retirement_diagnostics());
                    }
                    result
                })
                .map_err(|error| error.to_string())??;
            #[cfg(test)]
            last_poll.push(format!(
                "window={window:?},retired={retired},resources={diagnostic:?}"
            ));
            ready &= retired;
        }
        #[cfg(test)]
        {
            self.last_poll = Some(("RetireFinal", last_poll));
        }
        Ok(ready)
    }
}
