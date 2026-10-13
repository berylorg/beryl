use super::*;

#[test]
fn retained_advance_is_rejected_by_an_identical_foreign_home() {
    let payload = ComposerPayload::new(vec![ComposerAtom::text("same source").unwrap()]).unwrap();
    let a = Fixture::new("source-scope-a", payload.clone(), vec![], 65_536);
    let b = Fixture::new("source-scope-b", payload, vec![], 65_536);
    let pa = a.prepare();
    let pb = b.prepare();
    assert_eq!(pa.canonical_header(), pb.canonical_header());
    begin(&a, &pa);
    begin(&b, &pb);
    let before = b.head();
    assert!(matches!(
        b.storage.prepare_replacement_import_step(&b.store, &pa),
        Err(DraftReplacementErrorV1::Proof)
    ));
    let DraftReplacementAdvancePreparationV1::Ready(step) = a
        .storage
        .prepare_replacement_import_step(&a.store, &pa)
        .unwrap()
    else {
        panic!()
    };
    let outcome = execute(
        &b.store,
        b.storage
            .advance_replacement_draft_import(b.storage.revision(&b.store).unwrap(), step),
    );
    assert!(
        matches!(outcome, CommandOutcome::NotCommitted { .. }),
        "{outcome:?}"
    );
    assert_eq!(b.head(), before);
}

#[test]
fn reopening_retires_import_execution_but_preserves_nonselecting_cleanup() {
    let f = Fixture::new(
        "retired-import",
        ComposerPayload::new(vec![ComposerAtom::text("retained").unwrap()]).unwrap(),
        vec![],
        65_536,
    );
    let p = f.prepare();
    begin(&f, &p);
    let before = f.head();
    let DraftReplacementAdvancePreparationV1::Ready(step) = f
        .storage
        .prepare_replacement_import_step(&f.store, &p)
        .unwrap()
    else {
        panic!()
    };
    let f = f.reopen();
    assert_eq!(f.head(), before);
    assert!(matches!(
        f.storage.prepare_replacement_import_step(&f.store, &p),
        Err(DraftReplacementErrorV1::Proof)
    ));
    let outcome = execute(
        &f.store,
        f.storage
            .advance_replacement_draft_import(f.storage.revision(&f.store).unwrap(), step),
    );
    assert!(
        matches!(outcome, CommandOutcome::NotCommitted { .. }),
        "{outcome:?}"
    );
    assert_eq!(f.head(), before);
    let key = DraftPieceSettlementKeyV1::new(
        f.session.draft_id(),
        f.session.session_id(),
        p.operation_id(),
    );
    let prepared = f
        .storage
        .prepare_replacement_import_termination(&f.store, key)
        .unwrap();
    let outcome = execute(
        &f.store,
        f.storage.terminate_replacement_draft_import(
            f.storage.revision(&f.store).unwrap(),
            prepared.clone(),
        ),
    );
    assert_eq!(
        f.storage
            .reconcile_replacement_import_termination(&f.store, &prepared, outcome)
            .unwrap(),
        DraftReplacementCommandOutcomeV1::ImportTerminated
    );
}

#[test]
fn original_start_outcome_survives_reopen_and_foreign_outcome_lookup_rejects() {
    let payload = ComposerPayload::new(vec![ComposerAtom::text("durable copy").unwrap()]).unwrap();
    let f = Fixture::new("start-reopen", payload.clone(), vec![], 65_536);
    let p = f.prepare();
    begin(&f, &p);
    advance_all(&f, &p);
    let prepared = f
        .storage
        .prepare_replacement_draft_start(&f.store, &p, SyndicTimestamp::from_unix_millis(3))
        .unwrap();
    let outcome = f.store.execute(start_command(&f, &prepared));
    assert!(matches!(outcome, CommandOutcome::Committed { .. }));
    let f = f.reopen();
    assert_eq!(
        f.storage
            .reconcile_replacement_draft_start(&f.store, &prepared, outcome)
            .unwrap(),
        DraftReplacementCommandOutcomeV1::Started(prepared.selector())
    );
    let head = f.head();
    assert!(
        f.storage
            .draft_editor_candidate_is_saved(
                &f.store,
                DraftEditorCandidateActivationBindingV1::from_head(&head),
                ordinary::selector(&ordinary::current(&f.storage, &f.store, f.thread))
            )
            .unwrap()
    );
    let retired = execute(
        &f.store,
        f.storage
            .start_replacement_draft(f.storage.revision(&f.store).unwrap(), prepared.clone()),
    );
    assert!(matches!(retired, CommandOutcome::NotCommitted { .. }));
    assert_eq!(
        f.storage
            .reconcile_replacement_draft_start(&f.store, &prepared, retired)
            .unwrap(),
        DraftReplacementCommandOutcomeV1::Started(prepared.selector())
    );
    assert_eq!(f.head(), head);
    let foreign = Fixture::new("start-outcome-foreign", payload, vec![], 65_536);
    let rejected = execute(
        &foreign.store,
        foreign.storage.start_replacement_draft(
            foreign.storage.revision(&foreign.store).unwrap(),
            prepared.clone(),
        ),
    );
    assert!(matches!(
        foreign
            .storage
            .reconcile_replacement_draft_start(&foreign.store, &prepared, rejected),
        Err(DraftReplacementErrorV1::Proof)
    ));
    let cancellation = cancel(&f, &head, 237);
    let before = f.head();
    let f = f.reopen();
    assert_eq!(f.head(), before);
    let opened = ordinary::open_session(
        &f.storage,
        &f.store,
        &ordinary::current(&f.storage, &f.store, f.thread),
        238,
        239,
    );
    assert!(
        f.storage
            .draft_editor_candidate_is_saved(
                &f.store,
                DraftEditorCandidateActivationBindingV1::from_head(&opened),
                ordinary::selector(&ordinary::current(&f.storage, &f.store, f.thread))
            )
            .unwrap()
    );
    let rejected = execute(
        &f.store,
        f.storage
            .cancel_replacement_intent(f.storage.revision(&f.store).unwrap(), cancellation.clone()),
    );
    assert!(matches!(rejected, CommandOutcome::NotCommitted { .. }));
    assert_eq!(
        f.storage
            .reconcile_replacement_intent_cancellation(&f.store, &cancellation, rejected)
            .unwrap(),
        DraftReplacementCommandOutcomeV1::IntentCancelled(cancellation.receipt().selector())
    );
}
