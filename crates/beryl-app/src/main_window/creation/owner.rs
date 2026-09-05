use super::*;
use crate::theme_runtime::{AppearancePublicationTarget, GpuiAppearanceWindowSet};
use gpui::{App, AppContext, Context, Entity, Global, KeyBinding, Subscription, WeakEntity};
use std::collections::HashMap;
use std::time::Duration;

gpui::actions!(main_window, [NewWindow]);

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MainWindowCreationGate {
    Ready,
    NoRuntimes,
    ExitWaiting,
    Unavailable(String),
}

impl MainWindowCreationGate {
    fn reason(&self) -> Option<&str> {
        match self {
            Self::Ready => None,
            Self::NoRuntimes => Some(
                "Add a runtime using the New Thread ellipsis segment before opening another window.",
            ),
            Self::ExitWaiting => {
                Some("Application Exit is waiting for active work and durable state.")
            }
            Self::Unavailable(reason) => Some(reason),
        }
    }
}

struct CreationProcessOwner {
    _owner: Entity<MainWindowCreationOwner>,
}
impl Global for CreationProcessOwner {}

pub struct MainWindowCreationOwner {
    services: Arc<MainWindowCreationServices>,
    appearance: Entity<GpuiAppearanceWindowSet>,
    gate: MainWindowCreationGate,
    fenced: bool,
    generation: beryl_home_store::HomeGeneration,
    entries: HashMap<WindowId, CreationEntry>,
    last_error: Option<String>,
    #[cfg(feature = "test-faults")]
    test_completion_delay: Option<Duration>,
    #[cfg(feature = "test-faults")]
    test_hold_publication: bool,
}

struct CreationEntry {
    source: WeakEntity<MainWindowShellRoot>,
    selection: MainWindowComposerSelectionIdentity,
    cancellation: CommandCancellation,
    work: Option<MainWindowCreation>,
    hidden: Option<MainWindowShell>,
    running: bool,
    continuations: u8,
    observer: Option<Subscription>,
    timeout: Option<gpui::Task<()>>,
    continuation: Option<gpui::Task<()>>,
    _source_release: Subscription,
    #[cfg(feature = "test-faults")]
    test_completion_waiting: bool,
    #[cfg(feature = "test-faults")]
    test_publication_held: bool,
}

impl MainWindowCreationOwner {
    pub fn install(
        services: Arc<MainWindowCreationServices>,
        appearance: Entity<GpuiAppearanceWindowSet>,
        gate: MainWindowCreationGate,
        app: &mut App,
    ) -> Result<Entity<Self>, String> {
        if app.has_global::<CreationProcessOwner>() {
            return Err("The process main-window creation owner is already installed.".to_owned());
        }
        let generation = services
            .store
            .health()
            .generation()
            .ok_or_else(|| "The Beryl home service is unavailable.".to_owned())?;
        let owner = app.new(|_| Self {
            services,
            appearance,
            gate,
            fenced: false,
            generation,
            entries: HashMap::new(),
            last_error: None,
            #[cfg(feature = "test-faults")]
            test_completion_delay: None,
            #[cfg(feature = "test-faults")]
            test_hold_publication: false,
        });
        app.bind_keys([KeyBinding::new(
            "ctrl-shift-n",
            NewWindow,
            Some("MainWindow"),
        )]);
        app.set_global(CreationProcessOwner {
            _owner: owner.clone(),
        });
        Ok(owner)
    }

    pub fn pending_count(&self) -> usize {
        self.entries.len()
    }

    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    pub fn disabled_reason(&self, source: WindowId) -> Option<&str> {
        if self.fenced || self.services.store.health().generation() != Some(self.generation) {
            return Some("The Beryl home service is unavailable.");
        }
        if let Some(reason) = self.gate.reason() {
            return Some(reason);
        }
        if self
            .entries
            .values()
            .any(|entry| entry.selection.window_id() == source)
        {
            return Some("New Window is waiting for window creation and durable state.");
        }
        if self
            .services
            .acquisition
            .process_registry()
            .main_window_occupancy()
            >= beryl_state::MAX_RESTORABLE_WINDOWS
        {
            return Some("All 256 main-window slots are in use.");
        }
        None
    }

    pub fn set_gate(&mut self, gate: MainWindowCreationGate, cx: &mut Context<Self>) {
        self.gate = gate;
        if !matches!(self.gate, MainWindowCreationGate::Ready) {
            for entry in self.entries.values() {
                entry.cancellation.cancel();
            }
        }
        self.resume_pending(cx);
        cx.notify();
    }

    pub fn fence(&mut self, cx: &mut Context<Self>) {
        self.fenced = true;
        for entry in self.entries.values() {
            entry.cancellation.cancel();
        }
        self.resume_pending(cx);
        cx.notify();
    }

    pub fn activate(
        &mut self,
        source: &Entity<MainWindowShellRoot>,
        cx: &mut Context<Self>,
    ) -> Result<WindowId, String> {
        let (selection, target) = source
            .read(cx)
            .creation_target(cx)
            .ok_or_else(|| "The selected thread is not available for New Window.".to_owned())?;
        self.activate_captured(source.downgrade(), selection, target, cx)
    }

    pub(in crate::main_window) fn activate_captured(
        &mut self,
        source: WeakEntity<MainWindowShellRoot>,
        selection: MainWindowComposerSelectionIdentity,
        target: RememberedTarget,
        cx: &mut Context<Self>,
    ) -> Result<WindowId, String> {
        if let Some(reason) = self.disabled_reason(selection.window_id()) {
            return Err(reason.to_owned());
        }
        let mut bytes = [0; 16];
        getrandom::fill(&mut bytes)
            .map_err(|_| "New Window identity generation failed.".to_owned())?;
        let window_id = WindowId::from_bytes(bytes);
        let work = MainWindowCreation::admit(self.services.clone(), window_id, target)
            .map_err(|error| format!("New Window admission failed: {error:?}"))?;
        let source_entity = source
            .upgrade()
            .ok_or_else(|| "The invoking window was released.".to_owned())?;
        let cancellation = work.cancellation();
        let release = cx.observe_release(&source_entity, move |this, _, cx| {
            this.cancel(window_id, cx);
        });
        self.entries.insert(
            window_id,
            CreationEntry {
                source,
                selection,
                cancellation,
                work: Some(work),
                hidden: None,
                running: false,
                continuations: 0,
                observer: None,
                timeout: None,
                continuation: None,
                _source_release: release,
                #[cfg(feature = "test-faults")]
                test_completion_waiting: false,
                #[cfg(feature = "test-faults")]
                test_publication_held: false,
            },
        );
        let owner = cx.entity();
        let timeout = cx.spawn(async move |_, cx| {
            cx.background_executor()
                .timer(Duration::from_secs(30))
                .await;
            let _ = owner.update(cx, |owner, cx| owner.cancel(window_id, cx));
        });
        self.entries.get_mut(&window_id).unwrap().timeout = Some(timeout);
        self.last_error = None;
        self.drive(window_id, cx);
        cx.notify();
        Ok(window_id)
    }

    pub fn cancel(&mut self, window_id: WindowId, cx: &mut Context<Self>) {
        if let Some(entry) = self.entries.get(&window_id) {
            entry.cancellation.cancel();
        }
        self.drive(window_id, cx);
    }

    pub fn resume_pending(&mut self, cx: &mut Context<Self>) {
        let windows = self.entries.keys().copied().collect::<Vec<_>>();
        for window in windows {
            if let Some(entry) = self.entries.get_mut(&window) {
                entry.continuations = 0;
            }
            self.drive(window, cx);
        }
        cx.notify();
    }

    fn drive(&mut self, window_id: WindowId, cx: &mut Context<Self>) {
        let Some(entry) = self.entries.get_mut(&window_id) else {
            return;
        };
        if entry.running {
            return;
        }
        if entry.hidden.is_some() {
            self.publish_or_abandon(window_id, cx);
            return;
        }
        let Some(work) = entry.work.take() else {
            return;
        };
        entry.running = true;
        let appearance = self.appearance.read(cx).target().snapshot().current;
        let task = cx
            .background_executor()
            .spawn(async move { work.advance(appearance) });
        let owner = cx.entity();
        #[cfg(feature = "test-faults")]
        let delay = self.test_completion_delay.take();
        cx.spawn(async move |_, cx| {
            let outcome = task.await;
            #[cfg(feature = "test-faults")]
            if let Some(delay) = delay {
                let _ = owner.update(cx, |owner, _| {
                    owner
                        .entries
                        .get_mut(&window_id)
                        .unwrap()
                        .test_completion_waiting = true;
                });
                cx.background_executor().timer(delay).await;
            }
            let _ = owner.update(cx, |owner, cx| owner.complete(window_id, outcome, cx));
        })
        .detach();
    }

    fn complete(
        &mut self,
        window_id: WindowId,
        outcome: MainWindowCreationOutcome,
        cx: &mut Context<Self>,
    ) {
        let entry = self
            .entries
            .get_mut(&window_id)
            .expect("admitted creation retains its process entry");
        entry.running = false;
        #[cfg(feature = "test-faults")]
        {
            entry.test_completion_waiting = false;
        }
        match outcome {
            MainWindowCreationOutcome::Pending(work) => {
                entry.work = Some(work);
                entry.continuations = entry.continuations.saturating_add(1);
                let delay = Duration::from_millis(25 << entry.continuations.min(5));
                let owner = cx.entity();
                entry.continuation = Some(cx.spawn(async move |_, cx| {
                    cx.background_executor().timer(delay).await;
                    let _ = owner.update(cx, |owner, cx| owner.drive(window_id, cx));
                }));
            }
            MainWindowCreationOutcome::Settled { error, .. } => {
                self.last_error = error;
                self.entries.remove(&window_id);
            }
            MainWindowCreationOutcome::Prepared {
                prepared,
                cancellation,
            } => {
                let current = entry.source.upgrade().is_some_and(|source| {
                    source
                        .read(cx)
                        .creation_target(cx)
                        .is_some_and(|(selection, _)| selection == entry.selection)
                });
                if cancellation.is_cancelled()
                    || self.fenced
                    || !current
                    || self.services.store.health().generation() != Some(self.generation)
                {
                    entry.work = Some(MainWindowCreation::abandon(
                        self.services.clone(),
                        prepared.into_unpublished(),
                        "New Window creation was cancelled or its source changed.".to_owned(),
                    ));
                    self.drive(window_id, cx);
                } else {
                    let result = GpuiMainWindowShellHost::new(cx, self.appearance.clone())
                        .construct_hidden(prepared);
                    match result {
                        Err(failure) => {
                            entry.work = Some(MainWindowCreation::abandon(
                                self.services.clone(),
                                failure.into_unpublished(),
                                "New Window shell construction failed.".to_owned(),
                            ));
                            self.drive(window_id, cx);
                        }
                        Ok(shell) => {
                            #[cfg(feature = "test-faults")]
                            {
                                entry.test_publication_held =
                                    std::mem::take(&mut self.test_hold_publication);
                            }
                            let root = shell.window().entity(cx).expect("new hidden shell root");
                            entry.observer =
                                Some(cx.observe(&root, move |owner, _, cx| {
                                    owner.drive(window_id, cx)
                                }));
                            entry.hidden = Some(shell);
                            self.publish_or_abandon(window_id, cx);
                        }
                    }
                }
            }
        }
        cx.notify();
    }

    fn publish_or_abandon(&mut self, window_id: WindowId, cx: &mut Context<Self>) {
        let Some(entry) = self.entries.get_mut(&window_id) else {
            return;
        };
        let Some(mut shell) = entry.hidden.take() else {
            return;
        };
        let current = entry.source.upgrade().is_some_and(|source| {
            source
                .read(cx)
                .creation_target(cx)
                .is_some_and(|(selection, _)| selection == entry.selection)
        });
        let cancelled = entry.cancellation.is_cancelled()
            || self.fenced
            || !current
            || self.services.store.health().generation() != Some(self.generation);
        #[cfg(feature = "test-faults")]
        if !cancelled && entry.test_publication_held {
            entry.hidden = Some(shell);
            return;
        }
        if !cancelled && !shell.ready_to_publish(cx) {
            entry.hidden = Some(shell);
            return;
        }
        if !cancelled {
            shell.attach_creation(cx.entity(), cx);
            if shell.publish(cx).is_ok() {
                shell
                    .release_published_handle(cx)
                    .unwrap_or_else(|_| panic!("published shell transfers its handle"));
                self.entries.remove(&window_id);
                cx.notify();
                return;
            }
        }
        let unpublished = shell
            .close_before_publication(cx)
            .unwrap_or_else(|_| panic!("creation never abandons a published shell"));
        entry.observer = None;
        entry.work = Some(MainWindowCreation::abandon(
            self.services.clone(),
            unpublished,
            "New Window publication failed or was cancelled.".to_owned(),
        ));
        self.drive(window_id, cx);
    }

    #[cfg(feature = "test-faults")]
    pub fn test_uninstall(app: &mut App) {
        assert!(
            app.windows().is_empty(),
            "test removes every native window first"
        );
        let owner = &app.global::<CreationProcessOwner>()._owner;
        assert!(
            owner.read(app).entries.is_empty(),
            "test settles every creation first"
        );
        app.remove_global::<CreationProcessOwner>();
    }

    #[cfg(feature = "test-faults")]
    pub fn test_delay_next_completion(&mut self, delay: Duration) {
        self.test_completion_delay = Some(delay);
    }

    #[cfg(feature = "test-faults")]
    pub fn test_completion_is_waiting(&self, window_id: WindowId) -> bool {
        self.entries
            .get(&window_id)
            .is_some_and(|entry| entry.test_completion_waiting)
    }

    #[cfg(feature = "test-faults")]
    pub fn test_hold_next_publication(&mut self) {
        self.test_hold_publication = true;
    }

    #[cfg(feature = "test-faults")]
    pub fn test_hidden_window(
        &self,
        window_id: WindowId,
    ) -> Option<gpui::WindowHandle<MainWindowShellRoot>> {
        self.entries
            .get(&window_id)?
            .hidden
            .as_ref()
            .map(MainWindowShell::window)
    }

    #[cfg(feature = "test-faults")]
    pub fn test_release_publication(&mut self, window_id: WindowId, cx: &mut Context<Self>) {
        self.entries
            .get_mut(&window_id)
            .unwrap()
            .test_publication_held = false;
        self.drive(window_id, cx);
    }
}
