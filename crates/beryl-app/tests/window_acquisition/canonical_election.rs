use super::*;
use beryl_state::{CatalogClaimSummary, CatalogFreshness, CreateClaimedWindow};

fn create_without_projection(fixture: &Fixture, seed: u8, created: u64) -> SyndicThreadId {
    let id = SyndicThreadId::from_bytes([seed; 16]);
    execute_contribution(
        &fixture.store,
        fixture.syndic.create_thread(
            fixture.syndic.revision(&fixture.store).unwrap(),
            CreateThread::ordinary(
                id,
                SyndicDraftId::from_bytes([seed.wrapping_add(1); 16]),
                fixture.execution.clone(),
                SyndicTimestamp::from_unix_millis(created),
                history_policy(),
            ),
        ),
    );
    id
}

#[test]
fn missing_catalog_target_is_discovered_and_published_with_the_original_claim() {
    let fixture = Fixture::new(1);
    let target = create_without_projection(&fixture, 10, 1);
    let before = fixture.store.home_revision().unwrap();
    let request = fixture.request(20);
    let RuntimeBackedWindowAcquisitionOutcome::Committed { acquisition, .. } = fixture
        .service
        .acquire(request.clone(), CommandCancellation::new())
    else {
        panic!("canonical missing-row reuse must commit");
    };
    assert_eq!(acquisition.thread_id(), target);
    assert_eq!(
        acquisition.disposition(),
        RuntimeBackedWindowAcquisitionDisposition::Reused
    );
    assert_eq!(
        fixture.store.home_revision().unwrap(),
        before.checked_next().unwrap()
    );
    let row = fixture
        .state
        .catalog()
        .row(
            &fixture.store,
            target,
            CatalogPointReadLimit::schema_maximum(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(row.freshness(), CatalogFreshness::Current);
    assert!(
        matches!(row.facts().claim(), CatalogClaimSummary::Claimed { window_id, .. } if window_id == acquisition.window_id())
    );
    assert!(matches!(fixture.service.reconcile_natural_state(&request),
        RuntimeBackedWindowAcquisitionNaturalReconciliationOutcome::ExactCommitted { acquisition } if acquisition.thread_id() == target && acquisition.disposition() == RuntimeBackedWindowAcquisitionDisposition::Reused));
    let beryl_app::window_acquisition::RuntimeBackedWindowAbandonmentPreparationOutcome::ExactAcquired { abandonment } = fixture.service.prepare_abandonment(acquisition, CommandCancellation::new())
        else { panic!("missing-row reuse must retain eligible abandonment authority"); };
    match fixture
        .service
        .abandon(abandonment, CommandCancellation::new())
    {
        beryl_app::window_acquisition::RuntimeBackedWindowAbandonmentOutcome::Committed {
            ..
        } => {}
        beryl_app::window_acquisition::RuntimeBackedWindowAbandonmentOutcome::NotCommitted {
            evidence,
            ..
        } => panic!("missing-row reuse cleanup did not commit: {evidence:?}"),
        beryl_app::window_acquisition::RuntimeBackedWindowAbandonmentOutcome::Indeterminate {
            failure,
            ..
        } => panic!("missing-row reuse cleanup remains indeterminate: {failure:?}"),
    }
    assert!(
        fixture
            .syndic
            .current_draft(
                &fixture.store,
                target,
                syndic_storage::SyndicPointReadLimit::new(65536).unwrap()
            )
            .unwrap()
            .is_some()
    );
}

#[test]
fn released_live_claim_does_not_let_cached_claimed_catalog_suppress_reuse() {
    let fixture = Fixture::new(2);
    let target = fixture.publish_pristine_thread(12, 1);
    let first = fixture.request(22);
    let RuntimeBackedWindowAcquisitionOutcome::Committed { acquisition, .. } =
        fixture.service.acquire(first, CommandCancellation::new())
    else {
        panic!("first claim must commit");
    };
    let window = fixture
        .state
        .session()
        .capture_window_removal(&fixture.store, acquisition.window_id())
        .unwrap();
    execute_contribution(
        &fixture.store,
        fixture.state.session().remove_window(
            fixture.state.session().revision(&fixture.store).unwrap(),
            RemoveSessionWindow::new(
                window.header().revision(),
                acquisition.window_id(),
                window.window().revision(),
                window.window().selected_thread(),
            ),
        ),
    );
    assert!(
        fixture
            .state
            .session()
            .thread_claim_catalog_source(&fixture.store, target)
            .unwrap()
            .claim()
            .is_none()
    );
    let row = fixture
        .state
        .catalog()
        .row(
            &fixture.store,
            target,
            CatalogPointReadLimit::schema_maximum(),
        )
        .unwrap()
        .unwrap();
    assert!(matches!(
        row.facts().claim(),
        CatalogClaimSummary::Claimed { .. }
    ));
    let RuntimeBackedWindowAcquisitionOutcome::Committed { acquisition, .. } = fixture
        .service
        .acquire(fixture.request(32), CommandCancellation::new())
    else {
        panic!("live unclaimed source must override stale claimed projection");
    };
    assert_eq!(acquisition.thread_id(), target);
    assert_eq!(
        acquisition.disposition(),
        RuntimeBackedWindowAcquisitionDisposition::Reused
    );
}

#[test]
fn live_claim_excludes_a_candidate_even_when_catalog_still_says_unclaimed() {
    let fixture = Fixture::new(3);
    let target = fixture.publish_pristine_thread(13, 1);
    let bootstrap = fixture
        .state
        .session()
        .minimal_bootstrap(&fixture.store)
        .unwrap()
        .unwrap();
    execute_contribution(
        &fixture.store,
        fixture.state.session().create_claimed_window(
            fixture.state.session().revision(&fixture.store).unwrap(),
            CreateClaimedWindow::new(
                bootstrap.header().revision(),
                WindowId::from_bytes([23; 16]),
                RememberedTarget::new(fixture.runtime_id, fixture.root_id),
                target,
                placement(),
            ),
        ),
    );
    assert_eq!(
        fixture
            .state
            .catalog()
            .row(
                &fixture.store,
                target,
                CatalogPointReadLimit::schema_maximum()
            )
            .unwrap()
            .unwrap()
            .facts()
            .claim(),
        CatalogClaimSummary::Unclaimed
    );
    let request = fixture.request(33);
    let RuntimeBackedWindowAcquisitionOutcome::Committed { acquisition, .. } = fixture
        .service
        .acquire(request.clone(), CommandCancellation::new())
    else {
        panic!("occupied source must produce a fresh fallback");
    };
    assert_eq!(acquisition.thread_id(), request.fallback_thread_id());
    assert_eq!(
        acquisition.disposition(),
        RuntimeBackedWindowAcquisitionDisposition::Created
    );
}

#[test]
fn unrelated_stale_rows_do_not_consume_an_election_repair_budget() {
    let fixture = Fixture::new(4);
    let first = fixture.publish_pristine_thread(14, 1);
    let second = fixture.publish_pristine_thread(15, 2);
    fixture.mark_catalog_row_stale(first);
    fixture.mark_catalog_row_stale(second);
    let before = fixture.store.home_revision().unwrap();
    let RuntimeBackedWindowAcquisitionOutcome::Committed { acquisition, .. } = fixture
        .service
        .clone()
        .test_with_catalog_repair_budget(0)
        .acquire(fixture.request(34), CommandCancellation::new())
    else {
        panic!("election is independent of stale projection repair");
    };
    assert_eq!(acquisition.thread_id(), first);
    assert_eq!(
        fixture.store.home_revision().unwrap(),
        before.checked_next().unwrap()
    );
    assert_eq!(
        fixture
            .state
            .catalog()
            .row(
                &fixture.store,
                second,
                CatalogPointReadLimit::schema_maximum()
            )
            .unwrap()
            .unwrap()
            .freshness(),
        CatalogFreshness::Stale
    );
}

#[test]
fn typed_then_removed_target_reuses_and_abandons_without_pristine_deletion_authority() {
    let fixture = Fixture::new(5);
    let target = create_without_projection(&fixture, 16, 1);
    empty_after_edit::publish_empty_after_typing(
        &fixture.store,
        &fixture.state,
        &fixture.syndic,
        target,
    );
    let point = syndic_storage::SyndicPointReadLimit::new(65536).unwrap();
    let before = fixture
        .syndic
        .current_draft(&fixture.store, target, point)
        .unwrap()
        .unwrap();
    let request = fixture.request(36);
    let RuntimeBackedWindowAcquisitionOutcome::Committed { acquisition, .. } = fixture
        .service
        .acquire(request.clone(), CommandCancellation::new())
    else {
        panic!("authentic typed-and-removed target must reuse");
    };
    assert_eq!(acquisition.thread_id(), target);
    assert_eq!(
        acquisition.disposition(),
        RuntimeBackedWindowAcquisitionDisposition::Reused
    );
    assert!(matches!(fixture.service.reconcile_natural_state(&request),
        RuntimeBackedWindowAcquisitionNaturalReconciliationOutcome::ExactCommitted { acquisition } if acquisition.thread_id() == target));
    let beryl_app::window_acquisition::RuntimeBackedWindowAbandonmentPreparationOutcome::ExactAcquired { abandonment } = fixture.service.prepare_abandonment(acquisition, CommandCancellation::new())
        else { panic!("original eligible target proof must authorize validation-only abandonment"); };
    let seed = abandonment.audit_seed();
    assert!(matches!(
        fixture
            .service
            .abandon(abandonment, CommandCancellation::new()),
        beryl_app::window_acquisition::RuntimeBackedWindowAbandonmentOutcome::Committed { .. }
    ));
    let after = fixture
        .syndic
        .current_draft(&fixture.store, target, point)
        .unwrap()
        .unwrap();
    assert_eq!(after.draft().piece_root(), before.draft().piece_root());
    assert_eq!(after.draft().history(), before.draft().history());
    assert!(matches!(fixture.service.reconcile_abandonment(seed, CommandCancellation::new()),
        beryl_app::window_acquisition::RuntimeBackedWindowAbandonmentNaturalReconciliationOutcome::ExactAbandoned { .. }));
}

#[test]
fn restoring_live_claim_excludes_the_canonical_candidate_without_catalog_refresh() {
    let fixture = Fixture::new(6);
    let target = fixture.publish_pristine_thread(17, 1);
    let bootstrap = fixture
        .state
        .session()
        .minimal_bootstrap(&fixture.store)
        .unwrap()
        .unwrap();
    execute_contribution(
        &fixture.store,
        fixture.state.session().create_claimed_window(
            fixture.state.session().revision(&fixture.store).unwrap(),
            CreateClaimedWindow::new(
                bootstrap.header().revision(),
                WindowId::from_bytes([27; 16]),
                RememberedTarget::new(fixture.runtime_id, fixture.root_id),
                target,
                placement(),
            ),
        ),
    );
    let bootstrap = fixture
        .state
        .session()
        .minimal_bootstrap(&fixture.store)
        .unwrap()
        .unwrap();
    execute_contribution(
        &fixture.store,
        fixture.state.session().begin_restore(
            fixture.state.session().revision(&fixture.store).unwrap(),
            beryl_state::BeginSessionRestore::new(bootstrap.header().revision()),
        ),
    );
    assert_eq!(
        fixture
            .state
            .session()
            .thread_claim_catalog_source(&fixture.store, target)
            .unwrap()
            .claim()
            .unwrap()
            .state(),
        beryl_state::ThreadClaimState::Restoring
    );
    let request = fixture.request(37);
    let RuntimeBackedWindowAcquisitionOutcome::Committed { acquisition, .. } = fixture
        .service
        .acquire(request.clone(), CommandCancellation::new())
    else {
        panic!("restoring source must remain exclusive");
    };
    assert_eq!(acquisition.thread_id(), request.fallback_thread_id());
    assert_eq!(
        acquisition.disposition(),
        RuntimeBackedWindowAcquisitionDisposition::Created
    );
}

#[test]
fn reused_summary_refresh_ack_loss_keeps_the_original_target_until_exact_reconciliation() {
    let fixture = Fixture::new(7);
    let target = create_without_projection(&fixture, 18, 1);
    empty_after_edit::publish_empty_after_typing(
        &fixture.store,
        &fixture.state,
        &fixture.syndic,
        target,
    );
    let request = fixture.request(38);
    let blocker = fixture.faults.block_next(FaultPoint::BeforeCommit);
    let service = fixture.service.clone();
    let worker_request = request.clone();
    let worker =
        std::thread::spawn(move || service.acquire(worker_request, CommandCancellation::new()));
    assert!(blocker.wait_until_reached(Duration::from_secs(10)));
    fixture
        .faults
        .fail_next(FaultPoint::AfterCommitBeforePersist);
    blocker.release();
    let outcome = worker.join().unwrap();
    let RuntimeBackedWindowAcquisitionOutcome::Indeterminate { reconciliation, .. } = outcome
    else {
        panic!("original reuse must retain its indeterminate outcome: {outcome:?}");
    };
    assert!(matches!(
        fixture
            .service
            .acquire(request.clone(), CommandCancellation::new()),
        RuntimeBackedWindowAcquisitionOutcome::NotCommitted {
            evidence: RuntimeBackedWindowAcquisitionNotCommitted::DuplicateWindowIdentity,
            ..
        }
    ));
    let RuntimeBackedWindowAcquisitionReconciliationOutcome::ExactNew { acquisition, .. } =
        reconciliation.reconcile(&fixture.store)
    else {
        panic!("original reuse outcome must reconcile exactly");
    };
    assert_eq!(acquisition.thread_id(), target);
    assert_eq!(
        acquisition.disposition(),
        RuntimeBackedWindowAcquisitionDisposition::Reused
    );
    let beryl_app::window_acquisition::RuntimeBackedWindowAbandonmentPreparationOutcome::ExactAcquired { abandonment } = fixture.service.prepare_abandonment(acquisition, CommandCancellation::new())
        else { panic!("original reconciled target proof must survive"); };
    assert!(matches!(
        fixture
            .service
            .abandon(abandonment, CommandCancellation::new()),
        beryl_app::window_acquisition::RuntimeBackedWindowAbandonmentOutcome::Committed { .. }
    ));
}

#[test]
fn coincident_reserved_fallback_identity_refuses_the_elected_target_without_substitution() {
    for collide_draft_only in [false, true] {
        let fixture = Fixture::new(8);
        let target = create_without_projection(&fixture, 19, 1);
        let draft = SyndicDraftId::from_bytes([20; 16]);
        let request = RuntimeBackedWindowAcquisitionRequest::new(
            WindowId::from_bytes([39; 16]),
            RememberedTarget::new(fixture.runtime_id, fixture.root_id),
            placement(),
            if collide_draft_only {
                SyndicThreadId::from_bytes([40; 16])
            } else {
                target
            },
            if collide_draft_only {
                draft
            } else {
                SyndicDraftId::from_bytes([41; 16])
            },
            fixture.execution.clone(),
            SyndicTimestamp::from_unix_millis(100),
            history_policy(),
        )
        .unwrap();
        let before = fixture.store.home_revision().unwrap();
        assert!(matches!(
            fixture
                .service
                .acquire(request.clone(), CommandCancellation::new()),
            RuntimeBackedWindowAcquisitionOutcome::NotCommitted {
                evidence: RuntimeBackedWindowAcquisitionNotCommitted::WindowIdentityCollision,
                ..
            }
        ));
        assert_eq!(fixture.store.home_revision().unwrap(), before);
        assert!(
            fixture
                .state
                .catalog()
                .row(
                    &fixture.store,
                    target,
                    CatalogPointReadLimit::schema_maximum()
                )
                .unwrap()
                .is_none()
        );
        assert_eq!(
            fixture
                .syndic
                .current_draft(
                    &fixture.store,
                    target,
                    syndic_storage::SyndicPointReadLimit::new(65536).unwrap()
                )
                .unwrap()
                .unwrap()
                .draft()
                .id(),
            draft
        );
        assert!(!matches!(
            fixture.service.reconcile_natural_state(&request),
            RuntimeBackedWindowAcquisitionNaturalReconciliationOutcome::ExactCommitted { .. }
        ));
    }
}
