use super::*;
use beryl_home_store::CursorReadLimits;
use beryl_model::CasItemId;
use support::exact_cas::{admit_event, admit_item_frame, correlate_user_item, establish_turn};

fn command(completed: bool) -> ProviderItemV1 {
    ProviderItemV1::CommandExecution(ProviderCommandExecutionV1 {
        command: ProviderTextV1::inline("cargo check"),
        cwd: ProviderTextV1::inline("C:/workspace"),
        process_id: None,
        source: ProviderCommandSourceV1::Agent,
        status: if completed {
            ProviderCommandStatusV1::Completed
        } else {
            ProviderCommandStatusV1::InProgress
        },
        command_actions: Vec::new(),
        aggregated_output: completed.then(|| ProviderTextV1::inline("done")),
        exit_code: completed.then_some(0),
        duration_ms: completed.then_some(1),
    })
}

fn retire(f: &mut Fixture, running: usize) -> ActivityQueryHeadRecord {
    let source = establish_turn(&f.store, f.storage.clone(), id(30), f.turn, timestamp(101));
    admit_event(
        &f.store,
        f.storage.clone(),
        id(30),
        f.turn,
        &source,
        SourceEventPayload::TurnActivated,
        timestamp(101),
    );
    correlate_user_item(
        &f.store,
        f.storage.clone(),
        id(30),
        f.turn,
        SyndicItemId::from_bytes([221; 16]),
        &source,
        timestamp(102),
    );
    for index in 0..=running {
        let item = SyndicItemId::from_bytes((10_000u128 + index as u128).to_be_bytes());
        let cas = CasItemId::new(format!("enrollment-command-{index}")).unwrap();
        let at = timestamp(103 + index as u64 * 2);
        admit_item_frame(
            &f.store,
            f.storage.clone(),
            id(30),
            f.turn,
            item,
            &source,
            ProviderItemFrameV1::new(
                ProviderFrameOrdinalV1::FIRST,
                cas.clone(),
                ProviderItemObservationV1::Started {
                    observed_at: ProviderLifecycleTimestampMsV1::new(at.unix_millis()),
                    item: command(false),
                },
            ),
            at,
        );
        if index == 0 {
            let at = timestamp(at.unix_millis() + 1);
            admit_item_frame(
                &f.store,
                f.storage.clone(),
                id(30),
                f.turn,
                item,
                &source,
                ProviderItemFrameV1::new(
                    ProviderFrameOrdinalV1::new(2).unwrap(),
                    cas,
                    ProviderItemObservationV1::Completed {
                        observed_at: ProviderLifecycleTimestampMsV1::new(at.unix_millis()),
                        item: command(true),
                    },
                ),
                at,
            );
        }
    }
    admit_event(
        &f.store,
        f.storage.clone(),
        id(30),
        f.turn,
        &source,
        SourceEventPayload::TurnEnded(
            TurnEndStatus::new(
                TurnTerminalOutcome::Interrupted,
                Some(TurnIncompleteReason::ItemAuditFailed),
            )
            .unwrap(),
        ),
        timestamp(1000),
    );
    support::converge_and_release_terminal_history(&f.store, f.storage.clone(), id(30), f.turn);
    let prior = f.head();
    f.turn = support::exact_cas::submit_current_draft(
        &f.store,
        f.storage.clone(),
        id(30),
        draft_id(222),
        SyndicItemId::from_bytes([223; 16]),
        "later",
        timestamp(1001),
    );
    commit(
        &f.store,
        f.storage.clone(),
        batch([FixtureRecord::ActivityQueryHead(prior.clone())]),
    );
    prior
}

#[test]
fn bounded_cleanup_preserves_completed_rows_across_interrupted_enrollment() {
    let mut f = Fixture::new();
    let token = f.enroll();
    let running = ACTIVITY_ENROLLMENT_CLEANUP_ROWS * 2 + 1;
    let prior = retire(&mut f, running);
    assert_eq!(prior.completed_row_count(), 1);
    let mut cleanup_passes = 0;
    loop {
        let prepared = f.prepare(Some(&token));
        let cleanup = prepared.is_cleanup();
        let before = f
            .storage
            .fixture_activity_query_entry_count(
                &f.store,
                id(30),
                token.work_period(),
                CursorReadLimits::new(256, 65_536).unwrap(),
            )
            .unwrap()
            .0;
        let (command, witness) = prepared.into_command();
        assert!(matches!(
            f.store.execute(command),
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ));
        let ActivityEnrollmentStatus::Committed { token: result } = f
            .storage
            .activity_enrollment_status(&f.store, &witness)
            .unwrap()
        else {
            panic!("exact step")
        };
        let after = f
            .storage
            .fixture_activity_query_entry_count(
                &f.store,
                id(30),
                token.work_period(),
                CursorReadLimits::new(256, 65_536).unwrap(),
            )
            .unwrap()
            .0;
        assert_eq!(f.head().completed_row_count(), prior.completed_row_count());
        assert_eq!(
            f.head().completed_stored_bytes(),
            prior.completed_stored_bytes()
        );
        if !cleanup {
            assert_eq!(result, Some(token.clone()));
            break;
        }
        assert!(result.is_none());
        assert!(!f.head().source_active());
        assert!((1..=ACTIVITY_ENROLLMENT_CLEANUP_ROWS).contains(&(before - after)));
        cleanup_passes += 1;
        // Each prepared owner and witness is dropped before the next bounded pass.
        assert!(cleanup_passes <= 3);
    }
    assert_eq!(cleanup_passes, 3);
    assert_eq!(f.head().source_count(), 2);
    let page = f
        .storage
        .activity_query_page(
            &f.store,
            &f.head(),
            None,
            CursorReadLimits::new(32, 65_536).unwrap(),
        )
        .unwrap();
    assert_eq!(page.records().len(), 1);
    assert!(!page.records()[0].order().running());
}

#[test]
fn a_fresh_runtime_selects_empty_activity_without_cleaning_old_indexes() {
    let mut f = Fixture::new();
    let old = f.enroll();
    retire(&mut f, 1);
    let before = f
        .storage
        .fixture_activity_query_entry_count(
            &f.store,
            id(30),
            old.work_period(),
            CursorReadLimits::new(32, 65_536).unwrap(),
        )
        .unwrap();
    let new = f.enroll();
    assert!(new.work_period() > old.work_period());
    assert_eq!(f.head().logical_row_count(), 0);
    assert_eq!(
        f.storage
            .fixture_activity_query_entry_count(
                &f.store,
                id(30),
                old.work_period(),
                CursorReadLimits::new(32, 65_536).unwrap()
            )
            .unwrap(),
        before
    );
}

#[test]
fn fresh_period_replaces_stale_activity_after_canonical_only_terminal_progress() {
    let mut f = Fixture::new();
    let token = f.enroll();
    let old_source = f.head().source().unwrap();
    let prior = retire(&mut f, 0);
    let lagging = ActivityQuerySourceRecord::new(
        id(30),
        token.work_period(),
        old_source,
        None,
        0,
        true,
        None,
    );
    let stale = ActivityQueryHeadRecord::new(
        id(30),
        token.work_period(),
        Some(old_source),
        false,
        0,
        prior.revision().checked_next().unwrap(),
        1,
        0,
        0,
        0,
        0,
        None,
        ProjectionLifecycle::Stale,
    )
    .unwrap();
    commit(
        &f.store,
        f.storage.clone(),
        batch([
            FixtureRecord::ActivityQueryHead(stale),
            FixtureRecord::ActivityQuerySource(lagging),
        ]),
    );
    assert!(
        f.storage
            .prepare_activity_enrollment(&f.store, f.request(Some(&token)))
            .is_err()
    );
    let next = f.enroll();
    assert!(next.work_period() > token.work_period());
    assert_eq!(f.head().lifecycle(), ProjectionLifecycle::Current);
    assert_eq!(f.head().logical_row_count(), 0);
}
