use super::super::*;
use super::{
    read::{SourceReader, StoreSourceReader},
    stream::next_stream_step,
};
use beryl_home_store::HomeStore;

pub(crate) fn check_sealed_import_eof_corruption_for_test(
    store: &HomeStore,
    storage: &crate::SyndicStorage,
    key: DraftPieceSettlementKeyV1,
    ordered: bool,
) -> bool {
    let reader = StoreSourceReader { storage, store };
    let build = reader
        .point::<DraftPieceBuildsFamily>(&key)
        .unwrap()
        .unwrap();
    let DraftPieceBuildFrontierV1::SealedImport(mut frontier) = build.frontier() else {
        panic!("source fixture needs import")
    };
    assert_eq!(frontier.phase, DraftSealedImportPhaseV1::Streaming);
    assert_eq!(frontier.decoder, DraftSealedImportDecoderV1::BetweenAtoms);
    assert_eq!(
        frontier.atoms,
        frontier.basis.content.summary().atom_count()
    );
    assert_eq!(
        frontier.markers,
        frontier.basis.content.summary().image_marker_count()
    );
    let genuine = next_stream_step(&reader, frontier, None).unwrap();
    assert_eq!(
        genuine.frontier.phase,
        DraftSealedImportPhaseV1::CrossValidating
    );
    if ordered {
        frontier.ordered_digest[0] ^= 0x80;
    } else {
        frontier.sequential_digest[0] ^= 0x80;
    }
    assert!(frontier.is_locally_exact(DraftPieceBuildLifecycleV1::Open));
    next_stream_step(&reader, frontier, None).is_err()
}
