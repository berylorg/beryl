use beryl_home_store::{
    AdmittedSidecar, CheckedBatchFootprint, CommandBuildError, CommandPairRequirement,
    CommandParticipantKind, DomainMutation, DomainReader, DomainValidator, HomeCommand, HomeStore,
    MutationBuilder, MutationContribution, ReconciliationReservation, RecordCodec,
    ValidationContribution,
};
use beryl_model::{DomainRevision, GeneratedMediaUnavailableReason};
use sha2::{Digest, Sha256};

use super::super::{AssetSidecarState, codec::AssetMetadataCodec, metadata_point_limit};
use super::{
    AssetDomain, AssetMetadataRecord, AssetState, GeneratedMediaAdmission, GeneratedMediaCodec,
    GeneratedMediaError, GeneratedMediaFacts, GeneratedMediaLifecycle, GeneratedMediaRecord,
    GeneratedMediaTarget, codec,
};
use crate::RecordRevision;

const PROTOCOL: u64 = 0x67656e6d65646961;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GeneratedMediaCommandFootprint {
    reads: CheckedBatchFootprint,
    writes: CheckedBatchFootprint,
    reconciliation_records: usize,
}

impl GeneratedMediaCommandFootprint {
    pub const fn reads(self) -> CheckedBatchFootprint {
        self.reads
    }
    pub const fn writes(self) -> CheckedBatchFootprint {
        self.writes
    }
    pub const fn reconciliation_records(self) -> usize {
        self.reconciliation_records
    }
    pub fn preparation() -> Self {
        mutation_footprint()
    }
    pub fn publication() -> Self {
        mutation_footprint()
    }
    pub fn absence() -> Self {
        Self {
            reads: codec_footprint::<GeneratedMediaCodec>(),
            writes: CheckedBatchFootprint::new(0, 0, 0),
            reconciliation_records: 0,
        }
    }
}

fn codec_footprint<C: RecordCodec<AssetDomain>>() -> CheckedBatchFootprint {
    CheckedBatchFootprint::new(
        1,
        u64::try_from(C::MAX_KEY_BYTES).expect("fixed key bound"),
        u64::try_from(
            C::MAX_VALUE_BYTES
                .checked_add(beryl_home_store::RECORD_VERSION_BYTES)
                .expect("fixed value bound"),
        )
        .expect("fixed value bound"),
    )
}
fn mutation_footprint() -> GeneratedMediaCommandFootprint {
    let closure = codec_footprint::<AssetMetadataCodec>()
        .checked_add(codec_footprint::<GeneratedMediaCodec>())
        .expect("fixed generated media closure");
    GeneratedMediaCommandFootprint {
        reads: closure,
        writes: closure,
        reconciliation_records: 2,
    }
}

pub struct GeneratedMediaPreparation {
    sidecar: AdmittedSidecar,
    contribution: MutationContribution,
    target: GeneratedMediaTarget,
    facts: GeneratedMediaFacts,
}

impl GeneratedMediaPreparation {
    pub const fn target(&self) -> GeneratedMediaTarget {
        self.target
    }
    pub const fn facts(&self) -> &GeneratedMediaFacts {
        &self.facts
    }
    pub fn max_footprint(&self) -> GeneratedMediaCommandFootprint {
        GeneratedMediaCommandFootprint::preparation()
    }
    pub fn add_to(self, command: &mut HomeCommand) -> Result<(), CommandBuildError> {
        command.require_sidecar(self.sidecar)?;
        command.add(self.contribution)?;
        Ok(())
    }
}

impl AssetState {
    pub fn prepare_generated_media(
        &self,
        store: &HomeStore,
        expected_revision: DomainRevision,
        sidecar: AdmittedSidecar,
        target: GeneratedMediaTarget,
        byte_facts: GeneratedMediaFacts,
    ) -> Result<GeneratedMediaPreparation, GeneratedMediaError> {
        if sidecar.address() != &super::super::asset_sidecar_address(byte_facts.asset)
            || self.revision(store)? != expected_revision
        {
            return Err(GeneratedMediaError::Rejected);
        }
        let previous_metadata = self.metadata(store, byte_facts.asset)?;
        let facts = effective_facts(previous_metadata.as_ref(), &byte_facts)?;
        let previous = store.read_point::<AssetDomain, GeneratedMediaCodec>(
            &self.handle,
            &target.resource,
            codec::point_limit(),
        )?;
        if self.revision(store)? != expected_revision {
            return Err(GeneratedMediaError::Rejected);
        }
        if let Some(previous) = previous {
            if !previous.matches_target(target)
                || previous.facts != facts
                || previous_metadata
                    .as_ref()
                    .is_none_or(|m| !previous.matches_metadata(m))
            {
                return Err(GeneratedMediaError::Rejected);
            }
            return Err(GeneratedMediaError::NoEffect);
        }
        let metadata = match previous_metadata.as_ref() {
            Some(metadata) => metadata.clone(),
            None => AssetMetadataRecord {
                asset_id: facts.asset,
                media_type: facts.media_type.clone(),
                dimensions: facts.dimensions,
                creation_revision: expected_revision
                    .checked_next()
                    .map_err(|_| GeneratedMediaError::RevisionExhausted)?,
                sidecar_state: AssetSidecarState::Committed,
                revision: RecordRevision::INITIAL,
            },
        };
        let mutation = PrepareGeneratedMedia {
            target,
            byte_facts,
            facts: facts.clone(),
            previous_metadata,
            metadata,
        };
        Ok(GeneratedMediaPreparation {
            sidecar,
            contribution: self.handle.contribution(expected_revision, mutation),
            target,
            facts,
        })
    }

    pub fn publish_generated_media(
        &self,
        store: &HomeStore,
        expected_revision: DomainRevision,
        admission: &GeneratedMediaAdmission,
    ) -> Result<MutationContribution, GeneratedMediaError> {
        store.domain_revision(&admission.scope)?;
        if self.revision(store)? != expected_revision {
            return Err(GeneratedMediaError::Rejected);
        }
        let fresh = self
            .generated_media_admission(store, admission.target)?
            .ok_or(GeneratedMediaError::Rejected)?;
        if self.revision(store)? != expected_revision || fresh.facts() != admission.facts() {
            return Err(GeneratedMediaError::Rejected);
        }
        if fresh.lifecycle() == GeneratedMediaLifecycle::Published {
            return Err(GeneratedMediaError::NoEffect);
        }
        if fresh.record != admission.record {
            return Err(GeneratedMediaError::Rejected);
        }
        Ok(self.handle.contribution(
            expected_revision,
            PublishGeneratedMedia {
                target: admission.target,
                expected: admission.record.clone(),
            },
        ))
    }

    pub fn validate_generated_media_absence(
        &self,
        expected_revision: DomainRevision,
        target: GeneratedMediaTarget,
        reason: GeneratedMediaUnavailableReason,
    ) -> ValidationContribution {
        self.handle.validation(
            expected_revision,
            ValidateGeneratedMediaAbsence { target, reason },
        )
    }
}

fn effective_facts(
    metadata: Option<&AssetMetadataRecord>,
    bytes: &GeneratedMediaFacts,
) -> Result<GeneratedMediaFacts, GeneratedMediaError> {
    match metadata {
        None => Ok(bytes.clone()),
        Some(metadata) => {
            if metadata.asset_id() != bytes.asset
                || metadata.media_type() != &bytes.media_type
                || metadata
                    .dimensions()
                    .is_some_and(|d| Some(d) != bytes.dimensions)
            {
                return Err(GeneratedMediaError::Rejected);
            }
            Ok(GeneratedMediaFacts {
                asset: metadata.asset_id(),
                media_type: metadata.media_type().clone(),
                dimensions: metadata.dimensions(),
            })
        }
    }
}

struct PrepareGeneratedMedia {
    target: GeneratedMediaTarget,
    byte_facts: GeneratedMediaFacts,
    facts: GeneratedMediaFacts,
    previous_metadata: Option<AssetMetadataRecord>,
    metadata: AssetMetadataRecord,
}

struct PreparedGeneratedMedia {
    target: GeneratedMediaTarget,
    record: GeneratedMediaRecord,
    metadata: AssetMetadataRecord,
}

impl DomainMutation<AssetDomain> for PrepareGeneratedMedia {
    type Error = GeneratedMediaError;
    type Prepared = PreparedGeneratedMedia;
    fn command_pair(&self) -> Option<CommandPairRequirement> {
        Some(CommandPairRequirement::new::<PROTOCOL, 1>(
            "syndic",
            CommandParticipantKind::Validation,
            asset_correlation(1, self.target, &self.facts),
        ))
    }
    fn prepare(
        self,
        reader: &DomainReader<'_, AssetDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let metadata =
            reader.point::<AssetMetadataCodec>(&self.facts.asset, metadata_point_limit())?;
        if metadata != self.previous_metadata
            || effective_facts(metadata.as_ref(), &self.byte_facts)? != self.facts
            || reader
                .point::<GeneratedMediaCodec>(&self.target.resource, codec::point_limit())?
                .is_some()
        {
            return Err(GeneratedMediaError::Rejected);
        }
        Ok(PreparedGeneratedMedia {
            target: self.target,
            record: GeneratedMediaRecord {
                thread: self.target.thread,
                turn: self.target.turn,
                item: self.target.item,
                fingerprint: self.target.fingerprint,
                facts: self.facts,
                lifecycle: GeneratedMediaLifecycle::Prepared,
                revision: RecordRevision::INITIAL,
            },
            metadata: self.metadata,
        })
    }
    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, AssetDomain>,
    ) -> Result<(), Self::Error> {
        reserve(reservation)
    }
    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, AssetDomain>,
    ) -> Result<(), Self::Error> {
        contribute(prepared, mutations)
    }
}

struct PublishGeneratedMedia {
    target: GeneratedMediaTarget,
    expected: GeneratedMediaRecord,
}
impl DomainMutation<AssetDomain> for PublishGeneratedMedia {
    type Error = GeneratedMediaError;
    type Prepared = PreparedGeneratedMedia;
    fn command_pair(&self) -> Option<CommandPairRequirement> {
        Some(CommandPairRequirement::new::<PROTOCOL, 2>(
            "syndic",
            CommandParticipantKind::Mutation,
            asset_correlation(2, self.target, &self.expected.facts),
        ))
    }
    fn prepare(
        self,
        reader: &DomainReader<'_, AssetDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let current = reader
            .point::<GeneratedMediaCodec>(&self.target.resource, codec::point_limit())?
            .ok_or(GeneratedMediaError::Rejected)?;
        let metadata = reader
            .point::<AssetMetadataCodec>(&self.expected.facts.asset, metadata_point_limit())?
            .ok_or(GeneratedMediaError::Rejected)?;
        if current != self.expected
            || !current.matches_target(self.target)
            || !current.matches_metadata(&metadata)
            || current.lifecycle != GeneratedMediaLifecycle::Prepared
        {
            return Err(GeneratedMediaError::Rejected);
        }
        let mut record = current;
        record.lifecycle = GeneratedMediaLifecycle::Published;
        record.revision = record
            .revision
            .checked_next()
            .map_err(|_| GeneratedMediaError::RevisionExhausted)?;
        Ok(PreparedGeneratedMedia {
            target: self.target,
            record,
            metadata,
        })
    }
    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, AssetDomain>,
    ) -> Result<(), Self::Error> {
        reserve(reservation)
    }
    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, AssetDomain>,
    ) -> Result<(), Self::Error> {
        contribute(prepared, mutations)
    }
}

fn reserve(
    reservation: &mut ReconciliationReservation<'_, AssetDomain>,
) -> Result<(), GeneratedMediaError> {
    reservation.reserve_records::<AssetMetadataCodec>(1)?;
    reservation.reserve_records::<GeneratedMediaCodec>(1)?;
    Ok(())
}
fn contribute(
    prepared: PreparedGeneratedMedia,
    mutations: &mut MutationBuilder<'_, AssetDomain>,
) -> Result<(), GeneratedMediaError> {
    mutations.put::<AssetMetadataCodec>(&prepared.metadata.asset_id(), &prepared.metadata)?;
    mutations.put::<GeneratedMediaCodec>(&prepared.target.resource, &prepared.record)?;
    Ok(())
}

struct ValidateGeneratedMediaAbsence {
    target: GeneratedMediaTarget,
    reason: GeneratedMediaUnavailableReason,
}
impl DomainValidator<AssetDomain> for ValidateGeneratedMediaAbsence {
    type Error = GeneratedMediaError;
    fn command_pair(&self) -> Option<CommandPairRequirement> {
        Some(CommandPairRequirement::new::<PROTOCOL, 3>(
            "syndic",
            CommandParticipantKind::Mutation,
            unavailable_correlation(self.target, self.reason),
        ))
    }
    fn validate(&self, reader: &DomainReader<'_, AssetDomain>) -> Result<(), Self::Error> {
        if reader
            .point::<GeneratedMediaCodec>(&self.target.resource, codec::point_limit())?
            .is_some()
        {
            return Err(GeneratedMediaError::Rejected);
        }
        Ok(())
    }
}

fn correlation_prefix(operation: u64, target: GeneratedMediaTarget) -> Sha256 {
    let mut hash = Sha256::new();
    hash.update(b"beryl/generated-media-command/v1\0");
    hash.update(operation.to_le_bytes());
    hash.update(target.resource.as_bytes());
    hash.update(target.thread.as_bytes());
    hash.update(target.turn.as_bytes());
    hash.update(target.item.as_bytes());
    hash.update(target.fingerprint);
    hash
}
fn asset_correlation(
    operation: u64,
    target: GeneratedMediaTarget,
    facts: &GeneratedMediaFacts,
) -> [u8; 32] {
    let mut hash = correlation_prefix(operation, target);
    hash.update(facts.asset.digest());
    hash.update(facts.asset.length().get().to_le_bytes());
    let media = facts.media_type.as_str().as_bytes();
    hash.update([u8::try_from(media.len()).expect("bounded media type")]);
    hash.update(media);
    match facts.dimensions {
        None => hash.update([0]),
        Some(dimensions) => {
            hash.update([1]);
            hash.update(
                u32::try_from(dimensions.width().get())
                    .expect("validated generated width")
                    .to_le_bytes(),
            );
            hash.update(
                u32::try_from(dimensions.height().get())
                    .expect("validated generated height")
                    .to_le_bytes(),
            );
        }
    }
    hash.finalize().into()
}
fn unavailable_correlation(
    target: GeneratedMediaTarget,
    reason: GeneratedMediaUnavailableReason,
) -> [u8; 32] {
    let mut hash = correlation_prefix(3, target);
    hash.update([reason.code()]);
    hash.finalize().into()
}
