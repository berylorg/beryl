use beryl_home_store::{
    CursorDirection, CursorRange, CursorReadLimits, DomainReader, PointReadLimit, RecordCodec,
};
use beryl_model::{RootId, RuntimeId};

use super::{
    codec::{
        ExecutableIndexCodec, ExecutableKey, RootIdIndexCodec, RootPathIndexCodec, RootPathKey,
        RootRecordCodec, RuntimeHomeRootIndexCodec, RuntimeRecordCodec, RuntimeRootKey,
    },
    RuntimeRootDomain, RuntimeRootValidationError, ROOT_RECORD_LIMIT, RUNTIME_RECORD_LIMIT,
};

const VALIDATION_PAGE_ITEMS: usize = 64;
const VALIDATION_PAGE_BYTES: usize = 16 * 1024 * 1024;

pub(super) fn validate(
    reader: &DomainReader<'_, RuntimeRootDomain>,
) -> Result<(), RuntimeRootValidationError> {
    scan::<RuntimeRecordCodec>(reader, |key, runtime| {
        if *key != runtime.runtime_id {
            return invariant("runtime record key does not match its identity");
        }
        let indexed = reader.point::<ExecutableIndexCodec>(
            &ExecutableKey::new(runtime.canonical_executable.clone()),
            point_limit(32),
        )?;
        if indexed != Some(runtime.runtime_id) {
            return invariant("runtime executable index is missing or inconsistent");
        }
        let home_root = reader
            .point::<RuntimeHomeRootIndexCodec>(&runtime.runtime_id, point_limit(32))?
            .ok_or(RuntimeRootValidationError::Invariant(
                "runtime has no non-removable home-root index",
            ))?;
        let root_runtime = reader.point::<RootIdIndexCodec>(&home_root, point_limit(32))?;
        if root_runtime != Some(runtime.runtime_id) {
            return invariant("runtime home-root identity index is inconsistent");
        }
        let root = reader.point::<RootRecordCodec>(
            &RuntimeRootKey::new(runtime.runtime_id, home_root),
            point_limit(ROOT_RECORD_LIMIT),
        )?;
        if !root.is_some_and(|root| root.non_removable) {
            return invariant("runtime home-root index does not name a non-removable root");
        }
        Ok(())
    })?;

    scan::<RootRecordCodec>(reader, |key, root| {
        if key.runtime_id() != root.runtime_id || key.root_id() != root.root_id {
            return invariant("root record key does not match its identities");
        }
        let runtime = reader
            .point::<RuntimeRecordCodec>(&root.runtime_id, point_limit(RUNTIME_RECORD_LIMIT))?
            .ok_or(RuntimeRootValidationError::Invariant(
                "root references a missing runtime",
            ))?;
        if root.canonical_path.mode() != &runtime.mode {
            return invariant("root path mode differs from its runtime mode");
        }
        if reader.point::<RootIdIndexCodec>(&root.root_id, point_limit(32))?
            != Some(root.runtime_id)
        {
            return invariant("root identity index is missing or inconsistent");
        }
        if reader.point::<RootPathIndexCodec>(
            &RootPathKey::new(root.runtime_id, root.canonical_path.clone()),
            point_limit(32),
        )? != Some(root.root_id)
        {
            return invariant("root path index is missing or inconsistent");
        }
        let home_root =
            reader.point::<RuntimeHomeRootIndexCodec>(&root.runtime_id, point_limit(32))?;
        if root.non_removable != (home_root == Some(root.root_id)) {
            return invariant("root non-removable state disagrees with home-root index");
        }
        Ok(())
    })?;

    validate_indexes(reader)
}

fn validate_indexes(
    reader: &DomainReader<'_, RuntimeRootDomain>,
) -> Result<(), RuntimeRootValidationError> {
    scan::<ExecutableIndexCodec>(reader, |key, runtime_id| {
        let runtime = reader
            .point::<RuntimeRecordCodec>(runtime_id, point_limit(RUNTIME_RECORD_LIMIT))?
            .ok_or(RuntimeRootValidationError::Invariant(
                "executable index references a missing runtime",
            ))?;
        if *key != ExecutableKey::new(runtime.canonical_executable.clone()) {
            return invariant("executable index key disagrees with runtime record");
        }
        Ok(())
    })?;

    scan::<RootIdIndexCodec>(reader, |root_id, runtime_id| {
        let root = reader.point::<RootRecordCodec>(
            &RuntimeRootKey::new(*runtime_id, *root_id),
            point_limit(ROOT_RECORD_LIMIT),
        )?;
        if !root.is_some_and(|root| root.root_id == *root_id && root.runtime_id == *runtime_id) {
            return invariant("root identity index references a missing or different root");
        }
        Ok(())
    })?;

    scan::<RootPathIndexCodec>(reader, |key, root_id| {
        let runtime_id = reader
            .point::<RootIdIndexCodec>(root_id, point_limit(32))?
            .ok_or(RuntimeRootValidationError::Invariant(
                "root path index references a missing root identity",
            ))?;
        let root = reader
            .point::<RootRecordCodec>(
                &RuntimeRootKey::new(runtime_id, *root_id),
                point_limit(ROOT_RECORD_LIMIT),
            )?
            .ok_or(RuntimeRootValidationError::Invariant(
                "root path index references a missing root",
            ))?;
        if *key != RootPathKey::new(runtime_id, root.canonical_path.clone()) {
            return invariant("root path index key disagrees with root record");
        }
        Ok(())
    })?;

    scan::<RuntimeHomeRootIndexCodec>(reader, |runtime_id, root_id| {
        if reader
            .point::<RuntimeRecordCodec>(runtime_id, point_limit(RUNTIME_RECORD_LIMIT))?
            .is_none()
        {
            return invariant("home-root index references a missing runtime");
        }
        let root = reader.point::<RootRecordCodec>(
            &RuntimeRootKey::new(*runtime_id, *root_id),
            point_limit(ROOT_RECORD_LIMIT),
        )?;
        if !root.is_some_and(|root| root.non_removable) {
            return invariant("home-root index references a missing or removable root");
        }
        Ok(())
    })
}

trait ScanKey: Clone {
    fn lower() -> Self;
    fn upper() -> Self;
}

impl ScanKey for RuntimeId {
    fn lower() -> Self {
        Self::from_bytes([0; 16])
    }

    fn upper() -> Self {
        Self::from_bytes([u8::MAX; 16])
    }
}

impl ScanKey for RootId {
    fn lower() -> Self {
        Self::from_bytes([0; 16])
    }

    fn upper() -> Self {
        Self::from_bytes([u8::MAX; 16])
    }
}

impl ScanKey for RuntimeRootKey {
    fn lower() -> Self {
        Self::new(RuntimeId::from_bytes([0; 16]), RootId::from_bytes([0; 16]))
    }

    fn upper() -> Self {
        Self::new(
            RuntimeId::from_bytes([u8::MAX; 16]),
            RootId::from_bytes([u8::MAX; 16]),
        )
    }
}

impl ScanKey for ExecutableKey {
    fn lower() -> Self {
        Self::Lower
    }

    fn upper() -> Self {
        Self::Upper
    }
}

impl ScanKey for RootPathKey {
    fn lower() -> Self {
        Self::Lower
    }

    fn upper() -> Self {
        Self::Upper
    }
}

fn scan<R>(
    reader: &DomainReader<'_, RuntimeRootDomain>,
    mut visit: impl FnMut(&R::Key, &R::Value) -> Result<(), RuntimeRootValidationError>,
) -> Result<(), RuntimeRootValidationError>
where
    R: RecordCodec<RuntimeRootDomain>,
    R::Key: ScanKey,
{
    let mut after: Option<R::Key> = None;
    loop {
        let range = match &after {
            Some(after) => CursorRange::after(after.clone(), R::Key::upper()),
            None => CursorRange::closed(R::Key::lower(), R::Key::upper()),
        };
        let page = reader.cursor::<R>(
            &range,
            CursorDirection::Forward,
            CursorReadLimits::new(VALIDATION_PAGE_ITEMS, VALIDATION_PAGE_BYTES)
                .expect("validation limits are nonzero"),
        )?;
        for record in page.records() {
            visit(record.key(), record.value())?;
        }
        if !page.has_more() {
            return Ok(());
        }
        after = page.records().last().map(|record| record.key().clone());
        if after.is_none() {
            return invariant("bounded validation cursor reported more without a record");
        }
    }
}

fn point_limit(maximum_payload: usize) -> PointReadLimit {
    PointReadLimit::new(maximum_payload + 4).expect("schema point limit is nonzero")
}

fn invariant<T>(message: &'static str) -> Result<T, RuntimeRootValidationError> {
    Err(RuntimeRootValidationError::Invariant(message))
}
