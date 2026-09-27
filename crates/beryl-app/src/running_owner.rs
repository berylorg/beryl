use std::{cell::RefCell, rc::Rc};

use gpui::App;

use crate::startup_owner::StartedProcess;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum StartupCleanup {
    Pending,
    Settled,
    Failed(String),
}

pub(crate) struct RunningProcessOwner {
    process: StartedProcess,
    startup_cleanup: StartupCleanup,
}

impl RunningProcessOwner {
    pub(crate) fn start(mut process: StartedProcess, app: &mut App) -> Rc<RefCell<Self>> {
        let surface = process.startup_surface.take();
        let owner = Rc::new(RefCell::new(Self {
            process,
            startup_cleanup: if surface.is_some() {
                StartupCleanup::Pending
            } else {
                StartupCleanup::Settled
            },
        }));
        if let Some(mut surface) = surface {
            let retained = owner.clone();
            app.spawn(async move |cx| {
                let result = surface.close(cx).await;
                let mut owner = retained.borrow_mut();
                owner.startup_cleanup = match result {
                    Ok(()) => StartupCleanup::Settled,
                    Err(error) => {
                        owner.process.startup_surface = Some(surface);
                        StartupCleanup::Failed(error)
                    }
                };
            })
            .detach();
        }
        owner
    }

    pub(crate) fn startup_cleanup(&self) -> &StartupCleanup {
        &self.startup_cleanup
    }

    pub(crate) fn exit_requested(&self) -> bool {
        self.process.commands.exit_requested()
    }

    #[cfg(test)]
    pub(crate) fn test_process(&self) -> &StartedProcess {
        &self.process
    }

    #[cfg(test)]
    pub(crate) fn test_into_process(self) -> StartedProcess {
        assert_ne!(self.startup_cleanup, StartupCleanup::Pending);
        self.process
    }
}
