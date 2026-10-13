use super::*;

fn replace_gate(f: &Fixture, state: InputGateState, live: u64) {
    let gate = f
        .storage
        .input_gate(
            &f.store,
            f.thread,
            SyndicPointReadLimit::new(65_536).unwrap(),
        )
        .unwrap()
        .unwrap();
    support::commit(
        &f.store,
        f.storage.clone(),
        support::batch(vec![FixtureRecord::InputGate(
            InputGateRecord::new(
                f.thread,
                gate.revision().checked_next().unwrap(),
                state,
                live,
                None,
                None,
                0,
                live,
                live,
            )
            .unwrap(),
        )]),
    );
}

#[test]
fn prepared_import_rejects_later_staging_and_nonempty_candidate_without_resetting_them() {
    for staging in [true, false] {
        let mut f = Fixture::new(
            "late-import-session",
            ComposerPayload::new(vec![ComposerAtom::text("historical").unwrap()]).unwrap(),
            vec![],
            65_536,
        );
        let prepared = f.prepare();
        if staging {
            let identity = DraftMutationStagingIdentityV1::new(
                f.session.draft_id(),
                f.session.session_id(),
                DraftMutationOperationIdV1::from_bytes([241; 16]),
            );
            let input = DraftMutationBeginV1::new(
                identity,
                f.session.session_generation(),
                f.session.newest_candidate_generation(),
                f.session.newest_root(),
                f.session.newest_history(),
                f.session.logical_extent(),
                point(0),
                point(0),
                point(0),
                point(0),
                point(0),
                0,
                0,
            );
            let begin = f
                .storage
                .prepare_draft_mutation_staging_begin(input, &f.session)
                .unwrap();
            committed(execute(
                &f.store,
                f.storage
                    .draft_mutation_staging_command(f.storage.revision(&f.store).unwrap(), begin),
            ));
        } else {
            let tx =
                ordinary::transaction(&f.storage, &f.store, &f.session, 242, "unsaved", point(7));
            ordinary::build(&f.storage, &f.store, &tx);
            committed(execute(
                &f.store,
                f.storage.settle_draft_piece_edit(
                    f.storage.revision(&f.store).unwrap(),
                    tx.prepared.clone(),
                ),
            ));
            assert!(matches!(
                ordinary::settled(&f.storage, &f.store, &tx).closure(),
                DraftPieceSettlementClosureV1::Committed(_)
            ));
        }
        let before = f.head();
        let durable = ordinary::current(&f.storage, &f.store, f.thread);
        let revision = f.store.home_revision().unwrap();
        assert!(matches!(
            execute(
                &f.store,
                f.storage.begin_replacement_draft_import(
                    f.storage.revision(&f.store).unwrap(),
                    prepared
                )
            ),
            CommandOutcome::NotCommitted { .. }
        ));
        assert_eq!(f.head(), before);
        assert_eq!(ordinary::current(&f.storage, &f.store, f.thread), durable);
        assert_eq!(f.store.home_revision().unwrap(), revision);
        f.session = before.clone();
        assert!(
            f.storage
                .prepare_replacement_draft_import(&f.store, f.request())
                .is_err()
        );
        assert_eq!(f.head(), before);
        assert_eq!(f.store.home_revision().unwrap(), revision);
        if !staging {
            assert_eq!(text(&f, before.newest_root()), b"unsaved");
        }
    }
}

#[test]
fn nonidle_gate_and_live_accepted_work_refuse_import_without_writes() {
    for (state, live) in [
        (
            InputGateState::PendingTurn(SyndicTurnId::from_bytes([203; 16])),
            0,
        ),
        (InputGateState::Idle, 1),
    ] {
        let f = Fixture::new(
            "import-admission-gate",
            ComposerPayload::default(),
            vec![],
            65_536,
        );
        let prepared = f.prepare();
        replace_gate(&f, state, live);
        let before = f.head();
        let durable = ordinary::current(&f.storage, &f.store, f.thread);
        let revision = f.store.home_revision().unwrap();
        assert!(
            f.storage
                .prepare_replacement_draft_import(&f.store, f.request())
                .is_err()
        );
        assert!(matches!(
            execute(
                &f.store,
                f.storage.begin_replacement_draft_import(
                    f.storage.revision(&f.store).unwrap(),
                    prepared
                )
            ),
            CommandOutcome::NotCommitted { .. }
        ));
        assert_eq!(f.head(), before);
        assert_eq!(ordinary::current(&f.storage, &f.store, f.thread), durable);
        assert_eq!(f.store.home_revision().unwrap(), revision);
    }
}

#[test]
fn final_start_rechecks_gate_selected_path_and_canonical_source_before_cross_domain_selection() {
    for change in 0..4 {
        let marker = SyndicDraftMarkerId::from_bytes([243; 16]);
        let label = ImageLabelOrdinal::new(1).unwrap();
        let f = Fixture::new(
            "late-start-admission",
            ComposerPayload::new(vec![
                ComposerAtom::text("source").unwrap(),
                ComposerAtom::image_marker(marker, label),
            ])
            .unwrap(),
            vec![(marker, label)],
            65_536,
        );
        let p = f.prepare();
        begin(&f, &p);
        advance_all(&f, &p);
        let prepared = f
            .storage
            .prepare_replacement_draft_start(&f.store, &p, SyndicTimestamp::from_unix_millis(3))
            .unwrap();
        match change {
            0 => replace_gate(&f, InputGateState::Idle, 0),
            1 => replace_gate(
                &f,
                InputGateState::PendingTurn(f.intent.target_turn_id()),
                0,
            ),
            2 => {
                support::commit(
                    &f.store,
                    f.storage.clone(),
                    support::batch(vec![FixtureRecord::CanonicalItem(
                        CanonicalItemRecord::local_user_input(
                            f.item,
                            f.intent.target_turn_id(),
                            TurnItemOrdinal::FIRST,
                            beryl_model::ProjectionRevision::new(2).unwrap(),
                            f.content,
                            f.proof,
                        ),
                    )]),
                );
            }
            _ => {
                let current = ordinary::current(&f.storage, &f.store, f.thread);
                let thread = current.thread();
                let selected = SelectedPathProof::new(
                    thread.committed_tail(),
                    thread.revision().checked_next().unwrap(),
                    thread.selected_path_digest(),
                );
                support::commit(
                    &f.store,
                    f.storage.clone(),
                    support::batch(vec![
                        FixtureRecord::Thread(ThreadRecord::new(
                            f.thread,
                            selected,
                            thread.current_draft_id(),
                            thread.lineage(),
                            thread.context_owner_id(),
                        )),
                        FixtureRecord::DraftByThread(DraftByThreadRecord::new(
                            f.thread,
                            current.draft().id(),
                            current.draft().revision(),
                            selected.thread_revision(),
                        )),
                    ]),
                );
            }
        }
        let before = f.head();
        let durable = ordinary::current(&f.storage, &f.store, f.thread);
        let historical = f
            .state
            .assets()
            .owner_head(&f.store, AssetOwner::SubmittedTurnItem(f.item))
            .unwrap();
        let asset_revision = f.state.assets().revision(&f.store).unwrap();
        let revision = f.store.home_revision().unwrap();
        let outcome = f.store.execute(start_command(&f, &prepared));
        assert!(
            matches!(outcome, CommandOutcome::NotCommitted { .. }),
            "change {change}: {outcome:?}"
        );
        assert_eq!(f.head(), before);
        assert_eq!(ordinary::current(&f.storage, &f.store, f.thread), durable);
        assert_eq!(f.store.home_revision().unwrap(), revision);
        assert_eq!(f.state.assets().revision(&f.store).unwrap(), asset_revision);
        assert_eq!(
            f.state
                .assets()
                .owner_head(&f.store, AssetOwner::SubmittedTurnItem(f.item))
                .unwrap(),
            historical
        );
        assert!(
            f.state
                .assets()
                .owner_head(&f.store, AssetOwner::CurrentDraft(f.session.draft_id()))
                .unwrap()
                .is_none()
        );
    }
}
