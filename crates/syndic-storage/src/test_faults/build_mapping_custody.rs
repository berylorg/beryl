use crate::draft_piece::{
    DraftPieceBuildMappingV1, DraftPieceBuildRecordV1, DraftPieceMappingStageV1,
    build_mapping::model::MapRoot,
};
use crate::{
    SyndicMutationError, SyndicStorage,
    codec::Family,
    domain::SyndicDomain,
    draft_piece::{
        DraftPieceBuildProgressFamily,
        build_mapping::{DraftPieceBuildMappingCodec, DraftPieceBuildMappingFamily},
    },
};
use beryl_home_store::{
    CommandOutcome, DomainMutation, DomainReader, HomeCommand, HomeStore, MutationBuilder,
    ReconciliationReservation,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftBuildMappingSnapshotForTest {
    pub stage_tag: u8,
    pub current_source_units: u128,
    pub current_target_units: u128,
    pub pending_source_units: Option<u128>,
    pub pending_target_units: Option<u128>,
    pub completed_source_unit: u128,
    pub fragment_source_end_unit: Option<u128>,
    pub ready_kind: Option<u8>,
}

fn pending_root(mapping: DraftPieceBuildMappingV1) -> Option<MapRoot> {
    match mapping.mapping_stage {
        DraftPieceMappingStageV1::DeleteMap { target, .. }
        | DraftPieceMappingStageV1::MapComplete { target, .. } => Some(target),
        DraftPieceMappingStageV1::Ready(ready) => Some(ready.target),
        _ => None,
    }
}

pub fn draft_build_mapping_snapshot(
    build: &DraftPieceBuildRecordV1,
) -> Option<DraftBuildMappingSnapshotForTest> {
    let mapping = build.mapping()?;
    use DraftPieceMappingStageV1 as Stage;
    let stage_tag = match mapping.mapping_stage {
        Stage::Idle => 0,
        Stage::TextSourceStart { .. } => 1,
        Stage::TextSourceEnd { .. } => 2,
        Stage::TextPreviousStart { .. } => 3,
        Stage::TextPreviousEnd { .. } => 4,
        Stage::TextMapStart { .. } => 5,
        Stage::TextMapEnd { .. } => 6,
        Stage::TextResolveStart { .. } => 7,
        Stage::TextResolveEnd { .. } => 8,
        Stage::MarkerSource { .. } => 9,
        Stage::MarkerMapRemoval { .. } => 10,
        Stage::MarkerResolveRemoval { .. } => 11,
        Stage::MarkerWorkingIdentity { .. } => 12,
        Stage::MarkerMapBoundary => 13,
        Stage::MarkerResolveBoundary { .. } => 14,
        Stage::MarkerPlanningReady { .. } => 15,
        Stage::TextDeleteProof => 16,
        Stage::TextInsertProof => 17,
        Stage::DeleteMap { .. } => 18,
        Stage::InsertMap { .. } => 19,
        Stage::MapComplete { .. } => 20,
        Stage::Ready(_) => 21,
        Stage::RefreshMap => 22,
        Stage::RefreshSequence { .. } => 23,
        Stage::PublishReady { .. } => 24,
    };
    let pending = pending_root(mapping).map(MapRoot::measure);
    Some(DraftBuildMappingSnapshotForTest {
        stage_tag,
        current_source_units: mapping.current_map.measure().source,
        current_target_units: mapping.current_map.measure().target,
        pending_source_units: pending.map(|measure| measure.source),
        pending_target_units: pending.map(|measure| measure.target),
        completed_source_unit: mapping.completed_source_unit,
        fragment_source_end_unit: mapping.fragment_source_end_unit,
        ready_kind: match mapping.mapping_stage {
            Stage::Ready(ready) => Some(ready.kind as u8),
            _ => None,
        },
    })
}

#[derive(Clone, Copy, Debug)]
pub enum DraftBuildMappingRootForTest {
    Current,
    Pending,
    PreviousCurrent,
    PreviousPending,
}

pub fn draft_build_mapping_root_key_for_test(
    storage: &SyndicStorage,
    store: &HomeStore,
    build: &DraftPieceBuildRecordV1,
    selected: DraftBuildMappingRootForTest,
) -> Option<[u8; 64]> {
    let mapping = match selected {
        DraftBuildMappingRootForTest::Current | DraftBuildMappingRootForTest::Pending => {
            build.mapping()?
        }
        DraftBuildMappingRootForTest::PreviousCurrent
        | DraftBuildMappingRootForTest::PreviousPending => {
            let receipt = storage
                .point::<DraftPieceBuildProgressFamily>(
                    store,
                    build.progress_receipt().key(),
                    crate::draft_piece::point_limit(),
                )
                .unwrap()
                .unwrap();
            storage
                .point::<DraftPieceBuildProgressFamily>(
                    store,
                    receipt.previous()?.key(),
                    crate::draft_piece::point_limit(),
                )
                .unwrap()
                .unwrap()
                .mapping()?
        }
    };
    let root =
        match selected {
            DraftBuildMappingRootForTest::Current
            | DraftBuildMappingRootForTest::PreviousCurrent => mapping.current_map,
            DraftBuildMappingRootForTest::Pending
            | DraftBuildMappingRootForTest::PreviousPending => pending_root(mapping)?,
        };
    let MapRoot::Stored(descriptor) = root else {
        return None;
    };
    let mut key = [0; 64];
    key[..16].copy_from_slice(build.draft_id().as_bytes());
    key[16..32].copy_from_slice(build.session_id().as_bytes());
    key[32..48].copy_from_slice(build.operation_id().as_bytes());
    key[48..].copy_from_slice(&descriptor.id);
    Some(key)
}

pub fn draft_build_mapping_record_for_test(
    storage: &SyndicStorage,
    store: &HomeStore,
    key: [u8; 64],
) -> Option<Vec<u8>> {
    storage
        .point::<DraftPieceBuildMappingFamily>(store, key, crate::draft_piece::point_limit())
        .unwrap()
        .map(|node| DraftPieceBuildMappingFamily::encode_value(&node).unwrap())
}

pub fn delete_draft_build_mapping_record_for_test(
    storage: &SyndicStorage,
    store: &HomeStore,
    key: [u8; 64],
) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(
            storage
                .handle
                .contribution(storage.revision(store).unwrap(), DeleteMappingRecord(key)),
        )
        .unwrap();
    assert!(matches!(
        store.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
}

pub fn substitute_draft_build_mapping_record_for_test(
    storage: &SyndicStorage,
    store: &HomeStore,
    key: [u8; 64],
) {
    let mut node = storage
        .point::<DraftPieceBuildMappingFamily>(store, key, crate::draft_piece::point_limit())
        .unwrap()
        .unwrap();
    node.key[63] ^= 1;
    super::put_mapping_fixture_record::<DraftPieceBuildMappingFamily>(storage, store, &key, &node);
}

pub fn corrupt_draft_build_mapping_record_for_test(
    storage: &SyndicStorage,
    store: &HomeStore,
    key: [u8; 64],
) {
    use beryl_home_store::RecordCodec;
    let node = storage
        .point::<DraftPieceBuildMappingFamily>(store, key, crate::draft_piece::point_limit())
        .unwrap()
        .unwrap();
    let mut stored = DraftPieceBuildMappingFamily::RECORD_VERSION
        .get()
        .to_be_bytes()
        .to_vec();
    stored.extend_from_slice(&DraftPieceBuildMappingFamily::encode_value(&node).unwrap());
    *stored.last_mut().unwrap() ^= 1;
    let encoded_key =
        <DraftPieceBuildMappingCodec as RecordCodec<SyndicDomain>>::encode_key(&key).unwrap();
    store
        .inject_persisted_corrupt_record::<SyndicDomain, DraftPieceBuildMappingCodec>(
            &storage.handle,
            &encoded_key,
            &stored,
        )
        .unwrap();
}

pub fn draft_build_mapping_candidate_pair_for_test(
    storage: &SyndicStorage,
    store: &HomeStore,
    session: &crate::DraftEditorCandidateSessionV1,
) -> (
    crate::DraftPieceRootReferenceV1,
    crate::DraftEditHistoryFrontierReferenceV1,
) {
    use crate::draft_piece::{
        DraftEditorCandidateSessionRecordKeyV1, DraftEditorCandidateSessionRecordV1,
        DraftEditorCandidateSessionsFamily,
    };
    let key =
        DraftEditorCandidateSessionRecordKeyV1::head(session.draft_id(), session.session_id());
    let DraftEditorCandidateSessionRecordV1::Head(head) = storage
        .point::<DraftEditorCandidateSessionsFamily>(store, key, crate::draft_piece::point_limit())
        .unwrap()
        .unwrap()
    else {
        panic!("candidate head record changed kind");
    };
    assert_eq!(head.draft_id(), session.draft_id());
    assert_eq!(head.session_id(), session.session_id());
    (head.newest_root(), head.newest_history())
}

struct DeleteMappingRecord([u8; 64]);

impl DomainMutation<SyndicDomain> for DeleteMappingRecord {
    type Error = SyndicMutationError;
    type Prepared = Self;

    fn prepare(self, _: &DomainReader<'_, SyndicDomain>) -> Result<Self, Self::Error> {
        Ok(self)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DraftPieceBuildMappingCodec>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        mutations.delete::<DraftPieceBuildMappingCodec>(&prepared.0)?;
        Ok(())
    }
}
