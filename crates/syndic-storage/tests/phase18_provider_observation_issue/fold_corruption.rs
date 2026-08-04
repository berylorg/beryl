use super::*;

use syndic_storage::test_faults::{FixtureBatch, FixtureRecord};

const FOLD_MISMATCH: &str = "turn source-event frontier, issue, or exact end status disagrees";

fn publish_duplicate_start_issue(fixture: &Fixture, observation_byte: u8) {
    admit_agent_start(fixture);
    let issue = inspect_agent_start(fixture, observation_byte)
        .into_issue(ProviderObservationIssueReason::DuplicateItemStart);
    let event = next_event(
        &fixture.store,
        fixture.storage,
        fixture.thread,
        fixture.turn,
        &fixture.source,
        SourceEventPayload::ProviderObservationIssue(Box::new(issue)),
        timestamp(6),
    );
    execute(
        &fixture.store,
        fixture
            .storage
            .admit_live_source_event(fixture.storage.revision(&fixture.store).unwrap(), event),
    )
    .unwrap();
}

fn publish_completion_mismatch_terminal(fixture: &Fixture) {
    let event = next_event(
        &fixture.store,
        fixture.storage,
        fixture.thread,
        fixture.turn,
        &fixture.source,
        SourceEventPayload::TurnEnded(
            TurnEndStatus::new(
                TurnTerminalOutcome::Complete,
                Some(TurnIncompleteReason::CompletionMismatch),
            )
            .unwrap(),
        ),
        timestamp(7),
    );
    execute(
        &fixture.store,
        fixture
            .storage
            .admit_live_source_event(fixture.storage.revision(&fixture.store).unwrap(), event),
    )
    .unwrap();
}

fn replace_folded_issue(fixture: &Fixture, issue: Option<ProviderObservationIssueReason>) {
    let stored = fixture
        .storage
        .turn_state(&fixture.store, fixture.turn, limit())
        .unwrap()
        .unwrap();
    let state = stored;
    let corrupted = TurnStateRecord::with_capture_frontiers_and_issue(
        state.turn_id(),
        state.revision(),
        state.lifecycle(),
        state.source_event_count(),
        state.item_count(),
        state.finalized_item_count(),
        state.open_item_count(),
        state.history_blocking_item_count(),
        issue,
        state.end_status(),
        state.updated_at(),
    )
    .unwrap();
    let mut batch = FixtureBatch::new();
    batch.put(FixtureRecord::TurnState(corrupted)).unwrap();
    execute(
        &fixture.store,
        fixture
            .storage
            .fixture_contribution(fixture.storage.revision(&fixture.store).unwrap(), batch),
    )
    .unwrap();
}

fn assert_current_and_reopen_reject(fixture: Fixture) {
    let Fixture { store, home, .. } = fixture;
    let current_error = store.validate_registered_domains().unwrap_err();
    assert!(
        current_error.to_string().contains(FOLD_MISMATCH),
        "unexpected current-domain validation error: {current_error}"
    );

    store.close().unwrap();
    let mut reopened = open(home.path());
    let reopen_error = match SyndicStorage::register(&mut reopened) {
        Ok(_) => panic!("corrupted provider-observation issue fold reopened successfully"),
        Err(error) => error,
    };
    assert!(
        reopen_error.to_string().contains(FOLD_MISMATCH),
        "unexpected reopen validation error: {reopen_error}"
    );
}

#[test]
fn validation_rejects_a_missing_folded_issue_fact() {
    let fixture = setup("phase18-provider-observation-issue-missing-fold");
    publish_duplicate_start_issue(&fixture, 41);
    replace_folded_issue(&fixture, None);

    assert_current_and_reopen_reject(fixture);
}

#[test]
fn validation_rejects_a_wrong_folded_issue_fact() {
    let fixture = setup("phase18-provider-observation-issue-wrong-fold");
    publish_duplicate_start_issue(&fixture, 42);
    replace_folded_issue(
        &fixture,
        Some(ProviderObservationIssueReason::EventAfterCompletion),
    );

    assert_current_and_reopen_reject(fixture);
}

#[test]
fn terminal_state_must_retain_its_folded_issue_fact() {
    let fixture = setup("phase18-provider-observation-issue-terminal-fold");
    publish_duplicate_start_issue(&fixture, 43);
    publish_completion_mismatch_terminal(&fixture);
    replace_folded_issue(&fixture, None);

    assert_current_and_reopen_reject(fixture);
}
