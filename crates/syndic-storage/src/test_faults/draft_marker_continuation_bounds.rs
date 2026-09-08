#[path = "draft_marker_continuation_bounds/records.rs"]
mod records;
pub(crate) use records::put_marker_bounds_fixture_record;

#[derive(Clone, Debug)]
pub struct MarkerRemovalBoundsForTest {
    pub complete_closure: bool,
    pub input_height: u8,
    pub output_heights: [u8; 3],
    pub input_population: u64,
    pub output_populations: [u64; 3],
    pub acquired: [u64; 3],
    pub emitted: [usize; 3],
    pub emitted_internal_children: [Vec<usize>; 3],
    pub work: [crate::DraftPieceBuildWorkV1; 3],
    pub measured_max_internal_bytes: [u64; 3],
}

pub fn marker_removal_bounds_fixture(
    storage: &crate::SyndicStorage,
    store: &beryl_home_store::HomeStore,
    draft_id: beryl_model::SyndicDraftId,
    prototype: crate::DraftPieceMarkerV1,
    height: u8,
    sibling_fanout: usize,
    complete_closure: bool,
) -> MarkerRemovalBoundsForTest {
    crate::draft_piece::run_marker_removal_bounds_fixture(
        storage,
        store,
        draft_id,
        prototype,
        height,
        sibling_fanout,
        complete_closure,
    )
}

pub fn marker_constructor_reservation_fixture(
    storage: &crate::SyndicStorage,
    store: &beryl_home_store::HomeStore,
    draft_id: beryl_model::SyndicDraftId,
    prototype: crate::DraftPieceMarkerV1,
) -> [bool; 6] {
    crate::draft_piece::run_marker_constructor_reservation_fixture(
        storage, store, draft_id, prototype,
    )
}

pub fn marker_writer_admission_snapshot_for_test(
    build: &crate::DraftPieceBuildRecordV1,
) -> Option<crate::DraftMarkerWriterAdmissionV1> {
    build.writer_admission()
}

#[derive(Clone, Debug)]
pub struct MarkerLocatorBoundsForTest {
    pub exact_rank_and_ordinal: Option<(u64, u64)>,
    pub insertion_rank_and_ordinal: Option<(u64, u64)>,
    pub inserted_population: Option<u64>,
    pub removed_population: Option<u64>,
    pub work: [crate::DraftPieceBuildWorkV1; 2],
}

pub fn marker_locator_bounds_fixture(
    storage: &crate::SyndicStorage,
    store: &beryl_home_store::HomeStore,
    draft_id: beryl_model::SyndicDraftId,
    prototype: crate::DraftPieceMarkerV1,
) -> MarkerLocatorBoundsForTest {
    crate::draft_piece::run_marker_locator_fixture(storage, store, draft_id, prototype)
}
