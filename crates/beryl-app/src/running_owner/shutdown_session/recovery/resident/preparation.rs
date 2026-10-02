use super::*;
use crate::main_window::{
    MainWindowFailedComposerRetirement, MainWindowFailedResidentCandidateSource,
    MainWindowFailedResidentCapture, MainWindowFailedResidentPreparation,
};

pub(super) enum Preparation {
    Clean(Box<MainWindowComposerRecoveryPreparation<PreparedRecoveryServiceGraph>>),
    Failed(Box<MainWindowFailedResidentPreparation<PreparedRecoveryServiceGraph>>),
}

pub(super) enum ReturnedPreparation {
    Clean(Box<CancelledResidentPreparation>),
    Failed(
        Box<(
            PreparedRecoveryServiceGraph,
            Result<
                MainWindowFailedResidentCandidateSource,
                (MainWindowFailedComposerRetirement, String),
            >,
            MainWindowFailedResidentCapture,
        )>,
    ),
}

impl Preparation {
    pub(super) fn worker_pending(&self) -> bool {
        match self {
            Self::Clean(p) => p.worker_pending(),
            Self::Failed(p) => p.worker_pending(),
        }
    }
    pub(super) fn authenticated_source(
        &self,
    ) -> Result<Option<(RangeRestorationSeed, MainWindowComposerSelectionIdentity)>, String> {
        match self {
            Self::Clean(p) => p.authenticated_source(),
            Self::Failed(p) => {
                if let Some(error) = p.preparation_error() {
                    return Err(error);
                }
                Ok(p.authenticated_source())
            }
        }
    }
    pub(super) fn authenticated_window(&self) -> Result<beryl_state::SessionWindowRecord, String> {
        match self {
            Self::Clean(p) => p.authenticated_window(),
            Self::Failed(p) => p.authenticated_window(),
        }
    }
    pub(super) fn cancel(&mut self) {
        match self {
            Self::Clean(p) => p.cancel(),
            Self::Failed(p) => p.cancel(),
        }
    }
    pub(super) fn advance_cleanup(&mut self) -> Result<bool, String> {
        match self {
            Self::Clean(p) => p.advance_cleanup(),
            Self::Failed(p) => p.advance_cleanup(),
        }
    }
    pub(super) fn take_cancelled(&mut self) -> Option<ReturnedPreparation> {
        match self {
            Self::Clean(p) => p.take_cancelled_resources().map(|(graph, source)| {
                ReturnedPreparation::Clean(Box::new(CancelledResidentPreparation { graph, source }))
            }),
            Self::Failed(p) => p
                .take_cancelled_graph_resources()
                .map(|resources| ReturnedPreparation::Failed(Box::new(resources))),
        }
    }
    pub(super) fn advance(
        &mut self,
        resident: &Entity<MainWindowConversationComposer>,
        close: MainWindowConversationComposerCloseTicket,
        environment: &mut Option<Environment>,
        window: &Window,
        app: &mut App,
        completed: impl FnOnce(&mut App) + 'static,
    ) -> Result<Progress, String> {
        let composer = resident.read(app);
        let input = composer.gpui_input();
        match self {
            Self::Clean(p) => {
                composer.validate_recovery_retirement(close, app)?;
                let Some((seed, selection)) = p.authenticated_source()? else {
                    return Ok(Progress::Waiting);
                };
                if let Some(configure) = environment.take() {
                    let (environment, capacity) = configure(seed, selection, window)?;
                    p.admit(
                        input.read(app),
                        composer.recovery_snapshot().unwrap().protection(),
                        environment,
                        capacity,
                    )?;
                }
                input.update(app, |input, app| {
                    p.advance(input, window.text_system(), app, completed)
                })
            }
            Self::Failed(p) => {
                composer.validate_failed_capture(
                    p.capture()
                        .ok_or("failed resident capture is unavailable")?,
                    app,
                )?;
                if let Some(error) = p.preparation_error() {
                    return Err(error);
                }
                let Some((seed, selection)) = p.authenticated_source() else {
                    return Ok(Progress::Waiting);
                };
                if let Some(configure) = environment.take() {
                    let (environment, capacity) = configure(seed, selection, window)?;
                    p.admit(input.read(app), environment, capacity)?;
                }
                input.update(app, |input, app| {
                    p.advance(input, window.text_system(), app, completed)
                })
            }
        }
    }
}
