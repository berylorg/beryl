use super::*;

#[test]
fn duplicate_start_issue_with_the_wrong_reason_is_rejected_atomically() {
    let fixture = setup("phase18-provider-observation-issue-wrong-reason");
    admit_agent_start(&fixture);
    let canonical_before = canonical_item(&fixture);
    let issue = inspect_agent_start(&fixture, 21)
        .into_issue(ProviderObservationIssueReason::EventAfterCompletion);
    let event = next_event(
        &fixture.store,
        fixture.storage.clone(),
        fixture.thread,
        fixture.turn,
        &fixture.source,
        SourceEventPayload::ProviderObservationIssue(Box::new(issue)),
        timestamp(6),
    );
    let error = not_committed_command(execute(
        &fixture.store,
        fixture.storage.admit_live_source_event(
            fixture.storage.revision(&fixture.store).unwrap(),
            event.clone(),
        ),
    ));
    assert!(matches!(
        typed_error(&error),
        SyndicMutationError::ProviderObservationIssueConflict
    ));
    assert_eq!(canonical_item(&fixture), canonical_before);
    let state = fixture
        .storage
        .turn_state(&fixture.store, fixture.turn, limit())
        .unwrap()
        .unwrap();
    assert_eq!(state.source_event_count(), 2);
    assert_eq!(state.provider_observation_issue(), None);
    assert!(
        fixture
            .storage
            .source_event(&fixture.store, fixture.turn, event.sequence(), limit())
            .unwrap()
            .is_none()
    );
    fixture
        .store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    fixture.store.close().unwrap();
}

#[test]
fn legally_admissible_completion_only_observation_cannot_be_published_as_an_issue() {
    let fixture = setup("phase18-provider-observation-issue-admissible");
    let issue = inspect_completion_only(&fixture)
        .into_issue(ProviderObservationIssueReason::MissingItemStart);
    let event = next_event(
        &fixture.store,
        fixture.storage.clone(),
        fixture.thread,
        fixture.turn,
        &fixture.source,
        SourceEventPayload::ProviderObservationIssue(Box::new(issue)),
        timestamp(5),
    );
    let error = not_committed_command(execute(
        &fixture.store,
        fixture.storage.admit_live_source_event(
            fixture.storage.revision(&fixture.store).unwrap(),
            event.clone(),
        ),
    ));
    assert!(matches!(
        typed_error(&error),
        SyndicMutationError::ProviderObservationIssueConflict
    ));
    let state = fixture
        .storage
        .turn_state(&fixture.store, fixture.turn, limit())
        .unwrap()
        .unwrap();
    assert_eq!(state.source_event_count(), 1);
    assert_eq!(state.item_count(), 1);
    assert_eq!(state.provider_observation_issue(), None);
    assert!(
        fixture
            .storage
            .source_event(&fixture.store, fixture.turn, event.sequence(), limit())
            .unwrap()
            .is_none()
    );
    fixture
        .store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    fixture.store.close().unwrap();
}
