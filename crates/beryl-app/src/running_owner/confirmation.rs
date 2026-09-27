use super::*;
use beryl_model::WindowId;
use gpui::{
    WindowsNativeConfirmation, WindowsNativeConfirmationOutcome, WindowsNativeConfirmationRequest,
};

use crate::{cas_projection::ShutdownWorkObservation, window_acquisition::WindowCloseSnapshot};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ShutdownIntent {
    FinalWindowClose,
    ApplicationExit,
}

pub(crate) struct ShutdownConfirmationContext {
    pub(super) invoking: WindowId,
    pub(super) intent: ShutdownIntent,
    pub(super) snapshot: WindowCloseSnapshot,
    pub(super) observation: ShutdownWorkObservation,
}

impl ShutdownConfirmationContext {
    pub(crate) fn invoking(&self) -> WindowId {
        self.invoking
    }
    pub(crate) fn intent(&self) -> ShutdownIntent {
        self.intent
    }
    pub(crate) fn snapshot(&self) -> &WindowCloseSnapshot {
        &self.snapshot
    }
    pub(crate) fn observation(&self) -> &ShutdownWorkObservation {
        &self.observation
    }
}

pub(crate) enum ShutdownConfirmationResult {
    Confirmed(ShutdownConfirmationContext),
    Cancelled,
    WindowSetChanged,
}

pub(super) struct RunningConfirmation {
    identity: Rc<()>,
    control: Rc<WindowsNativeConfirmation>,
    context: ShutdownConfirmationContext,
    settled: Option<Result<WindowsNativeConfirmationOutcome, String>>,
}

impl RunningProcessOwner {
    #[cfg(test)]
    pub(crate) fn test_begin_shutdown_confirmation(
        owner: &Rc<RefCell<Self>>,
        invoking: WindowId,
        intent: ShutdownIntent,
        observation: ShutdownWorkObservation,
        app: &mut App,
        fault: Option<gpui::WindowsNativeConfirmationTestFault>,
    ) -> Result<(), String> {
        Self::begin_confirmation(
            owner,
            invoking,
            intent,
            observation,
            app,
            |request| match fault {
                Some(fault) => request.with_fault_for_test(fault),
                None => request,
            },
        )
    }

    pub(crate) fn begin_shutdown_confirmation(
        owner: &Rc<RefCell<Self>>,
        invoking: WindowId,
        intent: ShutdownIntent,
        observation: ShutdownWorkObservation,
        app: &mut App,
    ) -> Result<(), String> {
        Self::begin_confirmation(owner, invoking, intent, observation, app, |request| request)
    }

    fn begin_confirmation(
        owner: &Rc<RefCell<Self>>,
        invoking: WindowId,
        intent: ShutdownIntent,
        observation: ShutdownWorkObservation,
        app: &mut App,
        configure: impl FnOnce(WindowsNativeConfirmationRequest) -> WindowsNativeConfirmationRequest,
    ) -> Result<(), String> {
        if owner.borrow().shutdown.is_some() {
            return Err("the running owner already retains shutdown intent custody".into());
        }
        if owner.borrow().confirmation.is_some() {
            return Self::reveal_shutdown_confirmation(owner);
        }
        let (window, snapshot) = {
            let owner = owner.borrow();
            let window = owner
                .process
                .windows
                .shells()
                .iter()
                .find(|shell| {
                    shell
                        .window()
                        .read(app)
                        .ok()
                        .and_then(|root| root.controller())
                        .is_some_and(|controller| controller.window_id() == invoking)
                })
                .ok_or("the invoking main window is unavailable")?
                .window();
            let (snapshot, final_member) = owner.process.services.prepare_close_confirmation(
                owner.process.windows.window_ids(),
                invoking,
                &observation,
            )?;
            if intent == ShutdownIntent::FinalWindowClose && !final_member {
                return Err("the invoking main window is no longer final".into());
            }
            (window, snapshot)
        };
        let request = WindowsNativeConfirmationRequest::new(
            "Exit Beryl?",
            &format!("Running threads: {}. Running threads, including those not open in a window, will stop.", observation.running_threads()),
            "Cancel",
            "Exit Beryl",
        ).map_err(|error| error.to_string())?;
        let (control, completion, focus) = window
            .update(app, |_, window, cx| {
                let focus = window.focused(cx);
                window
                    .begin_windows_native_confirmation(configure(request))
                    .map(|(control, completion)| (control, completion, focus))
            })
            .map_err(|error| error.to_string())?
            .map_err(|error| error.to_string())?;
        let identity = Rc::new(());
        let control = Rc::new(control);
        owner.borrow_mut().confirmation = Some(RunningConfirmation {
            identity: identity.clone(),
            control: control.clone(),
            context: ShutdownConfirmationContext {
                invoking,
                intent,
                snapshot,
                observation,
            },
            settled: None,
        });
        let retained = owner.clone();
        app.spawn(async move |cx| {
            let result = completion.await.map_err(|error| error.to_string());
            if matches!(result, Ok(WindowsNativeConfirmationOutcome::Cancelled))
                || (result.is_err() && control.cleanup_settled())
            {
                if let Some(focus) = focus {
                    let _ = window.update(cx, |_, window, _| window.focus(&focus));
                }
            }
            let mut owner = retained.borrow_mut();
            if let Some(operation) = owner
                .confirmation
                .as_mut()
                .filter(|operation| Rc::ptr_eq(&operation.identity, &identity))
            {
                operation.settled = Some(result);
            }
        })
        .detach();
        Ok(())
    }

    pub(crate) fn reveal_shutdown_confirmation(owner: &Rc<RefCell<Self>>) -> Result<(), String> {
        let control = {
            let owner = owner.borrow();
            let operation = owner
                .confirmation
                .as_ref()
                .ok_or("no shutdown confirmation")?;
            if operation.settled.is_some() {
                return Err("shutdown confirmation already completed".into());
            }
            operation.control.clone()
        };
        control.reveal().map_err(|error| error.to_string())
    }

    pub(crate) fn cancel_shutdown_confirmation(owner: &Rc<RefCell<Self>>) -> Result<(), String> {
        let control = owner
            .borrow()
            .confirmation
            .as_ref()
            .ok_or("no shutdown confirmation")?
            .control
            .clone();
        control.cancel().map_err(|error| error.to_string())
    }

    pub(crate) fn take_shutdown_confirmation(
        &mut self,
    ) -> Result<Option<ShutdownConfirmationResult>, String> {
        let Some(operation) = self.confirmation.as_ref() else {
            return Ok(None);
        };
        let Some(result) = &operation.settled else {
            return Ok(None);
        };
        let outcome = match result {
            Ok(outcome) => outcome,
            Err(error) => {
                let error = error.clone();
                if operation.control.cleanup_settled() {
                    self.confirmation.take();
                }
                return Err(error);
            }
        };
        let valid = self
            .process
            .services
            .inspect_close_confirmation(&operation.context.snapshot, operation.context.invoking)
            .is_ok();
        let confirmed = *outcome == WindowsNativeConfirmationOutcome::Confirmed;
        let operation = self.confirmation.take().unwrap();
        Ok(Some(if !confirmed {
            ShutdownConfirmationResult::Cancelled
        } else if !valid {
            ShutdownConfirmationResult::WindowSetChanged
        } else {
            ShutdownConfirmationResult::Confirmed(operation.context)
        }))
    }
}
