use beryl_home_store::{
    DomainMutation, DomainReader, MutationBuilder, PointReadLimit, ReconciliationReservation,
};
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
    type Prepared = (RuntimeRecord, RootRecord);

    fn prepare(
        self,
        reader: &DomainReader<'_, RuntimeRootDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let Self { runtime, home_root } = self;
        ensure_runtime_missing(reader, &runtime)?;
        ensure_root_missing(reader, runtime.runtime_id, &home_root)?;
        if reader
            .point::<RuntimeHomeRootIndexCodec>(&runtime.runtime_id, point_limit(32))?
            .is_some()
        {
            return Err(RuntimeRootMutationError::RuntimeIdExists {
                runtime_id: runtime.runtime_id,
            });
        }
        let root = RootRecord::initial(runtime.runtime_id, &home_root, true);
        Ok((RuntimeRecord::initial(&runtime), root))
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, RuntimeRootDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<RuntimeRecordCodec>(1)?;
        reservation.reserve_records::<ExecutableIndexCodec>(1)?;
        reservation.reserve_records::<RootRecordCodec>(1)?;
        reservation.reserve_records::<RootIdIndexCodec>(1)?;
        reservation.reserve_records::<RootPathIndexCodec>(1)?;
        reservation.reserve_records::<RuntimeHomeRootIndexCodec>(1)?;
        Ok(())
    }

    fn contribute(
        (runtime, root): Self::Prepared,
        mutations: &mut MutationBuilder<'_, RuntimeRootDomain>,
    ) -> Result<(), Self::Error> {
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
    type Prepared = RootRecord;

    fn prepare(
        self,
        reader: &DomainReader<'_, RuntimeRootDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let Self { runtime_id, root } = self;
        let runtime = required_runtime(reader, runtime_id)?;
        if root.canonical_path.mode() != &runtime.mode {
            return Err(RuntimeRootMutationError::RuntimeModeMismatch);
        }
        ensure_root_missing(reader, runtime_id, &root)?;
        Ok(RootRecord::initial(runtime_id, &root, false))
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, RuntimeRootDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<RootRecordCodec>(1)?;
        reservation.reserve_records::<RootIdIndexCodec>(1)?;
        reservation.reserve_records::<RootPathIndexCodec>(1)?;
        Ok(())
    }

    fn contribute(
        root: Self::Prepared,
        mutations: &mut MutationBuilder<'_, RuntimeRootDomain>,
    ) -> Result<(), Self::Error> {
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
    type Prepared = RuntimeRecord;

    fn prepare(
        self,
        reader: &DomainReader<'_, RuntimeRootDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let Self {
            runtime_id,
            expected_record_revision,
            availability,
        } = self;
        let mut runtime = required_runtime(reader, runtime_id)?;
        ensure_record_revision("runtime", expected_record_revision, runtime.revision)?;
        runtime.availability = availability;
        runtime.revision = runtime.revision.checked_next()?;
        Ok(runtime)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, RuntimeRootDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<RuntimeRecordCodec>(1)?;
        Ok(())
    }

    fn contribute(
        runtime: Self::Prepared,
        mutations: &mut MutationBuilder<'_, RuntimeRootDomain>,
    ) -> Result<(), Self::Error> {
        mutations.put::<RuntimeRecordCodec>(&runtime.runtime_id, &runtime)?;
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
    type Prepared = RootRecord;

    fn prepare(
        self,
        reader: &DomainReader<'_, RuntimeRootDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let Self {
            root_id,
            expected_record_revision,
            availability,
        } = self;
        let mut root = required_root(reader, root_id)?;
        ensure_record_revision("root", expected_record_revision, root.revision)?;
        root.availability = availability;
        root.revision = root.revision.checked_next()?;
        Ok(root)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, RuntimeRootDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<RootRecordCodec>(1)?;
        Ok(())
    }

    fn contribute(
        root: Self::Prepared,
        mutations: &mut MutationBuilder<'_, RuntimeRootDomain>,
    ) -> Result<(), Self::Error> {
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
    type Prepared = RootRecord;

    fn prepare(
        self,
        reader: &DomainReader<'_, RuntimeRootDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let Self {
            root_id,
            expected_record_revision,
            last_activity_at,
        } = self;
        let mut root = required_root(reader, root_id)?;
        ensure_record_revision("root", expected_record_revision, root.revision)?;
        if root
            .last_activity_at
            .is_some_and(|current| last_activity_at <= current)
        {
            return Err(RuntimeRootMutationError::RootActivityNotLater);
        }
        root.last_activity_at = Some(last_activity_at);
        root.revision = root.revision.checked_next()?;
        Ok(root)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, RuntimeRootDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<RootRecordCodec>(1)?;
        Ok(())
    }

    fn contribute(
        root: Self::Prepared,
        mutations: &mut MutationBuilder<'_, RuntimeRootDomain>,
    ) -> Result<(), Self::Error> {
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
