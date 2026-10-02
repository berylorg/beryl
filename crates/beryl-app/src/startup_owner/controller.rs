use super::*;
use super::{
    surface::OwnedStartupSurface,
    worker::{Preparation, PreparationCancellation, Worker},
};
use crate::{
    main_window::{
        MainWindowNativeRestoreSetCancellation, MainWindowNativeRestoreSetCompletion,
        PublishedMainWindowRestoreSet, RetainedNativeMainWindowRestoreSet,
    },
    startup_surface::{StartupAttempt, StartupSurfaceEvent},
    theme_runtime::GpuiAppearanceWindowSet,
};
use gpui::{App, AsyncApp, Entity};
use std::{
    cell::RefCell,
    rc::Rc,
    task::{Poll, Waker},
};

mod running_commands;
pub(crate) use running_commands::{
    RunningExitCommands, RunningExitGate, RunningExitRequest, RunningWindowExit,
};

#[derive(Clone)]
enum Stage {
    Preparing(PreparationCancellation),
    Native(MainWindowNativeRestoreSetCancellation),
    Waiting,
    Blocked,
    Running,
    Finished,
}

struct Commands {
    stage: Stage,
    exit: bool,
    process_exit: bool,
    exit_window: Option<WindowId>,
    ordinary_close: bool,
    retry: Option<StartupAttempt>,
    wake: Option<Waker>,
    active_exit: Option<Rc<()>>,
    exit_gates: running_commands::RunningExitGates,
}

impl Commands {
    fn admit_process_exit(&mut self) -> bool {
        if self.process_exit
            && matches!(self.stage, Stage::Running)
            && !self.exit
            && self.active_exit.is_none()
            && self.exit_gates.disabled_reason().is_none()
        {
            self.process_exit = false;
            self.exit = true;
            self.exit_window = None;
            self.ordinary_close = false;
            return true;
        }
        false
    }
}

#[derive(Clone)]
pub(crate) struct StartupCommands(Rc<RefCell<Commands>>);

impl StartupCommands {
    #[cfg(test)]
    pub(crate) fn test_set_exit_gate(&self, gate: RunningExitGate, blocked: bool) {
        RunningExitCommands::new(self.clone()).set_gate(gate, blocked);
    }

    #[cfg(test)]
    pub(crate) fn test_process_exit_pending(&self) -> bool {
        self.0.borrow().process_exit
    }
    pub(crate) fn request_process_exit(&self) {
        let mut state = self.0.borrow_mut();
        if matches!(state.stage, Stage::Finished) {
            return;
        }
        if matches!(state.stage, Stage::Running) {
            let mut admitted = false;
            if !(state.exit || state.active_exit.is_some()) || state.ordinary_close {
                state.process_exit = true;
                admitted = state.admit_process_exit();
            }
            let wake = if admitted { state.wake.take() } else { None };
            drop(state);
            if let Some(wake) = wake {
                wake.wake();
            }
        } else {
            drop(state);
            self.request_exit();
        }
    }
    pub(crate) fn request_exit(&self) {
        self.request_exit_from(None);
    }

    fn request_exit_from(&self, invoking: Option<WindowId>) {
        let mut state = self.0.borrow_mut();
        if invoking.is_some() && !matches!(state.stage, Stage::Running) {
            return;
        }
        if matches!(state.stage, Stage::Running)
            && (state.active_exit.is_some() || state.exit_gates.disabled_reason().is_some())
        {
            return;
        }
        if !state.exit {
            state.exit_window = invoking;
            state.ordinary_close = false;
        }
        state.exit = true;
        let stage = state.stage.clone();
        let wake = state.wake.take();
        drop(state);
        match stage {
            Stage::Preparing(cancellation) => cancellation.cancel(),
            Stage::Native(cancellation) => cancellation.cancel(),
            _ => {}
        }
        if let Some(wake) = wake {
            wake.wake();
        }
    }

    pub(crate) fn exit_requested(&self) -> bool {
        self.0.borrow().exit
    }

    pub(crate) fn diagnostic_exit_pending(&self) -> bool {
        let state = self.0.borrow();
        state.exit || state.active_exit.is_some() || state.process_exit
    }

    fn event(&self, event: StartupSurfaceEvent) {
        match event {
            StartupSurfaceEvent::Exit => self.request_exit(),
            StartupSurfaceEvent::QuitAnyway(request) => request.terminate_process(),
            StartupSurfaceEvent::Retry(attempt) => {
                let mut state = self.0.borrow_mut();
                if !state.exit && matches!(state.stage, Stage::Waiting) && state.retry.is_none() {
                    state.retry = Some(attempt);
                    let wake = state.wake.take();
                    drop(state);
                    if let Some(wake) = wake {
                        wake.wake();
                    }
                }
            }
        }
    }

    async fn wait(&self) -> Option<StartupAttempt> {
        std::future::poll_fn(|cx| {
            let mut state = self.0.borrow_mut();
            if state.exit {
                return Poll::Ready(None);
            }
            if let Some(attempt) = state.retry.take() {
                return Poll::Ready(Some(attempt));
            }
            state.wake = Some(cx.waker().clone());
            Poll::Pending
        })
        .await
    }
}

pub(crate) struct StartedProcess {
    pub(crate) configuration: AppServiceConfiguration,
    pub(crate) services: ProcessServiceOwner,
    pub(crate) windows: PublishedMainWindowRestoreSet,
    pub(crate) appearance: Entity<GpuiAppearanceWindowSet>,
    pub(crate) startup_surface: Option<OwnedStartupSurface>,
    pub(crate) commands: RunningExitCommands,
}

pub(crate) enum StartupCompletion {
    Running(StartedProcess),
    Exit {
        unsuccessful: bool,
    },
    Unavailable {
        detail: String,
        custody: UnavailableStartup,
    },
}

pub(crate) struct UnavailableStartup {
    worker: Worker,
    surface: Option<OwnedStartupSurface>,
    retained_native: Option<RetainedNativeMainWindowRestoreSet>,
    appearance: Option<Entity<GpuiAppearanceWindowSet>>,
}

type Completion = Rc<RefCell<Option<Box<dyn FnOnce(StartupCompletion, &mut App)>>>>;

pub(crate) fn start(
    configuration: StartupConfiguration,
    completion: impl FnOnce(StartupCompletion, &mut App) + 'static,
    app: &mut App,
) -> StartupCommands {
    let cancellation = PreparationCancellation::new();
    let commands = StartupCommands(Rc::new(RefCell::new(Commands {
        stage: Stage::Preparing(cancellation),
        exit: false,
        process_exit: false,
        exit_window: None,
        ordinary_close: false,
        retry: None,
        wake: None,
        active_exit: None,
        exit_gates: Default::default(),
    })));
    let owner = Controller {
        configuration: Arc::new(configuration),
        commands: commands.clone(),
        completion: Rc::new(RefCell::new(Some(Box::new(completion)))),
        custody: UnavailableStartup {
            worker: Worker::new(),
            surface: None,
            retained_native: None,
            appearance: None,
        },
        unsuccessful: false,
    };
    app.spawn(async move |cx| owner.run(cx).await).detach();
    commands
}

struct Controller {
    configuration: Arc<StartupConfiguration>,
    commands: StartupCommands,
    completion: Completion,
    custody: UnavailableStartup,
    unsuccessful: bool,
}

impl Controller {
    async fn run(mut self, cx: &mut AsyncApp) {
        loop {
            let cancellation = PreparationCancellation::new();
            if self.commands.exit_requested() {
                cancellation.cancel();
            }
            self.commands.0.borrow_mut().stage = Stage::Preparing(cancellation.clone());
            let configuration = self.configuration.clone();
            let mut worker = self.custody.worker;
            let (worker, prepared) = cx
                .background_executor()
                .spawn(async move {
                    let prepared = worker.prepare(&configuration, &cancellation);
                    (worker, prepared)
                })
                .await;
            self.custody.worker = worker;
            let (detail, blocked, busy) = match prepared {
                Preparation::Ready { native, appearance } => {
                    let (sender, receiver) = futures_channel::oneshot::channel();
                    let commands = self.commands.clone();
                    let completion = self.completion.clone();
                    let configuration = self.configuration.services.clone();
                    let mut worker = self.custody.worker;
                    let surface = self.custody.surface.take();
                    let cancellation = cx
                        .update(|app| {
                            let appearance = GpuiAppearanceWindowSet::new(
                                appearance,
                                NonZeroUsize::new(beryl_state::MAX_RESTORABLE_WINDOWS).unwrap(),
                                app,
                            );
                            native.start(
                                appearance.clone(),
                                move |result, app| match result {
                                    MainWindowNativeRestoreSetCompletion::Published(windows) => {
                                        commands.0.borrow_mut().stage = Stage::Running;
                                        let result = StartedProcess {
                                            configuration,
                                            services: worker.services.take().expect(
                                                "native success retains its complete graph",
                                            ),
                                            windows,
                                            appearance,
                                            startup_surface: surface,
                                            commands: RunningExitCommands::new(commands),
                                        };
                                        completion
                                            .borrow_mut()
                                            .take()
                                            .expect("required startup completion")(
                                            StartupCompletion::Running(result),
                                            app,
                                        );
                                        assert!(
                                            sender.send(None).is_ok(),
                                            "startup owner retains native completion receiver"
                                        );
                                    }
                                    MainWindowNativeRestoreSetCompletion::Failed(failure) => {
                                        assert!(
                                            sender
                                                .send(Some((worker, surface, appearance, failure)))
                                                .is_ok(),
                                            "startup owner retains native failure custody"
                                        );
                                    }
                                },
                                app,
                            )
                        })
                        .expect("startup owns the live GUI executor");
                    if self.commands.exit_requested() {
                        cancellation.cancel();
                    }
                    self.commands.0.borrow_mut().stage = Stage::Native(cancellation);
                    let Some((worker, surface, appearance, failure)) = receiver
                        .await
                        .expect("native startup completes exactly once")
                    else {
                        return;
                    };
                    self.custody.worker = worker;
                    self.custody.surface = surface;
                    self.custody.appearance = Some(appearance);
                    self.custody.retained_native = failure.retained;
                    if self.custody.retained_native.is_some() {
                        (
                            crate::startup_surface::bounded_detail(&failure.error),
                            true,
                            false,
                        )
                    } else {
                        self.retire_appearance(cx);
                        let mut worker = self.custody.worker;
                        let (worker, failure) = cx
                            .background_executor()
                            .spawn(async move {
                                let failure = worker.failure(failure.error);
                                (worker, failure)
                            })
                            .await;
                        self.custody.worker = worker;
                        let Preparation::Failed { detail, blocked } = failure else {
                            unreachable!()
                        };
                        (detail, blocked, false)
                    }
                }
                Preparation::Busy => (String::new(), false, true),
                Preparation::Failed { detail, blocked } => (detail, blocked, false),
            };
            self.unsuccessful = true;
            if !blocked && self.commands.exit_requested() {
                match self.close_surface(cx).await {
                    Ok(()) => {
                        self.exit(cx);
                        return;
                    }
                    Err(error) => {
                        self.block(error, cx).await;
                        return;
                    }
                }
            }
            if busy && self.custody.surface.is_some() {
                if let Err(error) = self.close_surface(cx).await {
                    self.block(error, cx).await;
                    return;
                }
                if self.commands.exit_requested() {
                    self.exit(cx);
                    return;
                }
            }
            if let Err(error) = self.show(&detail, busy, blocked, cx) {
                self.unavailable(error, cx);
                return;
            }
            drop(detail);
            if blocked {
                self.commands.0.borrow_mut().stage = Stage::Blocked;
                std::future::pending::<()>().await;
                return;
            }
            self.commands.0.borrow_mut().stage = Stage::Waiting;
            match self.commands.wait().await {
                Some(attempt) => {
                    let surface = self.custody.surface.as_mut().expect("Retry surface");
                    surface.attempt = attempt;
                }
                None => {
                    match self.close_surface(cx).await {
                        Ok(()) => self.exit(cx),
                        Err(error) => self.block(error, cx).await,
                    }
                    return;
                }
            }
        }
    }

    fn show(
        &mut self,
        detail: &str,
        busy: bool,
        blocked: bool,
        cx: &mut AsyncApp,
    ) -> Result<(), String> {
        cx.update(|app| {
            if self.custody.surface.is_none() {
                let commands = self.commands.clone();
                self.custody.surface = Some(OwnedStartupSurface::open(
                    (!busy).then_some(detail),
                    move |event, _| commands.event(event),
                    app,
                )?);
            }
            let surface = self.custody.surface.as_mut().unwrap();
            let failure = surface.registration_failure().map(str::to_owned);
            if blocked || failure.is_some() {
                if surface.busy {
                    return Err(failure.unwrap_or_else(|| detail.to_owned()));
                }
                let detail = failure.as_deref().unwrap_or(detail);
                surface
                    .window
                    .update(app, |value, window, cx| {
                        value.block_cleanup(surface.attempt, detail, window, cx)
                    })
                    .map_err(|e| e.to_string())?;
                if let Some(error) = failure {
                    return Err(error);
                }
            } else if !busy {
                surface
                    .window
                    .update(app, |value, _, cx| {
                        value.complete_failure(surface.attempt, detail, cx)
                    })
                    .map_err(|e| e.to_string())?;
            }
            Ok(())
        })
        .expect("startup owns the live GUI executor")
    }

    fn retire_appearance(&mut self, cx: &mut AsyncApp) {
        if let Some(appearance) = self.custody.appearance.take() {
            appearance
                .update(cx, |appearance, _| appearance.retire())
                .expect("startup appearance remains owned");
        }
    }

    async fn close_surface(&mut self, cx: &mut AsyncApp) -> Result<(), String> {
        if let Some(surface) = &mut self.custody.surface {
            surface.close(cx).await?;
        }
        self.custody.surface = None;
        Ok(())
    }

    async fn block(mut self, detail: String, cx: &mut AsyncApp) {
        self.commands.0.borrow_mut().stage = Stage::Blocked;
        if let Err(error) = self.show(&detail, false, true, cx) {
            self.unavailable(error, cx);
            return;
        }
        drop(detail);
        std::future::pending::<()>().await;
    }

    fn unavailable(self, detail: String, cx: &mut AsyncApp) {
        self.commands.0.borrow_mut().stage = Stage::Blocked;
        let callback = self
            .completion
            .borrow_mut()
            .take()
            .expect("required startup completion");
        cx.update(|app| {
            callback(
                StartupCompletion::Unavailable {
                    detail,
                    custody: self.custody,
                },
                app,
            )
        })
        .expect("startup owns the live GUI executor");
    }

    fn exit(self, cx: &mut AsyncApp) {
        self.commands.0.borrow_mut().stage = Stage::Finished;
        let callback = self
            .completion
            .borrow_mut()
            .take()
            .expect("required startup completion");
        cx.update(|app| {
            callback(
                StartupCompletion::Exit {
                    unsuccessful: self.unsuccessful,
                },
                app,
            )
        })
        .expect("startup owns the live GUI executor");
    }
}
