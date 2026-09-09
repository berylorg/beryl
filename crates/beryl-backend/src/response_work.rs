use std::sync::{Arc, Mutex};

use thiserror::Error;

#[derive(Clone, Debug)]
pub struct ResponseWorkRevision {
    owner: Arc<()>,
    serial: u64,
}

impl PartialEq for ResponseWorkRevision {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.owner, &other.owner) && self.serial == other.serial
    }
}

impl Eq for ResponseWorkRevision {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResponseWorkSnapshot {
    revision: ResponseWorkRevision,
    session_generation: Option<u64>,
    response_written: bool,
    retained_capabilities: u8,
}

impl ResponseWorkSnapshot {
    pub const fn revision(&self) -> &ResponseWorkRevision {
        &self.revision
    }

    pub const fn session_generation(&self) -> Option<u64> {
        self.session_generation
    }

    pub const fn response_written(&self) -> bool {
        self.response_written
    }

    pub const fn retained_capabilities(&self) -> u8 {
        self.retained_capabilities
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ResponseWorkError {
    #[error("the response work revision belongs to another request")]
    ForeignRevision,
    #[error("the response work revision is stale")]
    StaleRevision,
    #[error("the response work revision is unavailable")]
    RevisionUnavailable,
    #[error("the response work observation lock is poisoned")]
    Poisoned,
}

#[derive(Debug)]
struct WorkState {
    revision: Option<u64>,
    session_generation: Option<u64>,
    response_written: bool,
    retained_capabilities: u8,
}

impl WorkState {
    fn changed(&mut self) {
        self.revision = self.revision.and_then(|revision| revision.checked_add(1));
    }
}

#[derive(Clone, Debug)]
pub struct ResponseWorkObserver {
    owner: Arc<()>,
    state: Arc<Mutex<WorkState>>,
}

impl ResponseWorkObserver {
    pub fn snapshot(&self) -> Result<ResponseWorkSnapshot, ResponseWorkError> {
        let state = self.state.lock().map_err(|_| ResponseWorkError::Poisoned)?;
        Ok(ResponseWorkSnapshot {
            revision: ResponseWorkRevision {
                owner: Arc::clone(&self.owner),
                serial: state
                    .revision
                    .ok_or(ResponseWorkError::RevisionUnavailable)?,
            },
            session_generation: state.session_generation,
            response_written: state.response_written,
            retained_capabilities: state.retained_capabilities,
        })
    }

    pub fn validate_revision(
        &self,
        revision: &ResponseWorkRevision,
    ) -> Result<(), ResponseWorkError> {
        if !Arc::ptr_eq(&self.owner, &revision.owner) {
            return Err(ResponseWorkError::ForeignRevision);
        }
        let state = self.state.lock().map_err(|_| ResponseWorkError::Poisoned)?;
        if state
            .revision
            .ok_or(ResponseWorkError::RevisionUnavailable)?
            != revision.serial
        {
            return Err(ResponseWorkError::StaleRevision);
        }
        Ok(())
    }
}

#[derive(Debug)]
pub(crate) struct ResponseWorkTracker {
    observer: ResponseWorkObserver,
}

impl ResponseWorkTracker {
    pub(crate) fn new(retained_capabilities: u8, session_generation: Option<u64>) -> Self {
        Self {
            observer: ResponseWorkObserver {
                owner: Arc::new(()),
                state: Arc::new(Mutex::new(WorkState {
                    revision: Some(0),
                    session_generation,
                    response_written: false,
                    retained_capabilities,
                })),
            },
        }
    }

    pub(crate) fn observe(&self) -> ResponseWorkObserver {
        self.observer.clone()
    }

    pub(crate) fn bind<T, E>(
        &self,
        generation: u64,
        bind: impl FnOnce() -> Result<T, E>,
    ) -> Result<T, E> {
        let mut state = self
            .observer
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let result = bind()?;
        state.session_generation = Some(generation);
        state.changed();
        Ok(result)
    }

    pub(crate) fn record_response(&self, written: bool, store: impl FnOnce()) {
        let mut state = self
            .observer
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        store();
        if state.response_written != written {
            state.response_written = written;
            state.changed();
        }
    }

    pub(crate) fn release_capability(&self) {
        let mut state = self
            .observer
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if let Some(remaining) = state.retained_capabilities.checked_sub(1) {
            state.retained_capabilities = remaining;
            state.changed();
        } else {
            state.revision = None;
        }
    }
}
