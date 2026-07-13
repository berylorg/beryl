use beryl_home_store::{DomainMutation, DomainReader, MutationBuilder, PointReadLimit};
use beryl_model::{RootId, RuntimeId};

use crate::{AvailabilitySnapshot, RecordRevision, UnixMillis};

use super::{
    ROOT_RECORD_LIMIT, RUNTIME_RECORD_LIMIT, RootRecord, RootRegistration, RuntimeRecord,
    RuntimeRegistration, RuntimeRootDomain, RuntimeRootMutationError,
    codec::{
        ExecutableIndexCodec, ExecutableKey, RootIdIndexCodec, RootPathIndexCodec, RootPathKey,
        RootRecordCodec, RuntimeHomeRootIndexCodec, RuntimeRecordCodec, RuntimeRootKey,
    },
};

/// Atomic creation of one runtime and its sole non-removable home root.
pub struct CreateRuntimeWithHomeRoot {
    pub(super) runtime: RuntimeRegistration,
    pub(super) home_root: RootRegistration,
}

impl CreateRuntimeWithHomeRoot {
    pub fn new(
        runtime: RuntimeRegistration,
        home_root: RootRegistration,
    ) -> Result<Self, RuntimeRootMutationError> {
        if home_root.canonical_path.mode() != &runtime.mode {
            return Err(RuntimeRootMutationError::RuntimeModeMismatch);
        }
        Ok(Self { runtime, home_root })
    }
}

impl DomainMutation<RuntimeRootDomain> for CreateRuntimeWithHomeRoot {
    type Error = RuntimeRootMutationError;

    fn validate(&self, reader: &DomainReader<'_, RuntimeRootDomain>) -> Result<(), Self::Error> {
        ensure_runtime_missing(reader, &self.runtime)?;
        ensure_root_missing(reader, self.runtime.runtime_id, &self.home_root)?;
        if reader
            .point::<RuntimeHomeRootIndexCodec>(&self.runtime.runtime_id, point_limit(32))?
            .is_some()
        {
            return Err(RuntimeRootMutationError::RuntimeIdExists {
                runtime_id: self.runtime.runtime_id,
            });
        }
        Ok(())
    }

    fn contribute(
        &self,
        _reader: &DomainReader<'_, RuntimeRootDomain>,
        mutations: &mut MutationBuilder<'_, RuntimeRootDomain>,
    ) -> Result<(), Self::Error> {
        let runtime = RuntimeRecord::initial(&self.runtime);
        let root = RootRecord::initial(self.runtime.runtime_id, &self.home_root, true);
        mutations.put::<RuntimeRecordCodec>(&runtime.runtime_id, &runtime)?;
        mutations.put::<ExecutableIndexCodec>(
            &ExecutableKey::new(runtime.canonical_executable.clone()),
            &runtime.runtime_id,
        )?;
        mutations
            .put::<RootRecordCodec>(&RuntimeRootKey::new(root.runtime_id, root.root_id), &root)?;
        mutations.put::<RootIdIndexCodec>(&root.root_id, &root.runtime_id)?;
        mutations.put::<RootPathIndexCodec>(
            &RootPathKey::new(root.runtime_id, root.canonical_path.clone()),
            &root.root_id,
        )?;
        mutations.put::<RuntimeHomeRootIndexCodec>(&runtime.runtime_id, &root.root_id)?;
        Ok(())
    }
}

/// Add one removable configured root to an existing runtime.
pub struct AddConfiguredRoot {
    runtime_id: RuntimeId,
    root: RootRegistration,
}

impl AddConfiguredRoot {
    #[must_use]
    pub const fn new(runtime_id: RuntimeId, root: RootRegistration) -> Self {
        Self { runtime_id, root }
    }
}

impl DomainMutation<RuntimeRootDomain> for AddConfiguredRoot {
    type Error = RuntimeRootMutationError;

    fn validate(&self, reader: &DomainReader<'_, RuntimeRootDomain>) -> Result<(), Self::Error> {
        let runtime = required_runtime(reader, self.runtime_id)?;
        if self.root.canonical_path.mode() != &runtime.mode {
            return Err(RuntimeRootMutationError::RuntimeModeMismatch);
        }
        ensure_root_missing(reader, self.runtime_id, &self.root)
    }

    fn contribute(
        &self,
        _reader: &DomainReader<'_, RuntimeRootDomain>,
        mutations: &mut MutationBuilder<'_, RuntimeRootDomain>,
    ) -> Result<(), Self::Error> {
        let root = RootRecord::initial(self.runtime_id, &self.root, false);
        mutations
            .put::<RootRecordCodec>(&RuntimeRootKey::new(root.runtime_id, root.root_id), &root)?;
        mutations.put::<RootIdIndexCodec>(&root.root_id, &root.runtime_id)?;
        mutations.put::<RootPathIndexCodec>(
            &RootPathKey::new(root.runtime_id, root.canonical_path.clone()),
            &root.root_id,
        )?;
        Ok(())
    }
}

/// Replace only one runtime's observed availability summary.
pub struct SetRuntimeAvailability {
    runtime_id: RuntimeId,
    expected_record_revision: RecordRevision,
    availability: AvailabilitySnapshot,
}

impl SetRuntimeAvailability {
    #[must_use]
    pub const fn new(
        runtime_id: RuntimeId,
        expected_record_revision: RecordRevision,
        availability: AvailabilitySnapshot,
    ) -> Self {
        Self {
            runtime_id,
            expected_record_revision,
            availability,
        }
    }
}

impl DomainMutation<RuntimeRootDomain> for SetRuntimeAvailability {
    type Error = RuntimeRootMutationError;

    fn validate(&self, reader: &DomainReader<'_, RuntimeRootDomain>) -> Result<(), Self::Error> {
        let runtime = required_runtime(reader, self.runtime_id)?;
        ensure_record_revision("runtime", self.expected_record_revision, runtime.revision)
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, RuntimeRootDomain>,
        mutations: &mut MutationBuilder<'_, RuntimeRootDomain>,
    ) -> Result<(), Self::Error> {
        let mut runtime = required_runtime(reader, self.runtime_id)?;
        runtime.availability = self.availability;
        runtime.revision = runtime.revision.checked_next()?;
        mutations.put::<RuntimeRecordCodec>(&self.runtime_id, &runtime)?;
        Ok(())
    }
}

/// Replace only one configured root's observed availability summary.
pub struct SetRootAvailability {
    root_id: RootId,
    expected_record_revision: RecordRevision,
    availability: AvailabilitySnapshot,
}

impl SetRootAvailability {
    #[must_use]
    pub const fn new(
        root_id: RootId,
        expected_record_revision: RecordRevision,
        availability: AvailabilitySnapshot,
    ) -> Self {
        Self {
            root_id,
            expected_record_revision,
            availability,
        }
    }
}

impl DomainMutation<RuntimeRootDomain> for SetRootAvailability {
    type Error = RuntimeRootMutationError;

    fn validate(&self, reader: &DomainReader<'_, RuntimeRootDomain>) -> Result<(), Self::Error> {
        let root = required_root(reader, self.root_id)?;
        ensure_record_revision("root", self.expected_record_revision, root.revision)
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, RuntimeRootDomain>,
        mutations: &mut MutationBuilder<'_, RuntimeRootDomain>,
    ) -> Result<(), Self::Error> {
        let mut root = required_root(reader, self.root_id)?;
        root.availability = self.availability;
        root.revision = root.revision.checked_next()?;
        mutations
            .put::<RootRecordCodec>(&RuntimeRootKey::new(root.runtime_id, root.root_id), &root)?;
        Ok(())
    }
}

/// Strictly advance one root's presentation-only last-activity time.
pub struct RootActivityUpdate {
    root_id: RootId,
    expected_record_revision: RecordRevision,
    last_activity_at: UnixMillis,
}

impl RootActivityUpdate {
    #[must_use]
    pub const fn new(
        root_id: RootId,
        expected_record_revision: RecordRevision,
        last_activity_at: UnixMillis,
    ) -> Self {
        Self {
            root_id,
            expected_record_revision,
            last_activity_at,
        }
    }
}

impl DomainMutation<RuntimeRootDomain> for RootActivityUpdate {
    type Error = RuntimeRootMutationError;

    fn validate(&self, reader: &DomainReader<'_, RuntimeRootDomain>) -> Result<(), Self::Error> {
        let root = required_root(reader, self.root_id)?;
        ensure_record_revision("root", self.expected_record_revision, root.revision)?;
        if root
            .last_activity_at
            .is_some_and(|current| self.last_activity_at <= current)
        {
            return Err(RuntimeRootMutationError::RootActivityNotLater);
        }
        Ok(())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, RuntimeRootDomain>,
        mutations: &mut MutationBuilder<'_, RuntimeRootDomain>,
    ) -> Result<(), Self::Error> {
        let mut root = required_root(reader, self.root_id)?;
        root.last_activity_at = Some(self.last_activity_at);
        root.revision = root.revision.checked_next()?;
        mutations
            .put::<RootRecordCodec>(&RuntimeRootKey::new(root.runtime_id, root.root_id), &root)?;
        Ok(())
    }
}

fn ensure_runtime_missing(
    reader: &DomainReader<'_, RuntimeRootDomain>,
    runtime: &RuntimeRegistration,
) -> Result<(), RuntimeRootMutationError> {
    if reader
        .point::<RuntimeRecordCodec>(&runtime.runtime_id, point_limit(RUNTIME_RECORD_LIMIT))?
        .is_some()
    {
        return Err(RuntimeRootMutationError::RuntimeIdExists {
            runtime_id: runtime.runtime_id,
        });
    }
    if let Some(runtime_id) = reader.point::<ExecutableIndexCodec>(
        &ExecutableKey::new(runtime.canonical_executable.clone()),
        point_limit(32),
    )? {
        return Err(RuntimeRootMutationError::ExecutableExists { runtime_id });
    }
    Ok(())
}

fn ensure_root_missing(
    reader: &DomainReader<'_, RuntimeRootDomain>,
    runtime_id: RuntimeId,
    root: &RootRegistration,
) -> Result<(), RuntimeRootMutationError> {
    if reader
        .point::<RootIdIndexCodec>(&root.root_id, point_limit(32))?
        .is_some()
    {
        return Err(RuntimeRootMutationError::RootIdExists {
            root_id: root.root_id,
        });
    }
    if let Some(root_id) = reader.point::<RootPathIndexCodec>(
        &RootPathKey::new(runtime_id, root.canonical_path.clone()),
        point_limit(32),
    )? {
        return Err(RuntimeRootMutationError::RootPathExists { root_id });
    }
    Ok(())
}

fn required_runtime(
    reader: &DomainReader<'_, RuntimeRootDomain>,
    runtime_id: RuntimeId,
) -> Result<RuntimeRecord, RuntimeRootMutationError> {
    reader
        .point::<RuntimeRecordCodec>(&runtime_id, point_limit(RUNTIME_RECORD_LIMIT))?
        .ok_or(RuntimeRootMutationError::RuntimeMissing { runtime_id })
}

fn required_root(
    reader: &DomainReader<'_, RuntimeRootDomain>,
    root_id: RootId,
) -> Result<RootRecord, RuntimeRootMutationError> {
    let runtime_id = reader
        .point::<RootIdIndexCodec>(&root_id, point_limit(32))?
        .ok_or(RuntimeRootMutationError::RootMissing { root_id })?;
    reader
        .point::<RootRecordCodec>(
            &RuntimeRootKey::new(runtime_id, root_id),
            point_limit(ROOT_RECORD_LIMIT),
        )?
        .ok_or(RuntimeRootMutationError::RootMissing { root_id })
}

fn ensure_record_revision(
    kind: &'static str,
    expected: RecordRevision,
    current: RecordRevision,
) -> Result<(), RuntimeRootMutationError> {
    if expected == current {
        Ok(())
    } else {
        Err(RuntimeRootMutationError::RecordRevisionConflict {
            kind,
            expected,
            current,
        })
    }
}

fn point_limit(maximum_payload: usize) -> PointReadLimit {
    PointReadLimit::new(maximum_payload + 4).expect("schema point limit is nonzero")
}
