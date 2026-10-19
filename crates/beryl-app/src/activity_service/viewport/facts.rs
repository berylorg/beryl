use super::*;
use crate::activity_service::collection::ActivityPublicationScope;

pub(crate) struct ActivityPreparedFacts {
    active: Weak<AtomicBool>,
    selection: ActivityWindowSelection,
    demand: ActivityDemand,
    facts: ActivityWindowFacts,
    scope: Option<ActivityPublicationScope>,
}

impl ActivityPreparedFacts {
    pub(crate) fn interaction_gate(&self) -> super::super::window::ActivityInteractionGate {
        super::super::window::ActivityInteractionGate::new(
            self.selection.clone(),
            self.facts.observation.clone(),
            self.scope.as_ref().map(ActivityPublicationScope::runtime),
        )
    }
    pub(crate) fn with_current<T>(
        &self,
        publish: impl FnOnce(&ActivityWindowFacts) -> T,
    ) -> Result<T, ActivityReadError> {
        let current = || {
            self.active
                .upgrade()
                .is_some_and(|active| active.load(Ordering::Acquire))
                && self.selection.current()
                && self.demand.current()
        };
        if !current() {
            return Err(ActivityReadError::Selection);
        }
        let publish = || {
            if !current() {
                return Err(ActivityReadError::Selection);
            }
            Ok(publish(&self.facts))
        };
        match &self.scope {
            Some(scope) => scope.with_observed(&self.facts.observation, publish)?,
            None => self
                .selection
                .publish_observed(&self.facts.observation, publish)?,
        }
    }
}

impl ActivityViewport {
    pub(crate) fn prepared_facts(&self) -> Option<ActivityPreparedFacts> {
        if !self.demand.current() {
            return None;
        }
        Some(ActivityPreparedFacts {
            active: Arc::downgrade(&self.active),
            selection: self.selection.clone(),
            demand: self.demand.clone(),
            facts: self.facts.as_ref()?.clone(),
            scope: self
                .collection
                .as_ref()
                .map(ActivityCollection::publication_scope)
                .or_else(|| {
                    self.request
                        .as_ref()
                        .map(ActivityCollectionRequest::publication_scope)
                }),
        })
    }
}
