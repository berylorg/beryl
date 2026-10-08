use std::sync::Arc;

use beryl_model::{DomainRevision, HomeRevision};
use thiserror::Error;

use super::{
    ReadError, ReadFamilies, ReadStage, fjall_storage, read_domain_metadata, read_home_revision,
};
use crate::{
    CommandCancellation, CursorDirection, CursorPage, CursorRange, CursorReadLimits, DomainHandle,
    HomeGenerationIdentity, HomeStore, PointReadLimit, RecordCodec, StorageDomain,
    candidate_access::StoreOperationAccess, domain::StoreInstanceId, store::StoreGeneration,
};

mod registry;

pub(crate) use registry::FrozenReadRegistry;
use registry::FrozenReadRequest;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FrozenHomeRead {
    owner: StoreInstanceId,
    store: StoreInstanceId,
    generation: HomeGenerationIdentity,
    id: u64,
    revision: HomeRevision,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum FrozenReadAccessError {
    #[error("frozen Home read was cancelled before admission")]
    Cancelled,
    #[error("frozen Home read retention is full ({maximum} slots)")]
    Saturated { maximum: usize },
    #[error("frozen Home read identity space is exhausted")]
    IdentityExhausted,
    #[error("frozen Home read belongs to another store or generation")]
    Foreign,
    #[error("frozen Home read has been released")]
    Released,
    #[error("frozen Home read has reached its admitted request limit")]
    RequestsSaturated,
}

impl FrozenHomeRead {
    pub const fn home_revision(&self) -> HomeRevision {
        self.revision
    }

    pub const fn generation_identity(&self) -> HomeGenerationIdentity {
        self.generation
    }
}

impl HomeStore {
    pub fn capture_frozen_read(
        &self,
        cancellation: &CommandCancellation,
    ) -> Result<FrozenHomeRead, ReadError> {
        if cancellation.is_cancelled() {
            return Err(FrozenReadAccessError::Cancelled.into());
        }
        let identity = self.generation_identity()?;
        let (read, reservation) =
            self.execute_read(StoreOperationAccess::Ordinary, |generation| {
                if cancellation.is_cancelled() {
                    return Err(FrozenReadAccessError::Cancelled.into());
                }
                let reservation = generation.frozen_reads.reserve()?;
                let snapshot = Arc::new(
                    generation
                        .database
                        .snapshot()
                        .map_err(|error| fjall_storage(ReadStage::HomeRevision, error))?,
                );
                let revision = read_home_revision(&snapshot, generation.header_keyspace())?;
                reservation.install(snapshot)?;
                let read = FrozenHomeRead {
                    owner: self.writer_id,
                    store: generation.instance_id,
                    generation: identity,
                    id: reservation.id(),
                    revision,
                };
                Ok((read, reservation))
            })?;
        reservation.commit();
        Ok(read)
    }

    pub fn retain_frozen_read(
        &self,
        read: &FrozenHomeRead,
        cancellation: &CommandCancellation,
    ) -> Result<FrozenHomeRead, ReadError> {
        if cancellation.is_cancelled() {
            return Err(FrozenReadAccessError::Cancelled.into());
        }
        let mut request_guard = None;
        let result = self.execute_read(StoreOperationAccess::Ordinary, |generation| {
            let request = request_guard.insert(self.frozen_request(generation, read)?);
            if cancellation.is_cancelled() {
                return Err(FrozenReadAccessError::Cancelled.into());
            }
            let reservation = generation.frozen_reads.reserve()?;
            reservation.install(request.shared_snapshot())?;
            let retained = FrozenHomeRead {
                id: reservation.id(),
                ..read.clone()
            };
            Ok((retained, reservation))
        });
        drop(request_guard);
        let (retained, reservation) = result?;
        reservation.commit();
        Ok(retained)
    }

    pub fn release_frozen_read(&self, read: &FrozenHomeRead) -> Result<(), ReadError> {
        if read.owner != self.writer_id {
            return Err(FrozenReadAccessError::Foreign.into());
        }
        let generation = self
            .generation
            .read()
            .map_err(|_| ReadError::GenerationPoisoned)?;
        let Some(generation) = generation.as_ref() else {
            return Err(FrozenReadAccessError::Released.into());
        };
        if read.store != generation.instance_id {
            return Err(FrozenReadAccessError::Foreign.into());
        }
        generation.frozen_reads.release(read.id)
    }

    pub fn frozen_domain_revision<D: StorageDomain>(
        &self,
        read: &FrozenHomeRead,
        handle: &DomainHandle<D>,
    ) -> Result<DomainRevision, ReadError> {
        self.execute_frozen_read(read, |generation, request| {
            let domain = generation
                .resolve_domain(handle)
                .ok_or(ReadError::ForeignDomain { domain: D::NAME })?;
            read_domain_metadata(
                request.snapshot(),
                generation.domains_keyspace(),
                domain.name,
            )
            .map(|metadata| metadata.revision)
        })
    }

    pub fn read_frozen_point<D: StorageDomain, R: RecordCodec<D>>(
        &self,
        read: &FrozenHomeRead,
        handle: &DomainHandle<D>,
        key: &R::Key,
        limit: PointReadLimit,
    ) -> Result<Option<R::Value>, ReadError> {
        self.execute_frozen_read(read, |generation, request| {
            let domain = generation
                .resolve_domain(handle)
                .ok_or(ReadError::ForeignDomain { domain: D::NAME })?;
            super::execute::read_point::<D, R>(
                request.snapshot(),
                ReadFamilies::Registered(domain),
                key,
                limit,
            )
        })
    }

    pub fn read_frozen_cursor<D: StorageDomain, R: RecordCodec<D>>(
        &self,
        read: &FrozenHomeRead,
        handle: &DomainHandle<D>,
        range: &CursorRange<R::Key>,
        direction: CursorDirection,
        limits: CursorReadLimits,
    ) -> Result<CursorPage<R::Key, R::Value>, ReadError> {
        self.execute_frozen_read(read, |generation, request| {
            let domain = generation
                .resolve_domain(handle)
                .ok_or(ReadError::ForeignDomain { domain: D::NAME })?;
            super::execute::read_cursor::<D, R>(
                request.snapshot(),
                ReadFamilies::Registered(domain),
                range,
                direction,
                limits,
            )
        })
    }

    pub fn retained_frozen_read_count(&self) -> Result<usize, ReadError> {
        let generation = self
            .generation
            .read()
            .map_err(|_| ReadError::GenerationPoisoned)?;
        generation
            .as_ref()
            .map_or(Ok(0), |generation| generation.frozen_reads.count())
    }

    fn execute_frozen_read<T>(
        &self,
        read: &FrozenHomeRead,
        operation: impl FnOnce(&StoreGeneration, &FrozenReadRequest) -> Result<T, ReadError>,
    ) -> Result<T, ReadError> {
        let mut request_guard = None;
        let result = self.execute_read(StoreOperationAccess::Ordinary, |generation| {
            let request = request_guard.insert(self.frozen_request(generation, read)?);
            operation(generation, request)
        });
        drop(request_guard);
        result
    }

    fn frozen_request(
        &self,
        generation: &StoreGeneration,
        read: &FrozenHomeRead,
    ) -> Result<FrozenReadRequest, ReadError> {
        if read.owner != self.writer_id || read.store != generation.instance_id {
            return Err(FrozenReadAccessError::Foreign.into());
        }
        generation.frozen_reads.request(read.id)
    }
}

#[cfg(feature = "test-faults")]
pub fn set_frozen_read_limits_for_test(
    store: &HomeStore,
    maximum: usize,
    next_id: u64,
) -> Result<(), ReadError> {
    store.execute_read(StoreOperationAccess::Ordinary, |generation| {
        generation.frozen_reads.set_limits(maximum, next_id);
        Ok(())
    })
}
