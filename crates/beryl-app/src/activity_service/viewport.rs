use std::{
    collections::VecDeque,
    ops::Range,
    sync::atomic::{AtomicBool, AtomicU64},
};

use beryl_model::ExecutionBinding;
use syndic_storage::{ActivityDisplayPreparation, ActivityDisplayReadError, ActivityDisplayStep};

use super::{
    display::{self, ActivityDisplayRow},
    *,
};

mod facts;
mod metadata;
mod preparation;
mod retry;
pub(crate) use facts::ActivityPreparedFacts;
pub(crate) use metadata::{ActivityMetadataCompletion, ActivityMetadataRequest};
pub(crate) use retry::ActivityRetry;

const CHECKPOINTS: usize = 8;
const RESIDENT_PAGES: usize = 4;

pub(crate) struct ActivityDemandOwner(Arc<AtomicU64>);

#[derive(Clone)]
pub(crate) struct ActivityDemand {
    owner: Weak<AtomicU64>,
    serial: u64,
    range: Range<u64>,
}

impl ActivityDemandOwner {
    pub(crate) fn new() -> Self {
        Self(Arc::new(AtomicU64::new(0)))
    }

    pub(crate) fn request(&self, range: Range<u64>) -> Option<ActivityDemand> {
        if range.start > range.end {
            return None;
        }
        let serial = self
            .0
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |v| v.checked_add(1))
            .ok()?
            + 1;
        Some(ActivityDemand {
            owner: Arc::downgrade(&self.0),
            serial,
            range,
        })
    }
}

impl ActivityDemand {
    fn current(&self) -> bool {
        self.owner
            .upgrade()
            .is_some_and(|owner| owner.load(Ordering::Acquire) == self.serial)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ActivityViewportStep {
    Pending,
    Ready,
    Failed,
    Retired,
}

struct Checkpoint {
    start: u64,
    cursor: ActivityContinuation,
}

struct Walk {
    start: u64,
    cursor: Option<ActivityContinuation>,
}

struct Preparing {
    start: u64,
    page: ActivityPage,
    rows: Vec<ActivityDisplayRow>,
    extraction: Option<ActivityDisplayPreparation>,
}

pub(crate) struct ActivityPreparedPage {
    pub(crate) start: u64,
    pub(crate) rows: Vec<ActivityDisplayRow>,
}

#[derive(Clone)]
pub(crate) struct ActivityPreparedWindow {
    active: Weak<AtomicBool>,
    selection: ActivityWindowSelection,
    collection: ActivityCollection,
    demand: ActivityDemand,
    facts: ActivityWindowFacts,
    pages: Vec<Arc<ActivityPreparedPage>>,
}

impl ActivityPreparedWindow {
    pub(crate) fn interaction_gate(&self) -> super::window::ActivityInteractionGate {
        super::window::ActivityInteractionGate::new(
            self.selection.clone(),
            self.facts.observation.clone(),
            Some(self.collection.runtime()),
        )
    }
    pub(crate) fn head(&self) -> &syndic_storage::ActivityQueryHeadRecord {
        self.collection.head()
    }
    pub(crate) fn pages(&self) -> &[Arc<ActivityPreparedPage>] {
        &self.pages
    }
    pub(crate) fn auto_visible(&self) -> bool {
        self.facts.auto_visible
    }

    pub(crate) fn with_current<T>(
        &self,
        publish: impl FnOnce(&Self) -> T,
    ) -> Result<T, ActivityReadError> {
        if !self
            .active
            .upgrade()
            .is_some_and(|active| active.load(Ordering::Acquire))
            || !self.selection.current()
            || !self.demand.current()
        {
            return Err(ActivityReadError::Selection);
        }
        self.collection.with_observed(&self.facts.observation, || {
            if !self
                .active
                .upgrade()
                .is_some_and(|active| active.load(Ordering::Acquire))
                || !self.selection.current()
                || !self.demand.current()
            {
                return Err(ActivityReadError::Selection);
            }
            Ok(publish(self))
        })?
    }
}

pub(crate) struct ActivityViewport {
    active: Arc<AtomicBool>,
    selection: ActivityWindowSelection,
    demand: ActivityDemand,
    request: Option<ActivityCollectionRequest>,
    collection: Option<ActivityCollection>,
    execution: Option<ExecutionBinding>,
    facts: Option<ActivityWindowFacts>,
    checkpoints: VecDeque<Checkpoint>,
    resident: VecDeque<Arc<ActivityPreparedPage>>,
    walk: Walk,
    preparing: Option<Preparing>,
    failure: Option<String>,
    retrying: bool,
    failure_token: Option<Arc<()>>,
    ready: bool,
    metadata: VecDeque<display::ActivityAgentMetadata>,
}

impl ActivityViewport {
    #[cfg(feature = "test-faults")]
    pub(super) fn request_state(&self) -> (bool, bool, u64) {
        (
            self.request.is_some(),
            self.collection.is_some(),
            self.walk.start,
        )
    }
    #[cfg(feature = "test-faults")]
    pub(super) fn diagnostics(&self) -> (usize, usize, bool) {
        (
            self.checkpoints.len(),
            self.resident.len(),
            self.preparing.is_some(),
        )
    }
    pub(crate) fn new(selection: ActivityWindowSelection, demand: ActivityDemand) -> Self {
        Self {
            active: Arc::new(AtomicBool::new(true)),
            selection,
            demand,
            request: None,
            collection: None,
            execution: None,
            facts: None,
            checkpoints: VecDeque::new(),
            resident: VecDeque::new(),
            walk: Walk {
                start: 0,
                cursor: None,
            },
            preparing: None,
            failure: None,
            retrying: false,
            failure_token: None,
            ready: false,
            metadata: VecDeque::new(),
        }
    }

    pub(crate) fn set_demand(&mut self, demand: ActivityDemand) {
        self.active.store(false, Ordering::Release);
        self.active = Arc::new(AtomicBool::new(true));
        self.demand = demand;
        self.preparing = None;
        self.failure = None;
        self.failure_token = None;
        self.retrying = false;
        self.ready = false;
        self.resident.retain(|page| {
            page.start < self.demand.range.end
                && page.start.saturating_add(page.rows.len() as u64) > self.demand.range.start
        });
        self.restart_walk();
    }

    pub(crate) fn failure(&self) -> Option<&str> {
        self.failure.as_deref()
    }
    pub(crate) fn retry(&mut self) -> bool {
        if self.failure.is_none() || self.retrying {
            return false;
        }
        self.retrying = true;
        true
    }

    pub(crate) fn retry_pending(&self) -> bool {
        self.retrying
    }
    pub(crate) fn facts(&self) -> Option<&ActivityWindowFacts> {
        self.facts.as_ref()
    }

    pub(crate) fn publish_facts<T>(
        &self,
        publish: impl FnOnce(&ActivityWindowFacts) -> T,
    ) -> Result<T, ActivityReadError> {
        self.prepared_facts()
            .ok_or(ActivityReadError::Unavailable)?
            .with_current(publish)
    }

    fn restart_walk(&mut self) {
        self.walk = self
            .checkpoints
            .iter()
            .rev()
            .filter(|checkpoint| checkpoint.start <= self.demand.range.start)
            .max_by_key(|checkpoint| checkpoint.start)
            .map(|checkpoint| Walk {
                start: checkpoint.start,
                cursor: Some(checkpoint.cursor.clone()),
            })
            .unwrap_or(Walk {
                start: 0,
                cursor: None,
            });
    }

    fn replace(&mut self) {
        self.active.store(false, Ordering::Release);
        self.active = Arc::new(AtomicBool::new(true));
        self.request = None;
        self.collection = None;
        self.preparing = None;
        self.checkpoints.clear();
        self.resident.clear();
        self.metadata.clear();
        self.facts = None;
        self.execution = None;
        self.failure = None;
        self.failure_token = None;
        self.retrying = false;
        self.ready = false;
        self.walk = Walk {
            start: 0,
            cursor: None,
        };
    }

    fn read_failure(&mut self, error: ActivityReadError) -> ActivityViewportStep {
        if busy(&error) {
            return ActivityViewportStep::Pending;
        }
        if matches!(
            &error,
            ActivityReadError::Read(
                SyndicReadError::StaleActivityQuery | SyndicReadError::ActivityQueryIsStale
            )
        ) {
            self.replace();
            return ActivityViewportStep::Pending;
        }
        self.fail(error)
    }

    fn fail(&mut self, error: impl std::fmt::Display) -> ActivityViewportStep {
        let mut text = error.to_string();
        if text.len() > 512 {
            let mut end = 509;
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            text.truncate(end);
            text.push_str("...");
        }
        self.failure = Some(text);
        self.failure_token.get_or_insert_with(|| Arc::new(()));
        self.retrying = false;
        self.ready = false;
        ActivityViewportStep::Failed
    }

    fn extraction_failure(&mut self, error: ActivityDisplayReadError) -> ActivityViewportStep {
        match error {
            ActivityDisplayReadError::Stale => {
                self.replace();
                ActivityViewportStep::Pending
            }
            ActivityDisplayReadError::Cancelled => {
                self.replace();
                ActivityViewportStep::Retired
            }
            error => self.fail(error),
        }
    }

    fn covered(&self) -> bool {
        let Some(collection) = &self.collection else {
            return false;
        };
        let count = collection.head().logical_row_count();
        let end = self.demand.range.end.min(count);
        let mut next = self.demand.range.start.min(count);
        while next < end {
            let Some(page) = self.resident.iter().find(|page| {
                page.start <= next && page.start.saturating_add(page.rows.len() as u64) > next
            }) else {
                return false;
            };
            next = page.start.saturating_add(page.rows.len() as u64);
        }
        true
    }

    pub(crate) fn prepared(&self) -> Option<ActivityPreparedWindow> {
        if !self.ready {
            return None;
        }
        self.snapshot()
    }

    pub(crate) fn snapshot(&self) -> Option<ActivityPreparedWindow> {
        if !self.demand.current() {
            return None;
        }
        Some(ActivityPreparedWindow {
            active: Arc::downgrade(&self.active),
            selection: self.selection.clone(),
            collection: self.collection.as_ref()?.clone(),
            demand: self.demand.clone(),
            facts: self.facts.as_ref()?.clone(),
            pages: self.resident.iter().cloned().collect(),
        })
    }

    pub(crate) fn step(&mut self, cancelled: &AtomicBool) -> ActivityViewportStep {
        self.step_inner(cancelled, false)
    }

    pub(crate) fn step_visible(&mut self, cancelled: &AtomicBool) -> ActivityViewportStep {
        self.step_inner(cancelled, true)
    }

    fn step_inner(&mut self, cancelled: &AtomicBool, visible_only: bool) -> ActivityViewportStep {
        if cancelled.load(Ordering::Acquire) || !self.selection.current() || !self.demand.current()
        {
            self.replace();
            self.failure = None;
            return ActivityViewportStep::Retired;
        }
        let facts = match self
            .selection
            .observe(self.collection.as_ref().map(ActivityCollection::head))
        {
            Ok(facts) => facts,
            Err(error) => return self.read_failure(error),
        };
        if self
            .execution
            .as_ref()
            .is_some_and(|execution| execution != &facts.execution)
        {
            self.replace();
            return ActivityViewportStep::Pending;
        }
        if visible_only && !facts.auto_visible {
            self.replace();
            self.facts = Some(facts);
            return ActivityViewportStep::Ready;
        }
        self.facts = Some(facts);
        let scope = if let Some(collection) = &self.collection {
            collection.with_current_head(|_| ())
        } else if let Some(request) = &self.request {
            request.with_current(|| ())
        } else {
            Ok(())
        };
        if let Err(error) = scope {
            if matches!(
                error,
                ActivityReadError::Runtime(RuntimeActivityReadError::RuntimeUnavailable)
            ) {
                self.replace();
                return ActivityViewportStep::Pending;
            }
            return self.read_failure(error);
        }
        if self.failure.is_some() && !self.retrying {
            return ActivityViewportStep::Failed;
        }
        if self.collection.is_none() {
            if self.request.is_none() {
                match self
                    .selection
                    .prepare_collection(self.facts.as_ref().unwrap())
                {
                    Ok(request) => {
                        self.request = Some(request);
                    }
                    Err(error) => return self.read_failure(error),
                }
            }
            match self.request.as_ref().unwrap().open() {
                Ok(collection) => {
                    self.execution = Some(self.facts.as_ref().unwrap().execution.clone());
                    self.collection = Some(collection);
                    self.request = None;
                }
                Err(error) => return self.read_failure(error),
            }
            return ActivityViewportStep::Pending;
        }
        if self.covered() {
            self.ready = true;
            self.failure = None;
            self.failure_token = None;
            self.retrying = false;
            return ActivityViewportStep::Ready;
        }
        if self.preparing.is_some() {
            return self.prepare_step(cancelled);
        }
        let collection = self.collection.as_ref().unwrap();
        let page = match collection.read_page(self.walk.cursor.as_ref()) {
            Ok(page) => page,
            Err(error) => return self.read_failure(error),
        };
        let end = self.walk.start.saturating_add(page.records().len() as u64);
        if page.records().len() > 64
            || (page.next().is_none() && end < collection.head().logical_row_count())
        {
            return self.fail("Activity page disagrees with its bounded query head");
        }
        if page.records().is_empty() && self.walk.start < collection.head().logical_row_count() {
            return self.fail("Activity page did not advance");
        }
        if end <= self.demand.range.start {
            self.advance(end, page.next());
            return ActivityViewportStep::Pending;
        }
        if self
            .resident
            .iter()
            .any(|resident| resident.start == self.walk.start)
        {
            self.advance(end, page.next());
            return ActivityViewportStep::Pending;
        }
        self.preparing = Some(Preparing {
            start: self.walk.start,
            page,
            rows: Vec::new(),
            extraction: None,
        });
        ActivityViewportStep::Pending
    }

    fn advance(&mut self, end: u64, next: Option<ActivityContinuation>) {
        if let Some(cursor) = &next {
            self.checkpoints.push_back(Checkpoint {
                start: end,
                cursor: cursor.clone(),
            });
            while self.checkpoints.len() > CHECKPOINTS {
                self.checkpoints.pop_front();
            }
        }
        self.walk = Walk {
            start: end,
            cursor: next,
        };
    }
}

fn busy(error: &ActivityReadError) -> bool {
    matches!(
        error,
        ActivityReadError::Busy
            | ActivityReadError::RequestPending
            | ActivityReadError::Runtime(RuntimeActivityReadError::Busy)
            | ActivityReadError::Coherence(HomeCoherenceError::Busy)
            | ActivityReadError::Observation(HomeObservedCoherenceError::Coherence(
                HomeCoherenceError::Busy
            ))
            | ActivityReadError::Observation(HomeObservedCoherenceError::Observation(
                beryl_home_store::HomeMutationObservationError::Busy
                    | beryl_home_store::HomeMutationObservationError::Stale
            ))
    )
}
