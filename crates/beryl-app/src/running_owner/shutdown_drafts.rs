use super::RunningProcessOwner;
use crate::main_window::{
    MainWindowConversationComposerCloseAdvance, MainWindowShellRoot, MainWindowShutdownDraft,
    MainWindowShutdownDraftAdvance, MainWindowShutdownDraftRelease,
};
use gpui::{App, WindowHandle};
use std::{cell::RefCell, rc::Rc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RunningShutdownDraftProgress {
    Pending,
    Ready,
    Released,
}

mod detached;
mod driver;
mod nonfinal_native;
mod recovery;
pub(crate) use driver::RunningShutdownDraftAction;

pub(crate) struct RunningShutdownDrafts {
    windows: Vec<(
        WindowHandle<MainWindowShellRoot>,
        Result<MainWindowShutdownDraft, String>,
    )>,
    driving: bool,
    prepared: bool,
    releasing: bool,
    released: bool,
    ready: bool,
    detached_preparing: bool,
    detached_prepared: bool,
}

impl RunningShutdownDrafts {
    pub(super) fn add_recovery_window(
        &mut self,
        window: WindowHandle<MainWindowShellRoot>,
        app: &mut App,
    ) {
        if self.windows.iter().any(|(captured, _)| *captured == window) {
            let entry = self
                .windows
                .iter_mut()
                .find(|(captured, _)| *captured == window)
                .unwrap();
            if entry.1.as_ref().is_ok_and(|draft| draft.failed.is_some()) {
                return;
            }
            entry.1 = window
                .update(app, |root, _, cx| root.begin_failed_shutdown_draft(cx))
                .map_err(|e| e.to_string())
                .and_then(|r| r);
            return;
        }
        let draft = window
            .update(app, |root, window, cx| {
                let _ = window;
                root.begin_failed_shutdown_draft(cx)
            })
            .map_err(|e| e.to_string())
            .and_then(|r| r);
        self.windows.push((window, draft));
        self.detached_prepared = false;
        self.prepared = true;
    }
    pub(crate) fn recovery_residents(
        &self,
    ) -> Vec<(
        gpui::AnyWindowHandle,
        gpui::EntityId,
        crate::main_window::MainWindowConversationComposerCloseTicket,
    )> {
        self.windows
            .iter()
            .filter_map(|(window, draft)| {
                let (resident, close) = draft.as_ref().ok()?.recovery_resident_identity()?;
                Some(((*window).into(), resident, close))
            })
            .collect()
    }

    pub(super) fn ready(&self) -> bool {
        self.ready && !self.driving && !self.releasing
    }

    pub(super) fn released(&self) -> bool {
        self.released && !self.driving
    }
}

impl RunningProcessOwner {
    #[cfg(test)]
    pub(crate) fn test_add_shutdown_draft_window(
        owner: &Rc<RefCell<Self>>,
        window: WindowHandle<MainWindowShellRoot>,
    ) {
        let drafts = Self::shutdown_drafts(owner, true).unwrap();
        let mut drafts = drafts.borrow_mut();
        assert!(!drafts.prepared);
        drafts
            .windows
            .push((window, Err("draft preparation has not run".into())));
    }

    fn shutdown_drafts(
        owner: &Rc<RefCell<Self>>,
        prepare: bool,
    ) -> Result<Rc<RefCell<RunningShutdownDrafts>>, String> {
        let mut owner = owner.borrow_mut();
        if owner.progress.is_some() || owner.process.services.is_none() {
            return Err(
                "shutdown drafts require returned services and consumed work progress".into(),
            );
        }
        if !owner
            .shutdown
            .as_ref()
            .is_some_and(|attempt| attempt.work_ready)
        {
            return Err("shutdown drafts require retained work readiness".into());
        }
        if let Some(drafts) = owner.shutdown.as_ref().unwrap().drafts.as_ref() {
            return Ok(drafts.clone());
        }
        if !prepare {
            return Err("no shutdown draft obligations are retained".into());
        }
        let windows = owner
            .process
            .windows
            .shells()
            .iter()
            .filter(|shell| {
                owner
                    .ordinary_close_window
                    .is_none_or(|window| window == shell.window())
            })
            .map(|shell| {
                (
                    shell.window(),
                    Err("shutdown draft preparation did not complete".into()),
                )
            })
            .collect::<Vec<_>>();
        if windows.is_empty() {
            return Err("shutdown drafts have no published windows".into());
        }
        let drafts = Rc::new(RefCell::new(RunningShutdownDrafts {
            windows,
            driving: false,
            prepared: false,
            releasing: false,
            released: false,
            ready: false,
            detached_preparing: false,
            detached_prepared: false,
        }));
        owner.shutdown.as_mut().unwrap().drafts = Some(drafts.clone());
        Ok(drafts)
    }

    pub(crate) fn advance_shutdown_drafts(
        owner: &Rc<RefCell<Self>>,
        app: &mut App,
    ) -> Result<RunningShutdownDraftProgress, String> {
        Self::advance_shutdown_drafts_inner(owner, app, false)
    }

    fn advance_shutdown_drafts_inner(
        owner: &Rc<RefCell<Self>>,
        app: &mut App,
        driving: bool,
    ) -> Result<RunningShutdownDraftProgress, String> {
        let retained = Self::shutdown_drafts(owner, true)?;
        let mut drafts = retained
            .try_borrow_mut()
            .map_err(|_| "shutdown drafts are being updated")?;
        if drafts.driving != driving {
            return Err("shutdown draft driver owns progression".into());
        }
        if drafts.releasing {
            return Err("shutdown draft recovery has already started".into());
        }
        if !drafts.prepared {
            #[cfg(test)]
            {
                let mut owner = owner.borrow_mut();
                if owner.shutdown.as_ref().is_some_and(|attempt| {
                    matches!(
                        attempt.intent(),
                        super::ShutdownIntent::NonfinalWindowClose
                            | super::ShutdownIntent::FinalWindowClose
                            | super::ShutdownIntent::ApplicationExit
                    )
                }) {
                    if let Some(hook) = owner.before_ordinary_draft_prepare.take() {
                        hook(
                            owner
                                .process
                                .services
                                .as_ref()
                                .unwrap()
                                .graph()
                                .unwrap()
                                .home(),
                        );
                    }
                }
            }
            for (window, preparation) in &mut drafts.windows {
                *preparation = window
                    .update(app, |root, window, cx| {
                        root.begin_shutdown_draft(window, cx)
                    })
                    .map_err(|error| format!("shutdown draft window is unavailable: {error}"))
                    .and_then(|result| result);
            }
            drafts.prepared = true;
        }
        let mut failure = None;
        let mut pending = false;
        for (window, preparation) in &drafts.windows {
            let result = match preparation {
                Ok(preparation) => window
                    .update(app, |root, window, cx| {
                        root.advance_shutdown_draft(preparation, window, cx)
                    })
                    .map_err(|error| format!("shutdown draft window is unavailable: {error}"))
                    .and_then(|result| result),
                Err(error) => Err(error.clone()),
            };
            match result {
                Ok(
                    MainWindowShutdownDraftAdvance::Threadless
                    | MainWindowShutdownDraftAdvance::Resident(
                        MainWindowConversationComposerCloseAdvance::Ready,
                    ),
                ) => {}
                Ok(MainWindowShutdownDraftAdvance::Resident(
                    MainWindowConversationComposerCloseAdvance::Preparing
                    | MainWindowConversationComposerCloseAdvance::Progress(_)
                    | MainWindowConversationComposerCloseAdvance::ReconciliationPending,
                )) => pending = true,
                Ok(other) => {
                    failure.get_or_insert_with(|| {
                        format!("shutdown draft cannot become ready: {other:?}")
                    });
                }
                Err(error) => {
                    failure.get_or_insert(error);
                }
            }
        }
        drafts.ready = failure.is_none() && !pending;
        match failure {
            Some(error) => Err(error),
            None if pending => Ok(RunningShutdownDraftProgress::Pending),
            None => Ok(RunningShutdownDraftProgress::Ready),
        }
    }

    pub(crate) fn release_shutdown_drafts(
        owner: &Rc<RefCell<Self>>,
        app: &mut App,
    ) -> Result<RunningShutdownDraftProgress, String> {
        Self::release_shutdown_drafts_inner(owner, app, false)
    }

    fn release_shutdown_drafts_inner(
        owner: &Rc<RefCell<Self>>,
        app: &mut App,
        driving: bool,
    ) -> Result<RunningShutdownDraftProgress, String> {
        Self::require_shutdown_session_released(owner)?;
        Self::require_shutdown_placements_settled(owner)?;
        let retained = Self::shutdown_drafts(owner, false)?;
        let mut drafts = retained
            .try_borrow_mut()
            .map_err(|_| "shutdown drafts are being updated")?;
        if drafts.driving != driving {
            return Err("shutdown draft driver owns progression".into());
        }
        if drafts.detached_preparing {
            return Err("detached source acquisition owns the draft set".into());
        }
        drafts.discard_detached_sources();
        drafts.releasing = true;
        drafts.released = false;
        let mut failure = None;
        let mut pending = false;
        for (window, preparation) in &drafts.windows {
            let Ok(preparation) = preparation else {
                continue;
            };
            let result = window
                .update(app, |root, window, cx| {
                    root.release_shutdown_draft(preparation, window, cx)
                })
                .map_err(|error| format!("shutdown draft window is unavailable: {error}"))
                .and_then(|result| result);
            match result {
                Ok(MainWindowShutdownDraftRelease::Released) => {}
                Ok(MainWindowShutdownDraftRelease::Pending) => pending = true,
                Err(error) => {
                    failure.get_or_insert(error);
                }
            }
        }
        match failure {
            Some(error) => Err(error),
            None if pending => Ok(RunningShutdownDraftProgress::Pending),
            None => {
                drafts.released = true;
                Ok(RunningShutdownDraftProgress::Released)
            }
        }
    }
}
