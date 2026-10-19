use crate::{SyndicMutationError, SyndicStorage, domain::SyndicDomain, draft_piece::*};
use beryl_home_store::{
    CommandOutcome, DomainMutation, DomainReader, HomeCommand, HomeStore, MutationBuilder,
    ReconciliationReservation,
};
pub fn sealed_import_eof_corruption_rejected_for_test(
    store: &HomeStore,
    storage: &SyndicStorage,
    key: DraftPieceSettlementKeyV1,
    ordered: bool,
) -> bool {
    crate::draft_piece::check_sealed_import_eof_corruption_for_test(store, storage, key, ordered)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SealedImportCheckpointForTest {
    pub build: DraftPieceBuildRecordV1,
    pub decoder: DraftSealedImportDecoderV1,
    pub next_leaf_rank: u64,
    pub checked_markers: u64,
    pub sequential_digest: [u8; 32],
    pub ordered_digest: [u8; 32],
    pub maximum_label: Option<beryl_model::ImageLabelOrdinal>,
}
pub fn sealed_import_checkpoint_for_test(
    store: &HomeStore,
    storage: &SyndicStorage,
    key: DraftPieceSettlementKeyV1,
) -> SealedImportCheckpointForTest {
    let build = storage
        .point::<DraftPieceBuildsFamily>(store, key, point_limit())
        .unwrap()
        .unwrap();
    let DraftPieceBuildFrontierV1::SealedImport(f) = build.frontier() else {
        panic!("fixture needs an import")
    };
    SealedImportCheckpointForTest {
        build,
        decoder: f.decoder,
        next_leaf_rank: f.next_leaf_rank,
        checked_markers: f.checked_markers,
        sequential_digest: f.sequential_digest,
        ordered_digest: f.ordered_digest,
        maximum_label: f.maximum_label,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SealedImportSubstitutionResultForTest {
    pub coherent_roots: DraftPieceBuildRootsV1,
    pub checked_leaf_count: u64,
    pub checked_marker_count: u64,
    pub rejected_completion: bool,
}
pub fn sealed_import_coherent_marker_substitution_for_test(
    store: &HomeStore,
    storage: &SyndicStorage,
    key: DraftPieceSettlementKeyV1,
    markers: &[DraftPieceMarkerV1],
) -> SealedImportSubstitutionResultForTest {
    assert!(!markers.is_empty() && markers.len() <= 64);
    let source = sealed_import_checkpoint_for_test(store, storage, key).build;
    let DraftPieceBuildFrontierV1::SealedImport(f) = source.frontier() else {
        unreachable!()
    };
    assert_eq!(f.phase, DraftSealedImportPhaseV1::CrossValidating);
    assert_eq!(f.logical_bytes, 0);
    assert_eq!(f.next_leaf_rank, 0);
    assert_eq!(f.checked_markers, 0);
    assert_eq!(markers.len() as u64, f.markers);
    let mut trial = copy(
        &source,
        DraftPieceBuildRootsV1::from_root(source.predecessor_root()),
        source.next_record_ordinal().checked_add(10000).unwrap(),
        f,
    );
    for marker in markers {
        let q = append_import_leaf(
            storage,
            store,
            &trial,
            DraftPieceBuildFrontierV1::SealedImport(f),
            Some(DraftPieceLeafValueV1::Marker(*marker)),
        )
        .unwrap();
        persist_quantum(store, storage, q.clone());
        trial = copy(&source, q.roots, q.next_record_ordinal, f);
    }
    let roots = trial.working_roots();
    assert!(draft_piece_build_roots_are_locally_exact_v1(roots));
    assert_eq!(roots.sequence_summary().marker_count(), f.markers);
    assert_eq!(roots.marker_index_summary().record_count(), f.markers);
    assert_eq!(roots.marker_commitment().marker_count(), f.markers);
    assert_eq!(
        roots.marker_commitment().maximum_image_label(),
        source
            .working_roots()
            .marker_commitment()
            .maximum_image_label()
    );
    assert!(import_build_is_exact(&trial));
    let mut frontier = f;
    for _ in 0..markers.len() {
        let q = cross_validate_import_leaf(storage, store, &trial, frontier)
            .expect("all three rewritten trees must match per marker");
        let DraftPieceBuildFrontierV1::SealedImport(next) = q.frontier else {
            unreachable!()
        };
        frontier = next;
        trial = copy(&source, roots, q.next_record_ordinal, frontier);
    }
    let rejected_completion = cross_validate_import_leaf(storage, store, &trial, frontier).is_err();
    SealedImportSubstitutionResultForTest {
        coherent_roots: roots,
        checked_leaf_count: frontier.next_leaf_rank,
        checked_marker_count: frontier.checked_markers,
        rejected_completion,
    }
}
fn copy(
    b: &DraftPieceBuildRecordV1,
    roots: DraftPieceBuildRootsV1,
    ordinal: u64,
    f: DraftSealedImportFrontierV1,
) -> DraftPieceBuildRecordV1 {
    DraftPieceBuildRecordV1::new(
        b.draft_id(),
        b.session_id(),
        b.predecessor_candidate_generation(),
        b.predecessor_root(),
        b.predecessor_history(),
        b.operation_id(),
        b.predecessor_caret(),
        b.predecessor_selection(),
        b.caret(),
        b.selection(),
        b.fragment_count(),
        b.fragment_chain(),
        b.canonical_header().to_vec(),
        b.staged_fragment_count(),
        b.staged_fragment_chain(),
        b.proposal_digest(),
        roots,
        DraftPieceBuildBoundaryV1::new(0, 0),
        DraftPieceBuildBoundaryV1::new(roots.sequence_summary().piece_count(), 0),
        ordinal,
        DraftPieceBuildFrontierV1::SealedImport(f),
        b.progress_receipt(),
        None,
        None,
        DraftPieceBuildLifecycleV1::Open,
    )
}
fn persist_quantum(store: &HomeStore, storage: &SyndicStorage, q: DraftPieceTreeQuantumV1) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(
            storage
                .handle
                .contribution(storage.revision(store).unwrap(), TreeRecords(q)),
        )
        .unwrap();
    let outcome = store.execute(command);
    assert!(
        matches!(
            outcome,
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ),
        "{outcome:?}"
    );
}
struct TreeRecords(DraftPieceTreeQuantumV1);
impl DomainMutation<SyndicDomain> for TreeRecords {
    type Error = SyndicMutationError;
    type Prepared = DraftPieceTreeQuantumV1;
    fn prepare(self, _: &DomainReader<'_, SyndicDomain>) -> Result<Self::Prepared, Self::Error> {
        Ok(self.0)
    }
    fn reserve_reconciliation(
        &self,
        r: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        if !self.0.leaves.is_empty() {
            r.reserve_records::<DraftPieceLeavesCodec>(self.0.leaves.len())?;
        }
        if !self.0.nodes.is_empty() {
            r.reserve_records::<DraftPieceNodesCodec>(self.0.nodes.len())?;
        }
        if !self.0.index_records.is_empty() {
            r.reserve_records::<DraftMarkerIdentityIndexCodec>(self.0.index_records.len())?;
        }
        if !self.0.marker_order_records.is_empty() {
            r.reserve_records::<DraftMarkerOrderCommitmentsCodec>(
                self.0.marker_order_records.len(),
            )?;
        }
        Ok(())
    }
    fn contribute(
        q: Self::Prepared,
        w: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        for v in q.leaves {
            w.put::<DraftPieceLeavesCodec>(&v.key(), &v)?;
        }
        for v in q.nodes {
            w.put::<DraftPieceNodesCodec>(&v.key(), &v)?;
        }
        for v in q.index_records {
            w.put::<DraftMarkerIdentityIndexCodec>(&v.key(), &v)?;
        }
        for v in q.marker_order_records {
            w.put::<DraftMarkerOrderCommitmentsCodec>(&v.key(), &v)?;
        }
        Ok(())
    }
}
