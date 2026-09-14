use crate::{
    finalizing_history_support::{
        completion_request, converge_first_item, finish_transcript, terminal_home,
    },
    recovery_support::{execute, ordered_id, point_limit},
    support::{commit, open},
};
use beryl_model::SyndicItemId;
use syndic_storage::{SyndicReadError, SyndicStorage};

#[test]
fn terminal_evidence_requires_the_same_fixed_point_as_gate_release() {
    for correlate_user in [false, true] {
        let fixture = terminal_home("terminal-settlement-evidence", 760, correlate_user);
        let read = || {
            fixture.storage.terminal_history_evidence(
                &fixture.store,
                fixture.thread,
                fixture.turn,
                point_limit(),
            )
        };
        assert_eq!(read().unwrap(), None);
        if correlate_user {
            converge_first_item(
                &fixture.store,
                fixture.storage.clone(),
                fixture.thread,
                fixture.turn,
                SyndicItemId::from_bytes(*ordered_id(30_760).as_bytes()),
            );
        }
        finish_transcript(&fixture.store, fixture.storage.clone(), fixture.thread);
        assert_eq!(read().unwrap(), None);
        let request = completion_request(
            &fixture.store,
            fixture.storage.clone(),
            fixture.thread,
            fixture.turn,
        );
        execute(
            &fixture.store,
            fixture.storage.complete_terminal_history(
                fixture.storage.revision(&fixture.store).unwrap(),
                request,
            ),
        );
        syndic_storage::test_faults::reset_syndic_point_read_count();
        let evidence = read().unwrap().unwrap();
        assert!(syndic_storage::test_faults::syndic_point_read_count() <= 48);
        assert_eq!(evidence.thread_id(), fixture.thread);
        assert_eq!(evidence.turn_id(), fixture.turn);
        assert_eq!(
            evidence.lifecycle(),
            if correlate_user {
                syndic_storage::TurnLifecycle::Complete
            } else {
                syndic_storage::TurnLifecycle::Incomplete
            }
        );
        assert_eq!(
            fixture
                .storage
                .pending_dispatch_evidence(&fixture.store, fixture.thread, point_limit())
                .unwrap(),
            None
        );
        fixture.store.close().unwrap();
        let mut reopened = open(fixture.home.path());
        let storage = SyndicStorage::register(&mut reopened).unwrap();
        let after = storage
            .terminal_history_evidence(&reopened, fixture.thread, fixture.turn, point_limit())
            .unwrap()
            .unwrap();
        assert_eq!(after.home_id(), evidence.home_id());
        assert_eq!(after.selected_path(), evidence.selected_path());
        assert_eq!(after.state_revision(), evidence.state_revision());
        reopened.close().unwrap();
    }
}

#[test]
fn terminal_evidence_rejects_mutable_anchor_drift() {
    let fixture = terminal_home("terminal-settlement-drift", 761, false);
    finish_transcript(&fixture.store, fixture.storage.clone(), fixture.thread);
    execute(
        &fixture.store,
        fixture.storage.complete_terminal_history(
            fixture.storage.revision(&fixture.store).unwrap(),
            completion_request(
                &fixture.store,
                fixture.storage.clone(),
                fixture.thread,
                fixture.turn,
            ),
        ),
    );
    let result = syndic_storage::test_faults::terminal_history_evidence_with_confirmation_hook(
        &fixture.storage,
        &fixture.store,
        fixture.thread,
        fixture.turn,
        point_limit(),
        || {
            let mut changes = syndic_storage::test_faults::FixtureBatch::new();
            changes
                .delete(
                    syndic_storage::test_faults::FixtureDelete::TranscriptViewHead(fixture.thread),
                )
                .unwrap();
            commit(&fixture.store, fixture.storage.clone(), changes);
        },
    );
    assert!(matches!(
        result,
        Err(SyndicReadError::ConcurrentChange { .. })
    ));
    assert_eq!(
        fixture
            .storage
            .terminal_history_evidence(&fixture.store, fixture.thread, fixture.turn, point_limit())
            .unwrap(),
        None
    );
}
