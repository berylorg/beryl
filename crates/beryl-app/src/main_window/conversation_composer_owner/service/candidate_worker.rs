use std::{cell::RefCell, future::Future, panic::AssertUnwindSafe, rc::Rc};

use beryl_home_store::HomeRecoveryCandidate;
use gpui::{App, AppContext};
use gpui_text_input::{
    ObjectPage, RangePage, RangePrepublicationEffect, RangePrepublicationSessionGeneration,
    RangePrepublicationValidationResponse,
};

use super::MainWindowComposerCandidateSource;

pub enum MainWindowComposerCandidateRead {
    Validation(RangePrepublicationValidationResponse),
    Page(RangePage),
    ObjectPage(ObjectPage),
}

pub struct MainWindowComposerCandidateCompletion {
    pub effect: RangePrepublicationEffect,
    pub result: Result<MainWindowComposerCandidateRead, String>,
}

struct State {
    resources: Option<(HomeRecoveryCandidate, MainWindowComposerCandidateSource)>,
    completion: Option<MainWindowComposerCandidateCompletion>,
    pending: bool,
    cancelled: bool,
}

pub struct MainWindowComposerCandidateWorker {
    generation: RangePrepublicationSessionGeneration,
    state: Rc<RefCell<State>>,
}

pub struct MainWindowComposerCandidateCustody {
    state: Rc<RefCell<State>>,
}

impl MainWindowComposerCandidateWorker {
    pub fn new(
        candidate: HomeRecoveryCandidate,
        source: MainWindowComposerCandidateSource,
        generation: RangePrepublicationSessionGeneration,
    ) -> (Self, MainWindowComposerCandidateCustody) {
        let state = Rc::new(RefCell::new(State {
            resources: Some((candidate, source)),
            completion: None,
            pending: false,
            cancelled: false,
        }));
        (
            Self {
                generation,
                state: state.clone(),
            },
            MainWindowComposerCandidateCustody { state },
        )
    }

    pub fn cancel(&mut self) {
        self.state.borrow_mut().cancelled = true;
    }

    pub fn start(
        &mut self,
        effect: RangePrepublicationEffect,
        app: &mut App,
        completed: impl FnOnce(&mut App) + 'static,
    ) -> Result<(), (RangePrepublicationEffect, String)> {
        self.start_with(effect, app, completed, async {})
    }

    fn start_with(
        &mut self,
        effect: RangePrepublicationEffect,
        app: &mut App,
        completed: impl FnOnce(&mut App) + 'static,
        before_read: impl Future<Output = ()> + Send + 'static,
    ) -> Result<(), (RangePrepublicationEffect, String)> {
        let generation = match &effect {
            RangePrepublicationEffect::ValidateOwner(request) => request.key.generation,
            RangePrepublicationEffect::Page { generation, .. }
            | RangePrepublicationEffect::ObjectPage { generation, .. } => *generation,
        };
        let mut state = self.state.borrow_mut();
        if generation != self.generation
            || state.cancelled
            || state.pending
            || state.completion.is_some()
            || state.resources.is_none()
        {
            return Err((
                effect,
                "candidate preparation worker is unavailable or stale".into(),
            ));
        }
        let (mut candidate, source) = state.resources.take().unwrap();
        state.pending = true;
        drop(state);
        let retained = self.state.clone();
        let work = app.background_executor().spawn(async move {
            before_read.await;
            let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
                read(&mut candidate, &source, &effect)
            }))
            .unwrap_or_else(|_| Err("candidate preparation read unwound".into()));
            (
                candidate,
                source,
                MainWindowComposerCandidateCompletion { effect, result },
            )
        });
        app.spawn(async move |cx| {
            let (candidate, source, completion) = work.await;
            {
                let mut state = retained.borrow_mut();
                state.resources = Some((candidate, source));
                state.completion = Some(completion);
                state.pending = false;
            }
            let _ = cx.update(completed);
        })
        .detach();
        Ok(())
    }

    #[cfg(feature = "test-faults")]
    pub fn test_start_with(
        &mut self,
        effect: RangePrepublicationEffect,
        app: &mut App,
        completed: impl FnOnce(&mut App) + 'static,
        before_read: impl Future<Output = ()> + Send + 'static,
    ) -> Result<(), (RangePrepublicationEffect, String)> {
        self.start_with(effect, app, completed, before_read)
    }
}

impl Drop for MainWindowComposerCandidateWorker {
    fn drop(&mut self) {
        self.state.borrow_mut().cancelled = true;
    }
}

impl MainWindowComposerCandidateCustody {
    pub fn pending(&self) -> bool {
        self.state.borrow().pending
    }

    pub fn cancelled(&self) -> bool {
        self.state.borrow().cancelled
    }

    pub fn take_completion(&mut self) -> Option<MainWindowComposerCandidateCompletion> {
        self.state.borrow_mut().completion.take()
    }

    pub fn take_resources(
        &mut self,
    ) -> Option<(HomeRecoveryCandidate, MainWindowComposerCandidateSource)> {
        let mut state = self.state.borrow_mut();
        if state.pending || state.completion.is_some() {
            return None;
        }
        state.resources.take()
    }
}

fn read(
    candidate: &mut HomeRecoveryCandidate,
    source: &MainWindowComposerCandidateSource,
    effect: &RangePrepublicationEffect,
) -> Result<MainWindowComposerCandidateRead, String> {
    let access = candidate.recovery_access().map_err(|e| e.to_string())?;
    match effect {
        RangePrepublicationEffect::ValidateOwner(request) => source
            .validate(&access, *request)
            .map(MainWindowComposerCandidateRead::Validation),
        RangePrepublicationEffect::Page { request, .. } => source
            .text_page(&access, *request)
            .map(MainWindowComposerCandidateRead::Page),
        RangePrepublicationEffect::ObjectPage { request, .. } => source
            .object_page(&access, *request)
            .map(MainWindowComposerCandidateRead::ObjectPage),
    }
}
