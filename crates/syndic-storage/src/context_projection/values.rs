use crate::{
    CasLineageProof, CasRepresentedPrefixProof, NativeCasLineage, RecoveryItemCount,
    RecoveryUtf8ByteCount, SyndicTimestamp, SyndicValueError,
};
use beryl_model::{
    BerylHomeId, BindingRevision, CasLoadedSessionGeneration, DomainRevision,
    RecoveryItemSequenceDigest, SyndicThreadId,
};
use sha2::{Digest, Sha256};

pub(crate) fn invalid(reason: &'static str) -> SyndicValueError {
    SyndicValueError::InvalidLineageProof { reason }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ContextSummary {
    pub(crate) count: u64,
    pub(crate) utf8_bytes: u64,
    pub(crate) digest: [u8; 32],
}

impl ContextSummary {
    pub fn empty() -> Self {
        Self {
            count: 0,
            utf8_bytes: 0,
            digest: Sha256::digest(b"syndic/context-sequence/v1/empty\0").into(),
        }
    }
    pub const fn count(self) -> u64 {
        self.count
    }
    pub const fn utf8_bytes(self) -> u64 {
        self.utf8_bytes
    }
    pub const fn digest(self) -> [u8; 32] {
        self.digest
    }
    pub(crate) fn checked(self) -> Result<Self, SyndicValueError> {
        if self.count == 0 {
            if self != Self::empty() {
                return Err(invalid("invalid empty context summary"));
            }
        } else if self
            .count
            .checked_mul(462)
            .is_none_or(|minimum| self.utf8_bytes < minimum)
        {
            return Err(invalid("context summary byte count is impossible"));
        }
        Ok(self)
    }
    pub(crate) fn append(self, entry: [u8; 32], bytes: u64) -> Result<Self, SyndicValueError> {
        let count = self
            .count
            .checked_add(1)
            .ok_or(invalid("context count overflow"))?;
        let utf8_bytes = self
            .utf8_bytes
            .checked_add(bytes)
            .ok_or(invalid("context byte overflow"))?;
        let mut hash = Sha256::new();
        hash.update(b"syndic/context-sequence/v1/step\0");
        hash.update(self.digest);
        hash.update(count.to_be_bytes());
        hash.update(entry);
        hash.update(bytes.to_be_bytes());
        Self {
            count,
            utf8_bytes,
            digest: hash.finalize().into(),
        }
        .checked()
    }
    pub(crate) fn hash_into(self, hash: &mut Sha256) {
        hash.update(self.count.to_be_bytes());
        hash.update(self.utf8_bytes.to_be_bytes());
        hash.update(self.digest);
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ContextSourceAnchor {
    pub(crate) thread: SyndicThreadId,
    pub(crate) revision: BindingRevision,
}
impl ContextSourceAnchor {
    pub const fn thread_id(self) -> SyndicThreadId {
        self.thread
    }
    pub const fn binding_revision(self) -> BindingRevision {
        self.revision
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ContextNativeCut {
    pub(crate) prefix: CasRepresentedPrefixProof,
    pub(crate) summary: ContextSummary,
}
impl ContextNativeCut {
    pub const fn represented_prefix(self) -> CasRepresentedPrefixProof {
        self.prefix
    }
    pub const fn summary(self) -> ContextSummary {
        self.summary
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ContextInjectionObservation {
    pub(crate) items: RecoveryItemCount,
    pub(crate) bytes: RecoveryUtf8ByteCount,
    pub(crate) digest: RecoveryItemSequenceDigest,
    pub(crate) completed_at: SyndicTimestamp,
}
impl ContextInjectionObservation {
    pub fn new(
        items: RecoveryItemCount,
        bytes: RecoveryUtf8ByteCount,
        digest: RecoveryItemSequenceDigest,
        completed_at: SyndicTimestamp,
    ) -> Result<Self, SyndicValueError> {
        if u64::from(items.get()) > bytes.get() {
            return Err(invalid("injection item count exceeds bytes"));
        }
        Ok(Self {
            items,
            bytes,
            digest,
            completed_at,
        })
    }
    pub const fn item_count(self) -> RecoveryItemCount {
        self.items
    }
    pub const fn utf8_bytes(self) -> RecoveryUtf8ByteCount {
        self.bytes
    }
    pub const fn sequence_digest(self) -> RecoveryItemSequenceDigest {
        self.digest
    }
    pub const fn completed_at(self) -> SyndicTimestamp {
        self.completed_at
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum ContextEstablishment {
    NativeSeed {
        source: ContextSourceAnchor,
        cut: ContextNativeCut,
        entry: [u8; 32],
        injection: ContextInjectionObservation,
    },
    Inherited {
        mechanism: NativeCasLineage,
        source: ContextSourceAnchor,
        cut: ContextNativeCut,
    },
    Replay {
        injection: ContextInjectionObservation,
    },
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ContextLineageProof {
    pub(crate) home: BerylHomeId,
    pub(crate) prefix: CasRepresentedPrefixProof,
    pub(crate) summary: ContextSummary,
    pub(crate) projection_digest: [u8; 32],
    pub(crate) ordering_floor: SyndicTimestamp,
    pub(crate) generation: CasLoadedSessionGeneration,
    pub(crate) establishment: ContextEstablishment,
}
impl ContextLineageProof {
    pub const fn home_id(self) -> BerylHomeId {
        self.home
    }
    pub const fn established_prefix(self) -> CasRepresentedPrefixProof {
        self.prefix
    }
    pub const fn summary(self) -> ContextSummary {
        self.summary
    }
    pub const fn projection_digest(self) -> [u8; 32] {
        self.projection_digest
    }
    pub const fn ordering_floor(self) -> SyndicTimestamp {
        self.ordering_floor
    }
    pub const fn loaded_generation(self) -> CasLoadedSessionGeneration {
        self.generation
    }
    pub const fn injection(self) -> Option<ContextInjectionObservation> {
        match self.establishment {
            ContextEstablishment::NativeSeed { injection, .. }
            | ContextEstablishment::Replay { injection } => Some(injection),
            _ => None,
        }
    }
    pub(crate) fn checked(self) -> Result<Self, SyndicValueError> {
        self.summary.checked()?;
        if self.summary.count == 0 || self.prefix.tail().is_none() {
            return Err(invalid(
                "context lineage requires a context and represented tail",
            ));
        }
        match self.establishment {
            ContextEstablishment::NativeSeed { cut, injection, .. } => {
                cut.summary.checked()?;
                if cut.prefix.tail() != self.prefix.tail()
                    || cut.prefix.digest() != self.prefix.digest()
                    || cut.summary.count.checked_add(1) != Some(self.summary.count)
                    || cut.summary.utf8_bytes.checked_add(injection.bytes.get())
                        != Some(self.summary.utf8_bytes)
                    || injection.items.get() != 1
                    || injection.bytes.get() < 462
                    || injection.bytes.get() > 65_997
                    || injection.completed_at != self.ordering_floor
                {
                    return Err(invalid("invalid native context seed observation"));
                }
            }
            ContextEstablishment::Inherited { mechanism, cut, .. } => {
                if mechanism == NativeCasLineage::Fresh
                    || cut.prefix.tail() != self.prefix.tail()
                    || cut.prefix.digest() != self.prefix.digest()
                    || cut.summary != self.summary
                {
                    return Err(invalid("invalid inherited context proof"));
                }
            }
            ContextEstablishment::Replay { injection } => {
                if injection.completed_at != self.ordering_floor
                    || u64::from(injection.items.get()) <= self.summary.count
                    || injection.bytes.get() <= self.summary.utf8_bytes
                {
                    return Err(invalid("invalid context replay observation"));
                }
            }
        }
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ContextPreparationKind {
    Native {
        source: ContextSourceAnchor,
        cut: ContextNativeCut,
        seed: Option<[u8; 32]>,
        seed_sequence: Option<(u64, RecoveryItemSequenceDigest)>,
        ordering_floor: SyndicTimestamp,
    },
    Replay,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ContextClassification {
    pub(crate) home: BerylHomeId,
    pub(crate) home_generation: beryl_home_store::HomeGenerationIdentity,
    pub(crate) thread: SyndicThreadId,
    pub(crate) prefix: CasRepresentedPrefixProof,
    pub(crate) selected: crate::SelectedPathProof,
    pub(crate) gate: beryl_model::InputGateRevision,
    pub(crate) summary: ContextSummary,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContextBindingClassification {
    pub(crate) context: ContextClassification,
    pub(crate) binding_revision: BindingRevision,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContextProjectionBasis {
    pub(crate) home: BerylHomeId,
    pub(crate) home_generation: beryl_home_store::HomeGenerationIdentity,
    pub(crate) thread: SyndicThreadId,
    pub(crate) prefix: CasRepresentedPrefixProof,
    pub(crate) summary: ContextSummary,
    pub(crate) projection_digest: [u8; 32],
    pub(crate) source_revision: DomainRevision,
    pub(crate) selected_path: crate::SelectedPathProof,
    pub(crate) expected_gate: beryl_model::InputGateRevision,
    pub(crate) source_head_revision: Option<BindingRevision>,
    pub(crate) kind: ContextPreparationKind,
}
impl ContextProjectionBasis {
    pub const fn needs_seed(self) -> bool {
        matches!(
            self.kind,
            ContextPreparationKind::Native { seed: Some(_), .. }
        )
    }
    pub const fn summary(self) -> ContextSummary {
        self.summary
    }
    pub const fn projection_digest(self) -> [u8; 32] {
        self.projection_digest
    }
    pub const fn source_revision(self) -> DomainRevision {
        self.source_revision
    }
    pub fn complete_native(
        self,
        mechanism: NativeCasLineage,
        generation: CasLoadedSessionGeneration,
        injection: Option<ContextInjectionObservation>,
    ) -> Result<CasLineageProof, SyndicValueError> {
        let ContextPreparationKind::Native {
            source,
            cut,
            seed,
            seed_sequence,
            ordering_floor,
        } = self.kind
        else {
            return Err(invalid("replay context cannot complete native lineage"));
        };
        let (establishment, floor) = match (seed, injection) {
            (Some(entry), Some(injection))
                if mechanism == NativeCasLineage::Fork
                    && injection.items.get() == 1
                    && seed_sequence == Some((injection.bytes.get(), injection.digest))
                    && injection.completed_at >= ordering_floor =>
            {
                (
                    ContextEstablishment::NativeSeed {
                        source,
                        cut,
                        entry,
                        injection,
                    },
                    injection.completed_at,
                )
            }
            (None, None) if mechanism != NativeCasLineage::Fresh => (
                ContextEstablishment::Inherited {
                    mechanism,
                    source,
                    cut,
                },
                ordering_floor,
            ),
            _ => {
                return Err(invalid(
                    "context completion does not match prepared native disposition",
                ));
            }
        };
        Ok(CasLineageProof::ContextAware(
            ContextLineageProof {
                home: self.home,
                prefix: self.prefix,
                summary: self.summary,
                projection_digest: self.projection_digest,
                ordering_floor: floor,
                generation,
                establishment,
            }
            .checked()?,
        ))
    }
    pub(crate) fn complete_replay(
        self,
        generation: CasLoadedSessionGeneration,
        injection: ContextInjectionObservation,
    ) -> Result<CasLineageProof, SyndicValueError> {
        if self.kind != ContextPreparationKind::Replay {
            return self.complete_native(NativeCasLineage::Fork, generation, Some(injection));
        }
        Ok(CasLineageProof::ContextAware(
            ContextLineageProof {
                home: self.home,
                prefix: self.prefix,
                summary: self.summary,
                projection_digest: self.projection_digest,
                ordering_floor: injection.completed_at,
                generation,
                establishment: ContextEstablishment::Replay { injection },
            }
            .checked()?,
        ))
    }
}
