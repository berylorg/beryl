use super::*;

#[test]
fn pending_and_archived_sessions_read_and_dispose_without_granting_mutation_authority() {
    for archive in [false, true] {
        let home = TestHome::new("discussion-readonly-session");
        let (store, storage, mut request) = seeded(&home, FaultController::new());
        let selected = storage
            .current_draft(&store, id(36), limit())
            .unwrap()
            .unwrap();
        let initial = support::open_session(&storage, &store, &selected, 180, 181);
        let adoption = edit_support::commit_edit(&storage, &store, &initial, 182, "visible draft");
        let source = storage
            .capture_draft_editor_candidate_publication_source(
                &store,
                DraftEditorCandidatePublicationSourceCaptureRequestV1::new(
                    support::selector(&selected),
                    DraftEditorCandidateActivationBindingV1::from_head(adoption.adopted_session()),
                    DraftPieceOperationIdV1::from_bytes([183; 16]),
                    timestamp(200),
                ),
            )
            .unwrap();
        let publication = storage
            .prepare_draft_editor_candidate_publication(
                &store,
                source,
                DraftEditorCandidatePublicationEvidenceV1::UnchangedEmpty,
            )
            .unwrap();
        committed(
            &store,
            storage.publish_draft_editor_candidate(storage.revision(&store).unwrap(), publication),
        );
        let selected = storage
            .current_draft(&store, id(36), limit())
            .unwrap()
            .unwrap();
        let DraftEditorCandidateSessionReadOutcomeV1::Active(original) = storage
            .draft_editor_candidate_session(&store, initial.draft_id(), initial.session_id())
            .unwrap()
        else {
            panic!("original session must remain active")
        };
        let DraftHistoricalRootSelectionV1::Prepared(undo) = storage
            .prepare_draft_historical_root_selection(
                &store,
                DraftHistoricalRootSelectionIntentV1::new(
                    DraftEditorCandidateActivationBindingV1::from_head(&original),
                    DraftPieceOperationIdV1::from_bytes([184; 16]),
                    DraftHistoricalRootDirectionV1::Undo,
                ),
            )
            .unwrap()
        else {
            panic!("published edit must retain undo")
        };
        request.thread_revision = selected.thread().revision();
        let pending = storage
            .prepare_discussion_handoff(&store, DiscussionHandoffMutation::Admit(request))
            .unwrap();
        let gate = pending.intent().new_gate();
        execute(&store, pending);
        if archive {
            execute(
                &store,
                storage
                    .prepare_discussion_handoff(
                        &store,
                        DiscussionHandoffMutation::ReleaseAndArchive {
                            expected: gate,
                            attributes_revision: storage
                                .thread_attributes(&store, id(36), limit())
                                .unwrap()
                                .unwrap()
                                .revision(),
                            archived_at: timestamp(205),
                        },
                    )
                    .unwrap(),
            );
        }
        let before_thread = storage.thread(&store, id(36), limit()).unwrap();
        let before_attributes = storage.thread_attributes(&store, id(36), limit()).unwrap();
        let before_input = storage.input_gate(&store, id(36), limit()).unwrap();
        let before_gate = storage
            .discussion_handoff_gate(&store, id(36), limit())
            .unwrap();
        let opening = storage
            .prepare_open_draft_editor_candidate_session(
                &store,
                support::open_request(&selected, 185, 186),
            )
            .unwrap();
        let outcome = if archive {
            store.execute_current(
                storage.current_open_draft_editor_candidate_session(opening.clone()),
            )
        } else {
            support::execute(
                &store,
                storage.open_draft_editor_candidate_session(
                    storage.revision(&store).unwrap(),
                    opening.clone(),
                ),
            )
        };
        let DraftEditorCandidateSessionOpenOutcomeV1::Opened(head) = storage
            .reconcile_draft_editor_candidate_session_open(&store, &opening, outcome)
            .unwrap()
        else {
            panic!("readonly session must open")
        };
        let replay = store
            .execute_current(storage.current_open_draft_editor_candidate_session(opening.clone()));
        assert!(
            matches!(storage.reconcile_draft_editor_candidate_session_open(&store, &opening, replay).unwrap(), DraftEditorCandidateSessionOpenOutcomeV1::ExactReplay(ref replayed) if replayed == &head)
        );
        let binding = DraftEditorCandidateActivationBindingV1::from_head(&head);
        let text = storage
            .candidate_draft_piece_text_demand(
                &store,
                binding,
                DraftPieceTextDemandV1::Forward(0),
                64,
            )
            .unwrap();
        assert_eq!(text.value().bytes(), b"visible draft");
        let edit =
            support::transaction(&storage, &store, &head, 187, "forbidden", support::point(9));
        let source = storage
            .capture_draft_editor_candidate_publication_source(
                &store,
                DraftEditorCandidatePublicationSourceCaptureRequestV1::new(
                    support::selector(&selected),
                    binding,
                    DraftPieceOperationIdV1::from_bytes([188; 16]),
                    timestamp(206),
                ),
            )
            .unwrap();
        let publication = storage
            .prepare_draft_editor_candidate_publication(
                &store,
                source,
                DraftEditorCandidatePublicationEvidenceV1::UnchangedEmpty,
            )
            .unwrap();
        blocked(
            &store,
            storage.begin_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared),
        );
        blocked(
            &store,
            storage.adopt_draft_historical_root(storage.revision(&store).unwrap(), undo),
        );
        blocked(
            &store,
            storage.publish_draft_editor_candidate(storage.revision(&store).unwrap(), publication),
        );
        let dispose = DraftEditorCandidateSessionDisposeRequestV1::new(
            head.draft_id(),
            head.session_id(),
            DraftPieceOperationIdV1::from_bytes([189; 16]),
            head.session_generation(),
            DraftRootHistoryPairV1::new(head.published_root(), head.published_history()),
        );
        let prepared = storage
            .prepare_dispose_draft_editor_candidate_session(&store, dispose)
            .unwrap();
        let outcome = support::execute(
            &store,
            storage.dispose_draft_editor_candidate_session(
                storage.revision(&store).unwrap(),
                prepared.clone(),
            ),
        );
        let disposition = storage
            .reconcile_draft_editor_candidate_session_disposal(&store, &prepared, outcome)
            .unwrap();
        let DraftEditorCandidateSessionDisposeOutcomeV1::Disposed(disposed) = disposition else {
            panic!("unchanged readonly session must dispose: {disposition:?}")
        };
        assert_eq!(disposed.published_root(), disposed.newest_root());
        assert_eq!(disposed.published_history(), disposed.newest_history());
        let replay = storage
            .prepare_dispose_draft_editor_candidate_session(&store, dispose)
            .unwrap();
        let outcome = support::execute(
            &store,
            storage.dispose_draft_editor_candidate_session(
                storage.revision(&store).unwrap(),
                replay.clone(),
            ),
        );
        assert!(matches!(
            storage
                .reconcile_draft_editor_candidate_session_disposal(&store, &replay, outcome)
                .unwrap(),
            DraftEditorCandidateSessionDisposeOutcomeV1::ExactReplay(_)
        ));
        assert_eq!(
            storage
                .current_draft(&store, id(36), limit())
                .unwrap()
                .unwrap()
                .draft(),
            selected.draft()
        );
        assert_eq!(
            storage.thread(&store, id(36), limit()).unwrap(),
            before_thread
        );
        assert_eq!(
            storage.thread_attributes(&store, id(36), limit()).unwrap(),
            before_attributes
        );
        assert_eq!(
            storage.input_gate(&store, id(36), limit()).unwrap(),
            before_input
        );
        assert_eq!(
            storage
                .discussion_handoff_gate(&store, id(36), limit())
                .unwrap(),
            before_gate
        );
        assert_eq!(
            storage
                .draft_editor_candidate_session(&store, original.draft_id(), original.session_id())
                .unwrap(),
            DraftEditorCandidateSessionReadOutcomeV1::Active(original)
        );
        store.close().unwrap();
    }
}

#[test]
fn session_opening_rejects_inconsistent_discussion_archive_and_gate_records() {
    use syndic_storage::test_faults::FixtureRecord;

    for archived_pending in [false, true] {
        let home = TestHome::new("discussion-session-structure");
        let (store, storage, request) = seeded(&home, FaultController::new());
        let selected = storage
            .current_draft(&store, id(36), limit())
            .unwrap()
            .unwrap();
        let opening = support::open_request(&selected, 190, 191);
        let prepared = storage
            .prepare_open_draft_editor_candidate_session(&store, opening)
            .unwrap();
        if archived_pending {
            let admission = storage
                .prepare_discussion_handoff(&store, DiscussionHandoffMutation::Admit(request))
                .unwrap();
            let pending = admission.intent().new_gate();
            execute(&store, admission);
            execute(
                &store,
                storage
                    .prepare_discussion_handoff(
                        &store,
                        DiscussionHandoffMutation::ReleaseAndArchive {
                            expected: pending,
                            attributes_revision: ThreadAttributesRevision::FIRST,
                            archived_at: timestamp(205),
                        },
                    )
                    .unwrap(),
            );
            crate::support::commit(
                &store,
                storage.clone(),
                crate::support::batch([FixtureRecord::DiscussionHandoffGate(pending)]),
            );
        } else {
            crate::support::commit(
                &store,
                storage.clone(),
                crate::support::batch([FixtureRecord::ThreadAttributes(
                    ThreadAttributesRecord::ordinary(id(36)),
                )]),
            );
        }
        let outcome = support::execute(
            &store,
            storage
                .open_draft_editor_candidate_session(storage.revision(&store).unwrap(), prepared),
        );
        assert!(
            matches!(outcome, CommandOutcome::NotCommitted { .. }),
            "{outcome:?}"
        );
        assert!(
            format!("{outcome:?}").contains("DiscussionHandoffConflict"),
            "{outcome:?}"
        );
        assert_eq!(
            storage
                .draft_editor_candidate_session(
                    &store,
                    opening.selector().draft_id(),
                    opening.session_id()
                )
                .unwrap(),
            DraftEditorCandidateSessionReadOutcomeV1::Absent
        );
        assert_eq!(
            storage.thread(&store, id(36), limit()).unwrap().unwrap(),
            *selected.thread()
        );
        store.close().unwrap();
    }
}
