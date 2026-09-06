use beryl_model::SyndicDraftMarkerId;
use gpui_text_input::{InlineObjectGap, RangeRestorationSeed, SourcePosition};
use syndic_storage::{DraftCompositeGapWitnessV1, DraftCompositePositionV1};

use super::*;

impl MainWindowComposerSlot {
    pub(in crate::main_window) fn native_lineage_prepublication_active(
        &self,
        selection: MainWindowComposerSelectionIdentity,
    ) -> bool {
        !self.disposed
            && self.native_lineage_suspension == Some(selection)
            && self.selected_identity() == Some(selection)
    }

    pub(in crate::main_window) fn prepare_native_lineage_validation(
        &self,
        selection: MainWindowComposerSelectionIdentity,
        seed: RangeRestorationSeed,
    ) -> Result<
        (
            syndic_storage::SyndicStorage,
            syndic_storage::DraftPieceRestorationV1,
        ),
        MainWindowComposerSlotError,
    > {
        if self.disposed {
            return Err(MainWindowComposerSlotError::Disposed);
        }
        if self.window_close.is_some()
            || self.pending.is_some()
            || self.disposal_stage.is_some()
            || self.submission_successor.is_some()
            || self
                .native_lineage_suspension
                .is_some_and(|suspension| suspension != selection)
            || self.selected_identity() != Some(selection)
            || seed.binding != selection.binding().range_binding()
            || seed.history != Some(selection.binding().range_history_frontier())
        {
            return Err(MainWindowComposerSlotError::StaleActivationReceipt);
        }
        let restoration = crate::composer_host::ComposerHostRestorationSeed::new(
            selection.binding().root(),
            selection.binding().history(),
            selection.binding().logical_extent(),
            position(seed.caret)?,
            position(seed.selection.anchor)?,
            position(seed.scroll.position)?,
        );
        Ok((self.storage.clone(), restoration.restoration()))
    }

    pub(in crate::main_window) fn begin_native_lineage_suspension(
        &mut self,
        selection: MainWindowComposerSelectionIdentity,
        seed: RangeRestorationSeed,
    ) -> Result<(), MainWindowComposerSlotError> {
        if self.disposed
            || self.window_close.is_some()
            || self.pending.is_some()
            || self.disposal_stage.is_some()
            || self.submission_successor.is_some()
            || self.native_lineage_suspension.is_some()
            || self.selected_identity() != Some(selection)
            || seed.binding != selection.binding().range_binding()
            || seed.history != Some(selection.binding().range_history_frontier())
        {
            return Err(MainWindowComposerSlotError::StaleActivationReceipt);
        }
        self.native_lineage_suspension = Some(selection);
        Ok(())
    }

    pub(in crate::main_window) fn cancel_native_lineage_suspension(
        &mut self,
        selection: MainWindowComposerSelectionIdentity,
    ) -> Result<(), MainWindowComposerSlotError> {
        if self.native_lineage_suspension.is_none() {
            return Ok(());
        }
        if self.native_lineage_suspension != Some(selection) {
            return Err(MainWindowComposerSlotError::StaleActivationReceipt);
        }
        self.native_lineage_suspension = None;
        Ok(())
    }

    pub(in crate::main_window) fn complete_native_lineage_restoration(
        &mut self,
        selection: MainWindowComposerSelectionIdentity,
    ) -> Result<(), MainWindowComposerSlotError> {
        if self.native_lineage_suspension != Some(selection)
            || self.selected_identity() != Some(selection)
        {
            return Err(MainWindowComposerSlotError::StaleActivationReceipt);
        }
        self.native_lineage_suspension = None;
        Ok(())
    }
}

fn position(
    position: SourcePosition,
) -> Result<DraftCompositePositionV1, MainWindowComposerSlotError> {
    let marker = |id: gpui_text_input::InlineObjectId| {
        SyndicDraftMarkerId::from_bytes(id.get().to_be_bytes())
    };
    let order = |value: gpui_text_input::InlineObjectOrder| {
        u64::try_from(value.get()).map_err(|_| MainWindowComposerSlotError::IdentityMismatch)
    };
    let gap = match position.gap {
        InlineObjectGap::NoObjects => DraftCompositeGapWitnessV1::Unambiguous,
        InlineObjectGap::Before(_) => DraftCompositeGapWitnessV1::BeforeAll,
        InlineObjectGap::After(_) => DraftCompositeGapWitnessV1::AfterAll,
        InlineObjectGap::Between {
            preceding,
            following,
        } => DraftCompositeGapWitnessV1::Between {
            left_order_key: order(preceding.order())?,
            left_marker_id: marker(preceding.id()),
            right_order_key: order(following.order())?,
            right_marker_id: marker(following.id()),
        },
    };
    Ok(DraftCompositePositionV1::new(
        position.byte_offset.get(),
        gap,
    ))
}
