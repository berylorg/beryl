use beryl_home_store::HomeStore;
use gpui_text_input::{
    ByteOffset, InlineObjectGap, InlineObjectId, InlineObjectNeighbor, InlineObjectOrder,
    SourcePosition,
};
use syndic_storage::{
    DraftCompositeGapWitnessV1, DraftCompositePositionV1, DraftPieceMarkerDemandV1,
    DraftPieceMarkerDirectionV1, DraftPieceMarkerScopeV1, DraftPieceTextDemandV1,
};

use super::super::{ComposerHostBinding, ComposerHostError, SyndicComposerHost};

const HISTORY_MARKER_NEIGHBOR_BYTES: usize = 4_096;

impl SyndicComposerHost {
    pub(super) fn history_position(
        &self,
        store: &HomeStore,
        binding: ComposerHostBinding,
        position: DraftCompositePositionV1,
    ) -> Result<SourcePosition, ComposerHostError> {
        let (text_demand, max_bytes) = if binding.logical_extent().logical_utf8_bytes() == 0 {
            (DraftPieceTextDemandV1::Forward(0), 4)
        } else {
            (DraftPieceTextDemandV1::Validate(position.utf8_offset()), 4)
        };
        self.storage
            .draft_piece_text_demand(store, binding.root(), text_demand, max_bytes)?;
        let gap = match position.gap() {
            DraftCompositeGapWitnessV1::Unambiguous => InlineObjectGap::NoObjects,
            DraftCompositeGapWitnessV1::Between {
                left_order_key,
                left_marker_id,
                right_order_key,
                right_marker_id,
            } => InlineObjectGap::Between {
                preceding: marker_neighbor(left_marker_id, left_order_key),
                following: marker_neighbor(right_marker_id, right_order_key),
            },
            DraftCompositeGapWitnessV1::BeforeAll => {
                InlineObjectGap::Before(self.history_boundary_neighbor(
                    store,
                    binding,
                    position.utf8_offset(),
                    DraftPieceMarkerDirectionV1::Forward,
                )?)
            }
            DraftCompositeGapWitnessV1::AfterAll => {
                InlineObjectGap::After(self.history_boundary_neighbor(
                    store,
                    binding,
                    position.utf8_offset(),
                    DraftPieceMarkerDirectionV1::Backward,
                )?)
            }
        };
        Ok(SourcePosition::new(
            ByteOffset::new(position.utf8_offset()),
            gap,
        ))
    }

    fn history_boundary_neighbor(
        &self,
        store: &HomeStore,
        binding: ComposerHostBinding,
        anchor: u64,
        direction: DraftPieceMarkerDirectionV1,
    ) -> Result<InlineObjectNeighbor, ComposerHostError> {
        let page = self.storage.draft_piece_marker_demand(
            store,
            binding.root(),
            DraftPieceMarkerDemandV1::new(
                DraftPieceMarkerScopeV1::ExactAnchor(anchor),
                direction,
                None,
                1,
                HISTORY_MARKER_NEIGHBOR_BYTES,
            ),
        )?;
        let marker = page
            .markers()
            .first()
            .filter(|marker| marker.anchor() == anchor)
            .ok_or(ComposerHostError::HistoryUnavailable)?;
        Ok(marker_neighbor(
            marker.marker().marker_id(),
            marker.marker().order_key(),
        ))
    }
}

fn marker_neighbor(
    marker_id: beryl_model::SyndicDraftMarkerId,
    order_key: u64,
) -> InlineObjectNeighbor {
    InlineObjectNeighbor::new(
        InlineObjectId::new(u128::from_be_bytes(*marker_id.as_bytes())),
        InlineObjectOrder::new(u128::from(order_key)),
    )
}
