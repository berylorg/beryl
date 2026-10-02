use std::{
    cell::{Ref, RefCell},
    future::Future,
    panic::AssertUnwindSafe,
    rc::Rc,
    sync::Arc,
};

use beryl_home_store::HomeRecoveryCandidate;
use gpui::{App, AppContext};
use gpui_text_input::{
    ObjectPage, RangePage, RangePrepublicationEffect, RangePrepublicationSessionGeneration,
    RangePrepublicationValidationResponse,
};

use super::super::prepublication::MainWindowNativeLineagePrepublicationSource;
use super::MainWindowComposerCandidateSource;
use crate::main_window::MainWindowComposerRetiredClose;

mod authentication;
mod cleanup;
mod source;
pub use source::MainWindowComposerPrepublicationSource;

type PreparedSource<S = MainWindowComposerCandidateSource, R = MainWindowComposerRetiredClose> =
    Result<S, (R, String)>;

type CandidateRead<C, S = MainWindowComposerCandidateSource> =
    fn(&mut C, &S, &RangePrepublicationEffect) -> Result<MainWindowComposerCandidateRead, String>;

pub enum MainWindowComposerCandidateRead {
    Validation(RangePrepublicationValidationResponse),
    Page(RangePage),
    ObjectPage(ObjectPage),
}

pub struct MainWindowComposerCandidateCompletion {
    pub effect: RangePrepublicationEffect,
    pub result: Result<MainWindowComposerCandidateRead, String>,
}

struct State<C, S: MainWindowComposerPrepublicationSource, R> {
    generation: Option<RangePrepublicationSessionGeneration>,
    resources: Option<(C, PreparedSource<S, R>)>,
    read: CandidateRead<C, S>,
    completion: Option<MainWindowComposerCandidateCompletion>,
    pending: bool,
    cancelled: bool,
    cleanup: Option<Arc<MainWindowNativeLineagePrepublicationSource>>,
}

pub struct MainWindowComposerCandidateWorker<
    C = HomeRecoveryCandidate,
    S: MainWindowComposerPrepublicationSource = MainWindowComposerCandidateSource,
    R = MainWindowComposerRetiredClose,
> {
    state: Rc<RefCell<State<C, S, R>>>,
}

pub struct MainWindowComposerCandidateCustody<
    C = HomeRecoveryCandidate,
    S: MainWindowComposerPrepublicationSource = MainWindowComposerCandidateSource,
    R = MainWindowComposerRetiredClose,
> {
    state: Rc<RefCell<State<C, S, R>>>,
}

impl MainWindowComposerCandidateWorker {
    pub fn new(
        candidate: HomeRecoveryCandidate,
        source: MainWindowComposerCandidateSource,
    ) -> (Self, MainWindowComposerCandidateCustody) {
        let state = Rc::new(RefCell::new(State {
            generation: None,
            resources: Some((candidate, Ok(source))),
            read,
            completion: None,
            pending: false,
            cancelled: false,
            cleanup: None,
        }));
        (
            Self {
                state: state.clone(),
            },
            MainWindowComposerCandidateCustody { state },
        )
    }
}

impl<C, S: MainWindowComposerPrepublicationSource, R> MainWindowComposerCandidateWorker<C, S, R> {
    pub fn cancel(&mut self) {
        let mut state = self.state.borrow_mut();
        state.cancelled = true;
        state.completion = None;
        if let Some(cleanup) = &state.cleanup {
            cleanup.release_owner();
        }
    }

    pub fn bind_prepublication(
        &mut self,
        generation: RangePrepublicationSessionGeneration,
        environment: &gpui_text_input::RangePrepublicationEnvironment,
    ) -> Result<(), String> {
        let mut state = self.state.borrow_mut();
        if state.generation.is_some()
            || state.cancelled
            || state.pending
            || state.completion.is_some()
            || !matches!(state.resources, Some((_, Ok(_))))
        {
            return Err("candidate preparation source is unavailable or already bound".into());
        }
        let (_, Ok(source)) = state.resources.as_ref().unwrap() else {
            unreachable!()
        };
        if environment.config().binding != source.seed().binding {
            return Err("candidate preparation environment has another source".into());
        }
        state.cleanup = Some(MainWindowNativeLineagePrepublicationSource::new(
            source.selection(),
            environment.id(),
            generation,
            environment.cleanup().clone(),
        ));
        state.generation = Some(generation);
        Ok(())
    }
}

impl<C: Send + 'static, S: MainWindowComposerPrepublicationSource, R: Send + 'static>
    MainWindowComposerCandidateWorker<C, S, R>
{
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
        if Some(generation) != state.generation
            || state.cancelled
            || state.pending
            || state.completion.is_some()
            || !matches!(state.resources, Some((_, Ok(_))))
        {
            return Err((
                effect,
                "candidate preparation worker is unavailable or stale".into(),
            ));
        }
        if let Err(error) = state
            .cleanup
            .as_ref()
            .unwrap()
            .begin(cleanup::copy_effect(&effect))
        {
            return Err((effect, error));
        }
        let (mut candidate, Ok(source)) = state.resources.take().unwrap() else {
            unreachable!("authenticated source checked before taking resources")
        };
        state.pending = true;
        let read = state.read;
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
                state.resources = Some((candidate, Ok(source)));
                state.pending = false;
                let token = cleanup::token(&completion.effect);
                if state.cancelled {
                    drop(completion);
                } else {
                    state.completion = Some(completion);
                }
                state.cleanup.as_ref().unwrap().finish_delivery(token);
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

impl<C, S: MainWindowComposerPrepublicationSource, R> Drop
    for MainWindowComposerCandidateWorker<C, S, R>
{
    fn drop(&mut self) {
        self.cancel();
    }
}

impl<C, S: MainWindowComposerPrepublicationSource, R> MainWindowComposerCandidateCustody<C, S, R> {
    pub(in crate::main_window) fn retain_adoption_cleanup(
        &self,
        executor: gpui::BackgroundExecutor,
    ) -> Result<(), String> {
        let state = self.state.borrow();
        if state.cancelled || state.pending || state.completion.is_some() {
            return Err("resident adoption source is not ready".into());
        }
        let Some((_, Ok(source))) = &state.resources else {
            return Err("resident adoption source is unavailable".into());
        };
        source.retain_cleanup(
            state
                .cleanup
                .clone()
                .ok_or("resident adoption is not bound")?,
            executor,
        )
    }

    pub fn source(&self) -> Option<Ref<'_, S>> {
        Ref::filter_map(self.state.borrow(), |state| {
            state.resources.as_ref()?.1.as_ref().ok()
        })
        .ok()
    }

    pub fn preparation_error(&self) -> Option<String> {
        self.state
            .borrow()
            .resources
            .as_ref()?
            .1
            .as_ref()
            .err()
            .map(|(_, error)| error.clone())
    }

    pub fn pending(&self) -> bool {
        self.state.borrow().pending
    }

    pub fn cancelled(&self) -> bool {
        self.state.borrow().cancelled
    }

    pub fn completion(&self) -> Option<Ref<'_, MainWindowComposerCandidateCompletion>> {
        Ref::filter_map(self.state.borrow(), |state| state.completion.as_ref()).ok()
    }

    pub fn deliver_completion(
        &mut self,
        session: &mut gpui_text_input::RangePrepublicationSession,
    ) -> Result<Option<gpui_text_input::RangePrepublicationDelivery>, String> {
        let mut state = self.state.borrow_mut();
        let Some(completion) = state.completion.as_ref() else {
            return Ok(None);
        };
        let generation = cleanup::generation(&completion.effect);
        if session.generation() != generation {
            return Err("candidate preparation completion belongs to another session".into());
        }
        let completion = state.completion.take().unwrap();
        let delivery = match completion.result? {
            MainWindowComposerCandidateRead::Validation(response) => {
                session.deliver_validation(response)
            }
            MainWindowComposerCandidateRead::Page(page) => session.deliver_page(generation, page),
            MainWindowComposerCandidateRead::ObjectPage(page) => {
                session.deliver_object_page(generation, page)
            }
        };
        Ok(Some(delivery))
    }

    pub fn take_resources(&mut self) -> Option<(C, S)> {
        let mut state = self.state.borrow_mut();
        if state.pending
            || state.completion.is_some()
            || !matches!(state.resources, Some((_, Ok(_))))
        {
            return None;
        }
        let (candidate, Ok(source)) = state.resources.take()? else {
            unreachable!("authenticated source checked before taking resources")
        };
        Some((candidate, source))
    }

    pub fn take_refused_resources(&mut self) -> Option<(C, R, String)> {
        let mut state = self.state.borrow_mut();
        if state.pending
            || state.completion.is_some()
            || !matches!(state.resources, Some((_, Err(_))))
        {
            return None;
        }
        let (candidate, Err((retired, error))) = state.resources.take()? else {
            unreachable!("refused source checked before taking resources")
        };
        Some((candidate, retired, error))
    }
}

fn read(
    candidate: &mut HomeRecoveryCandidate,
    source: &MainWindowComposerCandidateSource,
    effect: &RangePrepublicationEffect,
) -> Result<MainWindowComposerCandidateRead, String> {
    source.read_prepublication(candidate, effect)
}

impl MainWindowComposerCandidateSource {
    pub(crate) fn read_prepublication(
        &self,
        candidate: &mut HomeRecoveryCandidate,
        effect: &RangePrepublicationEffect,
    ) -> Result<MainWindowComposerCandidateRead, String> {
        let source = self;
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
}
