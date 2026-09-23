use super::*;
use beryl_model::{JobId, ResolutionIntentId, SyndicTurnId};
use syndic_storage::test_faults::{
    FixtureRecord, decode_discussion_gate_fixture, encode_discussion_gate_fixture,
};

#[test]
fn gate_codec_has_exact_shapes_and_rejects_reserved_or_trailing_values() {
    let names = syndic_storage::test_faults::syndic_v7_family_names();
    assert_eq!(names.len(), 89);
    assert_eq!(names[87], "non-idle-gate-sources");
    assert_eq!(names[88], "discussion-handoff-gates");
    for record in [
        DiscussionHandoffGateRecord::open(id(210)),
        DiscussionHandoffGateRecord::new(
            id(210),
            DiscussionHandoffGateRevision::new(2).unwrap(),
            DiscussionHandoffGateState::Pending {
                intent_id: ResolutionIntentId::from_bytes([1; 16]),
                job_id: JobId::from_bytes([2; 16]),
                resolving_turn_id: SyndicTurnId::from_bytes([3; 16]),
            },
        ),
    ] {
        let bytes = encode_discussion_gate_fixture(&record);
        assert_eq!(
            bytes.len(),
            if record.state() == DiscussionHandoffGateState::Open {
                25
            } else {
                73
            }
        );
        assert_eq!(decode_discussion_gate_fixture(&bytes).unwrap(), record);
        for length in 0..bytes.len() {
            assert!(decode_discussion_gate_fixture(&bytes[..length]).is_err());
        }
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(decode_discussion_gate_fixture(&trailing).is_err());
        let mut zero = bytes.clone();
        zero[16..24].fill(0);
        assert!(decode_discussion_gate_fixture(&zero).is_err());
        let mut unknown = bytes;
        unknown[24] = 2;
        assert!(decode_discussion_gate_fixture(&unknown).is_err());
    }
    assert!(DiscussionHandoffGateRevision::new(0).is_err());
    assert!(
        DiscussionHandoffGateRevision::new(u64::MAX)
            .unwrap()
            .checked_next()
            .is_err()
    );
}

#[test]
fn ordinary_thread_with_discussion_gate_is_not_pristine_or_valid() {
    let home = TestHome::new("ordinary-with-discussion-gate");
    let (store, storage) = seeded(&home);
    let request = CreateThread::ordinary(
        id(231),
        draft_id(232),
        support::exact_cas::execution_binding(),
        timestamp(1),
        DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
    );
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(storage.create_thread(storage.revision(&store).unwrap(), request.clone()))
        .unwrap();
    assert!(matches!(
        store.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    support::commit(
        &store,
        storage.clone(),
        support::batch([FixtureRecord::DiscussionHandoffGate(
            DiscussionHandoffGateRecord::open(request.thread_id()),
        )]),
    );
    assert_eq!(
        storage
            .thread_creation_status(&store, &request, limit())
            .unwrap(),
        ThreadCreationStatus::Collision
    );
    assert!(
        storage
            .inspect_pristine_thread(&store, request.thread_id(), request.execution())
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
            .is_err()
    );
    store.close().unwrap();
}
