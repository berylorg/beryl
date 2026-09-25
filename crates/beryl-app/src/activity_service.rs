use std::{
    num::NonZeroUsize,
    sync::{
        Arc, Condvar, Mutex, Weak,
        atomic::{AtomicUsize, Ordering},
    },
};

use beryl_home_store::{
    CursorReadLimits, HomeCoherenceError, HomeGeneration, HomeServiceReference, HomeStore,
};
use beryl_model::{BerylHomeId, RuntimeId, SyndicThreadId};
use syndic_storage::{SyndicPointReadLimit, SyndicReadError, SyndicStorage};

use crate::cas_projection::{RuntimeActivityReadError, RuntimeActivityReadSource};

mod collection;
mod preparation;
pub(crate) use collection::{
    ActivityCollection, ActivityCollectionRequest, ActivityContinuation, ActivityPage,
};
pub(crate) use preparation::PreparedActivityService;

#[derive(Clone, Copy)]
pub(crate) struct ActivityServiceLimits {
    collections: NonZeroUsize,
    pages: NonZeroUsize,
    page: CursorReadLimits,
}

impl ActivityServiceLimits {
    pub(crate) fn new(
        collections: NonZeroUsize,
        pages: NonZeroUsize,
        page: CursorReadLimits,
    ) -> Self {
        Self {
            collections,
            pages,
            page,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ActivityReadError {
    #[error("Activity service is unavailable")]
    Unavailable,
    #[error("Activity service belongs to another home generation")]
    Identity,
    #[error("Activity collection capacity is full")]
    CollectionCapacity,
    #[error("Activity page capacity is full")]
    PageCapacity,
    #[error("Activity query is already pending")]
    RequestPending,
    #[error("Activity initial request already completed")]
    RequestCompleted,
    #[error("Activity thread is unavailable")]
    ThreadUnavailable,
    #[error("Activity thread has not enrolled in the current runtime period")]
    Unenrolled,
    #[error("Activity continuation belongs to another collection")]
    InvalidContinuation,
    #[error("Activity runtime is unavailable: {0}")]
    Runtime(#[from] RuntimeActivityReadError),
    #[error("Activity read failed: {0}")]
    Read(#[from] SyndicReadError),
    #[error("Activity home authority is unavailable: {0}")]
    Coherence(#[from] HomeCoherenceError),
}

pub(crate) struct ActivityService {
    shared: Arc<Shared>,
}

struct Shared {
    home_id: BerylHomeId,
    generation: HomeGeneration,
    limits: ActivityServiceLimits,
    state: Mutex<State>,
    drained: Condvar,
    collections: AtomicUsize,
    pages: AtomicUsize,
}

struct State {
    live: bool,
    resources: Option<Resources>,
    active: usize,
}

#[derive(Clone)]
struct Resources {
    home: HomeServiceReference,
    storage: SyndicStorage,
    runtime: RuntimeActivityReadSource,
}

struct Operation {
    resources: Option<Resources>,
    shared: Arc<Shared>,
}

#[derive(Clone, Copy)]
enum CapacityKind {
    Collection,
    Page,
}

struct Capacity {
    shared: Weak<Shared>,
    kind: CapacityKind,
}

impl ActivityService {
    fn dormant(resources: Resources, limits: ActivityServiceLimits) -> Self {
        Self {
            shared: Arc::new(Shared {
                home_id: resources.runtime.home_id(),
                generation: resources.runtime.home_generation(),
                limits,
                state: Mutex::new(State {
                    live: false,
                    resources: Some(resources),
                    active: 0,
                }),
                drained: Condvar::new(),
                collections: AtomicUsize::new(0),
                pages: AtomicUsize::new(0),
            }),
        }
    }

    pub(crate) fn prepare_collection(
        &self,
        thread: SyndicThreadId,
        runtime: RuntimeId,
    ) -> Result<ActivityCollectionRequest, ActivityReadError> {
        collection::prepare(&self.shared, thread, runtime)
    }

    pub(crate) fn retire(&self) {
        let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
        state.live = false;
        while state.active != 0 {
            state = self
                .shared
                .drained
                .wait(state)
                .unwrap_or_else(|e| e.into_inner());
        }
        let resources = state.resources.take();
        drop(state);
        drop(resources);
    }
}

impl Drop for ActivityService {
    fn drop(&mut self) {
        self.retire();
    }
}

impl Shared {
    fn begin(self: &Arc<Self>) -> Result<Operation, ActivityReadError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| ActivityReadError::Unavailable)?;
        if !state.live {
            return Err(ActivityReadError::Unavailable);
        }
        let resources = state
            .resources
            .as_ref()
            .ok_or(ActivityReadError::Unavailable)?
            .clone();
        // Each admitted collection has at most one reader; initial reads own a collection slot.
        state.active = state
            .active
            .checked_add(1)
            .ok_or(ActivityReadError::Unavailable)?;
        Ok(Operation {
            resources: Some(resources),
            shared: Arc::clone(self),
        })
    }

    fn reserve(self: &Arc<Self>, kind: CapacityKind) -> Result<Capacity, ActivityReadError> {
        let state = self
            .state
            .lock()
            .map_err(|_| ActivityReadError::Unavailable)?;
        if !state.live {
            return Err(ActivityReadError::Unavailable);
        }
        let (count, maximum, error) = match kind {
            CapacityKind::Collection => (
                &self.collections,
                self.limits.collections.get(),
                ActivityReadError::CollectionCapacity,
            ),
            CapacityKind::Page => (
                &self.pages,
                self.limits.pages.get(),
                ActivityReadError::PageCapacity,
            ),
        };
        count
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < maximum).then(|| count + 1)
            })
            .map_err(|_| error)?;
        Ok(Capacity {
            shared: Arc::downgrade(self),
            kind,
        })
    }

    fn publish<T>(&self, publish: impl FnOnce() -> T) -> Result<T, ActivityReadError> {
        let state = self
            .state
            .lock()
            .map_err(|_| ActivityReadError::Unavailable)?;
        if !state.live {
            return Err(ActivityReadError::Unavailable);
        }
        let resources = state
            .resources
            .as_ref()
            .ok_or(ActivityReadError::Unavailable)?;
        Ok(resources
            .home
            .try_elect_coherent(self.generation, publish)?)
    }
}

impl Drop for Operation {
    fn drop(&mut self) {
        drop(self.resources.take());
        let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
        state.active -= 1;
        self.shared.drained.notify_all();
    }
}

impl Operation {
    fn resources(&self) -> &Resources {
        self.resources
            .as_ref()
            .expect("admitted Activity operation retains resources")
    }
}

impl Drop for Capacity {
    fn drop(&mut self) {
        if let Some(shared) = self.shared.upgrade() {
            let count = match self.kind {
                CapacityKind::Collection => &shared.collections,
                CapacityKind::Page => &shared.pages,
            };
            count.fetch_sub(1, Ordering::AcqRel);
        }
    }
}

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(65_536).expect("fixed Activity point-read limit")
}

#[cfg(all(test, feature = "test-faults"))]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/activity_service.rs"
    ));
}
