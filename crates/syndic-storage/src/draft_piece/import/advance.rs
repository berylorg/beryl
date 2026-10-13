use super::super::*;
use super::{
    admission::{authenticate_source, required},
    command::{authenticate_transition, custody, head_key, put_transition},
    read::{SourceReader, StoreSourceReader},
    request::*,
    stream::*,
};
use crate::{SyndicStorage, codec::*, domain::SyndicDomain};
use beryl_home_store::{
    DomainMutation, DomainReader, HomeStore, MutationBuilder, MutationContribution,
    ReconciliationReservation,
};

pub enum DraftReplacementAdvancePreparationV1 {
    Ready(PreparedDraftReplacementAdvanceV1),
    Image(DraftReplacementImageSourceV1),
    Complete(DraftPieceRootReferenceV1),
}

pub struct DraftReplacementImageSourceV1 {
    pub(super) prepared: PreparedDraftReplacementImportV1,
    pub(super) build: DraftPieceBuildRecordV1,
    pub(super) session: DraftEditorCandidateSessionV1,
    pub(super) marker_id: beryl_model::SyndicDraftMarkerId,
    pub(super) label: beryl_model::ImageLabelOrdinal,
}
impl DraftReplacementImageSourceV1 {
    pub const fn marker_id(&self) -> beryl_model::SyndicDraftMarkerId {
        self.marker_id
    }
    pub const fn label(&self) -> beryl_model::ImageLabelOrdinal {
        self.label
    }
    pub fn marker_ordinal(&self) -> u64 {
        let DraftPieceBuildFrontierV1::SealedImport(f) = self.build.frontier() else {
            unreachable!()
        };
        f.markers + 1
    }
    pub fn descriptive_page(
        &self,
        asset: beryl_model::AssetId,
    ) -> Option<beryl_model::HistoricalInputMarkerPageV1> {
        let DraftPieceBuildFrontierV1::SealedImport(f) = self.build.frontier() else {
            return None;
        };
        beryl_model::HistoricalInputMarkerPageV1::new(
            self.build.draft_id(),
            *self.build.session_id().as_bytes(),
            *self.build.operation_id().as_bytes(),
            self.build.progress_receipt().key().transition_ordinal(),
            *self.build.progress_receipt().digest().as_bytes(),
            self.build.canonical_header().to_vec(),
            f.basis.item_id,
            f.basis.assets?,
            f.encoded_cursor,
            f.next_piece,
            beryl_model::HistoricalInputMarkerAssociationV1::new(
                f.markers.checked_add(1)?,
                self.marker_id,
                self.label,
                asset,
            )?,
        )
    }
}

pub struct PreparedDraftReplacementAdvanceV1 {
    pub(super) scope: ImportCommandScope,
    pub(super) home_generation: beryl_home_store::HomeGeneration,
    pub(super) source: Box<DraftPieceBuildRecordV1>,
    pub(super) source_session: Box<DraftEditorCandidateSessionV1>,
    pub(super) target: Box<DraftPieceBuildRecordV1>,
    pub(super) receipt: Box<DraftPieceBuildProgressReceiptV1>,
    pub(super) target_session: Box<DraftEditorCandidateSessionV1>,
    pub(super) quantum: Box<DraftPieceTreeQuantumV1>,
}

impl SyndicStorage {
    pub fn prepare_replacement_import_step(
        &self,
        store: &HomeStore,
        prepared: &PreparedDraftReplacementImportV1,
    ) -> Result<DraftReplacementAdvancePreparationV1, DraftReplacementErrorV1> {
        prepared.scope.require_current(store)?;
        if prepared.home_generation != self.home_generation
            || store.health().generation() != Some(self.home_generation)
        {
            return Err(DraftReplacementErrorV1::Unavailable);
        }
        let r = StoreSourceReader {
            storage: self,
            store,
        };
        let key = DraftPieceSettlementKeyV1::new(
            prepared.basis.selector.draft_id(),
            prepared.session_id(),
            prepared.operation_id(),
        );
        let build = r
            .point::<DraftPieceBuildsFamily>(&key)?
            .ok_or(DraftReplacementErrorV1::Conflict)?;
        if build.canonical_header() != prepared.canonical_header || !import_build_is_exact(&build) {
            return Err(DraftReplacementErrorV1::Conflict);
        }
        let DraftEditorCandidateSessionRecordV1::Head(session) = r
            .point::<DraftEditorCandidateSessionsFamily>(
                &DraftEditorCandidateSessionRecordKeyV1::head(build.draft_id(), build.session_id()),
            )?
            .ok_or(DraftReplacementErrorV1::Conflict)?
        else {
            return Err(DraftReplacementErrorV1::Conflict);
        };
        let receipt = r
            .point::<DraftPieceBuildProgressFamily>(&build.progress_receipt().key())?
            .ok_or(DraftReplacementErrorV1::Conflict)?;
        if session.draft_id() != build.draft_id()
            || session.session_id() != build.session_id()
            || session.thread_id() != prepared.basis.selector.thread_id()
            || !progress_receipt_matches_build(&receipt, &build)
            || !session::progress_receipt_closure_is_exact(self, store, &receipt)?
            || session.active_operation() != Some(&custody(&build))
        {
            return Err(DraftReplacementErrorV1::Conflict);
        }
        let DraftPieceBuildFrontierV1::SealedImport(f) = build.frontier() else {
            return Err(DraftReplacementErrorV1::Conflict);
        };
        if build.lifecycle() == DraftPieceBuildLifecycleV1::Complete {
            return Ok(DraftReplacementAdvancePreparationV1::Complete(
                build.successor().ok_or(DraftReplacementErrorV1::Conflict)?,
            ));
        }
        if build.lifecycle() != DraftPieceBuildLifecycleV1::Open {
            return Err(DraftReplacementErrorV1::Unavailable);
        }
        if f.phase == DraftSealedImportPhaseV1::Streaming
            && f.decoder == DraftSealedImportDecoderV1::BetweenAtoms
            && f.atoms < f.basis.content.summary().atom_count()
            && super::read::encoded_range(&r, f.basis.content, f.encoded_cursor, 1)? == [1]
        {
            let (marker_id, label) = marker_frame(&r, f)?;
            return Ok(DraftReplacementAdvancePreparationV1::Image(
                DraftReplacementImageSourceV1 {
                    prepared: prepared.clone(),
                    build,
                    session,
                    marker_id,
                    label,
                },
            ));
        }
        Ok(DraftReplacementAdvancePreparationV1::Ready(prepare_step(
            self, store, build, session, None,
        )?))
    }

    pub fn advance_replacement_draft_import(
        &self,
        revision: beryl_model::DomainRevision,
        prepared: PreparedDraftReplacementAdvanceV1,
    ) -> MutationContribution {
        let generation_matches = prepared.home_generation == self.home_generation;
        prepared.scope.handle.clone().contribution(
            revision,
            AdvanceImport {
                prepared,
                generation_matches,
            },
        )
    }
}

pub(super) fn prepare_step(
    storage: &SyndicStorage,
    store: &HomeStore,
    build: DraftPieceBuildRecordV1,
    session: DraftEditorCandidateSessionV1,
    association: Option<beryl_model::HistoricalInputMarkerAssociationV1>,
) -> Result<PreparedDraftReplacementAdvanceV1, DraftReplacementErrorV1> {
    let DraftPieceBuildFrontierV1::SealedImport(f) = build.frontier() else {
        return Err(DraftReplacementErrorV1::Conflict);
    };
    let quantum = match f.phase {
        DraftSealedImportPhaseV1::Streaming => {
            let step = next_stream_step(&StoreSourceReader { storage, store }, f, association)?;
            append_import_leaf(storage, store, &build, step.frontier, step.leaf)?
        }
        DraftSealedImportPhaseV1::CrossValidating => {
            cross_validate_import_leaf(storage, store, &build, f)?
        }
        DraftSealedImportPhaseV1::Complete => return Err(DraftReplacementErrorV1::Conflict),
    };
    let lifecycle = if quantum.successor.is_some() {
        DraftPieceBuildLifecycleV1::Complete
    } else {
        DraftPieceBuildLifecycleV1::Open
    };
    let target = DraftPieceBuildRecordV1::new(
        build.draft_id(),
        build.session_id(),
        build.predecessor_candidate_generation(),
        build.predecessor_root(),
        build.predecessor_history(),
        build.operation_id(),
        build.predecessor_caret(),
        build.predecessor_selection(),
        build.caret(),
        build.selection(),
        0,
        build.fragment_chain(),
        build.canonical_header().to_vec(),
        0,
        build.staged_fragment_chain(),
        build.proposal_digest(),
        quantum.roots,
        quantum.base_frontier,
        quantum.successor_frontier,
        quantum.next_record_ordinal,
        quantum.frontier,
        build.progress_receipt(),
        quantum.successor.as_ref().map(|r| r.reference()),
        quantum.build_digest,
        lifecycle,
    );
    let (target, receipt) =
        authenticated_build_transition(target, Some(build.progress_receipt()), None)
            .map_err(|()| DraftReplacementErrorV1::Conflict)?;
    let target_session = session
        .with_active_operation(custody(&target))
        .ok_or(DraftReplacementErrorV1::Conflict)?;
    Ok(PreparedDraftReplacementAdvanceV1 {
        scope: ImportCommandScope::capture(storage, store)?,
        home_generation: storage.home_generation,
        source: Box::new(build),
        source_session: Box::new(session),
        target: Box::new(target),
        receipt: Box::new(receipt),
        target_session: Box::new(target_session),
        quantum: Box::new(quantum),
    })
}

struct AdvanceImport {
    prepared: PreparedDraftReplacementAdvanceV1,
    generation_matches: bool,
}
impl DomainMutation<SyndicDomain> for AdvanceImport {
    type Error = DraftReplacementErrorV1;
    type Prepared = Option<PreparedDraftReplacementAdvanceV1>;
    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        if !self.generation_matches {
            return Err(DraftReplacementErrorV1::Proof);
        }
        let p = self.prepared;
        let key = DraftPieceSettlementKeyV1::new(
            p.source.draft_id(),
            p.source.session_id(),
            p.source.operation_id(),
        );
        let actual = required::<DraftPieceBuildsFamily>(reader, &key)?;
        if actual == *p.target {
            authenticate_transition(reader, &p.target, &p.receipt, &p.target_session)?;
            authenticate_quantum(reader, &p.quantum)?;
            return Ok(None);
        }
        if actual != *p.source
            || required::<DraftEditorCandidateSessionsFamily>(reader, &head_key(&p.source_session))?
                != DraftEditorCandidateSessionRecordV1::Head(*p.source_session.clone())
            || super::read::SourceReader::point::<DraftPieceBuildProgressFamily>(
                reader,
                &p.receipt.key(),
            )?
            .is_some()
        {
            return Err(DraftReplacementErrorV1::Conflict);
        }
        let DraftPieceBuildFrontierV1::SealedImport(f) = p.source.frontier() else {
            return Err(DraftReplacementErrorV1::Conflict);
        };
        authenticate_source(reader, f.basis)?;
        let prior =
            required::<DraftPieceBuildProgressFamily>(reader, &p.source.progress_receipt().key())?;
        if !progress_receipt_matches_build(&prior, &p.source)
            || !import_transition_is_exact(&prior, &p.receipt)
        {
            return Err(DraftReplacementErrorV1::Conflict);
        }
        mutation::authenticate_progress_receipt(reader, &prior)?;
        require_absent_quantum(reader, &p.quantum)?;
        Ok(Some(p))
    }
    fn reserve_reconciliation(
        &self,
        r: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        r.reserve_records::<DraftPieceBuildsCodec>(1)?;
        r.reserve_records::<DraftPieceBuildProgressCodec>(1)?;
        r.reserve_records::<DraftEditorCandidateSessionsCodec>(1)?;
        let q = &self.prepared.quantum;
        if !q.leaves.is_empty() {
            r.reserve_records::<DraftPieceLeavesCodec>(q.leaves.len())?;
        }
        if !q.nodes.is_empty() {
            r.reserve_records::<DraftPieceNodesCodec>(q.nodes.len())?;
        }
        if !q.index_records.is_empty() {
            r.reserve_records::<DraftMarkerIdentityIndexCodec>(q.index_records.len())?;
        }
        if !q.marker_order_records.is_empty() {
            r.reserve_records::<DraftMarkerOrderCommitmentsCodec>(q.marker_order_records.len())?;
        }
        if q.successor.is_some() {
            r.reserve_records::<DraftPieceRootsCodec>(1)?;
        }
        Ok(())
    }
    fn contribute(
        prepared: Self::Prepared,
        w: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        if let Some(p) = prepared {
            let q = &p.quantum;
            for leaf in &q.leaves {
                w.put::<DraftPieceLeavesCodec>(&leaf.key(), leaf)?;
            }
            for node in &q.nodes {
                w.put::<DraftPieceNodesCodec>(&node.key(), node)?;
            }
            for index in &q.index_records {
                w.put::<DraftMarkerIdentityIndexCodec>(&index.key(), index)?;
            }
            for order in &q.marker_order_records {
                w.put::<DraftMarkerOrderCommitmentsCodec>(&order.key(), order)?;
            }
            if let Some(root) = &q.successor {
                w.put::<DraftPieceRootsCodec>(&root.reference().key(), root)?;
            }
            put_transition(w, &p.target, &p.receipt, &p.target_session)?;
        }
        Ok(())
    }
}

fn authenticate_quantum(
    r: &DomainReader<'_, SyndicDomain>,
    q: &DraftPieceTreeQuantumV1,
) -> Result<(), DraftReplacementErrorV1> {
    for leaf in &q.leaves {
        if required::<DraftPieceLeavesFamily>(r, &leaf.key())? != *leaf {
            return Err(DraftReplacementErrorV1::Conflict);
        }
    }
    for node in &q.nodes {
        if required::<DraftPieceNodesFamily>(r, &node.key())? != *node {
            return Err(DraftReplacementErrorV1::Conflict);
        }
    }
    for index in &q.index_records {
        if required::<DraftMarkerIdentityIndexFamily>(r, &index.key())? != *index {
            return Err(DraftReplacementErrorV1::Conflict);
        }
    }
    for order in &q.marker_order_records {
        if required::<DraftMarkerOrderCommitmentsFamily>(r, &order.key())? != *order {
            return Err(DraftReplacementErrorV1::Conflict);
        }
    }
    if let Some(root) = &q.successor {
        if required::<DraftPieceRootsFamily>(r, &root.reference().key())? != *root {
            return Err(DraftReplacementErrorV1::Conflict);
        }
    }
    Ok(())
}
fn require_absent_quantum(
    r: &DomainReader<'_, SyndicDomain>,
    q: &DraftPieceTreeQuantumV1,
) -> Result<(), DraftReplacementErrorV1> {
    macro_rules! absent {
        ($family:ty,$key:expr) => {
            if SourceReader::point::<$family>(r, &$key)?.is_some() {
                return Err(DraftReplacementErrorV1::Conflict);
            }
        };
    }
    for leaf in &q.leaves {
        absent!(DraftPieceLeavesFamily, leaf.key());
    }
    for node in &q.nodes {
        absent!(DraftPieceNodesFamily, node.key());
    }
    for index in &q.index_records {
        absent!(DraftMarkerIdentityIndexFamily, index.key());
    }
    for order in &q.marker_order_records {
        absent!(DraftMarkerOrderCommitmentsFamily, order.key());
    }
    if let Some(root) = &q.successor {
        absent!(DraftPieceRootsFamily, root.reference().key());
    }
    Ok(())
}
