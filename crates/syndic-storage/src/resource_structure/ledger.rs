use beryl_home_store::{DomainReader, PointReadLimit, RECORD_VERSION_BYTES};

use super::construction::ConstructionError;
use crate::{
    codec::{ExactCodec, Family},
    domain::SyndicDomain,
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct Ledger {
    pub(super) records: usize,
    pub(super) read_bytes: usize,
    pub(super) retained: usize,
    pub(super) writes: usize,
    pub(super) write_bytes: usize,
    pub(super) source_chunk: Option<std::sync::Arc<crate::ContentChunkRecord>>,
}

impl Ledger {
    pub(super) fn reserve_read(
        &mut self,
        records: usize,
        encoded: usize,
        retained: usize,
    ) -> Result<(), ConstructionError> {
        let next = Self {
            records: self
                .records
                .checked_add(records)
                .ok_or(ConstructionError::Limit)?,
            read_bytes: self
                .read_bytes
                .checked_add(encoded)
                .ok_or(ConstructionError::Limit)?,
            retained: self
                .retained
                .checked_add(retained)
                .ok_or(ConstructionError::Limit)?,
            ..self.clone()
        };
        if next.records > 520 || next.read_bytes > 524_288 || next.retained > 524_288 {
            return Err(ConstructionError::Limit);
        }
        *self = next;
        Ok(())
    }

    pub(super) fn reserve_write(
        &mut self,
        key: usize,
        payload: usize,
        completion: bool,
    ) -> Result<(), ConstructionError> {
        let bytes = key.checked_add(payload).ok_or(ConstructionError::Limit)?;
        let writes = self.writes.checked_add(1).ok_or(ConstructionError::Limit)?;
        let write_bytes = self
            .write_bytes
            .checked_add(bytes)
            .ok_or(ConstructionError::Limit)?;
        if writes > if completion { 517 } else { 515 } || write_bytes > 262_144 {
            return Err(ConstructionError::Limit);
        }
        self.writes = writes;
        self.write_bytes = write_bytes;
        Ok(())
    }

    pub(super) fn reserve_point<F: Family>(
        &mut self,
        payload_limit: usize,
    ) -> Result<(), ConstructionError> {
        let encoded = payload_limit
            .checked_add(RECORD_VERSION_BYTES)
            .and_then(|bytes| bytes.checked_add(F::MAX_KEY_BYTES))
            .ok_or(ConstructionError::Limit)?;
        self.reserve_read(1, encoded, encoded)
    }

    pub(super) fn point<F: Family>(
        &mut self,
        reader: &DomainReader<'_, SyndicDomain>,
        key: &F::Key,
        payload_limit: usize,
    ) -> Result<Option<F::Value>, ConstructionError> {
        self.reserve_point::<F>(payload_limit)?;
        let limit = PointReadLimit::new(payload_limit + RECORD_VERSION_BYTES)
            .map_err(|_| ConstructionError::Limit)?;
        match reader.point::<ExactCodec<F>>(key, limit) {
            Err(beryl_home_store::ReadError::BoundExceeded { .. }) => Err(ConstructionError::Limit),
            result => Ok(result?),
        }
    }
}
