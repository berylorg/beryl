use super::*;
use crate::main_window::{observe_windows_desktop, windows_window_placement_from_capture};
use beryl_model::{WindowId, WindowPlacement};
use std::{panic::AssertUnwindSafe, sync::Arc};

pub(super) struct RunningShutdownPlacements {
    settled: bool,
    result: Option<Result<Vec<(WindowId, WindowPlacement)>, String>>,
}

impl RunningProcessOwner {
    pub(super) fn require_shutdown_placements_settled(
        owner: &Rc<RefCell<Self>>,
    ) -> Result<(), String> {
        if owner.borrow().shutdown.as_ref().is_some_and(|attempt| {
            attempt
                .placements
                .as_ref()
                .is_some_and(|capture| !capture.try_borrow().is_ok_and(|capture| capture.settled))
        }) {
            return Err("shutdown placement native operations have not settled".into());
        }
        Ok(())
    }

    fn require_shutdown_placement_readiness(&self) -> Result<(), String> {
        if self.progress.is_some()
            || self.process.services.is_none()
            || !matches!(
                self.shutdown_status(),
                Some((
                    _,
                    ShutdownIntent::ApplicationExit,
                    RunningShutdownStatus::WorkReady
                ))
            )
            || !self.shutdown.as_ref().is_some_and(|attempt| {
                attempt
                    .drafts
                    .as_ref()
                    .is_some_and(|drafts| drafts.try_borrow().is_ok_and(|drafts| drafts.ready()))
            })
        {
            return Err("Exit placement requires retained work and draft readiness".into());
        }
        Ok(())
    }

    pub(crate) fn shutdown_placements(&self) -> Result<Vec<(WindowId, WindowPlacement)>, String> {
        self.require_shutdown_placement_readiness()?;
        let capture = self
            .shutdown
            .as_ref()
            .unwrap()
            .placements
            .as_ref()
            .ok_or("Exit placement capture has not started")?
            .borrow();
        if !capture.settled {
            return Err("Exit placement native settlement is unproven".into());
        }
        capture
            .result
            .clone()
            .ok_or_else(|| "Exit placement capture is pending".to_owned())?
    }

    pub(crate) fn capture_shutdown_placements(
        owner: &Rc<RefCell<Self>>,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, Result<(), String>, &mut App) + 'static,
    ) -> Result<(), String> {
        Self::capture_shutdown_placements_with(owner, app, completed, |_| {})
    }

    fn capture_shutdown_placements_with(
        owner: &Rc<RefCell<Self>>,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, Result<(), String>, &mut App) + 'static,
        before_observe: impl Fn(usize) + Send + Sync + 'static,
    ) -> Result<(), String> {
        let (windows, capture) = {
            let mut owner = owner.borrow_mut();
            owner.require_shutdown_placement_readiness()?;
            if owner.shutdown.as_ref().unwrap().placements.is_some() {
                return Err("Exit placement capture is already retained".into());
            }
            let published = &owner.process.windows;
            if published.window_ids().is_empty()
                || published.window_ids().len() != published.shells().len()
            {
                return Err("Exit placement requires a complete published window set".into());
            }
            let windows = published
                .window_ids()
                .iter()
                .copied()
                .map(|id| {
                    let shell = published
                        .shells()
                        .iter()
                        .find(|shell| {
                            shell
                                .retained_window_id(app)
                                .is_ok_and(|captured| captured == id)
                        })
                        .ok_or("Exit placement published window identity is unavailable")?;
                    Ok::<_, String>((id, shell.window()))
                })
                .collect::<Result<Vec<_>, _>>()?;
            let capture = Rc::new(RefCell::new(RunningShutdownPlacements {
                settled: false,
                result: None,
            }));
            owner.shutdown.as_mut().unwrap().placements = Some(capture.clone());
            (windows, capture)
        };
        let owner = owner.clone();
        let before_observe = Arc::new(before_observe);
        app.spawn(async move |cx| {
            let mut placements = Vec::with_capacity(windows.len());
            let mut failure = None;
            let mut settled = true;
            for (index, (id, window)) in windows.into_iter().enumerate() {
                let admitted = cx.update(|app| {
                    window
                        .update(app, |root, window, _| {
                            if root
                                .controller()
                                .is_none_or(|controller| controller.window_id() != id)
                            {
                                return Err("Exit placement window identity changed".to_owned());
                            }
                            let geometry =
                                window.capture_windows_window_placement().map_err(|error| {
                                    format!("Exit geometry capture failed: {error}")
                                })?;
                            let (lease, released) =
                                window.lease_published_windows_window().map_err(|error| {
                                    format!("Exit desktop lease admission failed: {error}")
                                })?;
                            Ok((geometry, lease, released))
                        })
                        .map_err(|error| format!("Exit placement window unavailable: {error}"))
                        .and_then(|result| result)
                });
                let Ok(admitted) = admitted else { return };
                let (geometry, lease, released) = match admitted {
                    Ok(admitted) => admitted,
                    Err(error) => {
                        failure.get_or_insert_with(|| format!("window {id}: {error}"));
                        continue;
                    }
                };
                let before_observe = before_observe.clone();
                let observed = cx
                    .background_executor()
                    .spawn(async move {
                        std::panic::catch_unwind(AssertUnwindSafe(move || {
                            before_observe(index);
                            observe_windows_desktop(lease)
                        }))
                        .map_err(|_| "Exit desktop worker unwound".to_owned())
                        .and_then(|result| {
                            result.map_err(|error| {
                                format!("Exit desktop observation failed: {error:?}")
                            })
                        })
                    })
                    .await;
                let release = released.await;
                let placement = observed.and_then(|desktop| {
                    windows_window_placement_from_capture(geometry, Some(desktop))
                        .map_err(|error| format!("Exit placement conversion failed: {error}"))
                });
                let settlement_error = match release {
                    Err(error) => {
                        settled = false;
                        Some(format!("Exit desktop native settlement failed: {error}"))
                    }
                    Ok(release) if release.native_destroyed => {
                        Some("Exit placement native window was destroyed".to_owned())
                    }
                    Ok(_) => None,
                };
                let result = match (placement, settlement_error) {
                    (Ok(_), Some(error)) => Err(error),
                    (Err(original), Some(error)) => Err(format!("{original}; {error}")),
                    (result, None) => result,
                };
                match result {
                    Ok(placement) => placements.push((id, placement)),
                    Err(error) => {
                        failure.get_or_insert_with(|| format!("window {id}: {error}"));
                    }
                }
            }
            let result = failure.map_or_else(|| Ok(placements), Err);
            let completion = result.as_ref().map(|_| ()).map_err(Clone::clone);
            let _ = cx.update(|app| {
                if !owner.borrow().shutdown.as_ref().is_some_and(|attempt| {
                    attempt
                        .placements
                        .as_ref()
                        .is_some_and(|current| Rc::ptr_eq(current, &capture))
                }) {
                    return;
                }
                {
                    let mut capture = capture.borrow_mut();
                    capture.settled = settled;
                    capture.result = Some(result);
                }
                completed(&owner, completion, app);
            });
        })
        .detach();
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn test_capture_shutdown_placements(
        owner: &Rc<RefCell<Self>>,
        app: &mut App,
        completed: impl FnOnce(&Rc<RefCell<Self>>, Result<(), String>, &mut App) + 'static,
        before_observe: impl Fn(usize) + Send + Sync + 'static,
    ) -> Result<(), String> {
        Self::capture_shutdown_placements_with(owner, app, completed, before_observe)
    }
}
