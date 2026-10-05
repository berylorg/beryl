use super::*;
use beryl_app::composer_host::{
    ComposerHostBinding, ComposerHostPrivatePasteActivation, ComposerHostPrivatePasteOrigin,
};
use beryl_home_store::{CommandOutcome, HomeCommand};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use syndic_storage::DraftPrivateClipboardSourceV1;

struct Activation {
    key: MutationKey,
    live: AtomicBool,
    submissions: AtomicUsize,
}

impl ComposerHostPrivatePasteActivation for Activation {
    fn activate(&self, key: MutationKey, submit: &mut dyn FnMut()) -> bool {
        assert_eq!(key, self.key);
        if !self.live.load(Ordering::SeqCst) {
            return false;
        }
        self.submissions.fetch_add(1, Ordering::SeqCst);
        submit();
        true
    }
}

fn marked_origin(fixture: &support::Fixture) -> (ComposerHostBinding, beryl_model::AssetId) {
    let asset = publish_image_asset(
        &fixture.store,
        fixture.assets.clone(),
        b"private-source-image",
    );
    let (mut host, binding) = activate(
        fixture.storage.clone(),
        &fixture.store,
        fixture.thread,
        171,
        172,
    );
    let object = InlineObjectId::new(1701);
    let order = InlineObjectOrder::new(1);
    let metadata = Box::new([ComposerHostImageMarkerMetadata::new(object, asset)]);
    let (mut editor, staging, proposal, finish) = replay::prepare(
        fixture,
        &mut host,
        binding,
        173,
        SourceRange::new(origin(), origin()).unwrap(),
        vec![MutationPageItem::Object(ObjectChange::Insert {
            object: SuccessorObject::new(object, ByteOffset::new(0), order, 1, 1),
        })],
        metadata.clone(),
        after(object, order),
    );
    stage(
        &mut host,
        &fixture.store,
        &mut editor,
        staging,
        proposal,
        metadata,
    );
    editor.finish_pass_input(staging, finish).unwrap();
    host.finish_mutation_input(&fixture.store, finish).unwrap();
    settle(&mut host, &fixture.store, finish.key());
    publish_origin(fixture, &mut host);
    (host.binding().unwrap(), asset)
}

fn publish_origin(fixture: &support::Fixture, host: &mut SyndicComposerHost) {
    use beryl_app::{
        composer_host::{
            ComposerHostFlushAdmission, ComposerHostFlushAdvance, ComposerHostFlushCapture,
            ComposerHostFlushPurpose, ComposerHostFlushState, ComposerHostMarkerSealAuthority,
        },
        composer_marker_seal::{DraftMarkerSealService, DraftMarkerSealServiceLimits},
    };
    use std::num::NonZeroUsize;
    let seals = DraftMarkerSealService::test_new(
        &fixture.store,
        fixture.store.health().generation().unwrap(),
        fixture.storage.clone(),
        fixture.assets.clone(),
        DraftMarkerSealServiceLimits::new(
            NonZeroUsize::new(1).unwrap(),
            NonZeroUsize::new(32).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let flush = match host
        .begin_flush(ComposerHostFlushPurpose::Submission)
        .unwrap()
    {
        ComposerHostFlushAdmission::Started { ticket, .. }
        | ComposerHostFlushAdmission::Joined { ticket, .. } => ticket,
        other => panic!("marked origin did not require publication: {other:?}"),
    };
    let authority = ComposerHostMarkerSealAuthority::new(
        syndic_storage::DraftMarkerSealOperationIdV1::from_bytes([175; 16]),
        beryl_state::AssetReferenceSetStagingAuthority::new(
            beryl_model::AssetReferenceSetId::from_bytes([176; 16]),
            [177; 32],
        ),
    );
    let capture = host
        .capture_flush_publication(
            &fixture.store,
            flush,
            fixture.assets.clone(),
            &seals,
            syndic_storage::DraftPieceOperationIdV1::from_bytes([174; 16]),
            Some(authority),
            syndic_storage::SyndicTimestamp::from_unix_millis(174),
            &CommandCancellation::new(),
        )
        .unwrap();
    assert!(
        matches!(capture, ComposerHostFlushCapture::Captured(_)),
        "marked origin publication was not captured: {capture:?}"
    );
    for _ in 0..128 {
        match host.advance_flush(&fixture.store, flush).unwrap() {
            ComposerHostFlushAdvance::Satisfied(ComposerHostFlushPurpose::Submission) => return,
            ComposerHostFlushAdvance::Progress(ComposerHostFlushState::CaptureRequired) => {
                let convergence = host
                    .capture_flush_publication(
                        &fixture.store,
                        flush,
                        fixture.assets.clone(),
                        &seals,
                        syndic_storage::DraftPieceOperationIdV1::from_bytes([178; 16]),
                        None,
                        syndic_storage::SyndicTimestamp::from_unix_millis(178),
                        &CommandCancellation::new(),
                    )
                    .unwrap();
                assert!(
                    matches!(
                        convergence,
                        ComposerHostFlushCapture::Satisfied(ComposerHostFlushPurpose::Submission)
                    ),
                    "published origin did not converge to its clean checkpoint: {convergence:?}"
                );
                return;
            }
            ComposerHostFlushAdvance::Progress(_)
            | ComposerHostFlushAdvance::ReconciliationPending => {}
            other => panic!("marked origin publication did not settle: {other:?}"),
        }
    }
    panic!("marked origin publication exceeded its bounded work budget");
}

fn foreign_destination(fixture: &support::Fixture) -> (SyndicComposerHost, ComposerHostBinding) {
    use beryl_model::{
        ExecutionBinding, PathFlavor, RootId, RuntimeId, RuntimeMode, RuntimeNativePath,
        SyndicDraftId, SyndicThreadId,
    };
    let thread = SyndicThreadId::from_bytes([180; 16]);
    let contribution = fixture.storage.create_thread(
        fixture.storage.revision(&fixture.store).unwrap(),
        syndic_storage::CreateThread::ordinary(
            thread,
            SyndicDraftId::from_bytes([181; 16]),
            ExecutionBinding::new(
                RuntimeId::from_bytes([182; 16]),
                RootId::from_bytes([183; 16]),
                RuntimeNativePath::from_admitted(
                    RuntimeMode::host(),
                    PathFlavor::Windows,
                    "C:\\private-origin",
                )
                .unwrap(),
            ),
            syndic_storage::SyndicTimestamp::from_unix_millis(1),
            syndic_storage::DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
        ),
    );
    let mut command = HomeCommand::new(fixture.store.home_revision().unwrap());
    command.add(contribution).unwrap();
    assert!(matches!(
        fixture.store.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    activate(fixture.storage.clone(), &fixture.store, thread, 184, 185)
}

fn source(binding: ComposerHostBinding) -> DraftPrivateClipboardSourceV1 {
    let candidate = binding.candidate();
    DraftPrivateClipboardSourceV1::from_candidate(
        candidate.draft_id(),
        candidate.session_id(),
        candidate.candidate_generation(),
        candidate.root(),
    )
    .unwrap()
}

fn prepare_private(
    fixture: &support::Fixture,
    host: &mut SyndicComposerHost,
    binding: ComposerHostBinding,
    source: DraftPrivateClipboardSourceV1,
    asset: beryl_model::AssetId,
) -> (
    RangeEditCoordinator,
    MutationPass,
    MutationPage,
    MutationFinishInput,
    Box<[ComposerHostImageMarkerMetadata]>,
    Arc<Activation>,
) {
    let (mut editor, begin, key) = begin_edit(
        binding,
        186,
        SourceRange::new(origin(), origin()).unwrap(),
        187,
    );
    let evidence = editor.request_evidence(key).unwrap();
    let activation = Arc::new(Activation {
        key,
        live: AtomicBool::new(true),
        submissions: AtomicUsize::new(0),
    });
    assert!(matches!(drive_evidence(
        host, &fixture.store, binding, &fixture.assets,
        ComposerHostMutationEvidenceRequest::BeginPrivate {
            begin, pass: evidence, origin: ComposerHostPrivatePasteOrigin::new(source, activation.clone()),
        },
    ), ComposerHostMutationEvidenceOutcome::Started(actual) if actual == evidence));
    let object = InlineObjectId::new(1702);
    let order = InlineObjectOrder::new(1);
    let proposal = page(
        &editor,
        key,
        MutationLane::Proposal,
        vec![MutationPageItem::Object(ObjectChange::Insert {
            object: SuccessorObject::new(object, ByteOffset::new(0), order, 1, 1),
        })],
    );
    let metadata: Box<[ComposerHostImageMarkerMetadata]> =
        Box::new([ComposerHostImageMarkerMetadata::from_source(
            object,
            asset,
            source.marker_selector(SyndicDraftMarkerId::from_bytes(1701_u128.to_be_bytes())),
        )]);
    let acknowledgement = editor
        .submit_evidence_page(evidence, proposal.clone())
        .unwrap();
    assert!(
        matches!(drive_evidence(host, &fixture.store, binding, &fixture.assets,
        ComposerHostMutationEvidenceRequest::Page { pass: evidence, page: proposal.clone(), metadata: metadata.clone() }),
        ComposerHostMutationEvidenceOutcome::PageAccepted(actual) if actual == evidence)
    );
    editor.acknowledge_evidence_page(acknowledgement).unwrap();
    let finish = finish_input(&editor, key, after(object, order));
    editor.finish_evidence(evidence, finish).unwrap();
    (editor, evidence, proposal, finish, metadata, activation)
}

fn dispose_origin(store: &HomeStore, storage: SyndicStorage, origin: ComposerHostBinding) {
    let session = storage
        .draft_editor_candidate_session(
            store,
            origin.candidate().draft_id(),
            origin.candidate().session_id(),
        )
        .unwrap();
    let syndic_storage::DraftEditorCandidateSessionReadOutcomeV1::Active(session) = session else {
        panic!("origin session was not active: {session:?}");
    };
    assert_eq!(session.newest_root(), origin.root());
    assert_eq!(session.published_root(), session.newest_root());
    assert_eq!(session.published_history(), session.newest_history());
    assert!(session.active_operation().is_none());
    let request = syndic_storage::DraftEditorCandidateSessionDisposeRequestV1::new(
        session.draft_id(),
        session.session_id(),
        syndic_storage::DraftPieceOperationIdV1::from_bytes([188; 16]),
        session.session_generation(),
        syndic_storage::DraftRootHistoryPairV1::new(
            session.published_root(),
            session.published_history(),
        ),
    );
    let prepared = storage
        .prepare_dispose_draft_editor_candidate_session(store, request)
        .unwrap();
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(storage.dispose_draft_editor_candidate_session(
            storage.revision(store).unwrap(),
            prepared.clone(),
        ))
        .unwrap();
    let outcome = store.execute(command);
    assert!(
        matches!(
            &outcome,
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ),
        "origin disposal did not commit: {outcome:?}"
    );
    let disposal = storage
        .reconcile_draft_editor_candidate_session_disposal(store, &prepared, outcome)
        .unwrap();
    assert!(
        matches!(
            disposal,
            syndic_storage::DraftEditorCandidateSessionDisposeOutcomeV1::Disposed(_)
                | syndic_storage::DraftEditorCandidateSessionDisposeOutcomeV1::ExactReplay(_)
        ),
        "origin disposal did not reconcile: {disposal:?}"
    );
}

#[test]
fn private_origin_retirement_before_submission_refuses_without_begin() {
    let fixture = fixture("private-origin-retirement", 170);
    let (origin, asset) = marked_origin(&fixture);
    let (mut host, binding) = foreign_destination(&fixture);
    let (_, evidence, _, finish, _, activation) =
        prepare_private(&fixture, &mut host, binding, source(origin), asset);
    activation.live.store(false, Ordering::SeqCst);
    assert!(
        matches!(drive_evidence(&mut host, &fixture.store, binding, &fixture.assets,
        ComposerHostMutationEvidenceRequest::Finish { pass: evidence, finish }),
        ComposerHostMutationEvidenceOutcome::Refused { key, failure }
            if key == finish.key() && matches!(failure.as_ref(), ComposerHostMutationAdmissionFailure::Conflict))
    );
    assert_eq!(activation.submissions.load(Ordering::SeqCst), 0);
    assert_eq!(host.binding(), Some(binding));
    assert_eq!(host.settlement_custody_in_use(), 0);
}

#[test]
fn private_origin_disposal_while_begin_is_queued_refuses_atomically() {
    let fixture = fixture("private-origin-queued", 170);
    let (origin, asset) = marked_origin(&fixture);
    let (mut host, binding) = foreign_destination(&fixture);
    let (_, evidence, _, finish, _, activation) =
        prepare_private(&fixture, &mut host, binding, source(origin), asset);
    host.test_arm_mutation_before_execute_fault(move |store, storage| {
        dispose_origin(store, storage, origin)
    });
    assert!(
        matches!(drive_evidence(&mut host, &fixture.store, binding, &fixture.assets,
        ComposerHostMutationEvidenceRequest::Finish { pass: evidence, finish }),
        ComposerHostMutationEvidenceOutcome::Refused { key, failure }
            if key == finish.key() && matches!(failure.as_ref(), ComposerHostMutationAdmissionFailure::Conflict))
    );
    assert_eq!(activation.submissions.load(Ordering::SeqCst), 1);
    assert_eq!(host.binding(), Some(binding));
    assert_eq!(host.settlement_custody_in_use(), 0);
    assert!(
        !fixture
            .storage
            .draft_private_clipboard_source_is_current(&fixture.store, source(origin))
            .unwrap()
    );
}

#[test]
fn private_origin_expiry_after_begin_preserves_immutable_staging_source() {
    let fixture = fixture("private-origin-admitted", 170);
    let (origin, asset) = marked_origin(&fixture);
    let (mut host, binding) = foreign_destination(&fixture);
    let (mut editor, evidence, proposal, finish, metadata, activation) =
        prepare_private(&fixture, &mut host, binding, source(origin), asset);
    assert!(
        matches!(drive_evidence(&mut host, &fixture.store, binding, &fixture.assets,
        ComposerHostMutationEvidenceRequest::Finish { pass: evidence, finish }),
        ComposerHostMutationEvidenceOutcome::Began(key) if key == finish.key())
    );
    activation.live.store(false, Ordering::SeqCst);
    dispose_origin(&fixture.store, fixture.storage.clone(), origin);
    editor.accept_preflight(finish.key(), None).unwrap();
    let staging = editor.mutation_restart(finish.key()).unwrap();
    editor.acknowledge_restart(staging).unwrap();
    stage(
        &mut host,
        &fixture.store,
        &mut editor,
        staging,
        proposal,
        metadata,
    );
    editor.finish_pass_input(staging, finish).unwrap();
    host.finish_mutation_input(&fixture.store, finish).unwrap();
    let committed = settle(&mut host, &fixture.store, finish.key());
    assert_eq!(
        occurrence(
            fixture.storage.clone(),
            &fixture.store,
            committed,
            InlineObjectId::new(1702)
        )
        .asset_id(),
        asset
    );
    assert_eq!(committed.root().summary().marker_count(), 1);
    assert_eq!(activation.submissions.load(Ordering::SeqCst), 1);
    assert_eq!(host.settlement_custody_in_use(), 0);
}
