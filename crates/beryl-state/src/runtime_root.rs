use beryl_home_store::{
    CursorDirection, CursorRange, CursorReadLimits, DomainHandle, DomainRegistrationError,
    DomainSchemaVersion, HomeStore, KeyspaceFamily, KeyspaceSchemaVersion, MutationContribution,
    PointReadLimit, ReadError, StorageDomain,
};
use beryl_model::{AdmittedHostPath, RootId, RuntimeId, RuntimeMode, RuntimeNativePath};

use crate::{AvailabilitySnapshot, RecordRevision, StatePage, UnixMillis};

mod codec;
mod error;
mod mutation;
mod validate;

use codec::{
    ExecutableIndexCodec, RootIdIndexCodec, RootPathIndexCodec, RootRecordCodec,
    RuntimeRecordCodec, RuntimeRootKey,
};
pub use error::RuntimeRootMutationError;
use error::RuntimeRootValidationError;
pub use mutation::{
    AddConfiguredRoot, CreateRuntimeWithHomeRoot, RootActivityUpdate, SetRootAvailability,
    SetRuntimeAvailability,
};

const RUNTIME_RECORD_LIMIT: usize = 132 * 1024;
const ROOT_RECORD_LIMIT: usize = 132 * 1024;

const RUNTIME_ROOT_FAMILIES: &[KeyspaceFamily] = &[
    KeyspaceFamily::new("runtimes", KeyspaceSchemaVersion::new(1)),
    KeyspaceFamily::new("runtime-executable-index", KeyspaceSchemaVersion::new(1)),
    KeyspaceFamily::new("roots", KeyspaceSchemaVersion::new(1)),
    KeyspaceFamily::new("root-id-index", KeyspaceSchemaVersion::new(1)),
    KeyspaceFamily::new("root-path-index", KeyspaceSchemaVersion::new(1)),
    KeyspaceFamily::new("runtime-home-root-index", KeyspaceSchemaVersion::new(1)),
];

pub(crate) struct RuntimeRootDomain;

impl StorageDomain for RuntimeRootDomain {
    const NAME: &'static str = "beryl-runtime-root";
    const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
    const KEYSPACES: &'static [KeyspaceFamily] = RUNTIME_ROOT_FAMILIES;
    type ValidationError = RuntimeRootValidationError;

    fn validate(
        reader: &beryl_home_store::DomainReader<'_, Self>,
    ) -> Result<(), Self::ValidationError> {
        validate::validate(reader)
    }
}

/// Already-admitted facts needed to persist one configured executable runtime.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeRegistration {
    runtime_id: RuntimeId,
    canonical_executable: AdmittedHostPath,
    mode: RuntimeMode,
    runtime_native_executable: RuntimeNativePath,
    environment_label: Box<str>,
    created_at: UnixMillis,
    availability: AvailabilitySnapshot,
}

impl RuntimeRegistration {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        runtime_id: RuntimeId,
        canonical_executable: AdmittedHostPath,
        mode: RuntimeMode,
        runtime_native_executable: RuntimeNativePath,
        created_at: UnixMillis,
        availability: AvailabilitySnapshot,
    ) -> Result<Self, RuntimeRootMutationError> {
        if runtime_native_executable.mode() != &mode {
            return Err(RuntimeRootMutationError::RuntimeModeMismatch);
        }
        let environment_label: Box<str> = match &mode {
            RuntimeMode::Host => "Host".into(),
            RuntimeMode::Wsl(distribution) => distribution.as_str().into(),
        };
        Ok(Self {
            runtime_id,
            canonical_executable,
            mode,
            runtime_native_executable,
            environment_label,
            created_at,
            availability,
        })
    }

    #[must_use]
    pub const fn runtime_id(&self) -> RuntimeId {
        self.runtime_id
    }
}

/// Already-admitted facts needed to persist one configured root.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RootRegistration {
    root_id: RootId,
    canonical_path: RuntimeNativePath,
    display_path: AdmittedHostPath,
    created_at: UnixMillis,
    availability: AvailabilitySnapshot,
}

impl RootRegistration {
    #[must_use]
    pub const fn new(
        root_id: RootId,
        canonical_path: RuntimeNativePath,
        display_path: AdmittedHostPath,
        created_at: UnixMillis,
        availability: AvailabilitySnapshot,
    ) -> Self {
        Self {
            root_id,
            canonical_path,
            display_path,
            created_at,
            availability,
        }
    }

    #[must_use]
    pub const fn root_id(&self) -> RootId {
        self.root_id
    }
}

/// Durable configured runtime record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeRecord {
    runtime_id: RuntimeId,
    canonical_executable: AdmittedHostPath,
    mode: RuntimeMode,
    runtime_native_executable: RuntimeNativePath,
    environment_label: Box<str>,
    created_at: UnixMillis,
    availability: AvailabilitySnapshot,
    revision: RecordRevision,
}

impl RuntimeRecord {
    fn initial(registration: &RuntimeRegistration) -> Self {
        Self {
            runtime_id: registration.runtime_id,
            canonical_executable: registration.canonical_executable.clone(),
            mode: registration.mode.clone(),
            runtime_native_executable: registration.runtime_native_executable.clone(),
            environment_label: registration.environment_label.clone(),
            created_at: registration.created_at,
            availability: registration.availability,
            revision: RecordRevision::INITIAL,
        }
    }

    #[must_use]
    pub const fn runtime_id(&self) -> RuntimeId {
        self.runtime_id
    }

    #[must_use]
    pub const fn canonical_executable(&self) -> &AdmittedHostPath {
        &self.canonical_executable
    }

    #[must_use]
    pub const fn mode(&self) -> &RuntimeMode {
        &self.mode
    }

    #[must_use]
    pub const fn runtime_native_executable(&self) -> &RuntimeNativePath {
        &self.runtime_native_executable
    }

    #[must_use]
    pub fn environment_label(&self) -> &str {
        &self.environment_label
    }

    #[must_use]
    pub const fn created_at(&self) -> UnixMillis {
        self.created_at
    }

    #[must_use]
    pub const fn availability(&self) -> AvailabilitySnapshot {
        self.availability
    }

    #[must_use]
    pub const fn revision(&self) -> RecordRevision {
        self.revision
    }
}

/// Durable configured root record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RootRecord {
    root_id: RootId,
    runtime_id: RuntimeId,
    canonical_path: RuntimeNativePath,
    display_path: AdmittedHostPath,
    non_removable: bool,
    created_at: UnixMillis,
    availability: AvailabilitySnapshot,
    last_activity_at: Option<UnixMillis>,
    revision: RecordRevision,
}

impl RootRecord {
    fn initial(
        runtime_id: RuntimeId,
        registration: &RootRegistration,
        non_removable: bool,
    ) -> Self {
        Self {
            root_id: registration.root_id,
            runtime_id,
            canonical_path: registration.canonical_path.clone(),
            display_path: registration.display_path.clone(),
            non_removable,
            created_at: registration.created_at,
            availability: registration.availability,
            last_activity_at: None,
            revision: RecordRevision::INITIAL,
        }
    }

    #[must_use]
    pub const fn root_id(&self) -> RootId {
        self.root_id
    }

    #[must_use]
    pub const fn runtime_id(&self) -> RuntimeId {
        self.runtime_id
    }

    #[must_use]
    pub const fn canonical_path(&self) -> &RuntimeNativePath {
        &self.canonical_path
    }

    #[must_use]
    pub const fn display_path(&self) -> &AdmittedHostPath {
        &self.display_path
    }

    #[must_use]
    pub const fn non_removable(&self) -> bool {
        self.non_removable
    }

    #[must_use]
    pub const fn created_at(&self) -> UnixMillis {
        self.created_at
    }

    #[must_use]
    pub const fn availability(&self) -> AvailabilitySnapshot {
        self.availability
    }

    #[must_use]
    pub const fn last_activity_at(&self) -> Option<UnixMillis> {
        self.last_activity_at
    }

    #[must_use]
    pub const fn revision(&self) -> RecordRevision {
        self.revision
    }
}

/// Opaque typed access to the runtime/root registry domain.
#[derive(Clone, Copy)]
pub struct RuntimeRootState {
    handle: DomainHandle<RuntimeRootDomain>,
}

impl RuntimeRootState {
    pub(crate) fn register(store: &mut HomeStore) -> Result<Self, DomainRegistrationError> {
        store
            .register_domain::<RuntimeRootDomain>()
            .map(|handle| Self { handle })
    }

    pub(crate) fn reacquire(
        store: &HomeStore,
    ) -> Result<Self, beryl_home_store::DomainHandleError> {
        store
            .domain_handle::<RuntimeRootDomain>()
            .map(|handle| Self { handle })
    }

    pub fn revision(&self, store: &HomeStore) -> Result<beryl_model::DomainRevision, ReadError> {
        store.domain_revision(self.handle)
    }

    pub fn runtime(
        &self,
        store: &HomeStore,
        runtime_id: RuntimeId,
    ) -> Result<Option<RuntimeRecord>, ReadError> {
        store.read_point::<RuntimeRootDomain, RuntimeRecordCodec>(
            self.handle,
            &runtime_id,
            point_limit(RUNTIME_RECORD_LIMIT),
        )
    }

    pub fn root(
        &self,
        store: &HomeStore,
        root_id: RootId,
    ) -> Result<Option<RootRecord>, ReadError> {
        let Some(runtime_id) = store.read_point::<RuntimeRootDomain, RootIdIndexCodec>(
            self.handle,
            &root_id,
            point_limit(32),
        )?
        else {
            return Ok(None);
        };
        store.read_point::<RuntimeRootDomain, RootRecordCodec>(
            self.handle,
            &RuntimeRootKey::new(runtime_id, root_id),
            point_limit(ROOT_RECORD_LIMIT),
        )
    }

    pub fn runtime_by_executable(
        &self,
        store: &HomeStore,
        canonical_executable: &AdmittedHostPath,
    ) -> Result<Option<RuntimeRecord>, ReadError> {
        let Some(runtime_id) = store.read_point::<RuntimeRootDomain, ExecutableIndexCodec>(
            self.handle,
            &codec::ExecutableKey::new(canonical_executable.clone()),
            point_limit(32),
        )?
        else {
            return Ok(None);
        };
        self.runtime(store, runtime_id)
    }

    pub fn root_by_path(
        &self,
        store: &HomeStore,
        runtime_id: RuntimeId,
        canonical_path: &RuntimeNativePath,
    ) -> Result<Option<RootRecord>, ReadError> {
        let Some(root_id) = store.read_point::<RuntimeRootDomain, RootPathIndexCodec>(
            self.handle,
            &codec::RootPathKey::new(runtime_id, canonical_path.clone()),
            point_limit(32),
        )?
        else {
            return Ok(None);
        };
        self.root(store, root_id)
    }

    pub fn list_runtimes(
        &self,
        store: &HomeStore,
        after: Option<RuntimeId>,
        limits: CursorReadLimits,
    ) -> Result<StatePage<RuntimeRecord>, ReadError> {
        let start = after.unwrap_or_else(|| RuntimeId::from_bytes([0; 16]));
        let end = RuntimeId::from_bytes([u8::MAX; 16]);
        let range = if after.is_some() {
            CursorRange::after(start, end)
        } else {
            CursorRange::closed(start, end)
        };
        let page = store.read_cursor::<RuntimeRootDomain, RuntimeRecordCodec>(
            self.handle,
            &range,
            CursorDirection::Forward,
            limits,
        )?;
        let stored_bytes = page.stored_bytes();
        let has_more = page.has_more();
        Ok(StatePage {
            records: page
                .into_records()
                .into_iter()
                .map(|record| record.into_parts().1)
                .collect(),
            stored_bytes,
            has_more,
        })
    }

    pub fn list_roots(
        &self,
        store: &HomeStore,
        runtime_id: RuntimeId,
        after: Option<RootId>,
        limits: CursorReadLimits,
    ) -> Result<StatePage<RootRecord>, ReadError> {
        let start = RuntimeRootKey::new(
            runtime_id,
            after.unwrap_or_else(|| RootId::from_bytes([0; 16])),
        );
        let end = RuntimeRootKey::new(runtime_id, RootId::from_bytes([u8::MAX; 16]));
        let range = if after.is_some() {
            CursorRange::after(start, end)
        } else {
            CursorRange::closed(start, end)
        };
        let page = store.read_cursor::<RuntimeRootDomain, RootRecordCodec>(
            self.handle,
            &range,
            CursorDirection::Forward,
            limits,
        )?;
        let stored_bytes = page.stored_bytes();
        let has_more = page.has_more();
        Ok(StatePage {
            records: page
                .into_records()
                .into_iter()
                .map(|record| record.into_parts().1)
                .collect(),
            stored_bytes,
            has_more,
        })
    }

    #[must_use]
    pub fn create_runtime_with_home_root(
        &self,
        expected_revision: beryl_model::DomainRevision,
        command: CreateRuntimeWithHomeRoot,
    ) -> MutationContribution {
        self.handle.contribution(expected_revision, command)
    }

    #[must_use]
    pub fn add_root(
        &self,
        expected_revision: beryl_model::DomainRevision,
        command: AddConfiguredRoot,
    ) -> MutationContribution {
        self.handle.contribution(expected_revision, command)
    }

    #[must_use]
    pub fn set_runtime_availability(
        &self,
        expected_revision: beryl_model::DomainRevision,
        command: SetRuntimeAvailability,
    ) -> MutationContribution {
        self.handle.contribution(expected_revision, command)
    }

    #[must_use]
    pub fn set_root_availability(
        &self,
        expected_revision: beryl_model::DomainRevision,
        command: SetRootAvailability,
    ) -> MutationContribution {
        self.handle.contribution(expected_revision, command)
    }

    #[must_use]
    pub fn update_root_activity(
        &self,
        expected_revision: beryl_model::DomainRevision,
        command: RootActivityUpdate,
    ) -> MutationContribution {
        self.handle.contribution(expected_revision, command)
    }
}

fn point_limit(maximum_payload: usize) -> PointReadLimit {
    PointReadLimit::new(maximum_payload + 4).expect("schema point limit is nonzero")
}
