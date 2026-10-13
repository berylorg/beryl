#![cfg(feature = "test-faults")]

#[path = "replacement_drafts/assets.rs"]
mod assets;
#[path = "replacement_drafts/fixture.rs"]
mod fixture;
#[path = "replacement_drafts/flow.rs"]
mod flow;
#[path = "draft_edit_history/support.rs"]
mod ordinary;
mod support;

use beryl_home_store::test_faults::{FaultController, FaultPoint};
use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    MutationContribution,
};
use beryl_model::{
    AssetId, AssetReferenceSetId, ImageLabelOrdinal, OrderedMarkerAssetSummaryV1,
    SealedAssetReferenceSetProof, SequentialMarkerSummaryV1, SyndicDraftMarkerId, SyndicItemId,
    SyndicTurnId,
};
use beryl_state::{AssetOwner, AssetOwnerHeadUpdate, BerylState, UpdateAssetOwnerHeads};
use fixture::*;
use flow::*;
use syndic_storage::test_faults::{
    FixtureRecord, fixture_advance_item_projection_digest, fixture_advance_transcript_digest,
    fixture_inline_paragraph_projection, fixture_item_projection_digest_seed,
    fixture_transcript_digest_seed,
};
use syndic_storage::*;

fn execute(store: &HomeStore, p: MutationContribution) -> CommandOutcome {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(p).unwrap();
    store.execute(command)
}
fn committed(outcome: CommandOutcome) {
    assert!(
        matches!(
            outcome,
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ),
        "{outcome:?}"
    );
}
fn point(offset: u64) -> DraftCompositePositionV1 {
    DraftCompositePositionV1::new(offset, DraftCompositeGapWitnessV1::Unambiguous)
}
fn operation(byte: u8) -> DraftPieceOperationIdV1 {
    DraftPieceOperationIdV1::from_bytes([byte; 16])
}
fn fixture(name: &str) -> Fixture {
    Fixture::new(
        name,
        ComposerPayload::new(vec![
            ComposerAtom::text("recover exact historical source").unwrap(),
        ])
        .unwrap(),
        vec![],
        65_536,
    )
}

fn recover_candidate(
    f: Fixture,
) -> (
    support::TestHome,
    beryl_home_store::HomeRecoveryCandidate,
    SyndicStorage,
) {
    if f.store.health().state() != beryl_home_store::HomeHealthState::Failed {
        f.faults.fail_next(FaultPoint::BeforeReadConfirmation);
        assert!(
            f.storage
                .thread(
                    &f.store,
                    f.thread,
                    SyndicPointReadLimit::new(65_536).unwrap()
                )
                .is_err()
        );
    }
    assert_eq!(
        f.store.health().state(),
        beryl_home_store::HomeHealthState::Failed
    );
    let Fixture {
        home,
        store,
        storage,
        state,
        ..
    } = f;
    drop(state);
    (home, store.recover_same_home().unwrap(), storage)
}

#[test]
fn committed_start_is_qualified_before_candidate_publication_without_reexecution() {
    let f = fixture("candidate-historical-start");
    let import = f.prepare();
    begin(&f, &import);
    advance_all(&f, &import);
    let prepared = start(&f, &import);
    let (_home, mut candidate, old_storage) = recover_candidate(f);
    let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let access = candidate.recovery_access().unwrap();
    assert!(
        old_storage
            .resume_replacement_draft_start_candidate(&access, &prepared)
            .is_err()
    );
    assert_eq!(
        storage
            .resume_replacement_draft_start_candidate(&access, &prepared)
            .unwrap(),
        DraftReplacementCommandOutcomeV1::Started(prepared.selector())
    );
    assert!(
        storage
            .prepare_replacement_import_termination_candidate(&access, &import)
            .is_err()
    );
    drop(access);
    drop(candidate.publish().unwrap());
}

#[test]
fn original_start_scope_settles_before_saved_successor_is_visible_to_recovery() {
    let f = fixture("candidate-historical-start-pending");
    let import = f.prepare();
    begin(&f, &import);
    advance_all(&f, &import);
    let prepared = f
        .storage
        .prepare_replacement_draft_start(&f.store, &import, SyndicTimestamp::from_unix_millis(3))
        .unwrap();
    let command = start_command(&f, &prepared);
    f.faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let outcome = f.store.execute(command);
    assert!(matches!(
        outcome,
        CommandOutcome::Indeterminate { .. } | CommandOutcome::Committed { .. }
    ));
    let _ = f
        .storage
        .reconcile_replacement_draft_start(&f.store, &prepared, outcome);
    let (_home, mut candidate, _) = recover_candidate(f);
    let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    if !candidate
        .recovery_access()
        .unwrap()
        .pending_reconciliations()
        .is_empty()
    {
        let failure = candidate.publish().unwrap_err();
        candidate = failure.into_parts().1;
    }
    let access = candidate.recovery_access().unwrap();
    assert_eq!(
        storage
            .resume_replacement_draft_start_candidate(&access, &prepared)
            .unwrap(),
        DraftReplacementCommandOutcomeV1::Started(prepared.selector())
    );
    assert!(access.pending_reconciliations().is_empty());
    drop(access);
    drop(candidate.publish().unwrap());
}

#[test]
fn fresh_candidate_termination_releases_only_the_original_import_custody() {
    let f = fixture("candidate-historical-termination");
    let import = f.prepare();
    begin(&f, &import);
    advance_one(&f, &import);
    let before = f.head();
    let old = f
        .storage
        .prepare_replacement_import_termination(
            &f.store,
            DraftPieceSettlementKeyV1::new(
                import.basis().selector().draft_id(),
                import.session_id(),
                import.operation_id(),
            ),
        )
        .unwrap();
    let (_home, mut candidate, _) = recover_candidate(f);
    let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let access = candidate.recovery_access().unwrap();
    let revision = storage.revision_candidate(&access).unwrap();
    assert!(
        storage
            .terminate_replacement_draft_import_candidate(&access, revision, old)
            .is_err()
    );
    let prepared = storage
        .prepare_replacement_import_termination_candidate(&access, &import)
        .unwrap();
    let DraftPieceSettlementClosureV1::Noncommit(closure) = prepared.settlement().closure() else {
        panic!()
    };
    assert_eq!(
        closure.observed_session().newest_root(),
        before.newest_root()
    );
    assert_eq!(
        closure.observed_session().newest_history(),
        before.newest_history()
    );
    assert!(closure.observed_session().active_operation().is_none());
    let mut command = HomeCommand::new(access.home_revision().unwrap());
    command
        .add(
            storage
                .terminate_replacement_draft_import_candidate(&access, revision, prepared.clone())
                .unwrap(),
        )
        .unwrap();
    assert_eq!(
        storage
            .reconcile_replacement_import_termination_candidate(
                &access,
                &prepared,
                access.execute(command)
            )
            .unwrap(),
        DraftReplacementCommandOutcomeV1::ImportTerminated
    );
    assert_eq!(
        storage
            .resume_replacement_import_termination_candidate(&access, &prepared)
            .unwrap(),
        DraftReplacementCommandOutcomeV1::ImportTerminated
    );
    drop(access);
    drop(candidate.publish().unwrap());
}

#[test]
fn metadata_cancellation_retains_its_exact_original_receipt_across_candidate_recovery() {
    let f = fixture("candidate-historical-cancellation");
    let import = f.prepare();
    begin(&f, &import);
    advance_all(&f, &import);
    start(&f, &import);
    let prepared = cancel(&f, &f.head(), 227);
    let (_home, mut candidate, _) = recover_candidate(f);
    let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    assert_eq!(
        storage
            .resume_replacement_intent_cancellation_candidate(
                &candidate.recovery_access().unwrap(),
                &prepared
            )
            .unwrap(),
        DraftReplacementCommandOutcomeV1::IntentCancelled(prepared.receipt().selector())
    );
}

#[test]
fn foreign_candidate_cannot_classify_or_terminate_retained_historical_work() {
    let f = fixture("candidate-historical-foreign-origin");
    let import = f.prepare();
    begin(&f, &import);
    advance_all(&f, &import);
    let prepared = start(&f, &import);
    let foreign = fixture("candidate-historical-foreign-destination");
    let (_home, mut candidate, _) = recover_candidate(foreign);
    let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let access = candidate.recovery_access().unwrap();
    assert!(matches!(
        storage.resume_replacement_draft_start_candidate(&access, &prepared),
        Err(DraftReplacementErrorV1::Proof)
    ));
    assert!(matches!(
        storage.prepare_replacement_import_termination_candidate(&access, &import),
        Err(DraftReplacementErrorV1::Proof)
    ));
    assert_eq!(
        f.storage
            .resume_replacement_draft_start(&f.store, &prepared)
            .unwrap(),
        DraftReplacementCommandOutcomeV1::Started(prepared.selector())
    );
}

#[test]
fn a_receipt_without_original_observation_cannot_classify_the_start() {
    let f = fixture("candidate-historical-unobserved-start");
    let import = f.prepare();
    begin(&f, &import);
    advance_all(&f, &import);
    let prepared = f
        .storage
        .prepare_replacement_draft_start(&f.store, &import, SyndicTimestamp::from_unix_millis(3))
        .unwrap();
    let outcome = f.store.execute(start_command(&f, &prepared));
    assert!(matches!(outcome, CommandOutcome::Committed { .. }));
    let (_home, mut candidate, _) = recover_candidate(f);
    let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let access = candidate.recovery_access().unwrap();
    assert!(matches!(
        storage.resume_replacement_draft_start_candidate(&access, &prepared),
        Err(DraftReplacementErrorV1::Unavailable)
    ));
    assert_eq!(
        storage
            .reconcile_replacement_draft_start_candidate(&access, &prepared, outcome)
            .unwrap(),
        DraftReplacementCommandOutcomeV1::Started(prepared.selector())
    );
}

#[test]
fn proven_original_noncommit_allows_only_nonselecting_candidate_cleanup() {
    let f = fixture("candidate-historical-noncommit-start");
    let import = f.prepare();
    begin(&f, &import);
    advance_all(&f, &import);
    let prepared = f
        .storage
        .prepare_replacement_draft_start(&f.store, &import, SyndicTimestamp::from_unix_millis(3))
        .unwrap();
    let command = start_command(&f, &prepared);
    f.faults.fail_next(FaultPoint::BeforeCommit);
    let outcome = f.store.execute(command);
    assert!(matches!(outcome, CommandOutcome::NotCommitted { .. }));
    let _ = f
        .storage
        .reconcile_replacement_draft_start(&f.store, &prepared, outcome);
    let (_home, mut candidate, _) = recover_candidate(f);
    let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let access = candidate.recovery_access().unwrap();
    assert_eq!(
        storage
            .resume_replacement_draft_start_candidate(&access, &prepared)
            .unwrap(),
        DraftReplacementCommandOutcomeV1::NotCommitted
    );
    let cleanup = storage
        .prepare_replacement_import_termination_candidate(&access, &import)
        .unwrap();
    let DraftPieceSettlementClosureV1::Noncommit(closure) = cleanup.settlement().closure() else {
        panic!()
    };
    assert_eq!(
        closure.observed_session().newest_root(),
        import.basis().selector().root()
    );
    let mut command = HomeCommand::new(access.home_revision().unwrap());
    command
        .add(
            storage
                .terminate_replacement_draft_import_candidate(
                    &access,
                    storage.revision_candidate(&access).unwrap(),
                    cleanup.clone(),
                )
                .unwrap(),
        )
        .unwrap();
    assert_eq!(
        storage
            .reconcile_replacement_import_termination_candidate(
                &access,
                &cleanup,
                access.execute(command)
            )
            .unwrap(),
        DraftReplacementCommandOutcomeV1::ImportTerminated
    );
}
