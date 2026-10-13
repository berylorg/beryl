use super::*;

fn interrupt_observation(f: &Fixture, outcome: &CommandOutcome) {
    if f.store.health().state() == beryl_home_store::HomeHealthState::Healthy {
        f.faults
            .fail_next(if matches!(outcome, CommandOutcome::Indeterminate { .. }) {
                FaultPoint::BeforeReconciliationSnapshot
            } else {
                FaultPoint::BeforeReadConfirmation
            });
    }
}

#[test]
fn original_start_custody_survives_unavailable_reconciliation_and_selects_both_asset_heads_atomically()
 {
    for images in [false, true] {
        let marker = SyndicDraftMarkerId::from_bytes([241; 16]);
        let label = ImageLabelOrdinal::new(11).unwrap();
        let (payload, markers) = if images {
            (
                ComposerPayload::new(vec![ComposerAtom::image_marker(marker, label)]).unwrap(),
                vec![(marker, label)],
            )
        } else {
            (
                ComposerPayload::new(vec![ComposerAtom::text("cut\r\n🙂").unwrap()]).unwrap(),
                vec![],
            )
        };
        let f = Fixture::new("start-acknowledgement-loss", payload, markers, 65_536);
        let p = f.prepare();
        begin(&f, &p);
        advance_all(&f, &p);
        let prepared = f
            .storage
            .prepare_replacement_draft_start(&f.store, &p, SyndicTimestamp::from_unix_millis(3))
            .unwrap();
        let historical = f
            .state
            .assets()
            .owner_head(&f.store, AssetOwner::SubmittedTurnItem(f.item))
            .unwrap();
        assert!(!matches!(
            f.storage
                .inspect_replacement_draft_start(&f.store, &prepared),
            Ok(DraftReplacementCommandOutcomeV1::NotCommitted)
        ));
        f.faults.fail_next(FaultPoint::AfterCommitBeforePersist);
        let outcome = f.store.execute(start_command(&f, &prepared));
        assert!(matches!(
            f.storage
                .inspect_replacement_draft_start(&f.store, &prepared),
            Err(DraftReplacementErrorV1::Unavailable)
        ));
        assert!(
            matches!(
                outcome,
                CommandOutcome::Indeterminate { .. } | CommandOutcome::Committed { .. }
            ),
            "{outcome:?}"
        );
        let has_custody = matches!(outcome, CommandOutcome::Indeterminate { .. });
        interrupt_observation(&f, &outcome);
        let observed = f
            .storage
            .reconcile_replacement_draft_start(&f.store, &prepared, outcome);
        assert!(
            observed.is_err(),
            "failed-health read must retain original command facts: {observed:?}"
        );
        if has_custody {
            assert!(!f.store.pending_reconciliations().is_empty());
        }
        let f = f.recover();
        assert_eq!(
            f.storage
                .resume_replacement_draft_start(&f.store, &prepared)
                .unwrap(),
            DraftReplacementCommandOutcomeV1::Started(prepared.selector())
        );
        let head = f.head();
        assert_eq!(head.newest_root(), prepared.selector().root());
        assert_eq!(
            f.state
                .assets()
                .owner_head(&f.store, AssetOwner::SubmittedTurnItem(f.item))
                .unwrap(),
            historical
        );
        let current = f
            .state
            .assets()
            .owner_head(&f.store, AssetOwner::CurrentDraft(head.draft_id()))
            .unwrap();
        assert_eq!(current.as_ref().map(|h| h.set()), f.proof);
        let before = (f.head(), f.store.home_revision().unwrap());
        assert_eq!(
            f.storage
                .resume_replacement_draft_start(&f.store, &prepared)
                .unwrap(),
            DraftReplacementCommandOutcomeV1::Started(prepared.selector())
        );
        assert_eq!((f.head(), f.store.home_revision().unwrap()), before);
    }
}

#[test]
fn exact_old_start_cut_keeps_the_complete_import_prefix_and_no_draft_asset_owner() {
    let f = Fixture::new(
        "start-exact-old",
        ComposerPayload::new(vec![ComposerAtom::text("copy").unwrap()]).unwrap(),
        vec![],
        65_536,
    );
    let p = f.prepare();
    begin(&f, &p);
    advance_all(&f, &p);
    let head = f.head();
    let current = ordinary::current(&f.storage, &f.store, f.thread);
    let prepared = f
        .storage
        .prepare_replacement_draft_start(&f.store, &p, SyndicTimestamp::from_unix_millis(3))
        .unwrap();
    f.faults.fail_next(FaultPoint::BeforeCommit);
    let outcome = f.store.execute(start_command(&f, &prepared));
    assert!(
        matches!(outcome, CommandOutcome::NotCommitted { .. }),
        "{outcome:?}"
    );
    let observed = f
        .storage
        .reconcile_replacement_draft_start(&f.store, &prepared, outcome);
    if let Ok(value) = observed {
        assert_eq!(value, DraftReplacementCommandOutcomeV1::NotCommitted);
    }
    let f = f.recover();
    assert_eq!(
        f.storage
            .resume_replacement_draft_start(&f.store, &prepared)
            .unwrap(),
        DraftReplacementCommandOutcomeV1::NotCommitted
    );
    assert_eq!(f.head(), head);
    assert_eq!(ordinary::current(&f.storage, &f.store, f.thread), current);
    assert!(
        f.state
            .assets()
            .owner_head(&f.store, AssetOwner::CurrentDraft(head.draft_id()))
            .unwrap()
            .is_none()
    );
}

#[test]
fn cancellation_and_termination_resume_only_the_original_acknowledgement_loss_scope() {
    for cancel_intent in [false, true] {
        let f = Fixture::new(
            "metadata-command-ack-loss",
            ComposerPayload::new(vec![ComposerAtom::text("retained").unwrap()]).unwrap(),
            vec![],
            65_536,
        );
        let p = f.prepare();
        begin(&f, &p);
        if cancel_intent {
            advance_all(&f, &p);
            start(&f, &p);
            let head = f.head();
            let request = DraftReplacementCancellationRequestV1::new(
                ordinary::selector(&ordinary::current(&f.storage, &f.store, f.thread)),
                head.session_id(),
                head.session_generation(),
                operation(242),
                f.intent,
                SyndicTimestamp::from_unix_millis(4),
            );
            let prepared = f
                .storage
                .prepare_replacement_intent_cancellation(&f.store, request)
                .unwrap();
            f.faults.fail_next(FaultPoint::AfterCommitBeforePersist);
            let outcome = execute(
                &f.store,
                f.storage.cancel_replacement_intent(
                    f.storage.revision(&f.store).unwrap(),
                    prepared.clone(),
                ),
            );
            interrupt_observation(&f, &outcome);
            assert!(
                f.storage
                    .reconcile_replacement_intent_cancellation(&f.store, &prepared, outcome)
                    .is_err()
            );
            let f = f.recover();
            assert_eq!(
                f.storage
                    .resume_replacement_intent_cancellation(&f.store, &prepared)
                    .unwrap(),
                DraftReplacementCommandOutcomeV1::IntentCancelled(prepared.receipt().selector())
            );
            assert_eq!(f.head().newest_root(), head.newest_root());
            assert_eq!(f.head().newest_history(), head.newest_history());
        } else {
            let key = DraftPieceSettlementKeyV1::new(
                f.session.draft_id(),
                f.session.session_id(),
                p.operation_id(),
            );
            let prepared = f
                .storage
                .prepare_replacement_import_termination(&f.store, key)
                .unwrap();
            f.faults.fail_next(FaultPoint::AfterCommitBeforePersist);
            let outcome = execute(
                &f.store,
                f.storage.terminate_replacement_draft_import(
                    f.storage.revision(&f.store).unwrap(),
                    prepared.clone(),
                ),
            );
            interrupt_observation(&f, &outcome);
            assert!(
                f.storage
                    .reconcile_replacement_import_termination(&f.store, &prepared, outcome)
                    .is_err()
            );
            let f = f.recover();
            assert_eq!(
                f.storage
                    .resume_replacement_import_termination(&f.store, &prepared)
                    .unwrap(),
                DraftReplacementCommandOutcomeV1::ImportTerminated
            );
            assert_eq!(f.head().newest_root(), f.session.newest_root());
            assert!(f.head().active_operation().is_none());
        }
    }
}

#[test]
fn foreign_outcome_lookup_retains_original_evidence_and_clones_share_only_the_exact_command() {
    for lose_acknowledgement in [false, true] {
        let payload =
            ComposerPayload::new(vec![ComposerAtom::text("foreign lookup").unwrap()]).unwrap();
        let f = Fixture::new("original-outcome-owner", payload.clone(), vec![], 65_536);
        let foreign = Fixture::new("foreign-outcome-target", payload, vec![], 65_536);
        let before = foreign.head();
        let p = f.prepare();
        begin(&f, &p);
        advance_all(&f, &p);
        let prepared = f
            .storage
            .prepare_replacement_draft_start(&f.store, &p, SyndicTimestamp::from_unix_millis(3))
            .unwrap();
        let clone = prepared.clone();
        let other = f
            .storage
            .prepare_replacement_draft_start(&f.store, &p, SyndicTimestamp::from_unix_millis(4))
            .unwrap();
        if lose_acknowledgement {
            f.faults.fail_next(FaultPoint::AfterCommitBeforePersist);
        }
        let outcome = f.store.execute(start_command(&f, &prepared));
        assert!(matches!(
            foreign
                .storage
                .reconcile_replacement_draft_start(&foreign.store, &prepared, outcome),
            Err(DraftReplacementErrorV1::Proof)
        ));
        assert_eq!(foreign.head(), before);
        assert!(matches!(
            foreign
                .storage
                .inspect_replacement_draft_start(&foreign.store, &clone),
            Err(DraftReplacementErrorV1::Proof)
        ));
        if lose_acknowledgement {
            assert!(!f.store.pending_reconciliations().is_empty());
        }
        let f = f.recover();
        assert!(matches!(
            f.storage.inspect_replacement_draft_start(&f.store, &other),
            Err(DraftReplacementErrorV1::Unavailable)
        ));
        assert_eq!(
            f.storage
                .resume_replacement_draft_start(&f.store, &clone)
                .unwrap(),
            DraftReplacementCommandOutcomeV1::Started(clone.selector())
        );
        assert_eq!(
            f.storage
                .inspect_replacement_draft_start(&f.store, &prepared)
                .unwrap(),
            DraftReplacementCommandOutcomeV1::Started(prepared.selector())
        );
        assert!(matches!(
            f.storage.resume_replacement_draft_start(&f.store, &other),
            Err(DraftReplacementErrorV1::Unavailable)
        ));
    }
}
