use super::*;
use crate::draft_piece::{
    DraftMarkerIdentityIndexSummaryV1, DraftPieceBuildBoundaryV1, DraftPieceRecordIdV1,
    DraftPieceSummaryV1,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DraftPieceMarkerRemovalSiteV1 {
    pub piece_rank: u64,
    pub marker_ordinal: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DraftPieceMarkerPlanningV1 {
    pub source_boundary: Option<DraftPieceBuildBoundaryV1>,
    pub previous_start: Option<DraftPieceBuildBoundaryV1>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DraftPieceMarkerInsertionSiteV1 {
    pub boundary: DraftPieceBuildBoundaryV1,
    pub marker_ordinal: u64,
    pub mapped_next_boundary: DraftPieceBuildBoundaryV1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DraftPieceSequenceDescriptorV1 {
    pub root_node_id: Option<DraftPieceRecordIdV1>,
    pub summary: DraftPieceSummaryV1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DraftPieceIdentityDescriptorV1 {
    pub root_node_id: Option<DraftPieceRecordIdV1>,
    pub summary: DraftMarkerIdentityIndexSummaryV1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum DraftPieceMarkerProofPurposeV1 {
    SourceBounds = 0,
    RemovalGap = 1,
    SourceOccurrence = 2,
    SourceIdentity = 3,
    PreviousStart = 4,
    PreviousEnd = 5,
    WorkingOccurrence = 6,
    WorkingIdentity = 7,
    InsertIdentityAbsent = 8,
    InsertAnchor = 9,
    InsertOrder = 10,
    InsertAfter = 11,
    SourceInsertIdentityAbsent = 12,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum DraftPieceMarkerProofComponentV1 {
    Primary = 0,
    Secondary = 1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DraftPieceMarkerPendingV1 {
    None,
    Proof {
        purpose: DraftPieceMarkerProofPurposeV1,
        component: DraftPieceMarkerProofComponentV1,
        primary_marker_rank: Option<u64>,
    },
    RemoveSequence,
    RemoveIdentity {
        sequence_target: DraftPieceSequenceDescriptorV1,
    },
    RemoveOrder {
        sequence_target: DraftPieceSequenceDescriptorV1,
        identity_target: DraftPieceIdentityDescriptorV1,
    },
    InsertSequence,
    InsertIdentity {
        sequence_target: DraftPieceSequenceDescriptorV1,
        new_leaf_id: DraftPieceRecordIdV1,
        new_leaf_digest: DraftPieceDigestV1,
    },
    InsertOrder {
        sequence_target: DraftPieceSequenceDescriptorV1,
        identity_target: DraftPieceIdentityDescriptorV1,
        new_leaf_id: DraftPieceRecordIdV1,
        new_leaf_digest: DraftPieceDigestV1,
    },
}

impl DraftPieceSequenceDescriptorV1 {
    pub(crate) fn is_locally_exact(self) -> bool {
        use crate::draft_piece::*;
        let s = self.summary;
        if self.root_node_id.is_none() {
            return s
                == DraftPieceSummaryV1::new(
                    0,
                    0,
                    0,
                    0,
                    0,
                    canonical_empty_marker_digest_v1(),
                    0,
                    canonical_empty_root_digest_v1(),
                );
        }
        s.piece_count() != 0
            && (1..=DRAFT_PIECE_MAX_HEIGHT).contains(&s.height())
            && s.marker_count() <= s.piece_count()
            && s.text_summary().is_canonical()
            && (s.marker_count() != 0 || s.marker_digest() == canonical_empty_marker_digest_v1())
    }
}

impl DraftPieceIdentityDescriptorV1 {
    pub(crate) fn is_locally_exact(self) -> bool {
        use crate::draft_piece::*;
        if self.root_node_id.is_none() {
            return self.summary
                == DraftMarkerIdentityIndexSummaryV1::new(
                    0,
                    0,
                    canonical_empty_marker_identity_index_digest_v1(),
                );
        }
        self.summary.record_count() != 0
            && (1..=DRAFT_PIECE_MAX_HEIGHT).contains(&self.summary.height())
    }
}

impl DraftPieceActiveMarkerEffectV1 {
    pub(crate) fn is_program_locally_exact(self) -> bool {
        use crate::draft_piece::draft_piece_build_roots_are_locally_exact_v1;
        use DraftPieceMarkerPendingV1 as Pending;
        use DraftPieceMarkerProofPurposeV1 as Purpose;
        if !draft_piece_build_roots_are_locally_exact_v1(self.source_roots())
            || !draft_piece_build_roots_are_locally_exact_v1(self.working_roots())
        {
            return false;
        }
        let removes = !matches!(self.effect(), DraftPieceMarkerEffectV1::Insert(_));
        let inserts = !matches!(self.effect(), DraftPieceMarkerEffectV1::Remove { .. });
        if !self.roots_have_program_deltas(removes, inserts) {
            return false;
        }
        if let Pending::Proof {
            purpose,
            component,
            primary_marker_rank,
        } = self.pending()
        {
            if match component {
                DraftPieceMarkerProofComponentV1::Primary => primary_marker_rank.is_some(),
                DraftPieceMarkerProofComponentV1::Secondary => {
                    primary_marker_rank.is_none()
                        || !matches!(
                            purpose,
                            Purpose::SourceBounds
                                | Purpose::RemovalGap
                                | Purpose::PreviousStart
                                | Purpose::PreviousEnd
                        )
                }
            } {
                return false;
            }
        }
        match self.pending() {
            Pending::RemoveIdentity { sequence_target }
            | Pending::InsertIdentity {
                sequence_target, ..
            } => {
                if !sequence_target.is_locally_exact() {
                    return false;
                }
            }
            Pending::RemoveOrder {
                sequence_target,
                identity_target,
            }
            | Pending::InsertOrder {
                sequence_target,
                identity_target,
                ..
            } => {
                if !sequence_target.is_locally_exact()
                    || !identity_target.is_locally_exact()
                    || sequence_target.summary.marker_count()
                        != identity_target.summary.record_count()
                {
                    return false;
                }
            }
            _ => {}
        }
        if let Some(planning) = self.planning() {
            if self.phase() != DraftPieceActiveMarkerPhaseV1::Removing
                || self.insertion_site().is_some()
            {
                return false;
            }
            let (source_absent, previous_present, removal_present) = match self.pending() {
                Pending::Proof { purpose, .. } => match purpose {
                    Purpose::SourceBounds => (true, false, false),
                    Purpose::SourceInsertIdentityAbsent if !removes => (false, false, false),
                    Purpose::RemovalGap
                    | Purpose::SourceOccurrence
                    | Purpose::SourceIdentity
                    | Purpose::WorkingOccurrence
                        if removes =>
                    {
                        (false, false, false)
                    }
                    Purpose::PreviousStart if self.fragment_key().ordinal() > 1 => {
                        (false, false, false)
                    }
                    Purpose::PreviousEnd if self.fragment_key().ordinal() > 1 => {
                        (false, true, false)
                    }
                    Purpose::WorkingIdentity if removes => (false, false, true),
                    _ => return false,
                },
                Pending::RemoveSequence
                | Pending::RemoveIdentity { .. }
                | Pending::RemoveOrder { .. }
                    if removes =>
                {
                    (false, false, true)
                }
                Pending::None => (false, false, removes),
                _ => return false,
            };
            return planning.source_boundary.is_none() == source_absent
                && planning.previous_start.is_some() == previous_present
                && self.removal_site().is_some() == removal_present
                && (matches!(self.pending(), Pending::None)
                    || self.working_roots() == self.source_roots());
        }
        if self.removal_site().is_some() {
            return false;
        }
        match self.phase() {
            DraftPieceActiveMarkerPhaseV1::Removing | DraftPieceActiveMarkerPhaseV1::Publishing => {
                self.insertion_site().is_none() && self.pending() == Pending::None
            }
            DraftPieceActiveMarkerPhaseV1::DerivingInsertionGap => {
                self.insertion_site().is_none()
                    && match self.pending() {
                        Pending::None => true,
                        Pending::Proof {
                            purpose:
                                Purpose::InsertIdentityAbsent
                                | Purpose::InsertAnchor
                                | Purpose::InsertOrder
                                | Purpose::InsertAfter,
                            component: DraftPieceMarkerProofComponentV1::Primary,
                            primary_marker_rank: None,
                        } => inserts,
                        _ => false,
                    }
            }
            DraftPieceActiveMarkerPhaseV1::Inserting => {
                inserts
                    && self.insertion_site().is_some()
                    && matches!(
                        self.pending(),
                        Pending::InsertSequence
                            | Pending::InsertIdentity { .. }
                            | Pending::InsertOrder { .. }
                    )
            }
        }
    }

    fn roots_have_program_deltas(self, removes: bool, inserts: bool) -> bool {
        use DraftPieceMarkerPendingV1 as Pending;
        let source = self.source_roots().sequence_summary();
        let working = self.working_roots().sequence_summary();
        let removed = removes && (self.planning().is_none() || self.pending() == Pending::None);
        let inserted = inserts && self.phase() == DraftPieceActiveMarkerPhaseV1::Publishing;
        let Some(markers) = source
            .marker_count()
            .checked_sub(u64::from(removed))
            .and_then(|count| count.checked_add(u64::from(inserted)))
        else {
            return false;
        };
        let Some(pieces) = source.piece_count().checked_sub(u64::from(removed)) else {
            return false;
        };
        if source.text_summary() != working.text_summary()
            || markers != working.marker_count()
            || if inserted {
                pieces.checked_add(1) != Some(working.piece_count())
                    && pieces.checked_add(2) != Some(working.piece_count())
            } else {
                pieces != working.piece_count()
            }
            || (!removed && !inserted && self.source_roots() != self.working_roots())
        {
            return false;
        }
        if self.removal_site().is_some_and(|site| {
            site.piece_rank >= source.piece_count() || site.marker_ordinal >= source.marker_count()
        }) {
            return false;
        }
        let (sequence, identity, insertion) = match self.pending() {
            Pending::RemoveIdentity { sequence_target } => (Some(sequence_target), None, false),
            Pending::RemoveOrder {
                sequence_target,
                identity_target,
            } => (Some(sequence_target), Some(identity_target), false),
            Pending::InsertIdentity {
                sequence_target, ..
            } => (Some(sequence_target), None, true),
            Pending::InsertOrder {
                sequence_target,
                identity_target,
                ..
            } => (Some(sequence_target), Some(identity_target), true),
            _ => (None, None, false),
        };
        if let Some(sequence) = sequence {
            let count = if insertion {
                working.marker_count().checked_add(1)
            } else {
                working.marker_count().checked_sub(1)
            };
            let pieces = if insertion {
                working.piece_count().checked_add(
                    1 + u64::from(
                        self.insertion_site()
                            .is_some_and(|site| site.boundary.inner() != 0),
                    ),
                )
            } else {
                working.piece_count().checked_sub(1)
            };
            if count != Some(sequence.summary.marker_count())
                || pieces != Some(sequence.summary.piece_count())
                || working.text_summary() != sequence.summary.text_summary()
            {
                return false;
            }
            if identity.is_some_and(|identity| {
                identity.summary.record_count() != sequence.summary.marker_count()
            }) {
                return false;
            }
        }
        true
    }
}
