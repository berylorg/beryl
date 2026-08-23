use super::*;

#[test]
fn promotion_reconciliation_accepts_a_later_current_draft_revision() {
    let (home, store, storage, fixture) = seeded_fixture(
        "phase58-promotion-draft-descendant",
        promotion_fixture(93, id(93)),
    );
    let request = promotion(&store, storage);
    assert!(matches!(
        execute_promotion(&store, storage, request.clone()),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    let draft = storage
        .draft(&store, fixture.current_draft, limit())
        .unwrap()
        .unwrap();
    let thread = storage
        .thread(&store, fixture.thread, limit())
        .unwrap()
        .unwrap();
    let summary = storage
        .history_summary(&store, fixture.thread, limit())
        .unwrap()
        .unwrap();
    let updated_at = timestamp(21);
    commit(
        &store,
        storage,
        batch([
            FixtureRecord::Draft(DraftRecord::new(
                draft.id(),
                draft.thread_id(),
                draft.revision().checked_next().unwrap(),
                draft.submission_intent(),
                draft.root_history(),
                draft.created_at(),
                updated_at,
            )),
            FixtureRecord::DraftByThread(DraftByThreadRecord::new(
                fixture.thread,
                draft.id(),
                draft.revision().checked_next().unwrap(),
                thread.revision(),
            )),
            FixtureRecord::HistorySummary(HistorySummaryRecord::new(
                summary.thread_id(),
                summary.revision().checked_next().unwrap(),
                summary.thread_revision(),
                summary.committed_tail(),
                summary.selected_path_digest(),
                summary.complete(),
                updated_at,
            )),
        ]),
    );
    assert_eq!(
        storage
            .accepted_input_promotion_status(&store, &request, limit())
            .unwrap(),
        AcceptedInputPromotionStatus::Exact
    );
    store.close().unwrap();
    let mut reopened = open(home.path());
    let reopened_storage = SyndicStorage::register(&mut reopened).unwrap();
    assert_eq!(
        reopened_storage
            .accepted_input_promotion_status(&reopened, &request, limit())
            .unwrap(),
        AcceptedInputPromotionStatus::Exact
    );
}

#[test]
fn promotion_reconciliation_accepts_a_later_accepted_generation() {
    let newer = promotion_support::promotion_fixture_with_newer_generation(94, id(94));
    let (home, store, storage, fixture) =
        seeded_fixture("phase58-promotion-accepted-descendant", newer.fixture);
    let request = PromoteAcceptedInput::new(
        candidate(&store, storage),
        SyndicTurnId::from_bytes([125; 16]),
        SyndicItemId::from_bytes([126; 16]),
        timestamp(20),
    );
    assert!(matches!(
        execute_promotion(&store, storage, request.clone()),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    assert_eq!(
        storage
            .accepted_input_promotion_status(&store, &request, limit())
            .unwrap(),
        AcceptedInputPromotionStatus::Exact
    );
    let sources = storage
        .accepted_next_source_page(
            &store,
            storage.revision(&store).unwrap(),
            None,
            CursorReadLimits::new(256, ACCEPTED_NEXT_PAGE_MAX_BYTES).unwrap(),
        )
        .unwrap();
    assert_eq!(sources.records().len(), 1);
    assert_eq!(sources.records()[0].generation(), newer.newer_generation);
    store.close().unwrap();
    let mut reopened = open(home.path());
    let reopened_storage = SyndicStorage::register(&mut reopened).unwrap();
    assert_eq!(
        reopened_storage
            .accepted_input_promotion_status(&reopened, &request, limit())
            .unwrap(),
        AcceptedInputPromotionStatus::Exact
    );
    drop(fixture);
}
