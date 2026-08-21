use super::DraftEditHistoryTransitionReferenceV1;

pub const DRAFT_EDIT_HISTORY_ANCESTOR_LEVELS: usize = 64;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftEditHistoryAncestorWitnessV1 {
    bitmap: u64,
    slots: [Option<DraftEditHistoryTransitionReferenceV1>; DRAFT_EDIT_HISTORY_ANCESTOR_LEVELS],
}

impl DraftEditHistoryAncestorWitnessV1 {
    pub const EMPTY: Self = Self {
        bitmap: 0,
        slots: [None; DRAFT_EDIT_HISTORY_ANCESTOR_LEVELS],
    };

    pub const fn from_parts(
        bitmap: u64,
        slots: [Option<DraftEditHistoryTransitionReferenceV1>; DRAFT_EDIT_HISTORY_ANCESTOR_LEVELS],
    ) -> Self {
        Self { bitmap, slots }
    }

    pub const fn bitmap(&self) -> u64 {
        self.bitmap
    }

    pub const fn ancestor(&self, level: usize) -> Option<DraftEditHistoryTransitionReferenceV1> {
        if level >= DRAFT_EDIT_HISTORY_ANCESTOR_LEVELS {
            return None;
        }
        self.slots[level]
    }

    pub const fn slots(
        &self,
    ) -> &[Option<DraftEditHistoryTransitionReferenceV1>; DRAFT_EDIT_HISTORY_ANCESTOR_LEVELS] {
        &self.slots
    }

    pub(crate) fn is_canonical_for_depth(&self, depth: u64) -> bool {
        if depth == 0 || self.bitmap != ancestor_bitmap_for_depth(depth) {
            return false;
        }
        self.slots.iter().enumerate().all(|(level, slot)| {
            let present = self.bitmap & (1_u64 << level) != 0;
            present == slot.is_some()
        })
    }
}

pub(crate) const fn ancestor_bitmap_for_depth(depth: u64) -> u64 {
    if depth <= 1 {
        return 0;
    }
    u64::MAX >> (depth - 1).leading_zeros()
}

#[cfg(test)]
mod tests {
    use beryl_model::SyndicDraftId;

    use super::*;
    use crate::draft_piece::{
        DraftEditHistoryTransitionKeyV1, DraftEditorCandidateSessionIdV1, DraftPieceDigestV1,
    };

    fn reference(depth: u64) -> DraftEditHistoryTransitionReferenceV1 {
        DraftEditHistoryTransitionReferenceV1::new(
            DraftEditHistoryTransitionKeyV1::new(
                SyndicDraftId::from_bytes([1; 16]),
                DraftEditorCandidateSessionIdV1::from_bytes([2; 16]),
                depth,
            ),
            depth,
            depth,
            DraftPieceDigestV1::from_bytes([3; 32]),
        )
    }

    #[test]
    fn zero_one_and_high_bit_depths_have_exact_canonical_slot_shapes() {
        assert!(!DraftEditHistoryAncestorWitnessV1::EMPTY.is_canonical_for_depth(0));
        assert!(DraftEditHistoryAncestorWitnessV1::EMPTY.is_canonical_for_depth(1));

        let mut slots = [None; DRAFT_EDIT_HISTORY_ANCESTOR_LEVELS];
        slots[0] = Some(reference(1));
        assert!(DraftEditHistoryAncestorWitnessV1::from_parts(1, slots).is_canonical_for_depth(2));

        let mut slots = [None; DRAFT_EDIT_HISTORY_ANCESTOR_LEVELS];
        slots[..63].fill(Some(reference(1)));
        let high_boundary = DraftEditHistoryAncestorWitnessV1::from_parts(u64::MAX >> 1, slots);
        assert!(high_boundary.is_canonical_for_depth(1_u64 << 63));
        assert_eq!(high_boundary.ancestor(63), None);

        let all_slots = [Some(reference(1)); DRAFT_EDIT_HISTORY_ANCESTOR_LEVELS];
        let maximum = DraftEditHistoryAncestorWitnessV1::from_parts(u64::MAX, all_slots);
        assert!(maximum.is_canonical_for_depth(u64::MAX));
        assert!(maximum.ancestor(63).is_some());
    }

    #[test]
    fn missing_or_extra_slots_are_not_canonical_for_depth() {
        let mut missing = [Some(reference(1)); DRAFT_EDIT_HISTORY_ANCESTOR_LEVELS];
        missing[31] = None;
        assert!(
            !DraftEditHistoryAncestorWitnessV1::from_parts(u64::MAX, missing)
                .is_canonical_for_depth(u64::MAX)
        );

        let mut extra = [None; DRAFT_EDIT_HISTORY_ANCESTOR_LEVELS];
        extra[0] = Some(reference(1));
        assert!(!DraftEditHistoryAncestorWitnessV1::from_parts(0, extra).is_canonical_for_depth(1));
    }
}
