use super::*;

fn edit(
    f: &Fixture,
    head: &DraftEditorCandidateSessionV1,
    op: u8,
    text: &str,
) -> DraftPieceCommittedAdoptionV1 {
    let tx = ordinary::transaction(
        &f.storage,
        &f.store,
        head,
        op,
        text,
        point(text.len() as u64),
    );
    ordinary::build(&f.storage, &f.store, &tx);
    committed(execute(
        &f.store,
        f.storage
            .settle_draft_piece_edit(f.storage.revision(&f.store).unwrap(), tx.prepared.clone()),
    ));
    let settlement = ordinary::settled(&f.storage, &f.store, &tx);
    let DraftPieceSettlementClosureV1::Committed(adoption) = settlement.closure() else {
        panic!("first real edit failed: {:?}", settlement.outcome())
    };
    adoption.clone()
}

#[test]
fn imported_baseline_first_real_edit_dirty_cancellation_and_undo_preserve_content_and_positions() {
    let f = Fixture::new(
        "baseline-real-edit",
        ComposerPayload::new(vec![ComposerAtom::text("original\r\n🙂").unwrap()]).unwrap(),
        vec![],
        65_536,
    );
    let p = f.prepare();
    begin(&f, &p);
    let imported = advance_all(&f, &p);
    let start = start(&f, &p);
    let baseline = f.head();
    let adoption = edit(&f, &baseline, 234, "prefix");
    let dirty = f.head();
    assert_eq!(adoption.transition().predecessor_root(), imported);
    assert_eq!(adoption.transition().before_caret(), point(0));
    assert!(matches!(
        dirty.newest_history().key(),
        DraftEditHistoryFrontierKeyV1::Session { .. }
    ));
    assert!(dirty.newest_history().availability().undo_available());
    assert_eq!(
        text(&f, dirty.newest_root()),
        "prefixoriginal\r\n🙂".as_bytes()
    );
    let cancellation = cancel(&f, &dirty, 235);
    let cancelled = f.head();
    assert_eq!(cancelled.newest_history(), dirty.newest_history());
    assert_eq!(cancelled.newest_root(), dirty.newest_root());
    assert_eq!(cancelled.dirty_generation(), dirty.dirty_generation());
    assert_eq!(
        cancelled.newest_candidate_generation(),
        dirty.newest_candidate_generation()
    );
    assert_eq!(
        ordinary::current(&f.storage, &f.store, f.thread)
            .draft()
            .piece_root(),
        imported
    );
    let intent = DraftHistoricalRootSelectionIntentV1::new(
        DraftEditorCandidateActivationBindingV1::from_head(&cancelled),
        operation(236),
        DraftHistoricalRootDirectionV1::Undo,
    );
    let DraftHistoricalRootSelectionV1::Prepared(undo) = f
        .storage
        .prepare_draft_historical_root_selection(&f.store, intent)
        .unwrap()
    else {
        panic!("undo unavailable")
    };
    committed(execute(
        &f.store,
        f.storage
            .adopt_draft_historical_root(f.storage.revision(&f.store).unwrap(), undo),
    ));
    let undone = f.head();
    assert_eq!(undone.newest_root(), imported);
    assert!(undone.newest_history().availability().redo_available());
    assert_eq!(text(&f, undone.newest_root()), "original\r\n🙂".as_bytes());
    let replay = execute(
        &f.store,
        f.storage
            .start_replacement_draft(f.storage.revision(&f.store).unwrap(), start.clone()),
    );
    assert!(
        matches!(
            replay,
            CommandOutcome::Committed { .. } | CommandOutcome::NotCommitted { .. }
        ),
        "{replay:?}"
    );
    assert_eq!(
        f.storage
            .reconcile_replacement_draft_start(&f.store, &start, replay)
            .unwrap(),
        DraftReplacementCommandOutcomeV1::Started(start.selector())
    );
    let replay = execute(
        &f.store,
        f.storage
            .cancel_replacement_intent(f.storage.revision(&f.store).unwrap(), cancellation.clone()),
    );
    assert!(
        matches!(
            replay,
            CommandOutcome::Committed { .. } | CommandOutcome::NotCommitted { .. }
        ),
        "{replay:?}"
    );
    assert_eq!(
        f.storage
            .reconcile_replacement_intent_cancellation(&f.store, &cancellation, replay)
            .unwrap(),
        DraftReplacementCommandOutcomeV1::IntentCancelled(cancellation.receipt().selector())
    );
    assert_eq!(f.head(), undone);
}
