use beryl_home_store::{
    CurrentDomainCommand, DomainCallbackError, DomainCallbackSource, DomainMutation, DomainReader,
    MutationBuildError, MutationBuilder, ReadError,
};

use crate::{
    ProviderObservationBuildRecord, ProviderObservationDigest, SyndicStorage, codec::*,
    domain::SyndicDomain,
};

/// One narrow codec-valid persisted provider-observation fault.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderObservationCorruption {
    MissingChunk { ordinal: u64 },
    BuildDigest,
}

/// Why a provider-observation fault could not be installed.
#[derive(Debug, thiserror::Error)]
pub enum ProviderObservationCorruptionError {
    #[error(transparent)]
    Read(#[from] ReadError),
    #[error(transparent)]
    Build(#[from] MutationBuildError),
    #[error("provider-observation fault target changed before writer admission")]
    TargetChanged,
    #[error("provider-observation fault names a chunk outside the build frontier")]
    InvalidOrdinal,
    #[error("provider-observation fault target chunk is missing")]
    ChunkMissing,
    #[error("provider-observation fault replacement build is invalid")]
    InvalidReplacement,
}

impl DomainCallbackError for ProviderObservationCorruptionError {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        match self {
            Self::Read(source) => Ok(DomainCallbackSource::Read(source)),
            source => Err(source),
        }
    }
}

impl SyndicStorage {
    /// Builds a current-domain command for one exact persisted observation corruption.
    pub fn current_corrupt_provider_observation(
        &self,
        build: &ProviderObservationBuildRecord,
        corruption: ProviderObservationCorruption,
    ) -> Result<CurrentDomainCommand, ProviderObservationCorruptionError> {
        Ok(self
            .handle
            .current_command(ProviderObservationFault::new(build, corruption)?))
    }
}

struct ProviderObservationFault {
    expected: ProviderObservationBuildRecord,
    replacement: ProviderObservationFaultReplacement,
}

enum ProviderObservationFaultReplacement {
    MissingChunk(ProviderObservationChunkKey),
    Build(ProviderObservationBuildRecord),
}

impl ProviderObservationFault {
    fn new(
        build: &ProviderObservationBuildRecord,
        corruption: ProviderObservationCorruption,
    ) -> Result<Self, ProviderObservationCorruptionError> {
        let replacement = match corruption {
            ProviderObservationCorruption::MissingChunk { ordinal } => {
                if ordinal == 0 || ordinal > build.chunk_count() {
                    return Err(ProviderObservationCorruptionError::InvalidOrdinal);
                }
                ProviderObservationFaultReplacement::MissingChunk(ProviderObservationChunkKey::new(
                    build.identity(),
                    ordinal,
                ))
            }
            ProviderObservationCorruption::BuildDigest => {
                let mut digest = *build.digest().as_bytes();
                digest[0] ^= 1;
                let replacement = ProviderObservationBuildRecord::from_stored_parts(
                    build.identity(),
                    build.begin(),
                    build.revision(),
                    build.chunk_count(),
                    build.canonical_bytes(),
                    ProviderObservationDigest::from_bytes(digest),
                    build.validator().clone(),
                    build.lifecycle(),
                )
                .map_err(|_| ProviderObservationCorruptionError::InvalidReplacement)?;
                ProviderObservationFaultReplacement::Build(replacement)
            }
        };
        Ok(Self {
            expected: build.clone(),
            replacement,
        })
    }
}

impl DomainMutation<SyndicDomain> for ProviderObservationFault {
    type Error = ProviderObservationCorruptionError;

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        let current = reader.point::<ProviderObservationBuildsCodec>(
            &self.expected.identity(),
            crate::codec::family_point_limit::<ProviderObservationBuildsFamily>(),
        )?;
        if current.as_ref() != Some(&self.expected) {
            return Err(ProviderObservationCorruptionError::TargetChanged);
        }
        if let ProviderObservationFaultReplacement::MissingChunk(key) = &self.replacement {
            let chunk = reader.point::<ProviderObservationChunksCodec>(
                key,
                crate::codec::family_point_limit::<ProviderObservationChunksFamily>(),
            )?;
            if chunk.is_none() {
                return Err(ProviderObservationCorruptionError::ChunkMissing);
            }
        }
        Ok(())
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut beryl_home_store::ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        match &self.replacement {
            ProviderObservationFaultReplacement::MissingChunk(_) => {
                reservation.reserve_records::<ProviderObservationChunksCodec>(1)?;
            }
            ProviderObservationFaultReplacement::Build(_) => {
                reservation.reserve_records::<ProviderObservationBuildsCodec>(1)?;
            }
        }
        Ok(())
    }

    fn contribute(
        &self,
        _reader: &DomainReader<'_, SyndicDomain>,
        builder: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        match &self.replacement {
            ProviderObservationFaultReplacement::MissingChunk(key) => {
                builder.delete::<ProviderObservationChunksCodec>(key)?;
            }
            ProviderObservationFaultReplacement::Build(build) => {
                builder.put::<ProviderObservationBuildsCodec>(&build.identity(), build)?;
            }
        }
        Ok(())
    }
}
