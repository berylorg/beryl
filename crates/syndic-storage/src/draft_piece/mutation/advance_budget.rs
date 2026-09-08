use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use super::*;
use crate::codec::{ExactCodec, Family};

const MAX_STORED_STRUCTURES: u64 = 256;
const MAX_POINT_ATTEMPTS: u64 = 512;
const MAX_ENCODED_BYTES: u64 = 4_194_304;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DraftPieceBuildWorkV1 {
    point_attempts: u64,
    stored_structure_records: u64,
    encoded_bytes: u64,
    peak_encoded_bytes: u64,
}

impl DraftPieceBuildWorkV1 {
    pub const fn point_attempts(self) -> u64 {
        self.point_attempts
    }
    pub const fn stored_structure_records(self) -> u64 {
        self.stored_structure_records
    }
    pub const fn encoded_bytes(self) -> u64 {
        self.encoded_bytes
    }
    pub const fn peak_encoded_bytes(self) -> u64 {
        self.peak_encoded_bytes
    }
}

#[derive(Default)]
struct Acquired {
    work: DraftPieceBuildWorkV1,
    records: BTreeMap<(&'static str, Vec<u8>), Option<Vec<u8>>>,
}

#[derive(Clone, Default)]
pub(in super::super) struct BuildBudget(Arc<Mutex<Acquired>>);

fn limit() -> DraftPiecePrepareErrorV1 {
    DraftPiecePrepareErrorV1::Rejected(DraftPieceRejectedReasonV1::TreeLimit)
}

fn is_structure<F: Family>() -> bool {
    matches!(
        F::NAME,
        "draft-piece-nodes"
            | "draft-piece-leaves"
            | "draft-marker-identity-index"
            | "draft-marker-order-commitments"
    )
}

impl BuildBudget {
    pub(in super::super) fn work(&self) -> DraftPieceBuildWorkV1 {
        self.0
            .lock()
            .expect("build budget lock is not reentrant")
            .work
    }

    fn reserve(
        &self,
        attempts: u64,
        structures: u64,
        bytes: u64,
    ) -> Result<(), DraftPiecePrepareErrorV1> {
        let mut state = self
            .0
            .lock()
            .map_err(|_| DraftPiecePrepareErrorV1::InvalidRoot)?;
        let work = &mut state.work;
        let next_attempts = work
            .point_attempts
            .checked_add(attempts)
            .ok_or_else(limit)?;
        let next_structures = work
            .stored_structure_records
            .checked_add(structures)
            .ok_or_else(limit)?;
        let next_bytes = work.encoded_bytes.checked_add(bytes).ok_or_else(limit)?;
        if next_attempts > MAX_POINT_ATTEMPTS
            || next_structures > MAX_STORED_STRUCTURES
            || next_bytes > MAX_ENCODED_BYTES
        {
            return Err(limit());
        }
        work.point_attempts = next_attempts;
        work.stored_structure_records = next_structures;
        work.encoded_bytes = next_bytes;
        work.peak_encoded_bytes = work.peak_encoded_bytes.max(next_bytes);
        Ok(())
    }

    fn acquire<F: Family>(
        &self,
        key: &F::Key,
        cached: bool,
        read: impl FnOnce() -> Result<Option<F::Value>, DraftPiecePrepareErrorV1>,
    ) -> Result<Option<F::Value>, DraftPiecePrepareErrorV1> {
        let encoded_key = F::encode_key(key).map_err(|_| DraftPiecePrepareErrorV1::InvalidRoot)?;
        let identity = (F::NAME, encoded_key);
        if cached {
            let state = self
                .0
                .lock()
                .map_err(|_| DraftPiecePrepareErrorV1::InvalidRoot)?;
            if let Some(value) = state.records.get(&identity) {
                return value
                    .as_ref()
                    .map(|bytes| {
                        F::decode_value(bytes).map_err(|_| DraftPiecePrepareErrorV1::InvalidRoot)
                    })
                    .transpose();
            }
        }
        self.reserve(
            1,
            u64::from(is_structure::<F>()),
            (identity.1.len() + F::MAX_VALUE_BYTES) as u64,
        )?;
        let value = read()?;
        let encoded = value
            .as_ref()
            .map(F::encode_value)
            .transpose()
            .map_err(|_| DraftPiecePrepareErrorV1::InvalidRoot)?;
        let value_bytes = encoded.as_ref().map_or(0, Vec::len);
        if value_bytes > F::MAX_VALUE_BYTES {
            return Err(DraftPiecePrepareErrorV1::InvalidRoot);
        }
        let mut state = self
            .0
            .lock()
            .map_err(|_| DraftPiecePrepareErrorV1::InvalidRoot)?;
        state.work.encoded_bytes -= (F::MAX_VALUE_BYTES - value_bytes) as u64;
        if value.is_none() && is_structure::<F>() {
            state.work.stored_structure_records -= 1;
        }
        if cached {
            state.records.insert(identity, encoded);
        }
        Ok(value)
    }

    pub(in super::super) fn emission<F: Family>(
        &self,
        key: &F::Key,
        value: &F::Value,
    ) -> Result<(), DraftPiecePrepareErrorV1> {
        let key = F::encode_key(key).map_err(|_| DraftPiecePrepareErrorV1::InvalidRoot)?;
        let value = F::encode_value(value).map_err(|_| DraftPiecePrepareErrorV1::InvalidRoot)?;
        self.reserve(
            0,
            u64::from(is_structure::<F>()),
            (key.len() + value.len()) as u64,
        )
    }

    pub(super) fn recheck<F: Family>(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
        key: &F::Key,
    ) -> Result<Option<F::Value>, SyndicMutationError> {
        self.acquire::<F>(key, false, || {
            reader
                .point::<ExactCodec<F>>(
                    key,
                    beryl_home_store::PointReadLimit::new(F::MAX_VALUE_BYTES)
                        .map_err(|_| DraftPiecePrepareErrorV1::InvalidRoot)?,
                )
                .map_err(|_| DraftPiecePrepareErrorV1::InvalidRoot)
        })
        .map_err(|_| SyndicMutationError::IdentityCollision)
    }
}

#[derive(Clone)]
pub(in super::super) struct BuildAcquisition<'a> {
    pub(in super::super) storage: &'a SyndicStorage,
    pub(in super::super) store: &'a HomeStore,
    pub(in super::super) budget: BuildBudget,
}

impl<'a> BuildAcquisition<'a> {
    pub(in super::super) fn new(storage: &'a SyndicStorage, store: &'a HomeStore) -> Self {
        Self {
            storage,
            store,
            budget: BuildBudget::default(),
        }
    }

    pub(in super::super) fn point<F: Family>(
        &self,
        key: F::Key,
    ) -> Result<Option<F::Value>, DraftPiecePrepareErrorV1> {
        self.budget.acquire::<F>(&key, true, || {
            self.storage
                .point::<F>(
                    self.store,
                    key.clone(),
                    crate::SyndicPointReadLimit::new(F::MAX_VALUE_BYTES)
                        .map_err(|_| DraftPiecePrepareErrorV1::InvalidRoot)?,
                )
                .map_err(DraftPiecePrepareErrorV1::from)
        })
    }
}
