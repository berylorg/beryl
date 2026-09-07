use std::num::NonZeroU64;

use beryl_app::{
    composer_host::{
        ComposerHostBinding, ComposerHostError, ComposerHostFlushAdvance, ComposerHostFlushPurpose,
        ComposerHostReadTarget, ComposerHostRequestId, ComposerHostRequestKey,
        ComposerHostRequestKind, ComposerHostRequestPurpose, SyndicComposerHost,
    },
    main_window::{MainWindowComposerDispatchError, MainWindowComposerDispatchOutcome},
};
use gpui_text_input::{
    ByteOffset, MutationKind, PageDirection, PagePurpose, PageRequest, PageRequestId,
    PageRequestKey, RangeTextInputRequest,
};
use syndic_storage::DraftPieceTextDemandV1;

use super::*;

#[test]
fn host_request_sequence_survives_edit_history_and_publication() {
    let fixture = Fixture::new("request-sequence", 211);
    let seals = fixture.marker_seals();
    let mut host = fixture.activated_host(fixture.selected_thread, 212, 213, 1);
    let empty = host.binding().unwrap();
    request(&mut host, &fixture.store, empty, 100).unwrap();

    let edited = composer::commit_text(&mut host, &fixture.store, empty, 1, 0, 0, "a", 1, 1);
    assert_sequence(&mut host, &fixture.store, empty, edited, 100);
    let undone = composer::select_history(&mut host, &fixture.store, edited, 2, MutationKind::Undo);
    assert_sequence(&mut host, &fixture.store, edited, undone, 101);
    let redone = composer::select_history(&mut host, &fixture.store, undone, 3, MutationKind::Redo);
    assert_sequence(&mut host, &fixture.store, undone, redone, 102);

    let ComposerHostFlushAdmission::Started { ticket, .. } = host
        .begin_flush(ComposerHostFlushPurpose::Submission)
        .unwrap()
    else {
        panic!("dirty candidate did not need publication")
    };
    assert!(matches!(
        host.capture_flush_publication(
            &fixture.store,
            ticket,
            fixture.assets(),
            &seals,
            composer::operation_id(4),
            None,
            SyndicTimestamp::from_unix_millis(20),
            &CommandCancellation::new(),
        )
        .unwrap(),
        ComposerHostFlushCapture::Captured(_)
    ));
    assert_eq!(
        host.advance_flush(&fixture.store, ticket).unwrap(),
        ComposerHostFlushAdvance::Progress(ComposerHostFlushState::CaptureRequired)
    );
    assert_eq!(
        host.capture_flush_publication(
            &fixture.store,
            ticket,
            fixture.assets(),
            &seals,
            composer::operation_id(5),
            None,
            SyndicTimestamp::from_unix_millis(21),
            &CommandCancellation::new(),
        )
        .unwrap(),
        ComposerHostFlushCapture::Satisfied(ComposerHostFlushPurpose::Submission)
    );
    let published = host.binding().unwrap();
    assert_sequence(&mut host, &fixture.store, redone, published, 103);
    assert_eq!(
        composer::candidate_text(fixture.storage.clone(), &fixture.store, published),
        b"a"
    );
    assert_eq!(host.pending_request_count(), 0);
    assert_eq!(seals.diagnostics().current_flights(), 0);
}

#[test]
fn exhausted_requests_restart_only_after_disposal_and_a_fresh_host_generation() {
    let fixture = Fixture::new("request-generation", 221);
    let seals = fixture.marker_seals();
    let mut host = fixture.activated_host(fixture.selected_thread, 222, 223, 1);
    let old = host.binding().unwrap();
    request(&mut host, &fixture.store, old, u64::MAX).unwrap();
    assert!(matches!(
        request(&mut host, &fixture.store, old, 1),
        Err(ComposerHostError::StaleRequestIdentity)
    ));
    let ComposerHostFlushAdmission::Started { ticket, .. } =
        host.begin_flush(ComposerHostFlushPurpose::Release).unwrap()
    else {
        panic!("active host did not need release")
    };
    assert_eq!(
        host.capture_flush_publication(
            &fixture.store,
            ticket,
            fixture.assets(),
            &seals,
            composer::operation_id(1),
            None,
            SyndicTimestamp::from_unix_millis(20),
            &CommandCancellation::new(),
        )
        .unwrap(),
        ComposerHostFlushCapture::State(ComposerHostFlushState::DisposalRequired)
    );
    assert_eq!(
        host.capture_flush_disposal(
            &fixture.store,
            ticket,
            composer::operation_id(2),
            &CommandCancellation::new(),
        )
        .unwrap(),
        ComposerHostFlushCapture::State(ComposerHostFlushState::DisposalRequired)
    );
    assert_eq!(
        host.advance_flush(&fixture.store, ticket).unwrap(),
        ComposerHostFlushAdvance::Satisfied(ComposerHostFlushPurpose::Release)
    );
    assert!(host.binding().is_none());
    assert!(matches!(
        request(&mut host, &fixture.store, old, 1),
        Err(ComposerHostError::OldBinding)
    ));
    let fresh = composer::reactivate(&mut host, &fixture.store, fixture.selected_thread, 224, 225);
    assert_eq!(
        fresh.host_generation().get(),
        old.host_generation().get() + 1
    );
    assert_ne!(fresh.candidate().session_id(), old.candidate().session_id());
    assert!(matches!(
        request(&mut host, &fixture.store, old, 1),
        Err(ComposerHostError::OldBinding)
    ));
    request(&mut host, &fixture.store, fresh, 1).unwrap();
    assert!(matches!(
        request(&mut host, &fixture.store, fresh, 1),
        Err(ComposerHostError::StaleRequestIdentity)
    ));
    assert_eq!(host.pending_request_count(), 0);
}

#[test]
fn dispatcher_continues_an_already_used_host_sequence_and_never_wraps() {
    let fixture = Fixture::new("request-exhaustion", 181);
    let (claim, _) = fixture.claims();
    let mut host = fixture.activated_host(fixture.selected_thread, 182, 183, 1);
    let binding = host.binding().unwrap();
    let key = ComposerHostRequestKey::new(
        binding,
        ComposerHostRequestId::new(NonZeroU64::new(u64::MAX - 1).unwrap()),
        ComposerHostRequestPurpose::Viewport,
    );
    let pending = host.begin_request(key, text_request()).unwrap();
    let execution = host.execute_pending(&fixture.store, pending);
    host.complete_request(execution).unwrap();
    let mut slot = MainWindowComposerSlot::new(
        fixture.window_id,
        claim,
        host,
        fixture.storage.clone(),
        MainWindowComposerMarkerMetadataAuthority::new(fixture.assets()),
    )
    .unwrap();
    let selection = slot.selected_identity().unwrap();
    let range = binding.range_binding();
    let page = PageRequest::new(
        PageRequestKey::adjacent(
            PageRequestId::new(1),
            range.binding(),
            range.revision(),
            PagePurpose::Viewport,
            ByteOffset::new(0),
            PageDirection::Forward,
            64,
        )
        .unwrap(),
    );
    assert!(matches!(
        slot.dispatch_selected_request(
            &fixture.store,
            selection,
            RangeTextInputRequest::Page(page),
            Box::new([]),
            &CommandCancellation::new(),
        ),
        Ok(MainWindowComposerDispatchOutcome::Page(_))
    ));
    for _ in 0..2 {
        assert!(matches!(
            slot.dispatch_selected_request(
                &fixture.store,
                selection,
                RangeTextInputRequest::Page(page),
                Box::new([]),
                &CommandCancellation::new(),
            ),
            Err(MainWindowComposerDispatchError::Malformed)
        ));
    }
    for id in [1, u64::MAX - 1, u64::MAX] {
        assert!(matches!(
            slot.test_selected_host_mut().unwrap().begin_request(
                ComposerHostRequestKey::new(
                    binding,
                    ComposerHostRequestId::new(NonZeroU64::new(id).unwrap()),
                    ComposerHostRequestPurpose::Viewport,
                ),
                text_request(),
            ),
            Err(ComposerHostError::StaleRequestIdentity)
        ));
    }
    assert_eq!(slot.selected_identity(), Some(selection));
    assert_eq!(
        slot.test_selected_host_mut()
            .unwrap()
            .pending_request_count(),
        0
    );
}

fn text_request() -> ComposerHostRequestKind {
    ComposerHostRequestKind::Text {
        target: ComposerHostReadTarget::Candidate,
        demand: DraftPieceTextDemandV1::Validate(0),
        max_bytes: 4,
    }
}

fn assert_sequence(
    host: &mut SyndicComposerHost,
    store: &beryl_home_store::HomeStore,
    prior: ComposerHostBinding,
    current: ComposerHostBinding,
    previous_id: u64,
) {
    assert_ne!(prior, current);
    assert_eq!(prior.host_generation(), current.host_generation());
    assert_eq!(
        prior.candidate().session_id(),
        current.candidate().session_id()
    );
    for id in [1, previous_id - 1, previous_id] {
        assert!(matches!(
            request(host, store, current, id),
            Err(ComposerHostError::StaleRequestIdentity)
        ));
    }
    assert!(matches!(
        request(host, store, prior, previous_id + 1),
        Err(ComposerHostError::OldBinding)
    ));
    request(host, store, current, previous_id + 1).unwrap();
}

fn request(
    host: &mut SyndicComposerHost,
    store: &beryl_home_store::HomeStore,
    binding: ComposerHostBinding,
    id: u64,
) -> Result<(), ComposerHostError> {
    let pending = host.begin_request(
        ComposerHostRequestKey::new(
            binding,
            ComposerHostRequestId::new(NonZeroU64::new(id).unwrap()),
            ComposerHostRequestPurpose::Viewport,
        ),
        text_request(),
    )?;
    let execution = host.execute_pending(store, pending);
    host.complete_request(execution).map(|_| ())
}
