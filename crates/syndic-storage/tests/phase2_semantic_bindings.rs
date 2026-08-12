#![cfg(feature = "test-faults")]

mod support;

use beryl_model::{BindingRevision, CasThreadId, ProjectionRevision, SyndicTurnId, ThreadRevision};
use syndic_storage::test_faults::{
    FixtureBatch, FixtureDelete, FixtureRecord, fixture_transcript_digest_seed,
};
use syndic_storage::*;

use support::populated::active_snapshot;
use support::semantic::{exercise_case, exercise_seeded_populated_case};
use support::*;

#[test]
fn transcript_binding_reservation_and_active_snapshot_corruption_fail_closed() {
    exercise_case(
        "transcript-frontier",
        "current transcript head is not a complete visible projection",
        seed_selected_turn,
        || {
            let turn = SyndicTurnId::from_bytes([3; 16]);
            let digest = root_turn_chain_digest(turn);
            batch([
                FixtureRecord::TranscriptViewHead(TranscriptViewHeadRecord::new(
                    id(1),
                    TranscriptGeneration::FIRST,
                    ProjectionRevision::new(1).unwrap(),
                    1,
                    Some(turn),
                    digest,
                    ProjectionLifecycle::Current,
                )),
                FixtureRecord::TranscriptBuild(TranscriptBuildRecord::new(
                    id(1),
                    TranscriptGeneration::FIRST,
                    ProjectionRevision::new(1).unwrap(),
                    ThreadRevision::new(1).unwrap(),
                    Some(turn),
                    digest,
                    1,
                    1,
                    fixture_transcript_digest_seed(),
                    true,
                    TranscriptBuildPhase::Complete,
                )),
            ])
        },
    );

    exercise_case(
        "binding-head-gap",
        "binding head record is missing",
        base,
        || {
            batch([FixtureRecord::BindingHead(BindingHeadRecord::new(
                id(1),
                BindingRevision::new(2).unwrap(),
                BindingLifecycle::Unbound,
                empty_selected_path_digest(),
            ))])
        },
    );

    exercise_case(
        "history-completeness",
        "history summary completeness derivation disagrees",
        seed_selected_turn,
        || {
            let turn = SyndicTurnId::from_bytes([3; 16]);
            batch([FixtureRecord::HistorySummary(HistorySummaryRecord::new(
                id(1),
                ProjectionRevision::new(2).unwrap(),
                ThreadRevision::new(1).unwrap(),
                Some(turn),
                root_turn_chain_digest(turn),
                false,
                timestamp(2),
            ))])
        },
    );
    exercise_case(
        "history-last-activity",
        "history summary last-activity derivation disagrees",
        seed_selected_turn,
        || {
            let turn = SyndicTurnId::from_bytes([3; 16]);
            batch([FixtureRecord::HistorySummary(HistorySummaryRecord::new(
                id(1),
                ProjectionRevision::new(2).unwrap(),
                ThreadRevision::new(1).unwrap(),
                Some(turn),
                root_turn_chain_digest(turn),
                true,
                timestamp(1),
            ))])
        },
    );

    exercise_case(
        "cas-thread-owner-collision",
        "CAS thread reservation owner or revision range disagrees",
        two_threads,
        duplicate_cas_binding,
    );
    exercise_seeded_populated_case(
        "active-missing-snapshot",
        "active binding snapshot is missing",
        |_store, _storage| {
            let mut batch = FixtureBatch::new();
            batch
                .delete(FixtureDelete::ExecutionSnapshot(active_snapshot()))
                .unwrap();
            batch
        },
    );
}

fn base() -> FixtureBatch {
    batch(empty_thread_records(id(1), draft_id(2)))
}

fn seed_selected_turn() -> FixtureBatch {
    let turn = SyndicTurnId::from_bytes([3; 16]);
    let digest = root_turn_chain_digest(turn);
    let mut records =
        thread_records_with_activity(id(1), draft_id(2), Some(turn), digest, timestamp(2));
    records.push(FixtureRecord::Turn(TurnRecord::new(
        turn,
        id(1),
        TurnKind::OrdinaryUser,
        ConversationParent::Root,
        None,
        TurnDepth::FIRST,
        digest,
        timestamp(2),
    )));
    records.push(FixtureRecord::TurnState(fixture_turn_state(
        turn,
        TurnStateRevision::FIRST,
        TurnLifecycle::Interrupted,
        1,
        0,
        timestamp(2),
    )));
    records.push(FixtureRecord::SourceEvent(
        SourceEventRecord::new(
            turn,
            SourceEventSequence::FIRST,
            None,
            SourceEventPayload::TurnEnded(
                TurnEndStatus::new(TurnTerminalOutcome::Interrupted, None).unwrap(),
            ),
        )
        .unwrap(),
    ));
    records.extend(item_free_transcript_build_records(
        id(1),
        ThreadRevision::new(1).unwrap(),
        &[(turn, digest, TurnLifecycle::Interrupted, 1, timestamp(2))],
    ));
    batch(records)
}

fn two_threads() -> FixtureBatch {
    let mut records = empty_thread_records(id(1), draft_id(2));
    records.extend(empty_thread_records(id(10), draft_id(11)));
    batch(records)
}

fn execution_binding() -> beryl_model::ExecutionBinding {
    let path = beryl_model::RuntimeNativePath::from_admitted(
        beryl_model::RuntimeMode::host(),
        beryl_model::PathFlavor::Windows,
        "C:\\fixture",
    )
    .unwrap();
    beryl_model::ExecutionBinding::new(
        beryl_model::RuntimeId::from_bytes([1; 16]),
        beryl_model::RootId::from_bytes([2; 16]),
        path,
    )
}

fn duplicate_cas_binding() -> FixtureBatch {
    let cas = CasThreadId::new("cas-shared").unwrap();
    let revision = BindingRevision::new(2).unwrap();
    let digest = empty_selected_path_digest();
    let selected = SelectedPathProof::new(None, ThreadRevision::new(1).unwrap(), digest);
    let represented = CasRepresentedPrefixProof::new(None, ThreadRevision::new(1).unwrap(), digest);
    let lineage = CasLineageProof::native(NativeCasLineage::Fresh, represented).unwrap();
    let first = BindingRecord::new(
        id(1),
        revision,
        selected,
        BindingState::valid(UsableCasBinding::new(
            execution_binding(),
            cas.clone(),
            represented,
            beryl_model::CasNativeTurnCount::ZERO,
            test_tool_profile(),
            lineage,
        )),
    );
    let second = BindingRecord::new(
        id(10),
        revision,
        selected,
        BindingState::valid(UsableCasBinding::new(
            execution_binding(),
            cas.clone(),
            represented,
            beryl_model::CasNativeTurnCount::ZERO,
            test_tool_profile(),
            lineage,
        )),
    );
    batch([
        FixtureRecord::Binding(first),
        FixtureRecord::Binding(second),
        FixtureRecord::BindingHead(BindingHeadRecord::new(
            id(1),
            revision,
            BindingLifecycle::Valid,
            digest,
        )),
        FixtureRecord::BindingHead(BindingHeadRecord::new(
            id(10),
            revision,
            BindingLifecycle::Valid,
            digest,
        )),
        FixtureRecord::CasThread(CasThreadIndexRecord::new(cas.clone(), id(1), revision)),
        FixtureRecord::CasThreadBinding(CasThreadBindingIndexRecord::new(cas, id(1), revision)),
    ])
}
