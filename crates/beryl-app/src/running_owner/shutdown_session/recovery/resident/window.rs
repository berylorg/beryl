use super::*;
use crate::main_window::{MainWindowConversationComposerConfig, MainWindowShellRoot};
use gpui::WindowHandle;

impl RunningProcessOwner {
    pub(crate) fn prepare_interrupted_exit_window_resident(
        owner: &Rc<RefCell<Self>>,
        request: &RunningExitRequest,
        window: WindowHandle<MainWindowShellRoot>,
        generation: beryl_home_store::HomeGeneration,
        retired: &mut Option<MainWindowComposerRetiredClose>,
        configure: impl FnOnce(
            RangeRestorationSeed,
            MainWindowComposerSelectionIdentity,
            &Window,
        ) -> Result<
            (MainWindowConversationComposerConfig, RangeSurfaceCharge),
            String,
        > + 'static,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, &mut App) + 'static,
    ) -> Result<ResidentPreparationKey, String> {
        let (mount, resident, close) = {
            let retained = owner.borrow();
            retained.interrupted_exit_services_result(request)?;
            if !retained
                .process
                .windows
                .shells()
                .iter()
                .any(|shell| shell.window() == window)
            {
                return Err("Recovery window is not retained by the process".into());
            }
            let root = window.read(app).map_err(|error| error.to_string())?;
            let mount = root
                .controller()
                .and_then(|controller| controller.composer_mount())
                .ok_or("Recovery window has no selected composer mount")?;
            let resident = mount
                .read(app)
                .contribution()
                .ok_or("Recovery window has no resident composer")?;
            let close = retained
                .interrupted_exit
                .as_ref()
                .ok_or("No reported failed Exit")?
                .residents
                .iter()
                .find(|(captured, entity, _)| {
                    *captured == window.into() && *entity == resident.entity_id()
                })
                .map(|(_, _, close)| *close)
                .ok_or("Resident is not captured by the interrupted Exit")?;
            (mount, resident, close)
        };
        let capture = {
            let mut retained = owner.borrow_mut();
            retained
                .shutdown
                .as_mut()
                .and_then(|attempt| attempt.drafts.as_ref())
                .ok_or("failed resident draft set is unavailable")?
                .borrow_mut()
                .take_failed_capture(window)?
        };
        if let Some(capture) = capture {
            let _ = mount;
            let prepared = Self::prepare_failed_interrupted_exit_resident(
                owner,
                request,
                &resident,
                close,
                window.into(),
                generation,
                capture,
                Box::new(move |seed, selection, window| {
                    let (config, capacity) = configure(seed, selection, window)?;
                    Ok((
                        config.resident_recovery_environment(seed, window)?,
                        capacity,
                    ))
                }),
                app,
                Box::new(completed),
            );
            return match prepared {
                Ok(key) => Ok(key),
                Err((capture, error)) => {
                    owner
                        .borrow_mut()
                        .shutdown
                        .as_mut()
                        .unwrap()
                        .drafts
                        .as_ref()
                        .unwrap()
                        .borrow_mut()
                        .return_failed_capture(window, capture);
                    Err(error)
                }
            };
        }
        if retired.is_none() {
            *retired = mount.update(app, |mount, cx| {
                mount.take_interrupted_exit_retirement(close, cx)
            })?;
        }
        Self::prepare_interrupted_exit_resident(
            owner,
            request,
            &resident,
            close,
            window.into(),
            generation,
            retired,
            move |seed, selection, window| {
                let (config, capacity) = configure(seed, selection, window)?;
                let environment = config.resident_recovery_environment(seed, window)?;
                Ok((environment, capacity))
            },
            app,
            completed,
        )
    }
}
