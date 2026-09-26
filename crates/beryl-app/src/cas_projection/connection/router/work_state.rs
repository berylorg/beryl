use std::ops::{Deref, DerefMut};
use std::sync::{
    Arc, Condvar, LockResult, Mutex, MutexGuard, PoisonError, TryLockError, TryLockResult,
};

use super::RouterState;
use crate::cas_projection::connection_work::{ConnectionWorkBoundary, ConnectionWorkMutation};

#[derive(Debug)]
pub(super) struct RouterWorkState {
    state: Mutex<RouterState>,
    pub(super) boundary: Arc<ConnectionWorkBoundary>,
}

#[derive(Debug)]
pub(super) struct RouterWorkStateGuard<'a> {
    state: Option<MutexGuard<'a, RouterState>>,
    boundary: &'a ConnectionWorkBoundary,
    change: Option<ConnectionWorkMutation>,
}

impl RouterWorkState {
    pub(super) fn new(state: RouterState, boundary: ConnectionWorkBoundary) -> Self {
        Self {
            state: Mutex::new(state),
            boundary: Arc::new(boundary),
        }
    }

    fn wrap<'a>(&'a self, state: MutexGuard<'a, RouterState>) -> RouterWorkStateGuard<'a> {
        RouterWorkStateGuard {
            state: Some(state),
            boundary: &self.boundary,
            change: None,
        }
    }

    pub(super) fn lock(&self) -> LockResult<RouterWorkStateGuard<'_>> {
        self.state
            .lock()
            .map(|state| self.wrap(state))
            .map_err(|poison| PoisonError::new(self.wrap(poison.into_inner())))
    }

    pub(super) fn try_lock(&self) -> TryLockResult<RouterWorkStateGuard<'_>> {
        self.state
            .try_lock()
            .map(|state| self.wrap(state))
            .map_err(|error| match error {
                TryLockError::WouldBlock => TryLockError::WouldBlock,
                TryLockError::Poisoned(poison) => {
                    TryLockError::Poisoned(PoisonError::new(self.wrap(poison.into_inner())))
                }
            })
    }
}

impl<'a> RouterWorkStateGuard<'a> {
    pub(super) fn wait_timeout(
        mut self,
        changed: &Condvar,
        timeout: std::time::Duration,
    ) -> LockResult<(Self, std::sync::WaitTimeoutResult)> {
        drop(self.change.take());
        let state = self
            .state
            .take()
            .expect("router wait owns its source guard");
        match changed.wait_timeout(state, timeout) {
            Ok((state, result)) => {
                self.state = Some(state);
                Ok((self, result))
            }
            Err(poison) => {
                let (state, result) = poison.into_inner();
                self.state = Some(state);
                Err(PoisonError::new((self, result)))
            }
        }
    }

    pub(super) fn wait(mut self, changed: &Condvar) -> LockResult<Self> {
        drop(self.change.take());
        let state = self
            .state
            .take()
            .expect("router wait owns its source guard");
        match changed.wait(state) {
            Ok(state) => {
                self.state = Some(state);
                Ok(self)
            }
            Err(poison) => {
                self.state = Some(poison.into_inner());
                Err(PoisonError::new(self))
            }
        }
    }
}

impl Deref for RouterWorkStateGuard<'_> {
    type Target = RouterState;

    fn deref(&self) -> &Self::Target {
        self.state
            .as_ref()
            .expect("router guard owns its source mutex")
    }
}

impl DerefMut for RouterWorkStateGuard<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.change
            .get_or_insert_with(|| self.boundary.begin_change());
        self.state
            .as_mut()
            .expect("router guard owns its source mutex")
    }
}

impl Drop for RouterWorkStateGuard<'_> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            self.boundary.invalidate();
        }
    }
}
