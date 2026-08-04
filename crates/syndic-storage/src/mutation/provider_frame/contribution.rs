use beryl_home_store::{
    CurrentDomainCommand, DomainCallbackError, DomainCallbackSource, DomainMutation, DomainReader,
    HomeStore, MutationBuildError, MutationBuilder, MutationContribution, ReadError,
};
use beryl_model::{DomainRevision, SyndicItemId};

use crate::{SyndicStorage, codec::*, domain::SyndicDomain};

use super::{PreparedProviderFrame, ProviderFrameStageBatch, ProviderFrameStageBatchError};
use crate::PreparedProviderObservationFrame;

/// Why a provider-frame begin-build or stage-batch contribution was rejected.
#[derive(Debug, thiserror::Error)]
pub enum ProviderFrameMutationError {
    #[error(transparent)]
    Read(#[from] ReadError),
    #[error(transparent)]
    Build(#[from] MutationBuildError),
    #[error(transparent)]
    Batch(#[from] ProviderFrameStageBatchError),
    #[error("provider-item build identity is already occupied")]
    BuildIdentityCollision,
    #[error("first provider-frame content manifest identity is already occupied")]
    ManifestIdentityCollision,
    #[error("provider-frame content manifest is missing")]
    ManifestMissing,
    #[error("provider-frame content manifest does not equal the build's published anchor")]
    ManifestMismatch,
    #[error("provider-item build is missing")]
    BuildMissing,
    #[error("provider-item build does not equal the stage batch's expected build")]
    BuildConflict,
    #[error("provider content chunk identity is already occupied")]
    ChunkIdentityCollision,
    #[error("provider content byte-span identity is already occupied")]
    ByteSpanIdentityCollision,
    #[error("provider narrative-span identity is already occupied")]
    NarrativeSpanIdentityCollision,
}

impl DomainCallbackError for ProviderFrameMutationError {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        match self {
            Self::Read(source) => Ok(DomainCallbackSource::Read(source)),
            source => Err(source),
        }
    }
}

impl SyndicStorage {
    /// Reads one exact durable provider-item build for restart reconciliation.
    pub fn provider_item_build(
        &self,
        store: &HomeStore,
        item_id: SyndicItemId,
        limit: crate::SyndicPointReadLimit,
    ) -> Result<Option<crate::ProviderItemBuildRecord>, crate::SyndicReadError> {
        self.point::<ProviderItemBuildsFamily>(store, item_id, limit)
    }

    /// Begins one unpublished provider frame against writer-admitted current domain state.
    #[must_use]
    pub fn current_begin_provider_frame_build(
        &self,
        prepared: &PreparedProviderFrame,
    ) -> CurrentDomainCommand {
        self.handle
            .current_command(BeginProviderFrameBuildMutation {
                build: prepared.initial_build().clone(),
            })
    }

    /// Begins one streamed-observation provider frame against writer-admitted current state.
    #[must_use]
    pub fn current_begin_provider_observation_frame_build(
        &self,
        prepared: &PreparedProviderObservationFrame,
    ) -> CurrentDomainCommand {
        self.handle
            .current_command(BeginProviderFrameBuildMutation {
                build: prepared.initial_build().clone(),
            })
    }

    /// Begins one unpublished provider frame only when its item build key is absent.
    #[must_use]
    pub fn begin_provider_frame_build(
        &self,
        expected_domain_revision: DomainRevision,
        prepared: &PreparedProviderFrame,
    ) -> MutationContribution {
        self.handle.contribution(
            expected_domain_revision,
            BeginProviderFrameBuildMutation {
                build: prepared.initial_build().clone(),
            },
        )
    }

    /// Begins one streamed-observation frame only when its item build key is absent.
    #[must_use]
    pub fn begin_provider_observation_frame_build(
        &self,
        expected_domain_revision: DomainRevision,
        prepared: &PreparedProviderObservationFrame,
    ) -> MutationContribution {
        self.handle.contribution(
            expected_domain_revision,
            BeginProviderFrameBuildMutation {
                build: prepared.initial_build().clone(),
            },
        )
    }

    /// Atomically writes one bounded unreachable staging batch and its exact next build.
    #[must_use]
    pub fn stage_provider_frame_batch(
        &self,
        expected_domain_revision: DomainRevision,
        batch: ProviderFrameStageBatch,
    ) -> MutationContribution {
        self.handle.contribution(
            expected_domain_revision,
            StageProviderFrameBatchMutation { batch },
        )
    }

    /// Stages one bounded batch against writer-admitted current domain state.
    #[must_use]
    pub fn current_stage_provider_frame_batch(
        &self,
        batch: ProviderFrameStageBatch,
    ) -> CurrentDomainCommand {
        self.handle
            .current_command(StageProviderFrameBatchMutation { batch })
    }
}

struct BeginProviderFrameBuildMutation {
    build: crate::ProviderItemBuildRecord,
}

impl DomainMutation<SyndicDomain> for BeginProviderFrameBuildMutation {
    type Error = ProviderFrameMutationError;

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        if provider_point::<ProviderItemBuildsFamily>(reader, &self.build.item_id())?.is_some() {
            return Err(ProviderFrameMutationError::BuildIdentityCollision);
        }
        let content_id = self.build.target().content().id();
        let manifest = provider_point::<ContentManifestsFamily>(reader, &content_id)?;
        match self.build.prior() {
            None if manifest.is_some() => {
                return Err(ProviderFrameMutationError::ManifestIdentityCollision);
            }
            Some(_) => validate_provider_manifest(&self.build, manifest.as_ref())?,
            None => {}
        }
        Ok(())
    }

    fn contribute(
        &self,
        _reader: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        if self.build.prior().is_none() {
            let manifest = first_provider_manifest(&self.build);
            mutations.put::<ContentManifestsCodec>(&manifest.id(), &manifest)?;
        }
        mutations.put::<ProviderItemBuildsCodec>(&self.build.item_id(), &self.build)?;
        Ok(())
    }
}

struct StageProviderFrameBatchMutation {
    batch: ProviderFrameStageBatch,
}

impl DomainMutation<SyndicDomain> for StageProviderFrameBatchMutation {
    type Error = ProviderFrameMutationError;

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        self.batch.validate()?;
        let item_id = self.batch.expected_build().item_id();
        let Some(current) = provider_point::<ProviderItemBuildsFamily>(reader, &item_id)? else {
            return Err(ProviderFrameMutationError::BuildMissing);
        };
        if &current != self.batch.expected_build() {
            return Err(ProviderFrameMutationError::BuildConflict);
        }
        let content_id = current.target().content().id();
        let manifest = provider_point::<ContentManifestsFamily>(reader, &content_id)?;
        validate_provider_manifest(&current, manifest.as_ref())?;
        for chunk in self.batch.chunks() {
            if provider_point::<ContentChunksFamily>(
                reader,
                &ContentChunkKey {
                    owner: chunk.content_id(),
                    ordinal: chunk.ordinal(),
                },
            )?
            .is_some()
            {
                return Err(ProviderFrameMutationError::ChunkIdentityCollision);
            }
        }
        for span in self.batch.byte_spans() {
            if provider_point::<ContentByteSpansFamily>(
                reader,
                &ContentByteSpanKey {
                    owner: span.content_id(),
                    start: span.start(),
                },
            )?
            .is_some()
            {
                return Err(ProviderFrameMutationError::ByteSpanIdentityCollision);
            }
        }
        for record in self.batch.narrative_spans() {
            let key = ProviderNarrativeSpanKey::new(
                record.content_id(),
                record.generation(),
                record.logical_start(),
            );
            if provider_point::<ProviderNarrativeSpansFamily>(reader, &key)?.is_some() {
                return Err(ProviderFrameMutationError::NarrativeSpanIdentityCollision);
            }
        }
        Ok(())
    }

    fn contribute(
        &self,
        _reader: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        for chunk in self.batch.chunks() {
            mutations.put::<ContentChunksCodec>(
                &ContentChunkKey {
                    owner: chunk.content_id(),
                    ordinal: chunk.ordinal(),
                },
                chunk,
            )?;
        }
        for span in self.batch.byte_spans() {
            mutations.put::<ContentByteSpansCodec>(
                &ContentByteSpanKey {
                    owner: span.content_id(),
                    start: span.start(),
                },
                span,
            )?;
        }
        for record in self.batch.narrative_spans() {
            mutations.put::<ProviderNarrativeSpansCodec>(
                &ProviderNarrativeSpanKey::new(
                    record.content_id(),
                    record.generation(),
                    record.logical_start(),
                ),
                record,
            )?;
        }
        mutations.put::<ProviderItemBuildsCodec>(
            &self.batch.next_build().item_id(),
            self.batch.next_build(),
        )?;
        Ok(())
    }
}

fn first_provider_manifest(build: &crate::ProviderItemBuildRecord) -> crate::ContentManifestRecord {
    let target = build.target().content();
    crate::ContentManifestRecord::with_owner(
        target.id(),
        Some(build.item_id()),
        target.revision(),
        crate::ContentEncoding::ProviderItemV1,
        crate::ContentLifecycle::Building,
        0,
        0,
        crate::content_chain_seed(crate::ContentEncoding::ProviderItemV1),
        target.summary(),
    )
}

fn validate_provider_manifest(
    build: &crate::ProviderItemBuildRecord,
    manifest: Option<&crate::ContentManifestRecord>,
) -> Result<(), ProviderFrameMutationError> {
    let Some(manifest) = manifest else {
        return Err(ProviderFrameMutationError::ManifestMissing);
    };
    if let Some(prior) = build.prior() {
        let published = prior.content();
        let summary = published.summary();
        if manifest.owner() != Some(build.item_id())
            || manifest.encoding() != crate::ContentEncoding::ProviderItemV1
            || manifest.lifecycle() != crate::ContentLifecycle::Live
            || manifest.current_reference() != Some(published)
            || manifest.chunk_count() != summary.chunk_count()
            || manifest.encoded_bytes() != summary.encoded_bytes()
            || manifest.chain_digest() != summary.digest()
        {
            return Err(ProviderFrameMutationError::ManifestMismatch);
        }
    } else if manifest != &first_provider_manifest(build) {
        return Err(ProviderFrameMutationError::ManifestMismatch);
    }
    Ok(())
}

fn provider_point<F: Family>(
    reader: &DomainReader<'_, SyndicDomain>,
    key: &F::Key,
) -> Result<Option<F::Value>, ProviderFrameMutationError> {
    reader
        .point::<ExactCodec<F>>(key, crate::codec::family_point_limit::<F>())
        .map_err(Into::into)
}
