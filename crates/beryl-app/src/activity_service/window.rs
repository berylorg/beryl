use beryl_home_store::{HomeMutationObservation, HomeMutationObserver};
use beryl_model::{ExecutionBinding, WindowId};
use beryl_state::{SessionState, ThreadClaimState, WindowClaimSelection};
use syndic_storage::{ActivityQueryHeadRecord, InputGateState, TurnLifecycle};

use super::*;

mod height_custody;
mod interaction;
mod presentation;
pub(crate) use height_custody::{ActivityHeightCustody, ActivityHeightFlight};
pub(crate) use interaction::ActivityInteractionGate;
pub(crate) use presentation::{ActivityHeightCommit, ActivityHeightError, ActivityHeightRead};

#[derive(Clone)]
pub(crate) struct ActivityWindowReader {
    service: Weak<Shared>,
    session: SessionState,
    mutations: HomeMutationObserver,
    graph: Weak<()>,
    heights: Arc<ActivityHeightCustody>,
}

#[derive(Clone)]
pub(crate) struct ActivityWindowSelection {
    reader: ActivityWindowReader,
    window: WindowId,
    claim: WindowClaimSelection,
    lifetime: Weak<()>,
}

#[derive(Clone)]
pub(crate) struct ActivityWindowFacts {
    pub(crate) auto_visible: bool,
    pub(crate) execution: ExecutionBinding,
    pub(crate) observation: HomeMutationObservation,
}

impl ActivityService {
    pub(crate) fn window_reader(
        &self,
        session: SessionState,
        mutations: HomeMutationObserver,
        graph: Weak<()>,
    ) -> ActivityWindowReader {
        ActivityWindowReader {
            service: Arc::downgrade(&self.shared),
            session,
            mutations,
            graph,
            heights: Arc::default(),
        }
    }
}

impl ActivityWindowReader {
    pub(crate) fn generation(&self) -> Option<beryl_home_store::HomeGenerationIdentity> {
        self.service.upgrade().and_then(|service| service.identity)
    }
    pub(crate) fn with_height_custody(mut self, custody: Arc<ActivityHeightCustody>) -> Self {
        self.heights = custody;
        self
    }
    pub(crate) fn same_publication(&self, other: &Self) -> bool {
        self.service.ptr_eq(&other.service) && self.graph.ptr_eq(&other.graph)
    }
    pub(crate) fn select(
        &self,
        window: WindowId,
        claim: WindowClaimSelection,
        lifetime: Weak<()>,
    ) -> ActivityWindowSelection {
        ActivityWindowSelection {
            reader: self.clone(),
            window,
            claim,
            lifetime,
        }
    }
}

impl ActivityWindowSelection {
    pub(super) fn window_id(&self) -> WindowId {
        self.window
    }
    pub(crate) fn current(&self) -> bool {
        self.reader.graph.strong_count() != 0 && self.lifetime.strong_count() != 0
    }

    pub(crate) fn thread_id(&self) -> SyndicThreadId {
        self.claim.thread_id()
    }

    fn shared(&self) -> Result<Arc<Shared>, ActivityReadError> {
        if !self.current() {
            return Err(ActivityReadError::Selection);
        }
        self.reader
            .service
            .upgrade()
            .ok_or(ActivityReadError::Unavailable)
    }

    pub(super) fn begin(&self) -> Result<Operation, ActivityReadError> {
        self.shared()?.begin()
    }

    pub(crate) fn observe(
        &self,
        expected: Option<&ActivityQueryHeadRecord>,
    ) -> Result<ActivityWindowFacts, ActivityReadError> {
        let shared = self.shared()?;
        let observation = self
            .reader
            .mutations
            .observe()
            .map_err(|error| ActivityReadError::Observation(error.into()))?;
        let operation = shared.begin()?;
        let resources = operation.resources();
        let claim = self
            .reader
            .session
            .window_claim_catalog_source(&resources.home, self.window)
            .map_err(|_| ActivityReadError::Selection)?
            .claim()
            .ok_or(ActivityReadError::Selection)?;
        if claim.window_id() != self.window
            || claim.thread_id() != self.claim.thread_id()
            || claim.generation() != self.claim.generation()
            || claim.revision() != self.claim.revision()
            || claim.state() != ThreadClaimState::Active
        {
            return Err(ActivityReadError::Selection);
        }
        let storage = &resources.storage;
        let home = &resources.home;
        let execution = storage
            .thread_execution(home, self.claim.thread_id(), point_limit())?
            .ok_or(ActivityReadError::ThreadUnavailable)?
            .execution()
            .clone();
        let gate = storage
            .input_gate(home, self.claim.thread_id(), point_limit())?
            .ok_or(ActivityReadError::ThreadUnavailable)?;
        let auto_visible = if matches!(gate.state(), InputGateState::Compacting { .. }) {
            true
        } else if let Some(turn) = gate.state().blocking_turn_id() {
            storage
                .turn_state(home, turn, point_limit())?
                .is_some_and(|state| {
                    matches!(
                        state.lifecycle(),
                        TurnLifecycle::Pending
                            | TurnLifecycle::Active
                            | TurnLifecycle::UnknownTerminal
                    )
                })
        } else {
            false
        };
        if let Some(expected) = expected {
            let actual =
                storage.activity_query_head(home, self.claim.thread_id(), point_limit())?;
            if actual.as_ref() != Some(expected) {
                return Err(SyndicReadError::StaleActivityQuery.into());
            }
        }
        self.publish_observed(&observation, || ())?;
        Ok(ActivityWindowFacts {
            auto_visible,
            execution,
            observation,
        })
    }

    pub(crate) fn prepare_collection(
        &self,
        facts: &ActivityWindowFacts,
    ) -> Result<ActivityCollectionRequest, ActivityReadError> {
        self.publish_observed(&facts.observation, || ())?;
        let request = collection::prepare(
            &self.shared()?,
            self.claim.thread_id(),
            facts.execution.runtime_id(),
        )?;
        self.publish_observed(&facts.observation, || ())?;
        Ok(request)
    }

    pub(crate) fn publish_observed<T>(
        &self,
        observation: &HomeMutationObservation,
        publish: impl FnOnce() -> T,
    ) -> Result<T, ActivityReadError> {
        self.shared()?.publish_observed(observation, || {
            if !self.current() {
                return Err(ActivityReadError::Selection);
            }
            Ok(publish())
        })?
    }
}
