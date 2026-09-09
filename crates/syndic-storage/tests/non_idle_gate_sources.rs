#![cfg(feature = "test-faults")]

#[path = "accepted_delivery/fixtures.rs"]
#[allow(dead_code)]
mod accepted_fixtures;
#[path = "accepted_delivery/accepted_support.rs"]
mod accepted_support;
mod support;

use accepted_support::{AcceptedOperation, limit, seeded_operation};
use beryl_home_store::{CommandOutcome, RecordVersion, WholeHomeScrubTrigger};
use beryl_model::InputGateRevision;
use support::{batch, commit, id, open};
use syndic_storage::test_faults::{
    FixtureBatch, FixtureDelete, FixtureRecord, decode_non_idle_gate_source_for_test,
    non_idle_gate_source_codec_bytes, non_idle_gate_source_codec_limits,
};
use syndic_storage::*;

#[test]
fn canonical_source_encoding_is_exact_and_rejects_invalid_bytes() {
    let names = syndic_storage::test_faults::syndic_v7_family_names();
    assert_eq!(names.len(), 88);
    assert_eq!(names[87], "non-idle-gate-sources");
    let revision = InputGateRevision::new(0x0102_0304_0506_0708).unwrap();
    let source = NonIdleGateSourceRecord::new(id(42), revision);
    let (key, payload) = non_idle_gate_source_codec_bytes(source);
    assert_eq!(
        non_idle_gate_source_codec_limits(),
        (RecordVersion::new(1), 16, 24)
    );
    assert_eq!(key, [42; 16]);
    let mut expected = vec![42; 16];
    expected.extend_from_slice(&revision.get().to_be_bytes());
    assert_eq!(payload, expected);
    assert_eq!(
        decode_non_idle_gate_source_for_test(&key, &payload),
        Some(source)
    );
    for length in 0..24 {
        assert!(decode_non_idle_gate_source_for_test(&key, &payload[..length]).is_none());
    }
    let mut trailing = payload.clone();
    trailing.push(0);
    assert!(decode_non_idle_gate_source_for_test(&key, &trailing).is_none());
    let mut zero_revision = payload.clone();
    zero_revision[16..].fill(0);
    assert!(decode_non_idle_gate_source_for_test(&key, &zero_revision).is_none());
    assert!(decode_non_idle_gate_source_for_test(&key[..15], &payload).is_none());
    assert!(decode_non_idle_gate_source_for_test(&[43; 16], &payload).is_none());
}

#[test]
fn delivery_changes_source_revision_atomically_and_reopens_exactly() {
    for operation in AcceptedOperation::ALL {
        let (home, store, storage) = seeded_operation("source-delivery", operation);
        let before = storage
            .non_idle_gate_source(&store, operation.thread(), limit())
            .unwrap()
            .unwrap();
        assert!(matches!(
            store.execute_current(operation.current_command(&storage)),
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ));
        let gate = storage
            .input_gate(&store, operation.thread(), limit())
            .unwrap()
            .unwrap();
        let expected = NonIdleGateSourceRecord::new(
            operation.thread(),
            before.gate_revision().checked_next().unwrap(),
        );
        assert_eq!(gate.revision(), expected.gate_revision());
        assert_eq!(
            storage
                .non_idle_gate_source(&store, operation.thread(), limit())
                .unwrap(),
            Some(expected)
        );
        store
            .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
            .unwrap();
        store.close().unwrap();
        let mut reopened = open(home.path());
        let storage = SyndicStorage::register_with_schema_validation(&mut reopened).unwrap();
        assert_eq!(
            storage
                .non_idle_gate_source(&reopened, operation.thread(), limit())
                .unwrap(),
            Some(expected)
        );
        reopened.close().unwrap();
    }
}

#[test]
fn corrupt_sources_reject_reads_and_mutations_without_repair() {
    for operation in AcceptedOperation::ALL {
        for corruption in 0..3 {
            let (_home, store, storage) = seeded_operation("source-corruption", operation);
            let mut changes = FixtureBatch::new();
            if corruption == 0 {
                changes
                    .delete(FixtureDelete::NonIdleGateSource(operation.thread()))
                    .unwrap();
            } else {
                let source_thread = if corruption == 1 {
                    operation.thread()
                } else {
                    id(249)
                };
                changes
                    .put(FixtureRecord::NonIdleGateSource {
                        thread_id: operation.thread(),
                        source: NonIdleGateSourceRecord::new(
                            source_thread,
                            InputGateRevision::new(999).unwrap(),
                        ),
                    })
                    .unwrap();
            }
            commit(&store, storage.clone(), changes);
            let before = storage.revision(&store).unwrap();
            let gate_before = storage
                .input_gate(&store, operation.thread(), limit())
                .unwrap();
            assert!(
                storage
                    .non_idle_gate_source(&store, operation.thread(), limit())
                    .is_err()
            );
            assert!(matches!(
                store.execute_current(operation.current_command(&storage)),
                CommandOutcome::NotCommitted { .. }
            ));
            assert_eq!(storage.revision(&store).unwrap(), before);
            assert_eq!(
                storage
                    .input_gate(&store, operation.thread(), limit())
                    .unwrap(),
                gate_before
            );
            assert!(
                storage
                    .non_idle_gate_source(&store, operation.thread(), limit())
                    .is_err()
            );
            assert!(
                store
                    .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
                    .is_err()
            );
            store.close().unwrap();
        }
    }
}

#[test]
fn idle_and_absent_gates_reject_orphan_sources_in_both_validation_directions() {
    for with_gate in [false, true] {
        let home = support::TestHome::new("orphan-source");
        let mut store = open(home.path());
        let storage = SyndicStorage::register(&mut store).unwrap();
        let creation = CreateThread::ordinary(
            id(240),
            support::draft_id(241),
            support::exact_cas::execution_binding(),
            support::timestamp(1),
            DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
        );
        if with_gate {
            support::seed_canonical_empty_thread(
                &store,
                storage.clone(),
                id(240),
                support::draft_id(241),
            );
        }
        assert_eq!(
            storage
                .non_idle_gate_source(&store, id(240), limit())
                .unwrap(),
            None
        );
        assert_eq!(
            storage
                .thread_creation_status(&store, &creation, limit())
                .unwrap(),
            if with_gate {
                ThreadCreationStatus::Exact
            } else {
                ThreadCreationStatus::Absent
            }
        );
        commit(
            &store,
            storage.clone(),
            batch([FixtureRecord::NonIdleGateSource {
                thread_id: id(240),
                source: NonIdleGateSourceRecord::new(id(240), InputGateRevision::new(1).unwrap()),
            }]),
        );
        assert!(
            storage
                .non_idle_gate_source(&store, id(240), limit())
                .is_err()
        );
        assert!(
            storage
                .thread_creation_status(&store, &creation, limit())
                .is_err()
        );
        assert!(
            store
                .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
                .is_err()
        );
        store.close().unwrap();
    }
}

#[test]
fn exact_delivery_reconciliation_rejects_a_missing_current_source() {
    for operation in AcceptedOperation::ALL {
        let (_home, store, storage) = seeded_operation("source-reconciliation", operation);
        assert!(matches!(
            store.execute_current(operation.current_command(&storage)),
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ));
        assert_eq!(
            operation.status(&store, &storage),
            AcceptedInputDeliveryTransitionStatus::Exact
        );
        let mut corruption = FixtureBatch::new();
        corruption
            .delete(FixtureDelete::NonIdleGateSource(operation.thread()))
            .unwrap();
        commit(&store, storage.clone(), corruption);
        assert!(matches!(
            operation.status_result(&store, &storage),
            Err(SyndicReadError::Invariant(
                "current input gate and non-idle source disagree"
            ))
        ));
        store.close().unwrap();
    }
}
