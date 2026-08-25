use beryl_app::{
    composer_host::{
        ComposerHostError, ComposerHostFlushAdmission, ComposerHostFlushAdvance,
        ComposerHostFlushCapture, ComposerHostFlushFailure, ComposerHostFlushPurpose,
        ComposerHostFlushState, ComposerHostFlushTicket, ComposerHostPublicationCapture,
        ComposerHostPublicationCompletion, ComposerHostPublicationDrive,
        ComposerHostPublicationTicket, ComposerHostPublicationUnavailable, SyndicComposerHost,
    },
    composer_marker_seal::{
        DraftMarkerSealAdmission, DraftMarkerSealFlightRequest, DraftMarkerSealService,
    },
};
use beryl_home_store::{
    CommandCancellation, CommandOutcome, HomeCommand, HomeHealthState, test_faults::FaultPoint,
};
use beryl_state::{AssetOwner, BerylState};
use gpui_text_input::MutationKind;
use syndic_storage::{
    CapturedDraftEditorCandidatePublicationSourceV1, DraftEditorCandidatePublicationEvidenceV1,
    DraftEditorCandidatePublicationRequestV1,
    DraftEditorCandidatePublicationSourceCaptureRequestV1,
    DraftEditorCandidateSessionDisposeRequestV1, DraftEditorCurrentSelectorV1,
    DraftHistoricalRootDirectionV1, DraftHistoricalRootSelectionIntentV1,
    DraftMarkerSealOperationIdV1, DraftMarkerSealRequestV1, DraftRootHistoryPairV1,
    SyndicTimestamp,
};

#[path = "phase141_syndic_composer_host/support.rs"]
mod base;
#[path = "phase166_syndic_composer_history/support.rs"]
mod composer;
#[path = "phase172_syndic_composer_publication/support.rs"]
mod publication;

use base::{current, fixture};
use composer::{
    activated, commit_text, direct_adopt, insert_marker, operation_id, remove_marker,
    select_history,
};
use publication::{
    authority, insert_later_marker, insert_published_marker, insert_text_after_published_marker,
    insert_two_markers, publish_image_asset, service,
};

trait LifecyclePublicationSettlement {
    fn drive_publication(
        &mut self,
        store: &beryl_home_store::HomeStore,
        ticket: ComposerHostPublicationTicket,
    ) -> Result<ComposerHostPublicationDrive, ComposerHostError>;

    fn execute_publication(
        &mut self,
        store: &beryl_home_store::HomeStore,
        ticket: ComposerHostPublicationTicket,
    ) -> Result<ComposerHostPublicationCompletion, ComposerHostError>;

    fn reconcile_publication(
        &mut self,
        store: &beryl_home_store::HomeStore,
        ticket: ComposerHostPublicationTicket,
    ) -> Result<ComposerHostPublicationCompletion, ComposerHostError>;
}

impl LifecyclePublicationSettlement for SyndicComposerHost {
    fn drive_publication(
        &mut self,
        store: &beryl_home_store::HomeStore,
        ticket: ComposerHostPublicationTicket,
    ) -> Result<ComposerHostPublicationDrive, ComposerHostError> {
        let flush = publication_flush(self, ticket)?;
        if self.lifecycle_diagnostics().publication_ready() {
            return Ok(ComposerHostPublicationDrive::Ready);
        }
        let advance = self.advance_flush(store, flush)?;
        match advance {
            ComposerHostFlushAdvance::Stale => Err(ComposerHostError::StalePublicationGeneration),
            ComposerHostFlushAdvance::Unsatisfied(_) => {
                Err(ComposerHostError::PublicationUnavailable)
            }
            _ if self.lifecycle_diagnostics().publication_ready() => {
                Ok(ComposerHostPublicationDrive::Ready)
            }
            _ => Ok(ComposerHostPublicationDrive::Progress),
        }
    }

    fn execute_publication(
        &mut self,
        store: &beryl_home_store::HomeStore,
        ticket: ComposerHostPublicationTicket,
    ) -> Result<ComposerHostPublicationCompletion, ComposerHostError> {
        settle_publication_step(self, store, ticket)
    }

    fn reconcile_publication(
        &mut self,
        store: &beryl_home_store::HomeStore,
        ticket: ComposerHostPublicationTicket,
    ) -> Result<ComposerHostPublicationCompletion, ComposerHostError> {
        settle_publication_step(self, store, ticket)
    }
}

fn publication_flush(
    host: &mut SyndicComposerHost,
    ticket: ComposerHostPublicationTicket,
) -> Result<ComposerHostFlushTicket, ComposerHostError> {
    let admission = host.begin_flush(ComposerHostFlushPurpose::Submission)?;
    let flush = match admission {
        ComposerHostFlushAdmission::Started {
            ticket: flush_ticket,
            ..
        }
        | ComposerHostFlushAdmission::Joined {
            ticket: flush_ticket,
            ..
        } => flush_ticket,
        ComposerHostFlushAdmission::Satisfied(_) => {
            return Err(ComposerHostError::StalePublicationGeneration);
        }
    };
    if host.lifecycle_diagnostics().joined_publication_ticket() != Some(ticket) {
        return Err(ComposerHostError::StalePublicationGeneration);
    }
    Ok(flush)
}

fn settle_publication_step(
    host: &mut SyndicComposerHost,
    store: &beryl_home_store::HomeStore,
    ticket: ComposerHostPublicationTicket,
) -> Result<ComposerHostPublicationCompletion, ComposerHostError> {
    let flush = publication_flush(host, ticket)?;
    match host.advance_flush(store, flush)? {
        ComposerHostFlushAdvance::ReconciliationPending => {
            Ok(ComposerHostPublicationCompletion::ReconciliationPending)
        }
        ComposerHostFlushAdvance::Unsatisfied(failure) => Ok(match failure {
            ComposerHostFlushFailure::Cancelled => {
                ComposerHostPublicationCompletion::CancelledBeforeAdmission
            }
            ComposerHostFlushFailure::NotCommitted => {
                ComposerHostPublicationCompletion::NotCommitted
            }
            ComposerHostFlushFailure::DurableBaseConflict => {
                ComposerHostPublicationCompletion::DurableBaseConflict
            }
            ComposerHostFlushFailure::SessionDisposed => {
                ComposerHostPublicationCompletion::SessionDisposed
            }
            ComposerHostFlushFailure::IdentityCollision => {
                ComposerHostPublicationCompletion::OccupiedIdentityCollision
            }
            ComposerHostFlushFailure::ReconciliationCollision => {
                ComposerHostPublicationCompletion::ReconciliationCollision
            }
            ComposerHostFlushFailure::Recoverable
            | ComposerHostFlushFailure::DisposalDirtyConflict
            | ComposerHostFlushFailure::GenerationLost => {
                return Err(ComposerHostError::PublicationUnavailable);
            }
        }),
        ComposerHostFlushAdvance::Stale => Err(ComposerHostError::StalePublicationGeneration),
        ComposerHostFlushAdvance::Progress(_) | ComposerHostFlushAdvance::Satisfied(_) => host
            .lifecycle_diagnostics()
            .last_publication_completion()
            .ok_or(ComposerHostError::PublicationPending),
    }
}

#[test]
fn unchanged_empty_is_derived_without_sealing_and_stale_ticket_preserves_newer_custody() {
    let (_home, mut store, storage, thread) = fixture("phase172-unchanged-empty", 11);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = service(&store, storage, assets, 1, 1);
    let (mut host, empty) = activated(storage, &store, thread, 12, 13);
    let dirty = commit_text(&mut host, &store, empty, 1, 0, 0, "a", 1, 1);

    let cancellation = CommandCancellation::new();
    let old = captured(capture_joined_publication(
        &mut host,
        &store,
        assets,
        &seals,
        operation_id(2),
        None,
        SyndicTimestamp::from_unix_millis(10),
        &cancellation,
    ));
    assert_eq!(
        host.drive_publication(&store, old).unwrap(),
        ComposerHostPublicationDrive::Ready
    );
    cancellation.cancel();
    assert_eq!(
        host.execute_publication(&store, old).unwrap(),
        ComposerHostPublicationCompletion::CancelledBeforeAdmission
    );

    let current_ticket = captured(capture_joined_publication(
        &mut host,
        &store,
        assets,
        &seals,
        operation_id(3),
        None,
        SyndicTimestamp::from_unix_millis(11),
        &CommandCancellation::new(),
    ));
    assert!(matches!(
        host.execute_publication(&store, old),
        Err(ComposerHostError::StalePublicationGeneration)
    ));
    assert_eq!(host.publication_custody_count(), 1);
    assert!(!host.is_unavailable());
    assert_eq!(
        host.execute_publication(&store, current_ticket).unwrap(),
        ComposerHostPublicationCompletion::Published
    );
    assert_eq!(
        current(storage, &store, thread).draft().piece_root(),
        dirty.root()
    );
    assert_eq!(seals.diagnostics().current_flights(), 0);
}

#[test]
fn changed_nonempty_streams_multiple_pages_and_later_edit_remains_dirty() {
    let (_home, mut store, storage, thread) = fixture("phase172-changed-nonempty", 21);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = service(&store, storage, assets, 1, 1);
    let (mut host, empty) = activated(storage, &store, thread, 22, 23);
    let marker_assets = [
        publish_image_asset(&store, assets, b"phase172-marker-a"),
        publish_image_asset(&store, assets, b"phase172-marker-b"),
    ];
    let captured_binding = insert_two_markers(&mut host, &store, empty, 10, marker_assets);
    let cancellation = CommandCancellation::new();
    let ticket = captured(capture_joined_publication(
        &mut host,
        &store,
        assets,
        &seals,
        operation_id(2),
        Some(authority(24)),
        SyndicTimestamp::from_unix_millis(20),
        &cancellation,
    ));
    let later = insert_later_marker(&mut host, &store, captured_binding, 12);
    let newest = insert_text_after_published_marker(&mut host, &store, later, 13);
    let mut progress = 0;
    loop {
        match host.drive_publication(&store, ticket).unwrap() {
            ComposerHostPublicationDrive::Progress
            | ComposerHostPublicationDrive::NotCommitted(_) => progress += 1,
            ComposerHostPublicationDrive::Ready => break,
            other => panic!("unexpected drive outcome: {other:?}"),
        }
    }
    assert!(progress >= 3);
    assert_eq!(
        host.execute_publication(&store, ticket).unwrap(),
        ComposerHostPublicationCompletion::Published
    );
    let current_binding = host.binding().unwrap();
    assert_eq!(current_binding.root(), newest.root());
    assert_eq!(current_binding.history(), newest.history());
    assert_eq!(
        current_binding.candidate().candidate_generation(),
        newest.candidate().candidate_generation()
    );
    assert!(host.is_dirty());
    assert!(
        assets
            .owner_head(
                &store,
                AssetOwner::CurrentDraft(captured_binding.candidate().draft_id())
            )
            .unwrap()
            .is_some()
    );
}

#[test]
fn marker_changing_undo_source_survives_two_later_candidates() {
    let (_home, mut store, storage, thread) = fixture("phase172-historical-undo", 25);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = service(&store, storage, assets, 1, 1);
    let (mut host, empty) = activated(storage, &store, thread, 26, 27);
    let asset = publish_image_asset(&store, assets, b"phase172-historical-undo-marker");
    let (marker, before, after) = insert_published_marker(&mut host, &store, empty, 1, asset);
    let removed = remove_marker(&mut host, &store, marker, 2, before, after, after);
    let captured_binding = select_history(&mut host, &store, removed, 3, MutationKind::Undo);
    assert_eq!(captured_binding.root(), marker.root());
    let ticket = captured(capture_joined_publication(
        &mut host,
        &store,
        assets,
        &seals,
        operation_id(4),
        Some(authority(28)),
        SyndicTimestamp::from_unix_millis(25),
        &CommandCancellation::new(),
    ));
    let later = insert_text_after_published_marker(&mut host, &store, captured_binding, 5);
    let newest = commit_text(&mut host, &store, later, 6, 1, 1, "y", 2, 1);
    publish_ticket(&mut host, &store, ticket);
    assert_captured_published_and_newest_dirty(storage, &store, thread, captured_binding, newest);
    assert!(host.is_dirty());
    assert!(
        assets
            .owner_head(
                &store,
                AssetOwner::CurrentDraft(captured_binding.candidate().draft_id())
            )
            .unwrap()
            .is_some()
    );
}

#[test]
fn marker_changing_redo_source_survives_two_later_candidates() {
    let (_home, mut store, storage, thread) = fixture("phase172-historical-redo", 29);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = service(&store, storage, assets, 1, 1);
    let (mut host, empty) = activated(storage, &store, thread, 30, 31);
    let asset = publish_image_asset(&store, assets, b"phase172-historical-redo-marker");
    let (marker, _, _) = insert_published_marker(&mut host, &store, empty, 1, asset);
    let reverted = select_history(&mut host, &store, marker, 2, MutationKind::Undo);
    let captured_binding = select_history(&mut host, &store, reverted, 3, MutationKind::Redo);
    assert_eq!(captured_binding.root(), marker.root());
    let ticket = captured(capture_joined_publication(
        &mut host,
        &store,
        assets,
        &seals,
        operation_id(4),
        Some(authority(32)),
        SyndicTimestamp::from_unix_millis(26),
        &CommandCancellation::new(),
    ));
    let owner = AssetOwner::CurrentDraft(marker.candidate().draft_id());
    assert!(assets.owner_head(&store, owner).unwrap().is_none());
    let later = insert_text_after_published_marker(&mut host, &store, captured_binding, 5);
    let newest = commit_text(&mut host, &store, later, 6, 1, 1, "y", 2, 1);
    publish_ticket(&mut host, &store, ticket);
    assert_captured_published_and_newest_dirty(storage, &store, thread, captured_binding, newest);
    assert!(host.is_dirty());
    assert!(assets.owner_head(&store, owner).unwrap().is_some());
}

#[test]
fn unchanged_nonempty_reuses_exact_head_without_marker_sealing() {
    let (_home, mut store, storage, thread) = fixture("phase172-empty-transition", 31);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = service(&store, storage, assets, 1, 1);
    let (mut host, empty) = activated(storage, &store, thread, 32, 33);
    let asset = publish_image_asset(&store, assets, b"phase172-marker-transition");
    let (marker, _, _) = insert_published_marker(&mut host, &store, empty, 1, asset);
    let marker_ticket = captured(capture_joined_publication(
        &mut host,
        &store,
        assets,
        &seals,
        operation_id(2),
        Some(authority(34)),
        SyndicTimestamp::from_unix_millis(34),
        &CommandCancellation::new(),
    ));
    let text = insert_text_after_published_marker(&mut host, &store, marker, 3);
    publish_ticket(&mut host, &store, marker_ticket);
    let current_binding = host.binding().unwrap();
    assert_eq!(current_binding.root(), text.root());
    assert_eq!(current_binding.history(), text.history());
    assert_eq!(
        current_binding.candidate().candidate_generation(),
        text.candidate().candidate_generation()
    );
    assert!(host.is_dirty());
    let owner = AssetOwner::CurrentDraft(marker.candidate().draft_id());
    let first_head = assets.owner_head(&store, owner).unwrap().unwrap();

    let ticket = captured(capture_joined_publication(
        &mut host,
        &store,
        assets,
        &seals,
        operation_id(4),
        None,
        SyndicTimestamp::from_unix_millis(40),
        &CommandCancellation::new(),
    ));
    assert_eq!(seals.diagnostics().current_flights(), 0);
    assert_eq!(
        host.execute_publication(&store, ticket).unwrap(),
        ComposerHostPublicationCompletion::Published
    );
    assert_eq!(
        assets.owner_head(&store, owner).unwrap().unwrap().set(),
        first_head.set()
    );
}

#[test]
fn changed_to_empty_seals_exact_summaries_and_removes_the_asset_head() {
    let (_home, mut store, storage, thread) = fixture("phase172-changed-empty", 36);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = service(&store, storage, assets, 1, 1);
    let (mut host, empty) = activated(storage, &store, thread, 37, 38);
    let asset = publish_image_asset(&store, assets, b"phase172-marker-removal");
    let (marker, before, after) = insert_published_marker(&mut host, &store, empty, 1, asset);
    let marker_ticket = captured(capture_joined_publication(
        &mut host,
        &store,
        assets,
        &seals,
        operation_id(2),
        Some(authority(39)),
        SyndicTimestamp::from_unix_millis(39),
        &CommandCancellation::new(),
    ));
    let empty_again = remove_marker(&mut host, &store, marker, 3, before, after, after);
    publish_ticket(&mut host, &store, marker_ticket);
    let current_binding = host.binding().unwrap();
    assert_eq!(current_binding.root(), empty_again.root());
    assert_eq!(current_binding.history(), empty_again.history());
    assert_eq!(
        current_binding.candidate().candidate_generation(),
        empty_again.candidate().candidate_generation()
    );
    assert!(host.is_dirty());
    let owner = AssetOwner::CurrentDraft(marker.candidate().draft_id());
    assert!(assets.owner_head(&store, owner).unwrap().is_some());

    publish_changed(&mut host, &store, assets, &seals, 100, 40);
    assert_eq!(
        current(storage, &store, thread).draft().piece_root(),
        empty_again.root()
    );
    assert!(assets.owner_head(&store, owner).unwrap().is_none());
}

#[test]
fn stale_changed_nonempty_cannot_swap_asset_after_unchanged_winner() {
    let (_home, mut store, storage, thread) = fixture("phase172-atomic-stale-nonempty", 112);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = service(&store, storage, assets, 1, 1);
    let durable = current(storage, &store, thread);
    let selector = DraftEditorCurrentSelectorV1::new(
        durable.thread().id(),
        durable.thread().revision(),
        durable.draft().id(),
        durable.draft().revision(),
        durable.draft().piece_root(),
        durable.draft().history(),
    );
    let (mut host, empty) = activated(storage, &store, thread, 113, 114);
    let asset = publish_image_asset(&store, assets, b"phase172-atomic-stale-nonempty");
    let (marker, before, after) = insert_published_marker(&mut host, &store, empty, 1, asset);
    let ticket = captured(capture_joined_publication(
        &mut host,
        &store,
        assets,
        &seals,
        operation_id(115),
        Some(authority(116)),
        SyndicTimestamp::from_unix_millis(115),
        &CommandCancellation::new(),
    ));
    let winner = remove_marker(&mut host, &store, marker, 2, before, after, after);
    drive_ticket_ready(&mut host, &store, ticket);
    let request = DraftEditorCandidatePublicationRequestV1::new(
        selector,
        winner.candidate().session_id(),
        operation_id(117),
        winner.candidate().candidate_generation(),
        DraftRootHistoryPairV1::new(winner.root(), winner.history()),
        DraftEditorCandidatePublicationEvidenceV1::UnchangedEmpty,
        SyndicTimestamp::from_unix_millis(117),
    );
    host.test_arm_publication_before_execute_fault(move |store, storage| {
        assert!(matches!(
            publish_lower(store, storage, request),
            syndic_storage::DraftEditorCandidatePublicationOutcomeV1::Published(_, _)
        ));
    });
    let owner = AssetOwner::CurrentDraft(marker.candidate().draft_id());
    assert!(assets.owner_head(&store, owner).unwrap().is_none());
    assert_eq!(
        host.execute_publication(&store, ticket).unwrap(),
        ComposerHostPublicationCompletion::Superseded
    );
    assert!(assets.owner_head(&store, owner).unwrap().is_none());
    assert_publication_head_is_coherent(storage, &store, thread, winner);
}

#[test]
fn stale_changed_to_empty_cannot_remove_asset_after_equal_head_winner() {
    let (_home, mut store, storage, thread) = fixture("phase172-atomic-stale-empty", 118);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = service(&store, storage, assets, 1, 1);
    let (mut host, empty) = activated(storage, &store, thread, 119, 120);
    let asset = publish_image_asset(&store, assets, b"phase172-atomic-stale-empty");
    let (marker, before, after) = insert_published_marker(&mut host, &store, empty, 1, asset);
    let marker_ticket = captured(capture_joined_publication(
        &mut host,
        &store,
        assets,
        &seals,
        operation_id(121),
        Some(authority(122)),
        SyndicTimestamp::from_unix_millis(121),
        &CommandCancellation::new(),
    ));
    let _empty_again = remove_marker(&mut host, &store, marker, 2, before, after, after);
    publish_ticket(&mut host, &store, marker_ticket);
    let empty_again = host.binding().unwrap();
    let durable = current(storage, &store, thread);
    let selector = DraftEditorCurrentSelectorV1::new(
        durable.thread().id(),
        durable.thread().revision(),
        durable.draft().id(),
        durable.draft().revision(),
        durable.draft().piece_root(),
        durable.draft().history(),
    );
    let ticket = captured(capture_joined_publication(
        &mut host,
        &store,
        assets,
        &seals,
        operation_id(123),
        Some(authority(124)),
        SyndicTimestamp::from_unix_millis(123),
        &CommandCancellation::new(),
    ));
    drive_ticket_ready(&mut host, &store, ticket);
    let owner = AssetOwner::CurrentDraft(marker.candidate().draft_id());
    let head = assets.owner_head(&store, owner).unwrap().unwrap();
    let session = storage
        .draft_editor_candidate_session(
            &store,
            empty_again.candidate().draft_id(),
            empty_again.candidate().session_id(),
        )
        .unwrap();
    let syndic_storage::DraftEditorCandidateSessionReadOutcomeV1::Active(session) = session else {
        panic!("candidate session was not active")
    };
    direct_adopt(
        &store,
        storage,
        DraftHistoricalRootSelectionIntentV1::new(
            syndic_storage::DraftEditorCandidateActivationBindingV1::from_head(&session),
            operation_id(125),
            DraftHistoricalRootDirectionV1::Undo,
        ),
    );
    let session = storage
        .draft_editor_candidate_session(
            &store,
            empty_again.candidate().draft_id(),
            empty_again.candidate().session_id(),
        )
        .unwrap();
    let syndic_storage::DraftEditorCandidateSessionReadOutcomeV1::Active(winner) = session else {
        panic!("candidate session was not active")
    };
    assert_eq!(winner.newest_root(), marker.root());
    let request = DraftEditorCandidatePublicationRequestV1::new(
        selector,
        winner.session_id(),
        operation_id(126),
        winner.newest_candidate_generation(),
        DraftRootHistoryPairV1::new(winner.newest_root(), winner.newest_history()),
        DraftEditorCandidatePublicationEvidenceV1::UnchangedNonempty {
            asset_proof: head.set(),
        },
        SyndicTimestamp::from_unix_millis(126),
    );
    assert!(matches!(
        publish_lower(&store, storage, request),
        syndic_storage::DraftEditorCandidatePublicationOutcomeV1::Published(_, _)
    ));
    assert_eq!(
        host.execute_publication(&store, ticket).unwrap(),
        ComposerHostPublicationCompletion::Superseded
    );
    assert_eq!(assets.owner_head(&store, owner).unwrap().unwrap(), head);
    let durable = current(storage, &store, thread);
    let session = storage
        .draft_editor_candidate_session(
            &store,
            empty_again.candidate().draft_id(),
            empty_again.candidate().session_id(),
        )
        .unwrap();
    let syndic_storage::DraftEditorCandidateSessionReadOutcomeV1::Active(session) = session else {
        panic!("candidate session was not active")
    };
    assert_eq!(durable.draft().piece_root(), session.published_root());
    assert_eq!(durable.draft().history(), session.published_history());
    assert_eq!(
        session.published_candidate_generation(),
        winner.newest_candidate_generation()
    );
}

#[test]
fn seal_preflight_and_release_are_bounded_and_do_not_retain_a_queue() {
    let (_home, mut store, storage, thread) = fixture("phase172-release", 41);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = service(&store, storage, assets, 1, 1);
    let (mut host, empty) = activated(storage, &store, thread, 42, 43);
    let (dirty, _, _) = insert_marker(&mut host, &store, empty, 1, true);
    assert!(matches!(
        capture_joined_publication(
            &mut host,
            &store,
            assets,
            &seals,
            operation_id(2),
            None,
            SyndicTimestamp::from_unix_millis(50),
            &CommandCancellation::new(),
        ),
        Err(ComposerHostError::PublicationUnavailable)
    ));
    assert_eq!(host.publication_custody_count(), 0);

    let auth = authority(44);
    let cancellation = CommandCancellation::new();
    let ticket = captured(capture_joined_publication(
        &mut host,
        &store,
        assets,
        &seals,
        operation_id(2),
        Some(auth),
        SyndicTimestamp::from_unix_millis(50),
        &cancellation,
    ));
    let coalesced_cancellation = CommandCancellation::new();
    let coalesced = seals
        .admit(
            &store,
            DraftMarkerSealFlightRequest::new(
                dirty.candidate(),
                syndic_storage::DraftMarkerSealOperationIdV1::from_bytes([44; 16]),
                beryl_state::AssetReferenceSetStagingAuthority::new(
                    beryl_model::AssetReferenceSetId::from_bytes([45; 16]),
                    [46; 32],
                ),
            ),
            &coalesced_cancellation,
        )
        .unwrap();
    assert!(matches!(coalesced, DraftMarkerSealAdmission::Coalesced(_)));
    assert_eq!(seals.diagnostics().current_flights(), 1);
    cancellation.cancel();
    coalesced_cancellation.cancel();
    for _ in 0..32 {
        if host.publication_custody_count() == 0 {
            break;
        }
        let _ = host.drive_publication(&store, ticket);
    }
    assert_eq!(host.publication_custody_count(), 0);
    assert_eq!(seals.diagnostics().current_flights(), 0);
    assert!(host.is_dirty());
}

#[test]
fn marker_seal_drive_and_release_collision_retain_terminal_publication_custody() {
    for (name, seed, collide_during_release) in [
        ("phase172-seal-drive-collision", 126, false),
        ("phase172-seal-release-collision", 136, true),
    ] {
        let (_home, mut store, storage, thread, faults) = base::fault_fixture(name, seed);
        let assets = BerylState::register(&mut store).unwrap().assets();
        let seals = service(&store, storage, assets, 1, 1);
        let (mut host, empty) = activated(storage, &store, thread, seed + 1, seed + 2);
        let marker_assets = [
            publish_image_asset(&store, assets, name.as_bytes()),
            publish_image_asset(&store, assets, &[seed, seed.wrapping_add(1)]),
        ];
        let dirty = insert_two_markers(&mut host, &store, empty, 1, marker_assets);
        let seed_request = DraftMarkerSealRequestV1::new(
            dirty.root(),
            DraftMarkerSealOperationIdV1::from_bytes([seed.wrapping_add(3); 16]),
        );
        let begin = storage
            .prepare_draft_marker_seal_begin(&store, seed_request)
            .unwrap();
        let mut command = HomeCommand::new(store.home_revision().unwrap());
        command
            .add(storage.begin_draft_marker_seal(storage.revision(&store).unwrap(), begin))
            .unwrap();
        assert!(matches!(
            store.execute(command),
            CommandOutcome::Committed { .. }
        ));

        let cancellation = CommandCancellation::new();
        let ticket = captured(capture_joined_publication(
            &mut host,
            &store,
            assets,
            &seals,
            operation_id(u64::from(seed.wrapping_add(4))),
            Some(authority(seed.wrapping_add(5))),
            SyndicTimestamp::from_unix_millis(u64::from(seed)),
            &cancellation,
        ));
        assert_eq!(host.test_publication_source_custody_count(), 1);
        let later = insert_later_marker(&mut host, &store, dirty, 3);
        assert_eq!(host.binding(), Some(later));
        assert!(host.is_dirty());
        if collide_during_release {
            assert!(matches!(
                host.drive_publication(&store, ticket).unwrap(),
                ComposerHostPublicationDrive::Progress
                    | ComposerHostPublicationDrive::NotCommitted(_)
            ));
        }
        if collide_during_release {
            cancellation.cancel();
        }
        seals.test_arm_before_reconcile_fault(move |store, storage, request| {
            let (_, collision) =
                syndic_storage::inject_draft_marker_seal_natural_identity_collision_for_test(
                    store,
                    storage,
                    seed_request.key(),
                    request.operation_id(),
                );
            let mut command = HomeCommand::new(store.home_revision().unwrap());
            command.add(collision).unwrap();
            assert!(matches!(
                store.execute(command),
                CommandOutcome::Committed { .. }
            ));
        });
        faults.fail_next(FaultPoint::AfterCommitBeforePersist);
        let _ = host.drive_publication(&store, ticket);
        assert_eq!(seals.diagnostics().current_flights(), 0);
        assert_eq!(host.publication_custody_count(), 1);
        assert_eq!(host.test_publication_source_custody_count(), 0);
        assert_eq!(host.binding(), Some(later));
        assert!(host.is_dirty());
        assert!(host.is_unavailable());
        assert_eq!(
            host.publication_unavailable(),
            Some(ComposerHostPublicationUnavailable::ReconciliationCollision)
        );
        assert!(host.drive_publication(&store, ticket).is_err());
        assert!(matches!(
            capture_joined_publication(
                &mut host,
                &store,
                assets,
                &seals,
                operation_id(u64::from(seed.wrapping_add(6))),
                Some(authority(seed.wrapping_add(7))),
                SyndicTimestamp::from_unix_millis(u64::from(seed.wrapping_add(1))),
                &CommandCancellation::new(),
            ),
            Err(ComposerHostError::PublicationUnavailable)
        ));
        assert!(matches!(
            host.begin_mutation(
                &store,
                later,
                gpui_text_input::MutationBeginRequest::new(
                    gpui_text_input::MutationProposal::new(
                        gpui_text_input::MutationKey::new(
                            gpui_text_input::BindingId::new(later.host_generation().get()),
                            gpui_text_input::SourceRevision::new(
                                later.candidate().candidate_generation(),
                            ),
                            gpui_text_input::OperationId::new(99),
                        ),
                        gpui_text_input::MutationKind::Edit,
                        gpui_text_input::MutationPositions::collapsed(composer::position(0)),
                        gpui_text_input::SourceRange::new(
                            composer::position(0),
                            composer::position(0),
                        )
                        .unwrap(),
                        0,
                    ),
                    gpui_text_input::MutationCursor::new(0),
                    gpui_text_input::MutationCursor::new(0),
                ),
            ),
            Err(ComposerHostError::HistoryUnavailable)
        ));
        for (operation, kind) in [
            (100, gpui_text_input::MutationKind::Undo),
            (101, gpui_text_input::MutationKind::Redo),
        ] {
            let intent = composer::history_intent(later, operation, kind, composer::position(0));
            assert!(matches!(
                host.begin_history_selection(&store, later, intent),
                Err(ComposerHostError::HistoryUnavailable)
            ));
        }
    }
}

#[test]
fn exact_replay_and_occupied_identity_collision_have_distinct_terminal_custody() {
    for (name, seed, collide) in [
        ("phase172-replay", 51, false),
        ("phase172-collision", 61, true),
    ] {
        let (_home, mut store, storage, thread) = fixture(name, seed);
        let assets = BerylState::register(&mut store).unwrap().assets();
        let seals = service(&store, storage, assets, 1, 1);
        let durable = current(storage, &store, thread);
        let selector = DraftEditorCurrentSelectorV1::new(
            durable.thread().id(),
            durable.thread().revision(),
            durable.draft().id(),
            durable.draft().revision(),
            durable.draft().piece_root(),
            durable.draft().history(),
        );
        let (mut host, empty) = activated(storage, &store, thread, seed + 1, seed + 2);
        let dirty = commit_text(&mut host, &store, empty, 1, 0, 0, "x", 1, 1);
        let operation = operation_id(u64::from(seed + 3));
        let published_at = SyndicTimestamp::from_unix_millis(60);
        let ticket = captured(capture_joined_publication(
            &mut host,
            &store,
            assets,
            &seals,
            operation,
            None,
            published_at,
            &CommandCancellation::new(),
        ));
        let competing_request = DraftEditorCandidatePublicationRequestV1::new(
            selector,
            dirty.candidate().session_id(),
            operation,
            dirty.candidate().candidate_generation(),
            DraftRootHistoryPairV1::new(dirty.root(), dirty.history()),
            DraftEditorCandidatePublicationEvidenceV1::UnchangedEmpty,
            if collide {
                SyndicTimestamp::from_unix_millis(61)
            } else {
                published_at
            },
        );
        let competing_source = capture_lower_source(&store, storage, competing_request);
        let dependent_binding = if collide {
            commit_text(&mut host, &store, dirty, 2, 1, 1, "y", 2, 2)
        } else {
            dirty
        };
        host.test_arm_publication_before_execute_fault(move |store, storage| {
            let prepared = storage
                .prepare_draft_editor_candidate_publication(
                    store,
                    competing_source,
                    competing_request.evidence(),
                )
                .unwrap();
            let outcome = base::execute(
                store,
                storage.publish_draft_editor_candidate(
                    storage.revision(store).unwrap(),
                    prepared.clone(),
                ),
            );
            let _ = storage
                .reconcile_draft_editor_candidate_publication(store, &prepared, outcome)
                .unwrap();
        });
        assert_eq!(
            host.execute_publication(&store, ticket).unwrap(),
            if collide {
                ComposerHostPublicationCompletion::OccupiedIdentityCollision
            } else {
                ComposerHostPublicationCompletion::ExactReplay
            }
        );
        if collide {
            assert_eq!(host.publication_custody_count(), 1);
            assert!(host.publication_unavailable().is_some());
            assert!(
                host.begin_mutation(
                    &store,
                    dependent_binding,
                    gpui_text_input::MutationBeginRequest::new(
                        gpui_text_input::MutationProposal::new(
                            gpui_text_input::MutationKey::new(
                                gpui_text_input::BindingId::new(
                                    dependent_binding.host_generation().get(),
                                ),
                                gpui_text_input::SourceRevision::new(
                                    dependent_binding.candidate().candidate_generation(),
                                ),
                                gpui_text_input::OperationId::new(99),
                            ),
                            gpui_text_input::MutationKind::Edit,
                            gpui_text_input::MutationPositions::collapsed(composer::position(2)),
                            gpui_text_input::SourceRange::new(
                                composer::position(2),
                                composer::position(2),
                            )
                            .unwrap(),
                            0,
                        ),
                        gpui_text_input::MutationCursor::new(0),
                        gpui_text_input::MutationCursor::new(0),
                    ),
                )
                .is_err()
            );
        } else {
            assert_eq!(host.publication_custody_count(), 0);
            assert!(!host.is_dirty());
        }
    }
}

#[test]
fn exact_replay_callback_converges_to_a_same_session_later_publication() {
    let (_home, mut store, storage, thread) = fixture("phase172-replay-descendant", 111);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = service(&store, storage, assets, 1, 1);
    let durable = current(storage, &store, thread);
    let selector = DraftEditorCurrentSelectorV1::new(
        durable.thread().id(),
        durable.thread().revision(),
        durable.draft().id(),
        durable.draft().revision(),
        durable.draft().piece_root(),
        durable.draft().history(),
    );
    let (mut host, empty) = activated(storage, &store, thread, 112, 113);
    let first = commit_text(&mut host, &store, empty, 1, 0, 0, "a", 1, 1);
    let operation = operation_id(2);
    let published_at = SyndicTimestamp::from_unix_millis(111);
    let first_request = DraftEditorCandidatePublicationRequestV1::new(
        selector,
        first.candidate().session_id(),
        operation,
        first.candidate().candidate_generation(),
        DraftRootHistoryPairV1::new(first.root(), first.history()),
        DraftEditorCandidatePublicationEvidenceV1::UnchangedEmpty,
        published_at,
    );
    let first_source = capture_lower_source(&store, storage, first_request);
    let ticket = captured(capture_joined_publication(
        &mut host,
        &store,
        assets,
        &seals,
        operation,
        None,
        published_at,
        &CommandCancellation::new(),
    ));
    let later = commit_text(&mut host, &store, first, 3, 1, 1, "b", 2, 1);
    drive_ticket_ready(&mut host, &store, ticket);
    assert!(matches!(
        publish_lower_source(&store, storage, first_source, first_request),
        syndic_storage::DraftEditorCandidatePublicationOutcomeV1::Published(_, _)
    ));
    let published_first = current(storage, &store, thread);
    let published_first_selector = DraftEditorCurrentSelectorV1::new(
        published_first.thread().id(),
        published_first.thread().revision(),
        published_first.draft().id(),
        published_first.draft().revision(),
        published_first.draft().piece_root(),
        published_first.draft().history(),
    );
    let later_request = DraftEditorCandidatePublicationRequestV1::new(
        published_first_selector,
        later.candidate().session_id(),
        operation_id(4),
        later.candidate().candidate_generation(),
        DraftRootHistoryPairV1::new(later.root(), later.history()),
        DraftEditorCandidatePublicationEvidenceV1::UnchangedEmpty,
        SyndicTimestamp::from_unix_millis(112),
    );
    let later_source = capture_lower_source(&store, storage, later_request);
    host.test_arm_publication_convergence_read_fault(move |store, storage| {
        assert!(matches!(
            publish_lower_source(store, storage, later_source, later_request),
            syndic_storage::DraftEditorCandidatePublicationOutcomeV1::Published(_, _)
        ));
    });
    assert!(matches!(
        host.execute_publication(&store, ticket),
        Err(ComposerHostError::PublicationPending)
    ));
    assert_eq!(host.publication_custody_count(), 1);
    assert_eq!(host.test_publication_source_custody_count(), 0);
    assert!(host.is_dirty());
    assert!(!host.is_unavailable());
    assert_eq!(
        host.execute_publication(&store, ticket).unwrap(),
        ComposerHostPublicationCompletion::ExactReplay
    );
    let converged = host.binding().unwrap();
    assert_eq!(converged.candidate().candidate_generation(), 2);
    assert_eq!(converged.root(), later.root());
    assert_ne!(converged, first);
    assert!(!host.is_dirty());
    assert!(!host.is_unavailable());
    assert_eq!(host.publication_custody_count(), 0);
    assert_eq!(host.test_publication_source_custody_count(), 0);
    assert_publication_head_is_coherent(storage, &store, thread, converged);
}

#[test]
fn exact_replay_callback_rejects_a_competing_session_later_publication() {
    run_on_publication_test_stack(exact_replay_callback_rejects_a_competing_session_case);
}

fn exact_replay_callback_rejects_a_competing_session_case() {
    let (_home, mut store, storage, thread) = fixture("phase172-replay-other-session", 116);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = service(&store, storage, assets, 1, 1);
    let durable = current(storage, &store, thread);
    let selector = DraftEditorCurrentSelectorV1::new(
        durable.thread().id(),
        durable.thread().revision(),
        durable.draft().id(),
        durable.draft().revision(),
        durable.draft().piece_root(),
        durable.draft().history(),
    );
    let (mut host, empty) = activated(storage, &store, thread, 117, 118);
    let first = commit_text(&mut host, &store, empty, 1, 0, 0, "a", 1, 1);
    let operation = operation_id(2);
    let published_at = SyndicTimestamp::from_unix_millis(116);
    let first_request = DraftEditorCandidatePublicationRequestV1::new(
        selector,
        first.candidate().session_id(),
        operation,
        first.candidate().candidate_generation(),
        DraftRootHistoryPairV1::new(first.root(), first.history()),
        DraftEditorCandidatePublicationEvidenceV1::UnchangedEmpty,
        published_at,
    );
    let first_source = capture_lower_source(&store, storage, first_request);
    let ticket = captured(capture_joined_publication(
        &mut host,
        &store,
        assets,
        &seals,
        operation,
        None,
        published_at,
        &CommandCancellation::new(),
    ));
    drive_ticket_ready(&mut host, &store, ticket);
    assert!(matches!(
        publish_lower_source(&store, storage, first_source, first_request),
        syndic_storage::DraftEditorCandidatePublicationOutcomeV1::Published(_, _)
    ));
    let competing = publish_competing_text_candidate(storage, &store, thread, 119, 120, 117);
    assert_eq!(
        host.execute_publication(&store, ticket).unwrap(),
        ComposerHostPublicationCompletion::DurableBaseConflict
    );
    assert_eq!(host.binding(), Some(first));
    assert!(host.is_unavailable());
    assert_eq!(host.publication_custody_count(), 1);
    assert_eq!(host.test_publication_source_custody_count(), 0);
    assert_eq!(
        host.publication_unavailable(),
        Some(ComposerHostPublicationUnavailable::DurableBaseConflict)
    );
    assert_eq!(
        current(storage, &store, thread).draft().piece_root(),
        competing.root()
    );
}

#[test]
fn superseded_callback_rejects_a_competing_session_later_publication() {
    run_on_publication_test_stack(superseded_callback_rejects_a_competing_session_case);
}

fn superseded_callback_rejects_a_competing_session_case() {
    let (_home, mut store, storage, thread) = fixture("phase172-superseded-other-session", 121);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = service(&store, storage, assets, 1, 1);
    let durable = current(storage, &store, thread);
    let selector = DraftEditorCurrentSelectorV1::new(
        durable.thread().id(),
        durable.thread().revision(),
        durable.draft().id(),
        durable.draft().revision(),
        durable.draft().piece_root(),
        durable.draft().history(),
    );
    let (mut host, empty) = activated(storage, &store, thread, 122, 123);
    let first = commit_text(&mut host, &store, empty, 1, 0, 0, "a", 1, 1);
    let operation = operation_id(2);
    let published_at = SyndicTimestamp::from_unix_millis(121);
    let ticket = captured(capture_joined_publication(
        &mut host,
        &store,
        assets,
        &seals,
        operation,
        None,
        published_at,
        &CommandCancellation::new(),
    ));
    let later = commit_text(&mut host, &store, first, 3, 1, 1, "b", 2, 1);
    drive_ticket_ready(&mut host, &store, ticket);
    let later_request = DraftEditorCandidatePublicationRequestV1::new(
        selector,
        later.candidate().session_id(),
        operation_id(4),
        later.candidate().candidate_generation(),
        DraftRootHistoryPairV1::new(later.root(), later.history()),
        DraftEditorCandidatePublicationEvidenceV1::UnchangedEmpty,
        SyndicTimestamp::from_unix_millis(122),
    );
    let later_source = capture_lower_source(&store, storage, later_request);
    assert!(matches!(
        publish_lower_source(&store, storage, later_source, later_request),
        syndic_storage::DraftEditorCandidatePublicationOutcomeV1::Published(_, _)
    ));
    let competing = publish_competing_text_candidate(storage, &store, thread, 124, 125, 123);
    assert_eq!(
        host.execute_publication(&store, ticket).unwrap(),
        ComposerHostPublicationCompletion::DurableBaseConflict
    );
    assert_eq!(host.binding(), Some(later));
    assert!(host.is_unavailable());
    assert_eq!(host.publication_custody_count(), 1);
    assert_eq!(host.test_publication_source_custody_count(), 0);
    assert_eq!(
        host.publication_unavailable(),
        Some(ComposerHostPublicationUnavailable::DurableBaseConflict)
    );
    assert_eq!(
        current(storage, &store, thread).draft().piece_root(),
        competing.root()
    );
}

#[test]
fn session_disposal_retains_terminal_publication_custody() {
    let (_home, mut store, storage, thread) = fixture("phase172-session-disposed", 70);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = service(&store, storage, assets, 1, 1);
    let durable = current(storage, &store, thread);
    let selector = DraftEditorCurrentSelectorV1::new(
        durable.thread().id(),
        durable.thread().revision(),
        durable.draft().id(),
        durable.draft().revision(),
        durable.draft().piece_root(),
        durable.draft().history(),
    );
    let (mut host, empty) = activated(storage, &store, thread, 71, 72);
    let dirty = commit_text(&mut host, &store, empty, 1, 0, 0, "x", 1, 1);
    let ticket = captured(capture_joined_publication(
        &mut host,
        &store,
        assets,
        &seals,
        operation_id(73),
        None,
        SyndicTimestamp::from_unix_millis(73),
        &CommandCancellation::new(),
    ));
    host.test_arm_publication_before_execute_fault(move |store, storage| {
        let request = DraftEditorCandidatePublicationRequestV1::new(
            selector,
            dirty.candidate().session_id(),
            operation_id(74),
            dirty.candidate().candidate_generation(),
            DraftRootHistoryPairV1::new(dirty.root(), dirty.history()),
            DraftEditorCandidatePublicationEvidenceV1::UnchangedEmpty,
            SyndicTimestamp::from_unix_millis(74),
        );
        let prepared = prepare_lower(store, storage, request);
        let outcome = base::execute(
            store,
            storage
                .publish_draft_editor_candidate(storage.revision(store).unwrap(), prepared.clone()),
        );
        let _ = storage
            .reconcile_draft_editor_candidate_publication(store, &prepared, outcome)
            .unwrap();
        let session = storage
            .draft_editor_candidate_session(
                store,
                dirty.candidate().draft_id(),
                dirty.candidate().session_id(),
            )
            .unwrap();
        let syndic_storage::DraftEditorCandidateSessionReadOutcomeV1::Active(session) = session
        else {
            panic!("candidate session was not active")
        };
        let request = DraftEditorCandidateSessionDisposeRequestV1::new(
            session.draft_id(),
            session.session_id(),
            operation_id(75),
            session.session_generation(),
            DraftRootHistoryPairV1::new(session.published_root(), session.published_history()),
        );
        let prepared = storage
            .prepare_dispose_draft_editor_candidate_session(store, request)
            .unwrap();
        let outcome = base::execute(
            store,
            storage.dispose_draft_editor_candidate_session(
                storage.revision(store).unwrap(),
                prepared.clone(),
            ),
        );
        let _ = storage
            .reconcile_draft_editor_candidate_session_disposal(store, &prepared, outcome)
            .unwrap();
    });
    assert_eq!(
        host.execute_publication(&store, ticket).unwrap(),
        ComposerHostPublicationCompletion::SessionDisposed
    );
    assert_eq!(host.publication_custody_count(), 1);
    assert_eq!(
        host.publication_unavailable(),
        Some(ComposerHostPublicationUnavailable::SessionDisposed)
    );
}

#[test]
fn newer_durable_publication_supersedes_only_the_captured_generation() {
    let (_home, mut store, storage, thread) = fixture("phase172-superseded", 65);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = service(&store, storage, assets, 1, 1);
    let durable = current(storage, &store, thread);
    let selector = DraftEditorCurrentSelectorV1::new(
        durable.thread().id(),
        durable.thread().revision(),
        durable.draft().id(),
        durable.draft().revision(),
        durable.draft().piece_root(),
        durable.draft().history(),
    );
    let (mut host, empty) = activated(storage, &store, thread, 66, 67);
    let first = commit_text(&mut host, &store, empty, 1, 0, 0, "a", 1, 1);
    let ticket = captured(capture_joined_publication(
        &mut host,
        &store,
        assets,
        &seals,
        operation_id(2),
        None,
        SyndicTimestamp::from_unix_millis(65),
        &CommandCancellation::new(),
    ));
    let later = commit_text(&mut host, &store, first, 3, 1, 1, "b", 2, 1);
    host.test_arm_publication_before_execute_fault(move |store, storage| {
        let request = DraftEditorCandidatePublicationRequestV1::new(
            selector,
            later.candidate().session_id(),
            operation_id(4),
            later.candidate().candidate_generation(),
            DraftRootHistoryPairV1::new(later.root(), later.history()),
            DraftEditorCandidatePublicationEvidenceV1::UnchangedEmpty,
            SyndicTimestamp::from_unix_millis(66),
        );
        let prepared = prepare_lower(store, storage, request);
        let outcome = base::execute(
            store,
            storage
                .publish_draft_editor_candidate(storage.revision(store).unwrap(), prepared.clone()),
        );
        let _ = storage
            .reconcile_draft_editor_candidate_publication(store, &prepared, outcome)
            .unwrap();
    });
    assert_eq!(
        host.execute_publication(&store, ticket).unwrap(),
        ComposerHostPublicationCompletion::Superseded
    );
    let binding = host.binding().unwrap();
    assert_eq!(binding.root(), later.root());
    assert_eq!(
        binding.candidate().candidate_generation(),
        later.candidate().candidate_generation()
    );
    assert!(!host.is_dirty());
}

#[test]
fn clean_disposal_replay_already_disposed_and_stale_callbacks_are_exact() {
    for (name, seed, already_disposed) in [
        ("phase172-disposal-replay", 81, false),
        ("phase172-disposal-already", 91, true),
    ] {
        let (_home, mut store, storage, thread) = fixture(name, seed);
        let assets = BerylState::register(&mut store).unwrap().assets();
        let seals = service(&store, storage, assets, 1, 1);
        let (mut host, empty) = activated(storage, &store, thread, seed + 1, seed + 2);
        let _dirty = commit_text(&mut host, &store, empty, 1, 0, 0, "x", 1, 1);
        publish_unchanged(&mut host, &store, assets, &seals, 2);
        let binding = host.binding().unwrap();
        let operation = operation_id(u64::from(seed + 3));
        let flush = started_release(&mut host);
        assert!(matches!(
            host.capture_flush_disposal(&store, flush, operation, &CommandCancellation::new(),)
                .unwrap(),
            ComposerHostFlushCapture::State(ComposerHostFlushState::DisposalRequired)
        ));
        let session = storage
            .draft_editor_candidate_session(
                &store,
                binding.candidate().draft_id(),
                binding.candidate().session_id(),
            )
            .unwrap();
        let syndic_storage::DraftEditorCandidateSessionReadOutcomeV1::Active(session) = session
        else {
            panic!("candidate session was not active")
        };
        let competing_operation = if already_disposed {
            operation_id(u64::from(seed + 4))
        } else {
            operation
        };
        host.test_arm_publication_before_execute_fault(move |store, storage| {
            let request = DraftEditorCandidateSessionDisposeRequestV1::new(
                session.draft_id(),
                session.session_id(),
                competing_operation,
                session.session_generation(),
                DraftRootHistoryPairV1::new(session.published_root(), session.published_history()),
            );
            let prepared = storage
                .prepare_dispose_draft_editor_candidate_session(store, request)
                .unwrap();
            let outcome = base::execute(
                store,
                storage.dispose_draft_editor_candidate_session(
                    storage.revision(store).unwrap(),
                    prepared.clone(),
                ),
            );
            let _ = storage
                .reconcile_draft_editor_candidate_session_disposal(store, &prepared, outcome)
                .unwrap();
        });
        assert_eq!(
            host.advance_flush(&store, flush).unwrap(),
            ComposerHostFlushAdvance::Satisfied(ComposerHostFlushPurpose::Release)
        );
    }

    let (_home, mut store, storage, thread) = fixture("phase172-disposal-stale", 101);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = service(&store, storage, assets, 1, 1);
    let (mut host, empty) = activated(storage, &store, thread, 102, 103);
    let _dirty = commit_text(&mut host, &store, empty, 10, 0, 0, "x", 1, 1);
    publish_unchanged(&mut host, &store, assets, &seals, 11);
    let cancellation = CommandCancellation::new();
    let stale = started_release(&mut host);
    assert!(matches!(
        host.capture_flush_disposal(&store, stale, operation_id(1), &cancellation)
            .unwrap(),
        ComposerHostFlushCapture::State(ComposerHostFlushState::DisposalRequired)
    ));
    cancellation.cancel();
    assert_eq!(
        host.advance_flush(&store, stale).unwrap(),
        ComposerHostFlushAdvance::Unsatisfied(ComposerHostFlushFailure::Cancelled)
    );
    let current = started_release(&mut host);
    assert!(matches!(
        host.capture_flush_disposal(
            &store,
            current,
            operation_id(2),
            &CommandCancellation::new(),
        )
        .unwrap(),
        ComposerHostFlushCapture::State(ComposerHostFlushState::DisposalRequired)
    ));
    assert_eq!(
        host.advance_flush(&store, stale).unwrap(),
        ComposerHostFlushAdvance::Stale
    );
    assert_eq!(
        host.advance_flush(&store, current).unwrap(),
        ComposerHostFlushAdvance::Satisfied(ComposerHostFlushPurpose::Release)
    );
}

#[test]
fn indeterminate_collision_retains_exact_terminal_custody() {
    let (_home, mut store, storage, thread, faults) =
        base::fault_fixture("phase172-indeterminate-collision", 106);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = service(&store, storage, assets, 1, 1);
    let (mut host, empty) = activated(storage, &store, thread, 107, 108);
    let dirty = commit_text(&mut host, &store, empty, 1, 0, 0, "x", 1, 1);
    let ticket = captured(capture_joined_publication(
        &mut host,
        &store,
        assets,
        &seals,
        operation_id(109),
        None,
        SyndicTimestamp::from_unix_millis(109),
        &CommandCancellation::new(),
    ));
    host.test_arm_publication_before_execute_fault(move |_, _| {
        faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    });
    assert_eq!(
        host.execute_publication(&store, ticket).unwrap(),
        ComposerHostPublicationCompletion::ReconciliationPending
    );
    let session = storage
        .draft_editor_candidate_session(
            &store,
            dirty.candidate().draft_id(),
            dirty.candidate().session_id(),
        )
        .unwrap();
    let syndic_storage::DraftEditorCandidateSessionReadOutcomeV1::Active(session) = session else {
        panic!("candidate session was not active")
    };
    direct_adopt(
        &store,
        storage,
        DraftHistoricalRootSelectionIntentV1::new(
            syndic_storage::DraftEditorCandidateActivationBindingV1::from_head(&session),
            operation_id(110),
            DraftHistoricalRootDirectionV1::Undo,
        ),
    );
    assert_eq!(
        host.reconcile_publication(&store, ticket).unwrap(),
        ComposerHostPublicationCompletion::ReconciliationCollision
    );
    assert_eq!(host.publication_custody_count(), 1);
    assert_eq!(
        host.publication_unavailable(),
        Some(ComposerHostPublicationUnavailable::ReconciliationCollision)
    );
}

#[test]
fn ambiguous_exact_new_and_clean_disposal_are_generation_qualified() {
    let (_home, mut store, storage, thread, faults) =
        base::fault_fixture("phase172-indeterminate", 71);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = service(&store, storage, assets, 1, 1);
    let (mut host, empty) = activated(storage, &store, thread, 72, 73);
    let _dirty = commit_text(&mut host, &store, empty, 1, 0, 0, "x", 1, 1);
    let ticket = captured(capture_joined_publication(
        &mut host,
        &store,
        assets,
        &seals,
        operation_id(2),
        None,
        SyndicTimestamp::from_unix_millis(70),
        &CommandCancellation::new(),
    ));
    host.test_arm_publication_before_execute_fault(move |_, _| {
        faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    });
    assert_eq!(
        host.execute_publication(&store, ticket).unwrap(),
        ComposerHostPublicationCompletion::ReconciliationPending
    );
    assert_eq!(store.health().state(), HomeHealthState::Healthy);
    assert_eq!(
        host.reconcile_publication(&store, ticket).unwrap(),
        ComposerHostPublicationCompletion::Published
    );
    assert_eq!(
        current(storage, &store, thread).draft().piece_root(),
        host.binding().unwrap().root()
    );

    let flush = started_release(&mut host);
    assert!(matches!(
        host.capture_flush_disposal(&store, flush, operation_id(3), &CommandCancellation::new(),)
            .unwrap(),
        ComposerHostFlushCapture::State(ComposerHostFlushState::DisposalRequired)
    ));
    assert_eq!(
        host.advance_flush(&store, flush).unwrap(),
        ComposerHostFlushAdvance::Satisfied(ComposerHostFlushPurpose::Release)
    );
}

fn started_release(host: &mut SyndicComposerHost) -> ComposerHostFlushTicket {
    match host.begin_flush(ComposerHostFlushPurpose::Release).unwrap() {
        ComposerHostFlushAdmission::Started {
            ticket,
            state: ComposerHostFlushState::DisposalRequired,
        } => ticket,
        other => panic!("release flush did not require disposal: {other:?}"),
    }
}

fn captured(
    result: Result<ComposerHostPublicationCapture, ComposerHostError>,
) -> ComposerHostPublicationTicket {
    match result.unwrap() {
        ComposerHostPublicationCapture::Captured(ticket) => ticket,
        other => panic!("publication was not captured: {other:?}"),
    }
}

fn capture_joined_publication(
    host: &mut SyndicComposerHost,
    store: &beryl_home_store::HomeStore,
    assets: beryl_state::AssetState,
    seals: &DraftMarkerSealService,
    operation_id: syndic_storage::DraftPieceOperationIdV1,
    marker_authority: Option<beryl_app::composer_host::ComposerHostMarkerSealAuthority>,
    published_at: SyndicTimestamp,
    cancellation: &CommandCancellation,
) -> Result<ComposerHostPublicationCapture, ComposerHostError> {
    let flush = match host.begin_flush(ComposerHostFlushPurpose::Submission)? {
        ComposerHostFlushAdmission::Started { ticket, .. }
        | ComposerHostFlushAdmission::Joined { ticket, .. } => ticket,
        ComposerHostFlushAdmission::Satisfied(_) => {
            return Ok(ComposerHostPublicationCapture::CleanNoOp);
        }
    };
    match host.capture_flush_publication(
        store,
        flush,
        assets,
        seals,
        operation_id,
        marker_authority,
        published_at,
        cancellation,
    )? {
        ComposerHostFlushCapture::Captured(ticket) => {
            Ok(ComposerHostPublicationCapture::Captured(ticket))
        }
        ComposerHostFlushCapture::Satisfied(_) => Ok(ComposerHostPublicationCapture::CleanNoOp),
        ComposerHostFlushCapture::Unsatisfied(_) => Err(ComposerHostError::PublicationUnavailable),
        ComposerHostFlushCapture::State(_) => Err(ComposerHostError::PublicationPending),
        ComposerHostFlushCapture::Stale => Err(ComposerHostError::StalePublicationGeneration),
    }
}

fn publish_changed(
    host: &mut beryl_app::composer_host::SyndicComposerHost,
    store: &beryl_home_store::HomeStore,
    assets: beryl_state::AssetState,
    seals: &DraftMarkerSealService,
    operation: u64,
    authority_seed: u8,
) {
    let ticket = captured(capture_joined_publication(
        host,
        store,
        assets,
        seals,
        operation_id(operation),
        Some(authority(authority_seed)),
        SyndicTimestamp::from_unix_millis(operation),
        &CommandCancellation::new(),
    ));
    publish_ticket(host, store, ticket);
}

fn publish_unchanged(
    host: &mut beryl_app::composer_host::SyndicComposerHost,
    store: &beryl_home_store::HomeStore,
    assets: beryl_state::AssetState,
    seals: &DraftMarkerSealService,
    operation: u64,
) {
    let ticket = captured(capture_joined_publication(
        host,
        store,
        assets,
        seals,
        operation_id(operation),
        None,
        SyndicTimestamp::from_unix_millis(operation),
        &CommandCancellation::new(),
    ));
    assert_eq!(
        host.execute_publication(store, ticket).unwrap(),
        ComposerHostPublicationCompletion::Published
    );
}

fn publish_ticket(
    host: &mut beryl_app::composer_host::SyndicComposerHost,
    store: &beryl_home_store::HomeStore,
    ticket: ComposerHostPublicationTicket,
) {
    loop {
        match host.drive_publication(store, ticket).unwrap() {
            ComposerHostPublicationDrive::Progress
            | ComposerHostPublicationDrive::NotCommitted(_) => {}
            ComposerHostPublicationDrive::Ready => break,
            other => panic!("publication did not become ready: {other:?}"),
        }
    }
    assert_eq!(
        host.execute_publication(store, ticket).unwrap(),
        ComposerHostPublicationCompletion::Published
    );
}

fn drive_ticket_ready(
    host: &mut beryl_app::composer_host::SyndicComposerHost,
    store: &beryl_home_store::HomeStore,
    ticket: ComposerHostPublicationTicket,
) {
    loop {
        match host.drive_publication(store, ticket).unwrap() {
            ComposerHostPublicationDrive::Progress
            | ComposerHostPublicationDrive::NotCommitted(_) => {}
            ComposerHostPublicationDrive::Ready => return,
            other => panic!("publication did not become ready: {other:?}"),
        }
    }
}

fn publish_lower(
    store: &beryl_home_store::HomeStore,
    storage: syndic_storage::SyndicStorage,
    request: DraftEditorCandidatePublicationRequestV1,
) -> syndic_storage::DraftEditorCandidatePublicationOutcomeV1 {
    let prepared = prepare_lower(store, storage, request);
    let outcome = base::execute(
        store,
        storage.publish_draft_editor_candidate(storage.revision(store).unwrap(), prepared.clone()),
    );
    storage
        .reconcile_draft_editor_candidate_publication(store, &prepared, outcome)
        .unwrap()
}

fn publish_lower_source(
    store: &beryl_home_store::HomeStore,
    storage: syndic_storage::SyndicStorage,
    source: CapturedDraftEditorCandidatePublicationSourceV1,
    request: DraftEditorCandidatePublicationRequestV1,
) -> syndic_storage::DraftEditorCandidatePublicationOutcomeV1 {
    let prepared = storage
        .prepare_draft_editor_candidate_publication(store, source, request.evidence())
        .unwrap();
    let outcome = base::execute(
        store,
        storage.publish_draft_editor_candidate(storage.revision(store).unwrap(), prepared.clone()),
    );
    storage
        .reconcile_draft_editor_candidate_publication(store, &prepared, outcome)
        .unwrap()
}

fn capture_lower_source(
    store: &beryl_home_store::HomeStore,
    storage: syndic_storage::SyndicStorage,
    request: DraftEditorCandidatePublicationRequestV1,
) -> CapturedDraftEditorCandidatePublicationSourceV1 {
    let session = storage
        .draft_editor_candidate_session(store, request.selector().draft_id(), request.session_id())
        .unwrap();
    let syndic_storage::DraftEditorCandidateSessionReadOutcomeV1::Active(session) = session else {
        panic!("candidate session was not active")
    };
    let candidate = syndic_storage::DraftEditorCandidateActivationBindingV1::from_head(&session);
    assert_eq!(
        candidate.candidate_generation(),
        request.candidate_generation()
    );
    assert_eq!(candidate.root(), request.candidate().root());
    assert_eq!(candidate.history(), request.candidate().history());
    storage
        .capture_draft_editor_candidate_publication_source(
            store,
            DraftEditorCandidatePublicationSourceCaptureRequestV1::new(
                request.selector(),
                candidate,
                request.operation_id(),
                request.published_at(),
            ),
        )
        .unwrap()
}

fn prepare_lower(
    store: &beryl_home_store::HomeStore,
    storage: syndic_storage::SyndicStorage,
    request: DraftEditorCandidatePublicationRequestV1,
) -> syndic_storage::PreparedDraftEditorCandidatePublicationV1 {
    let source = capture_lower_source(store, storage, request);
    storage
        .prepare_draft_editor_candidate_publication(store, source, request.evidence())
        .unwrap()
}

fn assert_publication_head_is_coherent(
    storage: syndic_storage::SyndicStorage,
    store: &beryl_home_store::HomeStore,
    thread: beryl_model::SyndicThreadId,
    binding: beryl_app::composer_host::ComposerHostBinding,
) {
    let durable = current(storage, store, thread);
    let session = storage
        .draft_editor_candidate_session(
            store,
            binding.candidate().draft_id(),
            binding.candidate().session_id(),
        )
        .unwrap();
    let syndic_storage::DraftEditorCandidateSessionReadOutcomeV1::Active(session) = session else {
        panic!("candidate session was not active")
    };
    assert_eq!(durable.draft().piece_root(), session.published_root());
    assert_eq!(durable.draft().history(), session.published_history());
    assert_eq!(
        session.published_candidate_generation(),
        binding.candidate().candidate_generation()
    );
    assert_eq!(
        session.newest_candidate_generation(),
        binding.candidate().candidate_generation()
    );
    assert_eq!(session.newest_root(), binding.root());
}

fn publish_competing_text_candidate(
    storage: syndic_storage::SyndicStorage,
    store: &beryl_home_store::HomeStore,
    thread: beryl_model::SyndicThreadId,
    session: u8,
    open_operation: u8,
    timestamp: u64,
) -> beryl_app::composer_host::ComposerHostBinding {
    let durable = current(storage, store, thread);
    let end = durable
        .draft()
        .piece_root()
        .summary()
        .logical_extent()
        .logical_utf8_bytes();
    let (mut host, base) = activated(storage, store, thread, session, open_operation);
    let competing = commit_text(&mut host, store, base, 3, end, end, "z", end + 1, 1);
    let selector = DraftEditorCurrentSelectorV1::new(
        durable.thread().id(),
        durable.thread().revision(),
        durable.draft().id(),
        durable.draft().revision(),
        durable.draft().piece_root(),
        durable.draft().history(),
    );
    let request = DraftEditorCandidatePublicationRequestV1::new(
        selector,
        competing.candidate().session_id(),
        operation_id(4),
        competing.candidate().candidate_generation(),
        DraftRootHistoryPairV1::new(competing.root(), competing.history()),
        DraftEditorCandidatePublicationEvidenceV1::UnchangedEmpty,
        SyndicTimestamp::from_unix_millis(timestamp),
    );
    let source = capture_lower_source(store, storage, request);
    assert!(matches!(
        publish_lower_source(store, storage, source, request),
        syndic_storage::DraftEditorCandidatePublicationOutcomeV1::Published(_, _)
    ));
    competing
}

fn run_on_publication_test_stack(test: fn()) {
    std::thread::Builder::new()
        .name("phase172-publication".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(test)
        .unwrap()
        .join()
        .unwrap();
}

fn assert_captured_published_and_newest_dirty(
    storage: syndic_storage::SyndicStorage,
    store: &beryl_home_store::HomeStore,
    thread: beryl_model::SyndicThreadId,
    captured: beryl_app::composer_host::ComposerHostBinding,
    newest: beryl_app::composer_host::ComposerHostBinding,
) {
    let durable = current(storage, store, thread);
    let session = storage
        .draft_editor_candidate_session(
            store,
            captured.candidate().draft_id(),
            captured.candidate().session_id(),
        )
        .unwrap();
    let syndic_storage::DraftEditorCandidateSessionReadOutcomeV1::Active(session) = session else {
        panic!("candidate session was not active")
    };
    assert_eq!(durable.draft().piece_root(), captured.root());
    assert_eq!(durable.draft().history(), session.published_history());
    assert_eq!(
        session.published_candidate_generation(),
        captured.candidate().candidate_generation()
    );
    assert_eq!(session.published_root(), captured.root());
    assert_eq!(
        session.published_history().candidate_generation(),
        captured.candidate().candidate_generation()
    );
    assert_eq!(session.published_history().root(), captured.root());
    assert_eq!(
        session.newest_candidate_generation(),
        newest.candidate().candidate_generation()
    );
    assert_eq!(session.newest_root(), newest.root());
    assert_eq!(session.newest_history(), newest.history());
    assert!(session.newest_candidate_generation() > session.published_candidate_generation());
}
