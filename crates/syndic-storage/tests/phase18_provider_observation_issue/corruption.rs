use super::*;

use beryl_home_store::DomainRegistrationError;
use syndic_storage::test_faults::ProviderObservationCorruption;

fn publish_duplicate_start_issue(
    fixture: &Fixture,
    inspected: InspectedProviderObservation,
) -> (ProviderObservationBuildRecord, LiveSourceEvent) {
    let issue = inspected.into_issue(ProviderObservationIssueReason::DuplicateItemStart);
    let identity = issue.observation().identity();
    let event = next_event(
        &fixture.store,
        fixture.storage,
        fixture.thread,
        fixture.turn,
        &fixture.source,
        SourceEventPayload::ProviderObservationIssue(Box::new(issue)),
        timestamp(6),
    );
    match execute(
        &fixture.store,
        fixture.storage.admit_live_source_event(
            fixture.storage.revision(&fixture.store).unwrap(),
            event.clone(),
        ),
    ) {
        beryl_home_store::CommandOutcome::Committed {
            later_failure: None, ..
        } => {}
        outcome => panic!("expected issue publication to commit without later failure, got {outcome:?}"),
    }
    let build = fixture
        .storage
        .provider_observation_build(&fixture.store, identity, limit())
        .unwrap()
        .unwrap()
        .clone();
    (build, event)
}

fn assert_published_issue_corruption_detected(
    fixture: Fixture,
    build: ProviderObservationBuildRecord,
    corruption: ProviderObservationCorruption,
) {
    fixture.store.validate_registered_domains().unwrap();
    match fixture.store.execute_current(
        fixture
            .storage
            .current_corrupt_provider_observation(&build, corruption)
            .unwrap(),
    ) {
        beryl_home_store::CommandOutcome::Committed {
            later_failure: None, ..
        } => {}
        outcome => panic!("expected corruption command to commit without later failure, got {outcome:?}"),
    }

    let current_error = fixture.store.validate_registered_domains().unwrap_err();
    assert!(
        current_error
            .to_string()
            .contains("provider-observation issue evidence is not exact sealed authority"),
        "unexpected current validation error: {current_error}"
    );

    fixture.store.close().unwrap();
    let mut reopened = open(fixture.home.path());
    let reopen_error = match SyndicStorage::register(&mut reopened) {
        Ok(_) => panic!("corrupted provider-observation issue reopened successfully"),
        Err(error) => error,
    };
    let DomainRegistrationError::Validation { domain, source } = reopen_error else {
        panic!("expected provider-observation issue validation failure, got {reopen_error:?}");
    };
    assert_eq!(domain, "syndic");
    assert_eq!(
        source.to_string(),
        "provider-observation issue evidence is not exact sealed authority"
    );
    reopened.close().unwrap();
}

#[test]
fn published_issue_rejects_a_missing_referenced_observation_chunk_on_current_and_reopen_validation()
{
    let fixture = setup("phase18-provider-observation-issue-missing-chunk");
    admit_agent_start(&fixture);
    let inspected = inspect_agent_start(&fixture, 51);
    let (build, _) = publish_duplicate_start_issue(&fixture, inspected);

    assert_published_issue_corruption_detected(
        fixture,
        build,
        ProviderObservationCorruption::MissingChunk { ordinal: 1 },
    );
}

#[test]
fn published_issue_rejects_a_mutated_observation_digest_on_current_and_reopen_validation() {
    let fixture = setup("phase18-provider-observation-issue-build-digest");
    admit_agent_start(&fixture);
    let inspected = inspect_agent_start(&fixture, 52);
    let (build, _) = publish_duplicate_start_issue(&fixture, inspected);

    assert_published_issue_corruption_detected(
        fixture,
        build,
        ProviderObservationCorruption::BuildDigest,
    );
}

fn inspect_large_agent_start(fixture: &Fixture) -> InspectedProviderObservation {
    let sealed = {
        let mut callback = observation_callback(&fixture.store, fixture.storage);
        let mut stager = ProviderObservationStager::begin(
            ProviderObservationId::from_bytes([53; 16]),
            ProviderObservationBegin::Item {
                lifecycle: ProviderObservationItemLifecycle::Started,
                kind: ProviderObservationItemKind::AgentMessage,
            },
            &mut callback,
        )
        .unwrap();
        scalar(
            &mut stager,
            ProviderField::LifecycleObservedAt,
            ProviderScalar::Unsigned(6),
            &mut callback,
        );
        text(
            &mut stager,
            ProviderField::ItemId,
            fixture.cas_item.as_str(),
            &mut callback,
        );

        let context = ProviderValueContext::Field(ProviderField::AgentMessageText);
        stager
            .control(
                ProviderObservationControl::BeginField(context),
                &mut callback,
            )
            .unwrap();
        let fragment = vec![b'x'; PROVIDER_OBSERVATION_CHUNK_MAX_BYTES];
        for _ in 0..18 {
            stager
                .fragment(
                    ProviderObservationStagingBytes::new(context, &fragment).unwrap(),
                    &mut callback,
                )
                .unwrap();
        }
        stager
            .control(ProviderObservationControl::EndField(context), &mut callback)
            .unwrap();
        stager.seal(&mut callback).unwrap()
    };
    let route = ProviderObservationRoute::new(
        fixture.source.thread_id().clone(),
        fixture.source.turn_id().clone(),
    );
    let bound = sealed.bind(route.clone(), route).unwrap();
    inspect_provider_observation(&fixture.storage, &fixture.store, bound, limit()).unwrap()
}

#[test]
fn arbitrarily_large_referenced_observation_issue_publishes_and_reopens() {
    let fixture = setup("phase18-provider-observation-issue-large");
    admit_agent_start(&fixture);
    let inspected = inspect_large_agent_start(&fixture);
    let (build, event) = publish_duplicate_start_issue(&fixture, inspected);

    assert!(build.canonical_bytes() > 1_000_000);
    assert!(build.chunk_count() > 18);
    let stored = fixture
        .storage
        .source_event(&fixture.store, fixture.turn, event.sequence(), limit())
        .unwrap()
        .unwrap();
    let SourceEventPayload::ProviderObservationIssue(issue) = stored.payload() else {
        panic!("expected the large observation issue source event");
    };
    assert_eq!(issue.observation().identity(), build.identity());
    assert_eq!(issue.observation().chunk_count(), build.chunk_count());
    assert_eq!(
        issue.observation().canonical_bytes(),
        build.canonical_bytes()
    );
    fixture.store.validate_registered_domains().unwrap();

    fixture.store.close().unwrap();
    let mut reopened = open(fixture.home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    reopened.validate_registered_domains().unwrap();
    let reopened_event = storage
        .source_event(&reopened, fixture.turn, event.sequence(), limit())
        .unwrap()
        .unwrap();
    let SourceEventPayload::ProviderObservationIssue(reopened_issue) = reopened_event.payload()
    else {
        panic!("expected the reopened large observation issue source event");
    };
    assert_eq!(reopened_issue.observation().identity(), build.identity());
    assert_eq!(
        reopened_issue.observation().canonical_bytes(),
        build.canonical_bytes()
    );
    assert_eq!(
        storage
            .live_source_event_status(&reopened, &event, limit())
            .unwrap(),
        LiveSourceEventStatus::Exact
    );
    reopened.close().unwrap();
}
