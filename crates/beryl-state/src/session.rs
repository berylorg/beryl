use beryl_home_store::{
    DomainHandle, DomainRegistrationError, DomainSchemaVersion, HomeStore, KeyspaceFamily,
    KeyspaceSchemaVersion, MutationContribution, ReadError, StorageDomain,
};
use beryl_model::{
    ClaimRevision, RootId, RuntimeId, SessionRevision, SyndicThreadId, WindowId, WindowPlacement,
};

use crate::RecordRevision;

mod bootstrap;
mod codec;
mod error;
mod mutation;
mod validate;

pub use error::{SessionMutationError, SessionReadError};
pub use mutation::{
    ActivateRestoringClaim, BeginSessionRestore, CreateClaimedWindow, InitializeThreadlessWindow,
    MarkOrderlyExit, RemoveSessionWindow, ReplaceWindowClaim, UpdateWindowPlacement,
};

/// Hard upper bound on main windows represented by one durable restore set.
pub const MAX_RESTORABLE_WINDOWS: usize = 256;

/// Exact V1 active-header payload size, excluding the store-owned version prefix.
pub const SESSION_HEADER_V1_BYTES: usize = 6_188;

/// Exact V1 window-record payload size, excluding the store-owned version prefix.
pub const SESSION_WINDOW_V1_BYTES: usize = 655;

pub(crate) const CLAIM_V1_BYTES: usize = 49;
pub(crate) const MAX_SESSION_CLAIMS: usize = MAX_RESTORABLE_WINDOWS * 2;

const SESSION_FAMILIES: &[KeyspaceFamily] = &[
    KeyspaceFamily::new("active-header", KeyspaceSchemaVersion::new(1)),
    KeyspaceFamily::new("windows", KeyspaceSchemaVersion::new(1)),
    KeyspaceFamily::new("claims-by-window", KeyspaceSchemaVersion::new(1)),
    KeyspaceFamily::new("claims-by-thread", KeyspaceSchemaVersion::new(1)),
];

pub(crate) struct SessionDomain;

impl StorageDomain for SessionDomain {
    const NAME: &'static str = "beryl-session";
    const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
    const KEYSPACES: &'static [KeyspaceFamily] = SESSION_FAMILIES;
    type ValidationError = error::SessionValidationError;

    fn validate(
        reader: &beryl_home_store::DomainReader<'_, Self>,
    ) -> Result<(), Self::ValidationError> {
        validate::validate(reader)
    }
}

/// Whether the last durable process transition was ordinary running state or dedicated Exit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionExitIntent {
    Running,
    OrderlyExit,
}

/// Complete remembered runtime/root target for a window or empty-restore fallback.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RememberedTarget {
    runtime_id: RuntimeId,
    root_id: RootId,
}

impl RememberedTarget {
    #[must_use]
    pub const fn new(runtime_id: RuntimeId, root_id: RootId) -> Self {
        Self {
            runtime_id,
            root_id,
        }
    }

    #[must_use]
    pub const fn runtime_id(self) -> RuntimeId {
        self.runtime_id
    }

    #[must_use]
    pub const fn root_id(self) -> RootId {
        self.root_id
    }
}

/// Exact selected-thread hook tying one window record to both claim copies.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct WindowClaimSelection {
    thread_id: SyndicThreadId,
    generation: SessionRevision,
    revision: ClaimRevision,
}

impl WindowClaimSelection {
    pub(crate) const fn new(
        thread_id: SyndicThreadId,
        generation: SessionRevision,
        revision: ClaimRevision,
    ) -> Self {
        Self {
            thread_id,
            generation,
            revision,
        }
    }

    #[must_use]
    pub const fn thread_id(self) -> SyndicThreadId {
        self.thread_id
    }

    #[must_use]
    pub const fn generation(self) -> SessionRevision {
        self.generation
    }

    #[must_use]
    pub const fn revision(self) -> ClaimRevision {
        self.revision
    }
}

/// Exact window identity and record revision published in the active restore set.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SessionWindowReference {
    window_id: WindowId,
    record_revision: RecordRevision,
}

impl SessionWindowReference {
    pub(crate) const fn new(window_id: WindowId, record_revision: RecordRevision) -> Self {
        Self {
            window_id,
            record_revision,
        }
    }

    #[must_use]
    pub const fn window_id(self) -> WindowId {
        self.window_id
    }

    #[must_use]
    pub const fn record_revision(self) -> RecordRevision {
        self.record_revision
    }
}

/// Fixed-capacity durable authority for the active main-window restore set.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionHeader {
    revision: SessionRevision,
    exit_intent: SessionExitIntent,
    fallback: Option<RememberedTarget>,
    windows: Vec<SessionWindowReference>,
}

impl SessionHeader {
    #[must_use]
    pub const fn revision(&self) -> SessionRevision {
        self.revision
    }

    #[must_use]
    pub const fn exit_intent(&self) -> SessionExitIntent {
        self.exit_intent
    }

    #[must_use]
    pub const fn fallback(&self) -> Option<RememberedTarget> {
        self.fallback
    }

    #[must_use]
    pub fn windows(&self) -> &[SessionWindowReference] {
        &self.windows
    }
}

/// Fixed-size durable record for one restorable main conversation window.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionWindowRecord {
    window_id: WindowId,
    remembered_target: Option<RememberedTarget>,
    selected_thread: Option<WindowClaimSelection>,
    placement: WindowPlacement,
    revision: RecordRevision,
}

impl SessionWindowRecord {
    #[must_use]
    pub const fn window_id(&self) -> WindowId {
        self.window_id
    }

    #[must_use]
    pub const fn remembered_target(&self) -> Option<RememberedTarget> {
        self.remembered_target
    }

    #[must_use]
    pub const fn selected_thread(&self) -> Option<WindowClaimSelection> {
        self.selected_thread
    }

    #[must_use]
    pub const fn placement(&self) -> &WindowPlacement {
        &self.placement
    }

    #[must_use]
    pub const fn revision(&self) -> RecordRevision {
        self.revision
    }
}

/// Durable lifecycle of one exclusive main-window thread claim.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ThreadClaimState {
    Active,
    Restoring,
}

/// Identical typed value retained in both reverse claim families.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ThreadClaimRecord {
    window_id: WindowId,
    thread_id: SyndicThreadId,
    generation: SessionRevision,
    state: ThreadClaimState,
    revision: ClaimRevision,
}

impl ThreadClaimRecord {
    pub(crate) const fn new(
        window_id: WindowId,
        thread_id: SyndicThreadId,
        generation: SessionRevision,
        state: ThreadClaimState,
        revision: ClaimRevision,
    ) -> Self {
        Self {
            window_id,
            thread_id,
            generation,
            state,
            revision,
        }
    }

    #[must_use]
    pub const fn window_id(self) -> WindowId {
        self.window_id
    }

    #[must_use]
    pub const fn thread_id(self) -> SyndicThreadId {
        self.thread_id
    }

    #[must_use]
    pub const fn generation(self) -> SessionRevision {
        self.generation
    }

    #[must_use]
    pub const fn state(self) -> ThreadClaimState {
        self.state
    }

    #[must_use]
    pub const fn revision(self) -> ClaimRevision {
        self.revision
    }

    pub(crate) const fn selection(self) -> WindowClaimSelection {
        WindowClaimSelection::new(self.thread_id, self.generation, self.revision)
    }
}

/// Exact bounded facts read before any main window becomes visible.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MinimalSessionBootstrap {
    header: SessionHeader,
    windows: Vec<SessionWindowRecord>,
}

impl MinimalSessionBootstrap {
    #[must_use]
    pub const fn header(&self) -> &SessionHeader {
        &self.header
    }

    #[must_use]
    pub fn windows(&self) -> &[SessionWindowRecord] {
        &self.windows
    }
}

/// Opaque typed access to durable session, window, and reverse-claim authority.
#[derive(Clone, Copy)]
pub struct SessionState {
    handle: DomainHandle<SessionDomain>,
}

impl SessionState {
    pub(crate) fn register(store: &mut HomeStore) -> Result<Self, DomainRegistrationError> {
        store
            .register_domain::<SessionDomain>()
            .map(|handle| Self { handle })
    }

    pub(crate) fn reacquire(
        store: &HomeStore,
    ) -> Result<Self, beryl_home_store::DomainHandleError> {
        store
            .domain_handle::<SessionDomain>()
            .map(|handle| Self { handle })
    }

    pub fn revision(&self, store: &HomeStore) -> Result<beryl_model::DomainRevision, ReadError> {
        store.domain_revision(self.handle)
    }

    /// Reads the active header and exactly its referenced window records, then
    /// rereads the header to reject a mixed concurrent publication.
    pub fn minimal_bootstrap(
        &self,
        store: &HomeStore,
    ) -> Result<Option<MinimalSessionBootstrap>, SessionReadError> {
        bootstrap::read(self.handle, store)
    }

    #[must_use]
    pub fn initialize_threadless(
        &self,
        expected_revision: beryl_model::DomainRevision,
        command: InitializeThreadlessWindow,
    ) -> MutationContribution {
        self.handle.contribution(expected_revision, command)
    }

    #[must_use]
    pub fn create_claimed_window(
        &self,
        expected_revision: beryl_model::DomainRevision,
        command: CreateClaimedWindow,
    ) -> MutationContribution {
        self.handle.contribution(expected_revision, command)
    }

    #[must_use]
    pub fn update_placement(
        &self,
        expected_revision: beryl_model::DomainRevision,
        command: UpdateWindowPlacement,
    ) -> MutationContribution {
        self.handle.contribution(expected_revision, command)
    }

    #[must_use]
    pub fn replace_claim(
        &self,
        expected_revision: beryl_model::DomainRevision,
        command: ReplaceWindowClaim,
    ) -> MutationContribution {
        self.handle.contribution(expected_revision, command)
    }

    #[must_use]
    pub fn begin_restore(
        &self,
        expected_revision: beryl_model::DomainRevision,
        command: BeginSessionRestore,
    ) -> MutationContribution {
        self.handle.contribution(expected_revision, command)
    }

    #[must_use]
    pub fn activate_restoring_claim(
        &self,
        expected_revision: beryl_model::DomainRevision,
        command: ActivateRestoringClaim,
    ) -> MutationContribution {
        self.handle.contribution(expected_revision, command)
    }

    #[must_use]
    pub fn remove_window(
        &self,
        expected_revision: beryl_model::DomainRevision,
        command: RemoveSessionWindow,
    ) -> MutationContribution {
        self.handle.contribution(expected_revision, command)
    }

    #[must_use]
    pub fn mark_orderly_exit(
        &self,
        expected_revision: beryl_model::DomainRevision,
        command: MarkOrderlyExit,
    ) -> MutationContribution {
        self.handle.contribution(expected_revision, command)
    }
}
