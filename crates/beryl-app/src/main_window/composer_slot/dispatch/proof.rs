use beryl_home_store::HomeStore;
use gpui_text_input::{
    ByteOffset, InlineObjectGap, MutationPositions, ObjectDemand, ObjectDemandEnvelope,
    ObjectDirection, ObjectRequestId, ObjectResidency, ObjectResidencyLimits, PageDemand,
    PageDemandEnvelope, PagePurpose, PageRequestId, PresentationGeneration, RangeResidency,
    ResidencyLimits,
};

use crate::composer_host::SyndicComposerHost;

use super::super::{MainWindowComposerSelectionIdentity, MainWindowComposerSlot};
use super::{MainWindowComposerDispatchError, MainWindowComposerDispatcher, translate};

#[derive(Clone, Copy)]
pub(in crate::main_window) struct MainWindowComposerSuccessorProofLimits {
    pub(in crate::main_window) text: ResidencyLimits,
    pub(in crate::main_window) text_page_bytes: u64,
    pub(in crate::main_window) objects: ObjectResidencyLimits,
    pub(in crate::main_window) presentation_generation: PresentationGeneration,
}

pub(in crate::main_window) struct MainWindowComposerSuccessorProof {
    pub(in crate::main_window) selection: MainWindowComposerSelectionIdentity,
    pub(in crate::main_window) binding: gpui_text_input::RangeBinding,
    pub(in crate::main_window) positions: MutationPositions,
    pub(in crate::main_window) text: RangeResidency,
    pub(in crate::main_window) objects: ObjectResidency,
}

impl MainWindowComposerSlot {
    pub(in crate::main_window) fn build_selected_successor_proof(
        &mut self,
        store: &HomeStore,
        selection: MainWindowComposerSelectionIdentity,
        positions: MutationPositions,
        limits: MainWindowComposerSuccessorProofLimits,
    ) -> Result<MainWindowComposerSuccessorProof, MainWindowComposerDispatchError> {
        self.build_selected_successor_proof_with_extra_positions(
            store,
            selection,
            positions,
            &[],
            limits,
        )
    }

    pub(in crate::main_window) fn build_selected_successor_proof_with_extra_positions(
        &mut self,
        store: &HomeStore,
        selection: MainWindowComposerSelectionIdentity,
        positions: MutationPositions,
        extra_positions: &[gpui_text_input::SourcePosition],
        limits: MainWindowComposerSuccessorProofLimits,
    ) -> Result<MainWindowComposerSuccessorProof, MainWindowComposerDispatchError> {
        let selected = self
            .selected
            .as_mut()
            .filter(|selected| selected.identity == selection)
            .ok_or(MainWindowComposerDispatchError::StaleSelection)?;
        if selected.dispatcher.in_dispatch
            || selected.dispatcher.binding != selection.binding()
            || selected.host.binding() != Some(selection.binding())
        {
            return Err(MainWindowComposerDispatchError::Busy);
        }
        selected.dispatcher.in_dispatch = true;
        let result = build_successor_proof(
            &mut selected.host,
            &mut selected.dispatcher,
            store,
            selection,
            positions,
            extra_positions,
            limits,
        );
        selected.dispatcher.in_dispatch = false;
        result
    }
}

fn build_successor_proof(
    host: &mut SyndicComposerHost,
    dispatcher: &mut MainWindowComposerDispatcher,
    store: &HomeStore,
    selection: MainWindowComposerSelectionIdentity,
    positions: MutationPositions,
    extra_positions: &[gpui_text_input::SourcePosition],
    limits: MainWindowComposerSuccessorProofLimits,
) -> Result<MainWindowComposerSuccessorProof, MainWindowComposerDispatchError> {
    let binding = selection.binding().range_binding();
    let text_residency_bytes = u64::try_from(
        limits
            .text
            .max_resident_bytes()
            .min(usize::try_from(limits.text.max_pending_bytes()).unwrap_or(usize::MAX)),
    )
    .map_err(|_| MainWindowComposerDispatchError::Malformed)?;
    let text_page_bytes = limits.text_page_bytes.min(text_residency_bytes);
    if text_page_bytes == 0 {
        return Err(MainWindowComposerDispatchError::Malformed);
    }
    let unique = unique_positions(positions, extra_positions)?;
    if limits.objects.max_resident_pages() < unique.len() {
        return Err(MainWindowComposerDispatchError::SuccessorProof(
            "resident page capacity",
        ));
    }
    let mut text = RangeResidency::new(binding, limits.text);
    let mut text_request_id = 0u64;
    for position in &unique {
        let offset = position.byte_offset;
        if offset.get() == 0 || offset.get() == binding.extent().byte_len() {
            continue;
        }
        text_request_id = text_request_id
            .checked_add(1)
            .ok_or(MainWindowComposerDispatchError::Malformed)?;
        let demand = text
            .demand(
                PageRequestId::new(text_request_id),
                PagePurpose::Selection,
                PageDemandEnvelope::Validation {
                    candidate: offset,
                    max_payload_bytes: text_page_bytes,
                },
            )
            .map_err(|_| MainWindowComposerDispatchError::Malformed)?;
        if let PageDemand::Requested(request) = demand {
            let page = translate::text_page(
                host,
                store,
                selection.binding(),
                dispatcher.allocate_host_request_id()?,
                request,
            )?;
            text.admit(page)
                .map_err(|_| MainWindowComposerDispatchError::Malformed)?;
        }
    }

    let mut objects = ObjectResidency::new(binding, limits.presentation_generation, limits.objects);
    let object_bytes = limits
        .objects
        .max_resident_bytes()
        .min(limits.objects.max_pending_bytes());
    if object_bytes == 0 || limits.objects.max_resident_objects() == 0 {
        return Err(MainWindowComposerDispatchError::Malformed);
    }
    for (index, position) in unique.iter().enumerate() {
        let (cursor, direction) = object_demand_start(*position);
        let demand =
            ObjectDemandEnvelope::anchor(position.byte_offset, cursor, direction, 1, object_bytes)
                .map_err(|_| MainWindowComposerDispatchError::SuccessorProof("object envelope"))?;
        let request_id =
            u64::try_from(index + 1).map_err(|_| MainWindowComposerDispatchError::Malformed)?;
        match objects
            .demand(
                ObjectRequestId::new(request_id),
                gpui_text_input::ObjectPurpose::MutationSuccessor,
                demand,
            )
            .map_err(|_| MainWindowComposerDispatchError::SuccessorProof("object demand"))?
        {
            ObjectDemand::Resident(_) => {}
            ObjectDemand::Coalesced(_) => {
                return Err(MainWindowComposerDispatchError::SuccessorProof(
                    "object demand coalesced",
                ));
            }
            ObjectDemand::Requested(request) => {
                let page = translate::object_page(
                    host,
                    store,
                    selection.binding(),
                    dispatcher.allocate_host_request_id()?,
                    request,
                )?;
                let anchor_proofs =
                    text.prove_object_page_anchors(binding, &page)
                        .map_err(|_| {
                            MainWindowComposerDispatchError::SuccessorProof("object anchors")
                        })?;
                objects.admit(page, anchor_proofs).map_err(|_| {
                    MainWindowComposerDispatchError::SuccessorProof("object admission")
                })?;
            }
        }
    }
    Ok(MainWindowComposerSuccessorProof {
        selection,
        binding,
        positions,
        text,
        objects,
    })
}

fn unique_positions(
    positions: MutationPositions,
    extra_positions: &[gpui_text_input::SourcePosition],
) -> Result<Vec<gpui_text_input::SourcePosition>, MainWindowComposerDispatchError> {
    const MAX_PROOF_POSITIONS: usize = 5;
    if extra_positions.len() > 2 {
        return Err(MainWindowComposerDispatchError::Malformed);
    }
    let mut unique = Vec::with_capacity(MAX_PROOF_POSITIONS);
    for position in [
        positions.caret(),
        positions.selection_anchor(),
        positions.selection_head(),
    ]
    .into_iter()
    .chain(extra_positions.iter().copied())
    {
        if !unique.contains(&position) {
            unique.push(position);
        }
    }
    Ok(unique)
}

fn object_demand_start(
    position: gpui_text_input::SourcePosition,
) -> (Option<gpui_text_input::ObjectCursor>, ObjectDirection) {
    match position.gap {
        InlineObjectGap::NoObjects | InlineObjectGap::Before(_) => (None, ObjectDirection::Forward),
        InlineObjectGap::After(_) => (None, ObjectDirection::Backward),
        InlineObjectGap::Between { preceding, .. } => (
            Some(gpui_text_input::ObjectCursor::new(
                ByteOffset::new(position.byte_offset.get()),
                preceding.order(),
                preceding.id(),
            )),
            ObjectDirection::Forward,
        ),
    }
}
