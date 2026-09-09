use super::*;

#[derive(Clone, Debug)]
pub struct ScheduledSessionWorkRevision {
    pub(super) owner: Arc<()>,
    pub(super) home_id: BerylHomeId,
    pub(super) home_generation: HomeGeneration,
    pub(super) service_generation: ProjectionServiceGeneration,
    pub(super) revision: u64,
}

impl PartialEq for ScheduledSessionWorkRevision {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.owner, &other.owner)
            && self.home_id == other.home_id
            && self.home_generation == other.home_generation
            && self.service_generation == other.service_generation
            && self.revision == other.revision
    }
}
impl Eq for ScheduledSessionWorkRevision {}

impl ScheduledSessionWorkRevision {
    pub const fn home_id(&self) -> BerylHomeId {
        self.home_id
    }
    pub const fn home_generation(&self) -> HomeGeneration {
        self.home_generation
    }
    pub const fn service_generation(&self) -> ProjectionServiceGeneration {
        self.service_generation
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScheduledSessionWorkCursor {
    pub(super) revision: ScheduledSessionWorkRevision,
    pub(super) after: SyndicThreadId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScheduledSessionWorkPageLimits {
    pub(super) max_records: usize,
    pub(super) max_bytes: usize,
}

impl ScheduledSessionWorkPageLimits {
    pub fn new(max_records: usize, max_bytes: usize) -> Result<Self, ScheduledSessionWorkError> {
        if max_records == 0 || max_bytes == 0 {
            return Err(ScheduledSessionWorkError::InvalidLimits);
        }
        Ok(Self {
            max_records: max_records.min(256),
            max_bytes: max_bytes.min(65_536),
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScheduledSessionWorkState {
    Available,
    CheckedOut,
    Retiring { checked_out: bool },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScheduledSessionFact {
    pub(super) registration_serial: u64,
    pub(super) binding: ExecutionBinding,
    pub(super) state: ScheduledSessionWorkState,
}

impl ScheduledSessionFact {
    pub const fn registration_serial(&self) -> u64 {
        self.registration_serial
    }
    pub const fn execution_binding(&self) -> &ExecutionBinding {
        &self.binding
    }
    pub const fn state(&self) -> ScheduledSessionWorkState {
        self.state
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScheduledSessionPreparationFact {
    pub(super) binding: ExecutionBinding,
    pub(super) complete: bool,
}

impl ScheduledSessionPreparationFact {
    pub const fn execution_binding(&self) -> &ExecutionBinding {
        &self.binding
    }
    pub const fn is_complete(&self) -> bool {
        self.complete
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScheduledSessionWorkRecord {
    pub(super) thread_id: SyndicThreadId,
    pub(super) session: Option<ScheduledSessionFact>,
    pub(super) preparation: Option<ScheduledSessionPreparationFact>,
}

impl ScheduledSessionWorkRecord {
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }
    pub const fn session(&self) -> Option<&ScheduledSessionFact> {
        self.session.as_ref()
    }
    pub const fn preparation(&self) -> Option<&ScheduledSessionPreparationFact> {
        self.preparation.as_ref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScheduledSessionWorkPage {
    pub(super) revision: ScheduledSessionWorkRevision,
    pub(super) records: Vec<ScheduledSessionWorkRecord>,
    pub(super) bytes: usize,
    pub(super) next_cursor: Option<ScheduledSessionWorkCursor>,
}

impl ScheduledSessionWorkPage {
    pub const fn revision(&self) -> &ScheduledSessionWorkRevision {
        &self.revision
    }
    pub fn records(&self) -> &[ScheduledSessionWorkRecord] {
        &self.records
    }
    pub const fn bytes(&self) -> usize {
        self.bytes
    }
    pub const fn next_cursor(&self) -> Option<&ScheduledSessionWorkCursor> {
        self.next_cursor.as_ref()
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ScheduledSessionWorkError {
    #[error("the session owner is not attached to a service")]
    Unattached,
    #[error("the session work owner is closed")]
    Closed,
    #[error("the session work revision is stale")]
    StaleRevision,
    #[error("the session work cursor belongs to another owner or revision")]
    ForeignCursor,
    #[error("session work page limits must be nonzero")]
    InvalidLimits,
    #[error("one session work record exceeds the page byte limit")]
    ByteLimit,
    #[error("the session work revision is exhausted")]
    RevisionExhausted,
    #[error("the session work owner lock is poisoned")]
    Poisoned,
}
