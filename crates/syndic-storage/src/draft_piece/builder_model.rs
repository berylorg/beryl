use super::{
    DraftCompositePositionV1, DraftMarkerIdentityOccurrenceV1, DraftMutationStagingIdentityV1,
    DraftMutationStagingLaneFrontierV1, DraftMutationStagingLaneV1,
    DraftMutationStagingProgressReceiptReferenceV1, DraftPieceBuildRootsV1, DraftPieceDigestV1,
    DraftPieceMarkerV1, DraftPieceSettlementKeyV1,
};
use sha2::{Digest, Sha256};

pub fn canonical_empty_marker_effect_chain_v1() -> DraftPieceDigestV1 {
    DraftPieceDigestV1::from_bytes(
        Sha256::digest(b"syndic/draft-marker-effect-chain/v1/empty").into(),
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
pub struct DraftPieceMarkerEffectScanFrontierV1 {
    next_fragment_ordinal: u64,
    scanned_endpoint: Option<super::DraftPieceCanonicalFragmentEndpointV1>,
    completed_effect_count: u64,
    effect_chain: DraftPieceDigestV1,
}

impl DraftPieceMarkerEffectScanFrontierV1 {
    pub fn canonical_empty() -> Self {
        Self {
            next_fragment_ordinal: 1,
            scanned_endpoint: None,
            completed_effect_count: 0,
            effect_chain: canonical_empty_marker_effect_chain_v1(),
        }
    }

    pub const fn new(
        next_fragment_ordinal: u64,
        scanned_endpoint: Option<super::DraftPieceCanonicalFragmentEndpointV1>,
        completed_effect_count: u64,
        effect_chain: DraftPieceDigestV1,
    ) -> Self {
        Self {
            next_fragment_ordinal,
            scanned_endpoint,
            completed_effect_count,
            effect_chain,
        }
    }

    pub const fn next_fragment_ordinal(self) -> u64 {
        self.next_fragment_ordinal
    }

    pub const fn scanned_endpoint(self) -> Option<super::DraftPieceCanonicalFragmentEndpointV1> {
        self.scanned_endpoint
    }

    pub const fn completed_effect_count(self) -> u64 {
        self.completed_effect_count
    }

    pub const fn effect_chain(self) -> DraftPieceDigestV1 {
        self.effect_chain
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftPieceMarkerEffectChargesV1 {
    logical_utf8_bytes: u64,
    marker_count: u64,
    encoded_bytes: u64,
}

impl DraftPieceMarkerEffectChargesV1 {
    pub const fn for_marker(_marker: DraftPieceMarkerV1) -> Self {
        Self::new(0, 1, 162)
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
pub enum DraftPieceActiveMarkerPhaseV1 {
    Removing,
    DerivingInsertionGap,
    Inserting,
    Publishing,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftPieceActiveMarkerEffectV1 {
    fragment_key: super::DraftPieceBuildFragmentKeyV1,
    fragment_digest: DraftPieceDigestV1,
    effect: DraftPieceMarkerEffectV1,
    source_roots: DraftPieceBuildRootsV1,
    working_roots: DraftPieceBuildRootsV1,
    source_frontier: u64,
    successor_frontier: u64,
    phase: DraftPieceActiveMarkerPhaseV1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftPieceMarkerEffectContinuationV1 {
    source_logical_frontier: u64,
    successor_logical_frontier: u64,
    scan: DraftPieceMarkerEffectScanFrontierV1,
    active: Option<DraftPieceActiveMarkerEffectV1>,
}

impl DraftPieceMarkerEffectContinuationV1 {
    pub fn canonical_empty() -> Self {
        Self::new(
            0,
            0,
            DraftPieceMarkerEffectScanFrontierV1::canonical_empty(),
            None,
        )
    }

    pub const fn new(
        source_logical_frontier: u64,
        successor_logical_frontier: u64,
        scan: DraftPieceMarkerEffectScanFrontierV1,
        active: Option<DraftPieceActiveMarkerEffectV1>,
    ) -> Self {
        Self {
            source_logical_frontier,
            successor_logical_frontier,
            scan,
            active,
        }
    }

    pub const fn source_logical_frontier(self) -> u64 {
        self.source_logical_frontier
    }

    pub const fn successor_logical_frontier(self) -> u64 {
        self.successor_logical_frontier
    }

    pub const fn scan(self) -> DraftPieceMarkerEffectScanFrontierV1 {
        self.scan
    }

    pub const fn active(self) -> Option<DraftPieceActiveMarkerEffectV1> {
        self.active
    }

    pub fn is_locally_exact(self, identity: DraftPieceSettlementKeyV1) -> bool {
        marker_scan_is_exact(self.scan, identity)
            && self.active.is_none_or(|active| {
                effect_is_exact(active.effect())
                    && active.fragment_key().draft_id() == identity.draft_id()
                    && active.fragment_key().session_id() == identity.session_id()
                    && active.fragment_key().operation_id() == identity.operation_id()
                    && active.fragment_key().ordinal() == self.scan.next_fragment_ordinal()
                    && active.source_frontier() == self.source_logical_frontier
                    && active.successor_frontier() == self.successor_logical_frontier
            })
    }
}

impl DraftPieceActiveMarkerEffectV1 {
    pub const fn new(
        fragment_key: super::DraftPieceBuildFragmentKeyV1,
        fragment_digest: DraftPieceDigestV1,
        effect: DraftPieceMarkerEffectV1,
        source_roots: DraftPieceBuildRootsV1,
        working_roots: DraftPieceBuildRootsV1,
        source_frontier: u64,
        successor_frontier: u64,
        phase: DraftPieceActiveMarkerPhaseV1,
    ) -> Self {
        Self {
            fragment_key,
            fragment_digest,
            effect,
            source_roots,
            working_roots,
            source_frontier,
            successor_frontier,
            phase,
        }
    }

    pub const fn fragment_key(self) -> super::DraftPieceBuildFragmentKeyV1 {
        self.fragment_key
    }

    pub const fn fragment_digest(self) -> DraftPieceDigestV1 {
        self.fragment_digest
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

    pub const fn source_frontier(self) -> u64 {
        self.source_frontier
    }

    pub const fn successor_frontier(self) -> u64 {
        self.successor_frontier
    }

    pub const fn phase(self) -> DraftPieceActiveMarkerPhaseV1 {
        self.phase
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftPieceDurableBuildContinuationV1 {
    finished: DraftPieceFinishedStagingReferenceV1,
    source: DraftMutationStagingLaneFrontierV1,
    proposal: DraftMutationStagingLaneFrontierV1,
    phase: DraftPieceBuildStagingPhaseV1,
}

impl DraftPieceDurableBuildContinuationV1 {
    pub const fn new(
        finished: DraftPieceFinishedStagingReferenceV1,
        source: DraftMutationStagingLaneFrontierV1,
        proposal: DraftMutationStagingLaneFrontierV1,
        phase: DraftPieceBuildStagingPhaseV1,
    ) -> Self {
        Self {
            finished,
            source,
            proposal,
            phase,
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
        {
            return false;
        }
        match self.phase {
            DraftPieceBuildStagingPhaseV1::Source => {
                self.source != finished.source() && self.proposal.item_total() == 0
            }
            DraftPieceBuildStagingPhaseV1::Proposal => {
                self.source == finished.source() && self.proposal != finished.proposal()
            }
            DraftPieceBuildStagingPhaseV1::Structure => {
                self.source == finished.source() && self.proposal == finished.proposal()
            }
        }
    }
}

fn marker_scan_is_exact(
    scan: DraftPieceMarkerEffectScanFrontierV1,
    identity: DraftPieceSettlementKeyV1,
) -> bool {
    if scan.next_fragment_ordinal() == 0 {
        return false;
    }
    match scan.scanned_endpoint() {
        None => {
            scan.next_fragment_ordinal() == 1
                && scan.completed_effect_count() == 0
                && scan.effect_chain() == canonical_empty_marker_effect_chain_v1()
        }
        Some(endpoint) => {
            let key = endpoint.key();
            key.draft_id() == identity.draft_id()
                && key.session_id() == identity.session_id()
                && key.operation_id() == identity.operation_id()
                && key.ordinal().checked_add(1) == Some(scan.next_fragment_ordinal())
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
                && removal.occurrence().asset_id() == insertion.marker().asset_id()
                && valid_charges(insertion.charges())
        }
    }
}
