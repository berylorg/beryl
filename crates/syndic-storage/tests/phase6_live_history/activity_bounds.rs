use super::*;

fn completed_activity_store(
    name: &str,
    count: usize,
    wide_cas_ids: bool,
) -> (TestHome, HomeStore, SyndicStorage, SyndicThreadId) {
    let home = TestHome::new(name);
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let (thread, turn) = seed_pending_turn(&store, storage);
    let source = establish_turn(&store, storage, thread, turn, timestamp(4));
    admit(
        &store,
        storage,
        thread,
        turn,
        &source,
        SourceEventPayload::TurnActivated,
        timestamp(4),
    );
    correlate_submitted_user_item(&store, storage, thread, turn, &source, timestamp(5));
    for index in 0..count {
        let identity = u128::try_from(index).unwrap() + 1_000;
        let item = SyndicItemId::from_bytes(identity.to_be_bytes());
        let label = format!("activity-retained-{index}");
        let cas = if wide_cas_ids {
            CasItemId::new(format!("{label:-<250}")).unwrap()
        } else {
            CasItemId::new(label).unwrap()
        };
        let at = 10 + u64::try_from(index).unwrap() * 2;
        admit_item_frame(
            &store,
            storage,
            thread,
            turn,
            item,
            &source,
            command_start(cas.clone(), timestamp(at)),
            timestamp(at),
        );
        admit_item_frame(
            &store,
            storage,
            thread,
            turn,
            item,
            &source,
            command_completion(
                ProviderFrameOrdinalV1::new(2).unwrap(),
                cas,
                "done",
                timestamp(at + 1),
            ),
            timestamp(at + 1),
        );
    }
    (home, store, storage, thread)
}

fn retained_activity(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
) -> (ActivityQueryHeadRecord, Vec<ActivityQueryEntryRecord>) {
    let head = storage
        .activity_query_head(store, thread, limit())
        .unwrap()
        .unwrap()
        .clone();
    let page = storage
        .activity_query_page(
            store,
            &head,
            None,
            CursorReadLimits::new(QUERY_PAGE_MAX_RECORDS, QUERY_PAGE_MAX_STORED_BYTES).unwrap(),
        )
        .unwrap();
    assert!(page.next_cursor().is_none());
    (head, page.records().to_vec())
}

#[test]
fn completed_retention_keeps_exact_newest_prefix_within_both_caps() {
    let (_home, store, storage, thread) =
        completed_activity_store("phase6-activity-row-retention", 260, false);
    let (head, records) = retained_activity(&store, storage, thread);
    let retained = usize::try_from(head.completed_row_count()).unwrap();
    assert!(retained <= 256);
    assert_eq!(head.logical_row_count(), head.completed_row_count());
    assert!(head.completed_stored_bytes() <= 65_536);
    assert_eq!(records.len(), retained);
    assert_eq!(
        records.first().unwrap().item_id(),
        SyndicItemId::from_bytes(1_259_u128.to_be_bytes())
    );
    assert_eq!(
        records.last().unwrap().item_id(),
        SyndicItemId::from_bytes(
            (1_000_u128 + u128::try_from(260 - retained).unwrap()).to_be_bytes()
        )
    );
    assert_eq!(
        head.completed_retention_cutoff(),
        Some(records.last().unwrap().order())
    );
    assert_eq!(
        storage
            .fixture_activity_query_entry_count(
                &store,
                thread,
                ActivityWorkPeriod::FIRST,
                CursorReadLimits::new(300, 1_000_000).unwrap(),
            )
            .unwrap(),
        (retained, false)
    );
}

#[test]
fn completed_retention_enforces_exact_stored_byte_cap_before_row_cap() {
    let count = 180_usize;
    let (_home, store, storage, thread) =
        completed_activity_store("phase6-activity-byte-retention", count, true);
    let (head, records) = retained_activity(&store, storage, thread);
    let retained = usize::try_from(head.completed_row_count()).unwrap();
    assert!(retained < count);
    assert!(retained < 256);
    assert!(head.completed_stored_bytes() <= 65_536);
    assert_eq!(records.len(), retained);
    let oldest_identity = 1_000_u128 + u128::try_from(count - retained).unwrap();
    assert_eq!(
        records.last().unwrap().item_id(),
        SyndicItemId::from_bytes(oldest_identity.to_be_bytes())
    );
    assert_eq!(
        head.completed_retention_cutoff(),
        Some(records.last().unwrap().order())
    );
}

#[test]
fn activity_pages_retire_stranded_rows_and_roll_work_periods_without_rewrites() {
    let home = TestHome::new("phase6-activity-bounded-retirement");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let (thread, turn) = seed_pending_turn(&store, storage);
    let source = establish_turn(&store, storage, thread, turn, timestamp(4));
    admit(
        &store,
        storage,
        thread,
        turn,
        &source,
        SourceEventPayload::TurnActivated,
        timestamp(4),
    );
    correlate_submitted_user_item(&store, storage, thread, turn, &source, timestamp(5));

    let mut completed_ids = Vec::new();
    for (index, value) in (20_u8..22).enumerate() {
        let event_at = 9 + u64::try_from(index).unwrap() * 2;
        let item = SyndicItemId::from_bytes([value; 16]);
        let cas = CasItemId::new(format!("phase6-activity-completed-{value}")).unwrap();
        admit_item_frame(
            &store,
            storage,
            thread,
            turn,
            item,
            &source,
            command_start(cas.clone(), timestamp(9)),
            timestamp(event_at),
        );
        admit_item_frame(
            &store,
            storage,
            thread,
            turn,
            item,
            &source,
            command_completion(
                ProviderFrameOrdinalV1::new(2).unwrap(),
                cas,
                "done",
                timestamp(10),
            ),
            timestamp(event_at + 1),
        );
        completed_ids.push(item);
    }
    for (index, value) in (30_u8..60).enumerate() {
        let item = SyndicItemId::from_bytes([value; 16]);
        admit_item_frame(
            &store,
            storage,
            thread,
            turn,
            item,
            &source,
            command_start(
                CasItemId::new(format!("phase6-activity-running-{value}")).unwrap(),
                timestamp(11),
            ),
            timestamp(13 + u64::try_from(index).unwrap()),
        );
    }

    let active_head = storage
        .activity_query_head(&store, thread, limit())
        .unwrap()
        .unwrap()
        .clone();
    assert_eq!(active_head.logical_row_count(), 32);
    assert_eq!(active_head.running_row_count(), 30);
    assert_eq!(active_head.completed_row_count(), 2);
    let mut cursor = None;
    let mut observed = Vec::new();
    loop {
        let page = storage
            .activity_query_page(
                &store,
                &active_head,
                cursor,
                CursorReadLimits::new(7, 1_000_000).unwrap(),
            )
            .unwrap();
        assert!(page.records().len() <= 7);
        observed.extend(page.records().iter().map(ActivityQueryEntryRecord::item_id));
        cursor = page.next_cursor();
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(observed.len(), 32);
    assert_eq!(&observed[30..], completed_ids.as_slice());
    assert_eq!(
        storage
            .fixture_activity_query_entry_count(
                &store,
                thread,
                ActivityWorkPeriod::FIRST,
                CursorReadLimits::new(64, 1_000_000).unwrap(),
            )
            .unwrap(),
        (32, false)
    );

    admit(
        &store,
        storage,
        thread,
        turn,
        &source,
        SourceEventPayload::TurnEnded(
            TurnEndStatus::new(
                TurnTerminalOutcome::Interrupted,
                Some(TurnIncompleteReason::ItemAuditFailed),
            )
            .unwrap(),
        ),
        timestamp(50),
    );
    let terminal_head = storage
        .activity_query_head(&store, thread, limit())
        .unwrap()
        .unwrap()
        .clone();
    assert_eq!(terminal_head.logical_row_count(), 2);
    assert_eq!(terminal_head.running_row_count(), 0);
    let terminal_page = storage
        .activity_query_page(
            &store,
            &terminal_head,
            None,
            CursorReadLimits::new(1, 1_000_000).unwrap(),
        )
        .unwrap();
    assert_eq!(terminal_page.records()[0].item_id(), completed_ids[0]);
    let old_period_cursor = terminal_page.next_cursor().unwrap();
    assert_eq!(
        storage
            .fixture_activity_query_entry_count(
                &store,
                thread,
                ActivityWorkPeriod::FIRST,
                CursorReadLimits::new(64, 1_000_000).unwrap(),
            )
            .unwrap(),
        (32, false),
        "terminal retirement must not delete stranded running rows"
    );
    converge_and_release_terminal_history(&store, storage, thread, turn);

    submit_current_draft(
        &store,
        storage,
        thread,
        draft_id(90),
        SyndicItemId::from_bytes([91; 16]),
        "next question",
        timestamp(52),
    );
    let next = storage
        .activity_query_head(&store, thread, limit())
        .unwrap()
        .unwrap()
        .clone();
    assert_eq!(next.work_period(), ActivityWorkPeriod::new(2).unwrap());
    assert_eq!(next.logical_row_count(), 0);
    assert!(matches!(
        storage.activity_query_page(
            &store,
            &next,
            Some(old_period_cursor),
            CursorReadLimits::new(4, 1_000_000).unwrap(),
        ),
        Err(SyndicReadError::InvalidActivityQueryCursor)
    ));
    assert!(matches!(
        storage.activity_query_page(
            &store,
            &terminal_head,
            None,
            CursorReadLimits::new(4, 1_000_000).unwrap(),
        ),
        Err(SyndicReadError::StaleActivityQuery)
    ));
    store.close().unwrap();
    let mut reopened = open(home.path());
    let reopened_storage = SyndicStorage::register(&mut reopened).unwrap();
    let reopened_head = reopened_storage
        .activity_query_head(&reopened, thread, limit())
        .unwrap()
        .unwrap()
        .clone();
    assert_eq!(
        reopened_head.work_period(),
        ActivityWorkPeriod::new(2).unwrap()
    );
    assert!(reopened_storage
        .activity_query_page(
            &reopened,
            &reopened_head,
            None,
            CursorReadLimits::new(4, 1_000_000).unwrap(),
        )
        .unwrap()
        .records()
        .is_empty());
    assert_eq!(
        reopened_storage
            .fixture_activity_query_entry_count(
                &reopened,
                thread,
                ActivityWorkPeriod::FIRST,
                CursorReadLimits::new(64, 1_000_000).unwrap(),
            )
            .unwrap(),
        (32, false)
    );
}
