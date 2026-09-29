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
        Self::prepare_task(
            async move {
                before_authentication.await;
                let result = MainWindowComposerCandidateSource::new(
                    &mut candidate,
                    retired,
                    storage,
                    &home_state,
                    seed,
                );
                (candidate, result)
            },
            read,
            app,
            completed,
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

impl<C: Send + 'static> MainWindowComposerCandidateWorker<C> {
    fn prepare_task(
        task: impl Future<Output = (C, PreparedSource)> + Send + 'static,
        read: CandidateRead<C>,
        app: &mut App,
        completed: impl FnOnce(&mut App) + 'static,
    ) -> (Self, MainWindowComposerCandidateCustody<C>) {
        let state = Rc::new(RefCell::new(State {
            generation: None,
            resources: None,
            read,
            completion: None,
            pending: true,
            cancelled: false,
            cleanup: None,
        }));
        let retained = state.clone();
        let work = app.background_executor().spawn(task);
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
                state: state.clone(),
            },
            MainWindowComposerCandidateCustody { state },
        )
    }
}

use crate::app_services::recovery_graph::PreparedRecoveryServiceGraph;

impl MainWindowComposerCandidateWorker<PreparedRecoveryServiceGraph> {
    pub(crate) fn prepare_graph(
        graph: PreparedRecoveryServiceGraph,
        retired: MainWindowComposerRetiredClose,
        seed: RangeRestorationSeed,
        app: &mut App,
        completed: impl FnOnce(&mut App) + 'static,
    ) -> (
        Self,
        MainWindowComposerCandidateCustody<PreparedRecoveryServiceGraph>,
    ) {
        Self::prepare_graph_with(graph, retired, seed, app, completed, async {})
    }

    pub(crate) fn prepare_graph_with(
        mut graph: PreparedRecoveryServiceGraph,
        retired: MainWindowComposerRetiredClose,
        seed: RangeRestorationSeed,
        app: &mut App,
        completed: impl FnOnce(&mut App) + 'static,
        before_authentication: impl Future<Output = ()> + Send + 'static,
    ) -> (
        Self,
        MainWindowComposerCandidateCustody<PreparedRecoveryServiceGraph>,
    ) {
        Self::prepare_task(
            async move {
                before_authentication.await;
                let result = graph.composer_recovery_source(retired, seed);
                (graph, result)
            },
            PreparedRecoveryServiceGraph::composer_recovery_read,
            app,
            completed,
        )
    }
}
