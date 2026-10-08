use super::*;
use beryl_state::{CreateClaimedWindow, RemoveSessionWindow};

#[test]
fn same_window_discovers_missing_catalog_target_and_publishes_it_atomically() {
    let fixture = Fixture::new();
    let prior = fixture.commit(fixture.prepare(None, 4));
    disqualify_submission(&fixture, 4);
    assert!(
        fixture
            .state
            .catalog()
            .row(
                &fixture.store,
                thread(3),
                CatalogPointReadLimit::schema_maximum()
            )
            .unwrap()
            .is_none()
    );
    let before = fixture.store.home_revision().unwrap();
    let commit = settled(&fixture, prepared(&fixture, Some(prior.selection), 9));
    assert_eq!(commit.disposition, SameWindowThreadDisposition::Reused);
    assert_eq!(commit.selection.thread_id(), thread(3));
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
                thread(3),
                CatalogPointReadLimit::schema_maximum()
            )
            .unwrap()
            .unwrap()
            .facts()
            .claim(),
        CatalogClaimSummary::claimed(fixture.window, CatalogClaimKind::Active)
    );
}

#[test]
fn same_window_uses_live_absence_after_claim_removal_despite_cached_claimed_row() {
    let fixture = Fixture::new();
    let prior = fixture.commit(fixture.prepare(None, 4));
    disqualify_submission(&fixture, 4);
    let other = WindowId::from_bytes([8; 16]);
    let bootstrap = fixture
        .state
        .session()
        .minimal_bootstrap(&fixture.store)
        .unwrap()
        .unwrap();
    execute(
        &fixture.store,
        fixture.state.session().create_claimed_window(
            fixture.state.session().revision(&fixture.store).unwrap(),
            CreateClaimedWindow::new(
                bootstrap.header().revision(),
                other,
                fixture.target,
                thread(3),
                placement(),
            ),
        ),
    );
    project(&fixture, 3);
    let removal = fixture
        .state
        .session()
        .capture_window_removal(&fixture.store, other)
        .unwrap();
    execute(
        &fixture.store,
        fixture.state.session().remove_window(
            fixture.state.session().revision(&fixture.store).unwrap(),
            RemoveSessionWindow::new(
                removal.header().revision(),
                other,
                removal.window().revision(),
                removal.window().selected_thread(),
            ),
        ),
    );
    assert!(
        fixture
            .state
            .session()
            .thread_claim_catalog_source(&fixture.store, thread(3))
            .unwrap()
            .claim()
            .is_none()
    );
    assert!(matches!(
        fixture
            .state
            .catalog()
            .row(
                &fixture.store,
                thread(3),
                CatalogPointReadLimit::schema_maximum()
            )
            .unwrap()
            .unwrap()
            .facts()
            .claim(),
        CatalogClaimSummary::Claimed { .. }
    ));
    let commit = settled(&fixture, prepared(&fixture, Some(prior.selection), 9));
    assert_eq!(commit.selection.thread_id(), thread(3));
    assert_eq!(commit.disposition, SameWindowThreadDisposition::Reused);
}

#[test]
fn same_window_pages_canonical_identities_to_elect_the_global_oldest_empty_thread() {
    let fixture = Fixture::new();
    let prior = fixture.commit(fixture.prepare(None, 4));
    disqualify_submission(&fixture, 4);
    for index in 0..17_u8 {
        let seed = 10 + index;
        execute(
            &fixture.store,
            fixture.syndic.create_thread(
                fixture.syndic.revision(&fixture.store).unwrap(),
                CreateThread::ordinary(
                    thread(seed),
                    SyndicDraftId::from_bytes([seed; 16]),
                    ExecutionBinding::new(
                        fixture.target.runtime_id(),
                        fixture.target.root_id(),
                        native(r"C:\Work\Beryl"),
                    ),
                    SyndicTimestamp::from_unix_millis(u64::from(17 - index)),
                    DraftEditHistoryPolicyV1::new(65536, 1).unwrap(),
                ),
            ),
        );
    }
    let commit = settled(&fixture, prepared(&fixture, Some(prior.selection), 9));
    assert_eq!(commit.selection.thread_id(), thread(26));
    assert_eq!(commit.disposition, SameWindowThreadDisposition::Reused);
}

#[test]
fn same_window_reuses_a_draft_emptied_by_real_unsubmitted_text_edits() {
    let fixture = Fixture::new();
    let prior = fixture.commit(fixture.prepare(None, 4));
    disqualify_submission(&fixture, 4);
    empty_after_edit::publish_empty_after_typing(
        &fixture.store,
        &fixture.state,
        &fixture.syndic,
        thread(3),
    );
    let before = fixture
        .syndic
        .current_draft(
            &fixture.store,
            thread(3),
            syndic_storage::SyndicPointReadLimit::new(65536).unwrap(),
        )
        .unwrap()
        .unwrap();
    let commit = settled(&fixture, prepared(&fixture, Some(prior.selection), 9));
    assert_eq!(commit.disposition, SameWindowThreadDisposition::Reused);
    assert_eq!(commit.selection.thread_id(), thread(3));
    let after = fixture
        .syndic
        .current_draft(
            &fixture.store,
            thread(3),
            syndic_storage::SyndicPointReadLimit::new(65536).unwrap(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(after.draft().piece_root(), before.draft().piece_root());
    assert_eq!(after.draft().history(), before.draft().history());
}
