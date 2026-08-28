use std::sync::Arc;

use gpui_text_input::{
    ByteRange, ClipboardCompletion, ClipboardId, ClipboardKind, ClipboardLimits, ClipboardProgress,
    ClipboardWriteRequest, InlineObjectGap, InlineObjectNeighbor, MutationBeginRequest,
    MutationCursor, MutationFinishInput, MutationIdentity, MutationLane, MutationLimits,
    MutationPage, MutationPageItem, MutationPageKey, MutationPositions, MutationStreamFinish,
    MutationTotals, ObjectChange, ObjectCursor, ObjectDemandEnvelope, ObjectDirection, ObjectPage,
    ObjectPageEdgeFact, ObjectPurpose, ObjectRequest, ObjectRequestId, ObjectRequestKey,
    ObjectTarget, PageRequestId, RangeClipboardCoordinator, RangeSourceSelection, RangeTextInput,
    RangeTextInputRequest, SourcePosition, SourceRange, TextInputAtomClipboardPolicy,
};

use super::MainWindowConversationComposerService;
use crate::main_window::{
    MainWindowComposerDispatchOutcome, MainWindowComposerSelectionIdentity, MainWindowComposerSlot,
    MainWindowComposerSuccessorProof, MainWindowComposerSuccessorProofLimits,
};

mod collection;
mod paging;

pub(super) use collection::{ActivePropagatedClipboard, PropagatedClipboardAction};
use paging::{cursor_after_gap, deletion_caret, deletion_extent, read_cut_page};

pub(super) struct ActivePropagatedCut {
    key: gpui_text_input::MutationKey,
    scan: PropagatedCutScan,
    prepared_items: Option<Vec<MutationPageItem>>,
    object_count: usize,
    item_count: usize,
    text_deletion_pending: bool,
    next_cursor: MutationCursor,
    next_ordinal: u64,
    cumulative_identity: MutationIdentity,
    intended_extent: gpui_text_input::LogicalExtent,
    intended: MutationPositions,
}

pub(super) struct PreparedPropagatedCut {
    deletion: gpui_text_input::CutDeletion,
    proof: MainWindowComposerSuccessorProof,
    scan: PropagatedCutScan,
    prepared_items: Vec<MutationPageItem>,
}

#[derive(Clone, Copy)]
pub(super) struct PropagatedCutPageRequest {
    selection: SourceRange,
    cursor: Option<ObjectCursor>,
    request_id: u64,
    max_objects: usize,
    max_retained_bytes: usize,
    presentation_generation: gpui_text_input::PresentationGeneration,
    pending_tail: Option<PendingCutMarker>,
}

pub(super) struct PreparedPropagatedCutPage {
    items: Vec<MutationPageItem>,
    continuation: Option<ObjectCursor>,
    pending_tail: Option<PendingCutMarker>,
    complete: bool,
}

#[derive(Clone, Copy)]
struct PendingCutMarker {
    anchor: gpui_text_input::ByteOffset,
    id: gpui_text_input::InlineObjectId,
    order: gpui_text_input::InlineObjectOrder,
    preceding: Option<ObjectCursor>,
}

#[derive(Clone, Copy)]
struct PropagatedCutScan {
    selection: SourceRange,
    cursor: Option<ObjectCursor>,
    next_request_id: u64,
    max_objects: usize,
    max_retained_bytes: usize,
    presentation_generation: gpui_text_input::PresentationGeneration,
    pending_tail: Option<PendingCutMarker>,
    complete: bool,
}

pub(super) fn prepare_cut_after_write(
    service: &Arc<MainWindowConversationComposerService>,
    selection: MainWindowComposerSelectionIdentity,
    deletion: gpui_text_input::CutDeletion,
    proof_limits: MainWindowComposerSuccessorProofLimits,
    mutation_limits: MutationLimits,
) -> Result<PreparedPropagatedCut, String> {
    if deletion.binding() != selection.binding().range_binding() {
        return Err("composer cut binding changed after clipboard write".into());
    }
    let replacement = deletion.selection();
    let proof_positions = deletion.predecessor();
    let mut slot = service
        .slot
        .lock()
        .map_err(|_| "conversation composer service lock failed".to_owned())?;
    let proof = slot
        .build_selected_successor_proof_with_extra_positions(
            &service.store,
            selection,
            proof_positions,
            &[replacement.start(), replacement.end()],
            proof_limits,
        )
        .map_err(|_| "composer cut position proof failed".to_owned())?;
    let mut scan = PropagatedCutScan::new(
        replacement,
        mutation_limits,
        proof_limits.objects.max_pending_bytes(),
        proof_limits.presentation_generation,
    )?;
    let prepared = read_cut_page(&mut slot, &service.store, selection, scan.request()?)?;
    scan.admit(&prepared)?;
    Ok(PreparedPropagatedCut {
        deletion,
        proof,
        scan,
        prepared_items: prepared.items,
    })
}

pub(super) fn prepare_next_cut_page(
    service: &Arc<MainWindowConversationComposerService>,
    selection: MainWindowComposerSelectionIdentity,
    request: PropagatedCutPageRequest,
) -> Result<PreparedPropagatedCutPage, String> {
    let mut slot = service
        .slot
        .lock()
        .map_err(|_| "conversation composer service lock failed".to_owned())?;
    read_cut_page(&mut slot, &service.store, selection, request)
}

impl PreparedPropagatedCut {
    pub(super) fn begin(
        self,
        input: &mut RangeTextInput,
        cx: &mut gpui::Context<RangeTextInput>,
    ) -> Result<ActivePropagatedCut, String> {
        let Self {
            deletion,
            proof,
            scan,
            prepared_items,
        } = self;
        let operation = input
            .lease_host_operation()
            .map_err(|_| "composer cut operation lease was rejected".to_owned())?;
        let replacement = deletion.selection();
        let proposal = deletion
            .proposal(operation.operation(), replacement)
            .map_err(|_| "composer cut proposal was rejected".to_owned())?;
        let intended_extent = deletion_extent(deletion)?;
        let intended = MutationPositions::collapsed(deletion_caret(replacement));
        let mut positions = Vec::with_capacity(5);
        for position in [
            proposal.predecessor().caret(),
            proposal.predecessor().selection_anchor(),
            proposal.predecessor().selection_head(),
            replacement.start(),
            replacement.end(),
        ] {
            if !positions.contains(&position) {
                positions.push(position);
            }
        }
        let begin =
            MutationBeginRequest::new(proposal, MutationCursor::new(0), MutationCursor::new(0));
        let key = input
            .begin_host_mutation(
                operation,
                begin,
                &positions,
                &proof.text,
                &proof.objects,
                cx,
            )
            .map_err(|_| "composer cut mutation start was rejected".to_owned())?;
        Ok(ActivePropagatedCut {
            key,
            scan,
            prepared_items: (!prepared_items.is_empty()).then_some(prepared_items),
            object_count: 0,
            item_count: 0,
            text_deletion_pending: replacement.start().byte_offset != replacement.end().byte_offset,
            next_cursor: MutationCursor::new(0),
            next_ordinal: 0,
            cumulative_identity: MutationIdentity::ROOT,
            intended_extent,
            intended,
        })
    }
}

impl ActivePropagatedCut {
    pub(super) const fn key(&self) -> gpui_text_input::MutationKey {
        self.key
    }

    pub(super) fn submit_next(
        &mut self,
        input: &mut RangeTextInput,
        cx: &mut gpui::Context<RangeTextInput>,
    ) -> Result<(), String> {
        if let Some(items) = self.prepared_items.take() {
            let item_count = items.len();
            let object_count = items
                .iter()
                .filter(|item| matches!(item, MutationPageItem::Object(_)))
                .count();
            let page = MutationPage::new(
                MutationPageKey::new(
                    self.key,
                    MutationLane::Proposal,
                    self.next_cursor,
                    self.next_ordinal,
                    self.cumulative_identity,
                ),
                MutationCursor::new(self.next_cursor.get().saturating_add(1)),
                items,
            )
            .map_err(|_| "composer cut mutation page was rejected".to_owned())?;
            self.object_count = self.object_count.saturating_add(object_count);
            self.item_count = self.item_count.saturating_add(item_count);
            self.next_cursor = page.next_cursor();
            self.next_ordinal = self.next_ordinal.saturating_add(1);
            self.cumulative_identity = page.cumulative_identity();
            input
                .submit_mutation_page(page, cx)
                .map_err(|_| "composer cut mutation page submission was rejected".to_owned())?;
            if item_count == 0 {
                return Err("composer cut prepared an empty mutation page".into());
            }
            return Ok(());
        }
        if !self.scan.complete {
            return Err("composer cut marker page was not prepared".into());
        }
        if self.text_deletion_pending {
            self.text_deletion_pending = false;
            self.prepared_items = Some(vec![MutationPageItem::Utf8 {
                inserted_offset: 0,
                text: "".into(),
            }]);
            return self.submit_next(input, cx);
        }
        let source = MutationStreamFinish {
            next_cursor: MutationCursor::new(0),
            next_ordinal: 0,
            cumulative_identity: MutationIdentity::ROOT,
            totals: MutationTotals::default(),
        };
        let proposal = MutationStreamFinish {
            next_cursor: self.next_cursor,
            next_ordinal: self.next_ordinal,
            cumulative_identity: self.cumulative_identity,
            totals: MutationTotals {
                pages: self.next_ordinal,
                items: self.item_count as u64,
                objects: self.object_count as u64,
                ..MutationTotals::default()
            },
        };
        input
            .submit_mutation_finish(
                MutationFinishInput::new(
                    self.key,
                    source,
                    proposal,
                    self.intended_extent,
                    self.intended,
                ),
                cx,
            )
            .map_err(|_| "composer cut mutation finish was rejected".to_owned())
    }

    pub(super) fn next_page_request(&self) -> Option<PropagatedCutPageRequest> {
        (self.prepared_items.is_none() && !self.scan.complete)
            .then(|| self.scan.request())
            .transpose()
            .ok()
            .flatten()
    }

    pub(super) fn admit_prepared_page(
        &mut self,
        prepared: PreparedPropagatedCutPage,
    ) -> Result<(), String> {
        if self.prepared_items.is_some() {
            return Err("composer cut already retains a prepared marker page".into());
        }
        self.scan.admit(&prepared)?;
        self.prepared_items = (!prepared.items.is_empty()).then_some(prepared.items);
        Ok(())
    }
}

impl PropagatedCutScan {
    fn new(
        selection: SourceRange,
        limits: MutationLimits,
        max_retained_bytes: usize,
        presentation_generation: gpui_text_input::PresentationGeneration,
    ) -> Result<Self, String> {
        let max_objects = limits.max_page_items().min(limits.max_page_objects());
        if max_objects == 0 || max_retained_bytes == 0 {
            return Err("composer cut scan has zero capacity".into());
        }
        Ok(Self {
            selection,
            cursor: cursor_after_gap(selection.start()),
            next_request_id: 1,
            max_objects,
            max_retained_bytes,
            presentation_generation,
            pending_tail: None,
            complete: false,
        })
    }

    fn request(self) -> Result<PropagatedCutPageRequest, String> {
        if self.complete {
            return Err("composer cut scan is already complete".into());
        }
        Ok(PropagatedCutPageRequest {
            selection: self.selection,
            cursor: self.cursor,
            request_id: self.next_request_id,
            max_objects: self.max_objects,
            max_retained_bytes: self.max_retained_bytes,
            presentation_generation: self.presentation_generation,
            pending_tail: self.pending_tail,
        })
    }

    fn admit(&mut self, prepared: &PreparedPropagatedCutPage) -> Result<(), String> {
        if self.complete {
            return Err("composer cut admitted a page after scan completion".into());
        }
        self.next_request_id = self
            .next_request_id
            .checked_add(1)
            .ok_or_else(|| "composer cut request identity exhausted".to_owned())?;
        self.cursor = prepared.continuation;
        self.pending_tail = prepared.pending_tail;
        self.complete = prepared.complete;
        if !self.complete && self.cursor.is_none() {
            return Err("composer cut marker page omitted its continuation".into());
        }
        Ok(())
    }
}
