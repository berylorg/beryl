use std::sync::{MutexGuard, TryLockError};

use syndic_storage::{ActivityQueryCursor, ActivityQueryHeadRecord, ActivityQueryPage};

use super::*;
use crate::cas_projection::RuntimeActivityObservation;

pub(crate) struct ActivityCollectionRequest {
    scope: Arc<Scope>,
    completed: Mutex<bool>,
}

struct Scope {
    service: Weak<Shared>,
    runtime: RuntimeActivityObservation,
    thread: SyndicThreadId,
    _capacity: Capacity,
}

pub(crate) struct ActivityCollection {
    query: Arc<Query>,
}

struct Query {
    scope: Arc<Scope>,
    head: ActivityQueryHeadRecord,
    request: Mutex<()>,
}

pub(crate) struct ActivityContinuation {
    query: Weak<Query>,
    cursor: ActivityQueryCursor,
}

pub(crate) struct ActivityPage {
    query: Arc<Query>,
    page: ActivityQueryPage,
    _capacity: Capacity,
}

pub(super) fn prepare(
    shared: &Arc<Shared>,
    thread: SyndicThreadId,
    runtime: RuntimeId,
) -> Result<ActivityCollectionRequest, ActivityReadError> {
    let capacity = shared.reserve(CapacityKind::Collection)?;
    let operation = shared.begin()?;
    let runtime = operation.resources().runtime.observe(runtime)?;
    runtime.with_current(|| shared.publish(|| ()))??;
    Ok(ActivityCollectionRequest {
        scope: Arc::new(Scope {
            service: Arc::downgrade(shared),
            runtime,
            thread,
            _capacity: capacity,
        }),
        completed: Mutex::new(false),
    })
}

impl Scope {
    fn with_current<T>(&self, publish: impl FnOnce() -> T) -> Result<T, ActivityReadError> {
        let shared = self
            .service
            .upgrade()
            .ok_or(ActivityReadError::Unavailable)?;
        self.runtime.with_current(|| shared.publish(publish))?
    }
}

impl ActivityCollectionRequest {
    pub(crate) fn with_current<T>(
        &self,
        publish: impl FnOnce() -> T,
    ) -> Result<T, ActivityReadError> {
        self.scope.with_current(publish)
    }

    pub(crate) fn open(&self) -> Result<ActivityCollection, ActivityReadError> {
        let mut completed = try_request(&self.completed)?;
        if *completed {
            return Err(ActivityReadError::RequestCompleted);
        }
        let shared = self
            .scope
            .service
            .upgrade()
            .ok_or(ActivityReadError::Unavailable)?;
        let operation = shared.begin()?;
        self.scope.with_current(|| ())?;
        let Resources { home, storage, .. } = operation.resources();
        let execution = storage
            .thread_execution(home, self.scope.thread, point_limit())?
            .ok_or(ActivityReadError::ThreadUnavailable)?;
        if execution.execution().runtime_id() != self.scope.runtime.runtime_id() {
            return Err(ActivityReadError::Identity);
        }
        let head = storage
            .activity_query_head(home, self.scope.thread, point_limit())?
            .ok_or(ActivityReadError::Unenrolled)?;
        if head.work_period() != self.scope.runtime.work_period() || head.source().is_none() {
            return Err(ActivityReadError::Unenrolled);
        }
        if head.lifecycle() != syndic_storage::ProjectionLifecycle::Current {
            return Err(SyndicReadError::ActivityQueryIsStale.into());
        }
        self.scope.with_current(|| ())?;
        *completed = true;
        Ok(ActivityCollection {
            query: Arc::new(Query {
                scope: Arc::clone(&self.scope),
                head,
                request: Mutex::new(()),
            }),
        })
    }
}

impl ActivityCollection {
    pub(crate) fn with_current_head<T>(
        &self,
        publish: impl FnOnce(&ActivityQueryHeadRecord) -> T,
    ) -> Result<T, ActivityReadError> {
        self.query.scope.with_current(|| publish(&self.query.head))
    }

    pub(crate) fn read_page(
        &self,
        continuation: Option<&ActivityContinuation>,
    ) -> Result<ActivityPage, ActivityReadError> {
        let _request = try_request(&self.query.request)?;
        let cursor = match continuation {
            Some(next) if next.query.ptr_eq(&Arc::downgrade(&self.query)) => Some(next.cursor),
            Some(_) => return Err(ActivityReadError::InvalidContinuation),
            None => None,
        };
        let shared = self
            .query
            .scope
            .service
            .upgrade()
            .ok_or(ActivityReadError::Unavailable)?;
        let capacity = shared.reserve(CapacityKind::Page)?;
        let operation = shared.begin()?;
        self.query.scope.with_current(|| ())?;
        let page = operation.resources().storage.activity_query_page(
            &operation.resources().home,
            &self.query.head,
            cursor,
            shared.limits.page,
        )?;
        self.query.scope.with_current(|| ())?;
        Ok(ActivityPage {
            query: Arc::clone(&self.query),
            page,
            _capacity: capacity,
        })
    }
}

impl ActivityPage {
    pub(crate) fn with_current<T>(
        &self,
        publish: impl FnOnce(&ActivityQueryHeadRecord, &ActivityQueryPage) -> T,
    ) -> Result<T, ActivityReadError> {
        self.query
            .scope
            .with_current(|| publish(&self.query.head, &self.page))
    }

    pub(crate) fn continuation(&self) -> Result<Option<ActivityContinuation>, ActivityReadError> {
        self.query.scope.with_current(|| {
            self.page.next_cursor().map(|cursor| ActivityContinuation {
                query: Arc::downgrade(&self.query),
                cursor,
            })
        })
    }
}

fn try_request<T>(mutex: &Mutex<T>) -> Result<MutexGuard<'_, T>, ActivityReadError> {
    mutex.try_lock().map_err(|error| match error {
        TryLockError::WouldBlock => ActivityReadError::RequestPending,
        TryLockError::Poisoned(_) => ActivityReadError::Unavailable,
    })
}
