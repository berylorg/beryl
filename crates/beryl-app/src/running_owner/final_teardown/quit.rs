use super::*;
use gpui::{
    WindowsNativeConfirmation, WindowsNativeConfirmationOutcome, WindowsNativeConfirmationRequest,
};

pub(super) struct BlockedQuitConfirmation {
    identity: Rc<()>,
    control: Rc<WindowsNativeConfirmation>,
    settled: bool,
}

impl RunningProcessOwner {
    pub(crate) fn begin_blocked_quit(
        owner: &Rc<RefCell<Self>>,
        invoking: WindowId,
        app: &mut App,
    ) -> Result<(), String> {
        Self::begin_blocked_quit_with(owner, invoking, app, |request| request)
    }

    fn begin_blocked_quit_with(
        owner: &Rc<RefCell<Self>>,
        invoking: WindowId,
        app: &mut App,
        configure: impl FnOnce(WindowsNativeConfirmationRequest) -> WindowsNativeConfirmationRequest,
    ) -> Result<(), String> {
        let (window, attempt_identity) = {
            let owner = owner.borrow();
            let attempt = owner
                .final_teardown
                .as_ref()
                .ok_or("no blocked shutdown is retained")?;
            if !matches!(attempt.status, FinalTeardownStatus::Blocked(_))
                || attempt.termination_admitted
            {
                return Err("Quit Anyway requires an unterminated blocked shutdown".into());
            }
            if let Some(quit) = &attempt.quit {
                return quit.control.reveal().map_err(|error| error.to_string());
            }
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
                .ok_or("the invoking blocked window is unavailable")?
                .window();
            (window, attempt.identity.clone())
        };
        let request = WindowsNativeConfirmationRequest::new(
            "Quit Beryl anyway?",
            "Quit Anyway stops Beryl immediately. Cleanup is incomplete; work already sent may still complete.",
            "Cancel",
            "Quit Anyway",
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
        owner.borrow_mut().final_teardown.as_mut().unwrap().quit = Some(BlockedQuitConfirmation {
            identity: identity.clone(),
            control: control.clone(),
            settled: false,
        });
        let retained = owner.clone();
        app.spawn(async move |cx| {
            let result = completion.await;
            let clean = control.cleanup_settled();
            if clean && !matches!(result, Ok(WindowsNativeConfirmationOutcome::Confirmed)) {
                if let Some(focus) = focus {
                    let _ = window.update(cx, |_, window, _| window.focus(&focus));
                }
            }
            let terminate = {
                let mut owner = retained.borrow_mut();
                let attempt = owner.final_teardown.as_mut().unwrap();
                if !Rc::ptr_eq(&attempt.identity, &attempt_identity)
                    || !matches!(attempt.status, FinalTeardownStatus::Blocked(_))
                    || attempt.termination_admitted
                    || !attempt
                        .quit
                        .as_ref()
                        .is_some_and(|quit| Rc::ptr_eq(&quit.identity, &identity) && !quit.settled)
                {
                    false
                } else {
                    attempt.quit.as_mut().unwrap().settled = true;
                    if clean && matches!(result, Ok(WindowsNativeConfirmationOutcome::Confirmed)) {
                        attempt.termination_admitted = true;
                        true
                    } else {
                        if clean {
                            attempt.quit.take();
                        }
                        false
                    }
                }
            };
            if terminate {
                Self::terminate_blocked_process(&retained);
            }
        })
        .detach();
        Ok(())
    }

    fn terminate_blocked_process(owner: &Rc<RefCell<Self>>) {
        #[cfg(test)]
        if let Some(termination) = owner
            .borrow_mut()
            .final_teardown
            .as_mut()
            .unwrap()
            .termination
            .take()
        {
            termination();
            return;
        }
        #[cfg(not(test))]
        let _ = owner;
        unsafe {
            use windows::Win32::System::Threading::{GetCurrentProcess, TerminateProcess};
            let _ = TerminateProcess(GetCurrentProcess(), 1);
        }
        std::process::exit(1);
    }

    #[cfg(test)]
    pub(crate) fn test_blocked_quit_pending(&self) -> bool {
        self.final_teardown
            .as_ref()
            .is_some_and(|attempt| attempt.quit.is_some())
    }

    #[cfg(test)]
    pub(crate) fn test_blocked_termination(&mut self, termination: impl FnOnce() + 'static) {
        self.final_teardown.as_mut().unwrap().termination = Some(Box::new(termination));
    }
}
