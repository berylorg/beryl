use super::*;

#[test]
fn sealed_text_import_is_bounded_exact_clean_and_undo_free() {
    let input = "a\r\n🙂e\u{301}\r\n".repeat(10_000);
    let f = Fixture::new(
        "sealed-text",
        ComposerPayload::new(vec![ComposerAtom::text(&input).unwrap()]).unwrap(),
        vec![],
        65_536,
    );
    let p = f.prepare();
    let original = ordinary::current(&f.storage, &f.store, f.thread);
    begin(&f, &p);
    assert_eq!(ordinary::current(&f.storage, &f.store, f.thread), original);
    let root = advance_all(&f, &p);
    assert_eq!(text(&f, root), input.as_bytes());
    let asset_revision = f.state.assets().revision(&f.store).unwrap();
    let started = start(&f, &p);
    let head = f.head();
    assert!(head.active_operation().is_none());
    assert_eq!(head.published_root(), root);
    assert_eq!(head.newest_root(), root);
    assert_eq!(head.published_history(), head.newest_history());
    assert_eq!(head.dirty_generation(), f.session.dirty_generation());
    assert_eq!(
        head.newest_candidate_generation(),
        f.session.newest_candidate_generation() + 1
    );
    assert!(matches!(
        head.newest_history().key(),
        DraftEditHistoryFrontierKeyV1::SealedImportBaseline { .. }
    ));
    assert!(!head.newest_history().availability().undo_available());
    assert!(!head.newest_history().availability().redo_available());
    let current = ordinary::current(&f.storage, &f.store, f.thread);
    assert_eq!(
        current.draft().submission_intent(),
        DraftSubmissionIntent::Replacement(f.intent)
    );
    assert_eq!(current.thread(), original.thread());
    assert_eq!(current.draft().history(), head.newest_history());
    assert!(
        f.state
            .assets()
            .owner_head(&f.store, AssetOwner::CurrentDraft(head.draft_id()))
            .unwrap()
            .is_none()
    );
    assert!(
        f.state
            .assets()
            .owner_head(&f.store, AssetOwner::SubmittedTurnItem(f.item))
            .unwrap()
            .is_none()
    );
    assert_eq!(
        f.state.assets().revision(&f.store).unwrap(),
        asset_revision.checked_next().unwrap()
    );
    let DraftPieceSettlementClosureV1::SealedImport(c) = started.settlement().closure() else {
        panic!();
    };
    assert!(c.baseline().journal_head().is_none());
    assert!(c.baseline().retained_encoded_bytes() > 0);
    assert!(c.session_fork().retained_encoded_bytes() > 0);
    assert_ne!(
        c.baseline().retained_encoded_bytes(),
        c.session_fork().retained_encoded_bytes()
    );
}

#[test]
fn clean_cancellation_keeps_the_imported_root_and_baseline() {
    let f = Fixture::new(
        "clean-cancel",
        ComposerPayload::new(vec![ComposerAtom::text("copy\r\n🙂").unwrap()]).unwrap(),
        vec![],
        65_536,
    );
    let p = f.prepare();
    begin(&f, &p);
    advance_all(&f, &p);
    start(&f, &p);
    let before = f.head();
    let before_text = text(&f, before.newest_root());
    let summary = f
        .storage
        .history_summary(
            &f.store,
            f.thread,
            SyndicPointReadLimit::new(65_536).unwrap(),
        )
        .unwrap();
    let receipt = cancel(&f, &before, 211);
    let after = f.head();
    assert_eq!(after.newest_root(), before.newest_root());
    assert_eq!(after.published_root(), before.published_root());
    assert_eq!(after.newest_history(), before.newest_history());
    assert_eq!(after.published_history(), before.published_history());
    assert_eq!(
        after.newest_candidate_generation(),
        before.newest_candidate_generation()
    );
    assert_eq!(after.dirty_generation(), before.dirty_generation());
    assert_eq!(after.session_generation(), before.session_generation() + 1);
    assert_eq!(text(&f, after.newest_root()), before_text);
    assert_eq!(
        after.durable_base_selector_revision(),
        before.durable_base_selector_revision()
    );
    assert_eq!(
        ordinary::current(&f.storage, &f.store, f.thread)
            .draft()
            .submission_intent(),
        DraftSubmissionIntent::Ordinary
    );
    assert_eq!(
        f.storage
            .history_summary(
                &f.store,
                f.thread,
                SyndicPointReadLimit::new(65_536).unwrap()
            )
            .unwrap(),
        summary
    );
    assert_eq!(
        after.published_selector_revision(),
        receipt.receipt().selector().selector_revision()
    );
}

#[test]
fn nonselecting_termination_releases_custody_and_preserves_published_state() {
    let f = Fixture::new(
        "import-termination",
        ComposerPayload::new(vec![ComposerAtom::text("kept").unwrap()]).unwrap(),
        vec![],
        65_536,
    );
    let before = ordinary::current(&f.storage, &f.store, f.thread);
    let p = f.prepare();
    begin(&f, &p);
    let key = DraftPieceSettlementKeyV1::new(
        f.session.draft_id(),
        f.session.session_id(),
        p.operation_id(),
    );
    let termination = f
        .storage
        .prepare_replacement_import_termination(&f.store, key)
        .unwrap();
    let outcome = execute(
        &f.store,
        f.storage.terminate_replacement_draft_import(
            f.storage.revision(&f.store).unwrap(),
            termination.clone(),
        ),
    );
    assert_eq!(
        f.storage
            .reconcile_replacement_import_termination(&f.store, &termination, outcome)
            .unwrap(),
        DraftReplacementCommandOutcomeV1::ImportTerminated
    );
    assert_eq!(ordinary::current(&f.storage, &f.store, f.thread), before);
    let after = f.head();
    assert!(after.active_operation().is_none());
    assert_eq!(after.newest_root(), f.session.newest_root());
    assert_eq!(after.newest_history(), f.session.newest_history());
    assert!(
        f.storage
            .prepare_replacement_import_step(&f.store, &p)
            .is_err()
    );
}
