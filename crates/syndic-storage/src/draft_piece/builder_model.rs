use super::{
    DraftCompositePositionV1, DraftMarkerIdentityOccurrenceV1, DraftMutationStagingIdentityV1,
    DraftMutationStagingLaneFrontierV1, DraftMutationStagingLaneV1,
    DraftMutationStagingProgressReceiptReferenceV1, DraftPieceBuildRootsV1, DraftPieceDigestV1,
    DraftPieceMarkerV1,
};
use sha2::{Digest, Sha256};

pub fn canonical_empty_changed_occurrence_digest_v1() -> DraftPieceDigestV1 {
    DraftPieceDigestV1::from_bytes(
        Sha256::digest(b"syndic/draft-piece-changed-occurrences/v1/empty").into(),
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftPieceFinishedStagingReferenceV1 {
    identity: DraftMutationStagingIdentityV1,
    head_digest: DraftPieceDigestV1,
    receipt: DraftMutationStagingProgressReceiptReferenceV1,
    source: DraftMutationStagingLaneFrontierV1,
    proposal: DraftMutationStagingLaneFrontierV1,
}

impl DraftPieceFinishedStagingReferenceV1 {
    pub const fn new(
        identity: DraftMutationStagingIdentityV1,
        head_digest: DraftPieceDigestV1,
        receipt: DraftMutationStagingProgressReceiptReferenceV1,
        source: DraftMutationStagingLaneFrontierV1,
        proposal: DraftMutationStagingLaneFrontierV1,
    ) -> Self {
        Self {
            identity,
            head_digest,
            receipt,
            source,
            proposal,
        }
    }

    pub const fn identity(self) -> DraftMutationStagingIdentityV1 {
        self.identity
    }

    pub const fn head_digest(self) -> DraftPieceDigestV1 {
        self.head_digest
    }

    pub const fn receipt(self) -> DraftMutationStagingProgressReceiptReferenceV1 {
        self.receipt
    }

    pub const fn source(self) -> DraftMutationStagingLaneFrontierV1 {
        self.source
    }

    pub const fn proposal(self) -> DraftMutationStagingLaneFrontierV1 {
        self.proposal
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftPieceBuildStagingPhaseV1 {
    Source,
    Proposal,
    Structure,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftPieceChangedOccurrenceFrontierV1 {
    count: u64,
    digest: DraftPieceDigestV1,
}

impl DraftPieceChangedOccurrenceFrontierV1 {
    pub const fn new(count: u64, digest: DraftPieceDigestV1) -> Self {
        Self { count, digest }
    }

    pub const fn count(self) -> u64 {
        self.count
    }

    pub const fn digest(self) -> DraftPieceDigestV1 {
        self.digest
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftPieceMarkerEffectChargesV1 {
    logical_utf8_bytes: u64,
    marker_count: u64,
    encoded_bytes: u64,
}

impl DraftPieceMarkerEffectChargesV1 {
    pub const fn canonical_single_marker() -> Self {
        Self::new(0, 1, 121)
    }

    pub const fn new(logical_utf8_bytes: u64, marker_count: u64, encoded_bytes: u64) -> Self {
        Self {
            logical_utf8_bytes,
            marker_count,
            encoded_bytes,
        }
    }

    pub const fn logical_utf8_bytes(self) -> u64 {
        self.logical_utf8_bytes
    }

    pub const fn marker_count(self) -> u64 {
        self.marker_count
    }

    pub const fn encoded_bytes(self) -> u64 {
        self.encoded_bytes
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftPieceMarkerRemovalProofV1 {
    position: DraftCompositePositionV1,
    occurrence: DraftMarkerIdentityOccurrenceV1,
}

impl DraftPieceMarkerRemovalProofV1 {
    pub const fn new(
        position: DraftCompositePositionV1,
        occurrence: DraftMarkerIdentityOccurrenceV1,
    ) -> Self {
        Self {
            position,
            occurrence,
        }
    }

    pub const fn position(self) -> DraftCompositePositionV1 {
        self.position
    }

    pub const fn occurrence(self) -> DraftMarkerIdentityOccurrenceV1 {
        self.occurrence
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftPieceMarkerInsertionV1 {
    anchor: u64,
    marker: DraftPieceMarkerV1,
    charges: DraftPieceMarkerEffectChargesV1,
}

impl DraftPieceMarkerInsertionV1 {
    pub const fn new(
        anchor: u64,
        marker: DraftPieceMarkerV1,
        charges: DraftPieceMarkerEffectChargesV1,
    ) -> Self {
        Self {
            anchor,
            marker,
            charges,
        }
    }

    pub const fn anchor(self) -> u64 {
        self.anchor
    }

    pub const fn marker(self) -> DraftPieceMarkerV1 {
        self.marker
    }

    pub const fn charges(self) -> DraftPieceMarkerEffectChargesV1 {
        self.charges
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftPieceMarkerEffectV1 {
    Insert(DraftPieceMarkerInsertionV1),
    Remove {
        removal: DraftPieceMarkerRemovalProofV1,
        charges: DraftPieceMarkerEffectChargesV1,
    },
    Move {
        removal: DraftPieceMarkerRemovalProofV1,
        insertion: DraftPieceMarkerInsertionV1,
    },
    SameIdReplacement {
        removal: DraftPieceMarkerRemovalProofV1,
        insertion: DraftPieceMarkerInsertionV1,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftPiecePendingMarkerPhaseV1 {
    Removing,
    DerivingInsertionGap,
    Inserting,
    Publishing,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftPiecePendingMarkerEffectV1 {
    effect: DraftPieceMarkerEffectV1,
    source_roots: DraftPieceBuildRootsV1,
    working_roots: DraftPieceBuildRootsV1,
    phase: DraftPiecePendingMarkerPhaseV1,
}

impl DraftPiecePendingMarkerEffectV1 {
    pub const fn new(
        effect: DraftPieceMarkerEffectV1,
        source_roots: DraftPieceBuildRootsV1,
        working_roots: DraftPieceBuildRootsV1,
        phase: DraftPiecePendingMarkerPhaseV1,
    ) -> Self {
        Self {
            effect,
            source_roots,
            working_roots,
            phase,
        }
    }

    pub const fn effect(self) -> DraftPieceMarkerEffectV1 {
        self.effect
    }

    pub const fn source_roots(self) -> DraftPieceBuildRootsV1 {
        self.source_roots
    }

    pub const fn working_roots(self) -> DraftPieceBuildRootsV1 {
        self.working_roots
    }

    pub const fn phase(self) -> DraftPiecePendingMarkerPhaseV1 {
        self.phase
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftPieceDurableBuildContinuationV1 {
    finished: DraftPieceFinishedStagingReferenceV1,
    source: DraftMutationStagingLaneFrontierV1,
    proposal: DraftMutationStagingLaneFrontierV1,
    phase: DraftPieceBuildStagingPhaseV1,
    changed_occurrences: DraftPieceChangedOccurrenceFrontierV1,
    pending_marker_effect: Option<DraftPiecePendingMarkerEffectV1>,
}

impl DraftPieceDurableBuildContinuationV1 {
    pub const fn new(
        finished: DraftPieceFinishedStagingReferenceV1,
        source: DraftMutationStagingLaneFrontierV1,
        proposal: DraftMutationStagingLaneFrontierV1,
        phase: DraftPieceBuildStagingPhaseV1,
        changed_occurrences: DraftPieceChangedOccurrenceFrontierV1,
        pending_marker_effect: Option<DraftPiecePendingMarkerEffectV1>,
    ) -> Self {
        Self {
            finished,
            source,
            proposal,
            phase,
            changed_occurrences,
            pending_marker_effect,
        }
    }

    pub const fn finished(self) -> DraftPieceFinishedStagingReferenceV1 {
        self.finished
    }

    pub const fn source(self) -> DraftMutationStagingLaneFrontierV1 {
        self.source
    }

    pub const fn proposal(self) -> DraftMutationStagingLaneFrontierV1 {
        self.proposal
    }

    pub const fn phase(self) -> DraftPieceBuildStagingPhaseV1 {
        self.phase
    }

    pub const fn changed_occurrences(self) -> DraftPieceChangedOccurrenceFrontierV1 {
        self.changed_occurrences
    }

    pub const fn pending_marker_effect(self) -> Option<DraftPiecePendingMarkerEffectV1> {
        self.pending_marker_effect
    }

    pub const fn lane(self) -> Option<DraftMutationStagingLaneV1> {
        match self.phase {
            DraftPieceBuildStagingPhaseV1::Source => Some(DraftMutationStagingLaneV1::Source),
            DraftPieceBuildStagingPhaseV1::Proposal => Some(DraftMutationStagingLaneV1::Proposal),
            DraftPieceBuildStagingPhaseV1::Structure => None,
        }
    }

    pub fn is_locally_exact(self) -> bool {
        let finished = self.finished;
        if finished.receipt().identity() != finished.identity()
            || !lane_is_prefix(self.source, finished.source())
            || !lane_is_prefix(self.proposal, finished.proposal())
            || self
                .pending_marker_effect
                .is_some_and(|pending| !effect_is_exact(pending.effect()))
        {
            return false;
        }
        match self.phase {
            DraftPieceBuildStagingPhaseV1::Source => {
                self.source != finished.source()
                    && self.proposal.item_total() == 0
                    && self.pending_marker_effect.is_none()
            }
            DraftPieceBuildStagingPhaseV1::Proposal => {
                self.source == finished.source()
                    && self.proposal != finished.proposal()
                    && self.pending_marker_effect.is_none()
            }
            DraftPieceBuildStagingPhaseV1::Structure => {
                self.source == finished.source() && self.proposal == finished.proposal()
            }
        }
    }
}

fn lane_is_prefix(
    consumed: DraftMutationStagingLaneFrontierV1,
    finished: DraftMutationStagingLaneFrontierV1,
) -> bool {
    consumed.next_cursor() <= finished.next_cursor()
        && consumed.next_ordinal() <= finished.next_ordinal()
        && consumed.item_total() <= finished.item_total()
        && consumed.canonical_byte_total() <= finished.canonical_byte_total()
        && (consumed.next_ordinal() != finished.next_ordinal() || consumed == finished)
}

fn effect_is_exact(effect: DraftPieceMarkerEffectV1) -> bool {
    let valid_charges = |charges: DraftPieceMarkerEffectChargesV1| {
        charges.logical_utf8_bytes() == 0
            && charges.marker_count() == 1
            && charges.encoded_bytes() > 0
    };
    match effect {
        DraftPieceMarkerEffectV1::Insert(insertion) => valid_charges(insertion.charges()),
        DraftPieceMarkerEffectV1::Remove { charges, .. } => valid_charges(charges),
        DraftPieceMarkerEffectV1::Move { removal, insertion }
        | DraftPieceMarkerEffectV1::SameIdReplacement { removal, insertion } => {
            removal.occurrence().marker_id() == insertion.marker().marker_id()
                && removal.occurrence().label() == insertion.marker().label()
                && valid_charges(insertion.charges())
        }
    }
}
