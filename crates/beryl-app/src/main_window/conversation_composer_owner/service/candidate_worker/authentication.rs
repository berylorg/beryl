use super::*;
use beryl_state::BerylState;
use gpui_text_input::RangeRestorationSeed;
use syndic_storage::SyndicStorage;

impl MainWindowComposerCandidateWorker {
    pub fn prepare(
        candidate: HomeRecoveryCandidate,
        retired: MainWindowComposerRetiredClose,
        storage: SyndicStorage,
        state: BerylState,
        seed: RangeRestorationSeed,
        app: &mut App,
        completed: impl FnOnce(&mut App) + 'static,
    ) -> (Self, MainWindowComposerCandidateCustody) {
        Self::prepare_with(
            candidate,
            retired,
            storage,
            state,
            seed,
            app,
            completed,
            async {},
        )
    }

    fn prepare_with(
        mut candidate: HomeRecoveryCandidate,
        retired: MainWindowComposerRetiredClose,
        storage: SyndicStorage,
        home_state: BerylState,
        seed: RangeRestorationSeed,
        app: &mut App,
        completed: impl FnOnce(&mut App) + 'static,
        before_authentication: impl Future<Output = ()> + Send + 'static,
    ) -> (Self, MainWindowComposerCandidateCustody) {
        let state = Rc::new(RefCell::new(State {
            resources: None,
            completion: None,
            pending: true,
            cancelled: false,
        }));
        let retained = state.clone();
        let work = app.background_executor().spawn(async move {
            before_authentication.await;
            let result = MainWindowComposerCandidateSource::new(
                &mut candidate,
                retired,
                storage,
                &home_state,
                seed,
            );
            (candidate, result)
        });
        app.spawn(async move |cx| {
            let resources = work.await;
            {
                let mut state = retained.borrow_mut();
                state.resources = Some(resources);
                state.pending = false;
            }
            let _ = cx.update(completed);
        })
        .detach();
        (
            Self {
                generation: None,
                state: state.clone(),
            },
            MainWindowComposerCandidateCustody { state },
        )
    }

    #[cfg(feature = "test-faults")]
    pub fn test_prepare_with(
        candidate: HomeRecoveryCandidate,
        retired: MainWindowComposerRetiredClose,
        storage: SyndicStorage,
        state: BerylState,
        seed: RangeRestorationSeed,
        app: &mut App,
        completed: impl FnOnce(&mut App) + 'static,
        before_authentication: impl Future<Output = ()> + Send + 'static,
    ) -> (Self, MainWindowComposerCandidateCustody) {
        Self::prepare_with(
            candidate,
            retired,
            storage,
            state,
            seed,
            app,
            completed,
            before_authentication,
        )
    }
}
