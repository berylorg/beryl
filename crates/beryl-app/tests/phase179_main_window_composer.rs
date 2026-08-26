#![cfg(feature = "test-faults")]

use std::num::NonZeroU64;

#[path = "phase166_syndic_composer_history/support.rs"]
mod composer;
#[path = "phase141_syndic_composer_host/support.rs"]
mod composer_base;
#[path = "phase177_main_window_composer_slot/support.rs"]
mod support;

use beryl_app::{
    composer_host::{
        ComposerHostActivationOutcome, ComposerHostActivationRequest, ComposerHostAutosaveAdvance,
        ComposerHostAutosaveCapture, ComposerHostBinding, ComposerHostError,
        ComposerHostFlushAdmission, ComposerHostFlushAdvance, ComposerHostFlushCapture,
        ComposerHostFlushPurpose, ComposerHostFlushState, ComposerHostImageMarkerMetadata,
        ComposerHostInitialDemand, ComposerHostMutationOutcome, ComposerHostRequestId,
        ComposerHostRequestPurpose, SyndicComposerHost,
    },
    main_window::{
        ComposerImagePresentationState, ComposerImagePreviewCommandState,
        ComposerMarkerActivationDisposition, ComposerMarkerCommand, ComposerMarkerFocusTarget,
        ComposerPreviewCommand, MainWindowComposerActivationAdvance,
        MainWindowComposerDispatchError, MainWindowComposerDispatchOutcome,
        MainWindowComposerDisposalAdvance, MainWindowComposerDraftState,
        MainWindowComposerImageSurfaces, MainWindowComposerMarkerMetadataAuthority,
        MainWindowComposerPublishAdvance, MainWindowComposerRetirementAdvance,
        MainWindowComposerSlot,
    },
};
use beryl_home_store::{
    CommandCancellation, CommandOutcome, HomeCommand, SidecarByteLimit, SidecarNamespace,
};
use beryl_model::{AssetId, ImageLabelOrdinal};
use beryl_state::{AssetMediaType, PublishAssetMetadata};
use gpui::{Bounds, Point, Size, px};
use gpui_text_input::{
    ByteOffset, InlineObjectActivation, InlineObjectGap, InlineObjectId, InlineObjectInputOrigin,
    InlineObjectNeighbor, InlineObjectOrder, LayoutEpoch, LogicalExtent, MutationBeginRequest,
    MutationCommitRequest, MutationCursor, MutationFinishInput, MutationIdentity, MutationKey,
    MutationKind, MutationLane, MutationPage, MutationPageItem, MutationPageKey,
    MutationPageRequest, MutationPositions, MutationProposal, MutationStreamFinish, MutationTotals,
    ObjectChange, ObjectDemandEnvelope, ObjectDirection, ObjectPurpose, ObjectRequest,
    ObjectRequestId, ObjectRequestKey, ObjectTarget, OperationId, PageDirection, PagePurpose,
    PageRequest, PageRequestId, PageRequestKey, PresentationGeneration, RangeHistoryOutcome,
    RangeTextInputRequest, RealizedInlineObjectAnchor, SourcePosition, SourceRange,
    SuccessorObject,
};
use syndic_storage::{
    DraftEditorCandidateSessionIdV1, DraftHistoricalRootAdoptionErrorReasonV1,
    DraftHistoricalRootDirectionV1, DraftHistoricalRootSelectionIntentV1,
    DraftHistoricalRootSelectionV1, DraftPieceMarkerDemandV1, DraftPieceMarkerDirectionV1,
    DraftPieceMarkerScopeV1, DraftPieceTextDemandV1,
};

use support::Fixture;

#[test]
fn initial_and_reselection_mounts_retain_the_prior_coherent_selection() {
    let fixture = Fixture::new("phase179-selection", 9);
    let (selected_claim, target_claim) = fixture.claims();
    let mut host = SyndicComposerHost::new(fixture.storage);
    let activation = ComposerHostActivationRequest::new(
        fixture.selected_thread,
        DraftEditorCandidateSessionIdV1::from_bytes([10; 16]),
        support::operation_id(11),
        NonZeroU64::new(3).unwrap(),
        None,
        vec![
            ComposerHostInitialDemand::Text {
                request_id: ComposerHostRequestId::new(NonZeroU64::new(1).unwrap()),
                purpose: ComposerHostRequestPurpose::Geometry,
                demand: DraftPieceTextDemandV1::Forward(0),
                max_bytes: 32,
            },
            ComposerHostInitialDemand::Markers {
                request_id: ComposerHostRequestId::new(NonZeroU64::new(2).unwrap()),
                purpose: ComposerHostRequestPurpose::Geometry,
                demand: DraftPieceMarkerDemandV1::new(
                    DraftPieceMarkerScopeV1::Range { start: 0, end: 0 },
                    DraftPieceMarkerDirectionV1::Forward,
                    None,
                    32,
                    65_536,
                ),
            },
        ]
        .into_boxed_slice(),
    );
    assert!(matches!(
        host.test_activate(&fixture.store, activation, &CommandCancellation::new())
            .unwrap(),
        ComposerHostActivationOutcome::Activated { .. }
    ));
    let mut slot = MainWindowComposerSlot::new(
        fixture.window_id,
        selected_claim,
        host,
        fixture.storage,
        MainWindowComposerMarkerMetadataAuthority::new(fixture.assets()),
    )
    .unwrap();
    let prior = slot.selected_identity().unwrap();
    let initial = slot.take_selected_initial_presentation(prior).unwrap();
    assert_eq!(initial.selection(), prior);
    assert!(!initial.responses().is_empty());

    let MainWindowComposerActivationAdvance::Ready(receipt) = slot
        .begin_activation(
            &fixture.store,
            target_claim,
            support::activation(fixture.target_thread, 12, 13, 4),
            support::operation_id(14),
            &CommandCancellation::new(),
        )
        .unwrap()
    else {
        panic!("target activation did not become coherent")
    };
    assert_eq!(slot.selected_identity(), Some(prior));
    assert_eq!(
        slot.retire_pending(&fixture.store, receipt).unwrap(),
        MainWindowComposerRetirementAdvance::Retired
    );
    assert_eq!(slot.selected_identity(), Some(prior));
}

#[test]
fn selected_dispatch_is_bounded_and_rejects_stale_binding() {
    let fixture = Fixture::new("phase179-dispatch", 17);
    let (claim, _) = fixture.claims();
    let host = fixture.activated_host(fixture.selected_thread, 18, 19, 3);
    let mut slot = MainWindowComposerSlot::new(
        fixture.window_id,
        claim,
        host,
        fixture.storage,
        MainWindowComposerMarkerMetadataAuthority::new(fixture.assets()),
    )
    .unwrap();
    let selection = slot.selected_identity().unwrap();
    let binding = selection.binding().range_binding();
    let key = PageRequestKey::adjacent(
        PageRequestId::new(1),
        binding.binding(),
        binding.revision(),
        PagePurpose::Viewport,
        ByteOffset::new(0),
        PageDirection::Forward,
        64,
    )
    .unwrap();
    let outcome = slot
        .dispatch_selected_request(
            &fixture.store,
            selection,
            RangeTextInputRequest::Page(PageRequest::new(key)),
            Box::new([]),
            &CommandCancellation::new(),
        )
        .unwrap();
    let MainWindowComposerDispatchOutcome::Page(page) = outcome else {
        panic!("unexpected dispatcher outcome")
    };
    assert!(page.text().is_empty());
    assert!(page.retained_bytes() <= 64);

    let current = slot.test_selected_host_mut().unwrap().binding().unwrap();
    let _ = composer::commit_text(
        slot.test_selected_host_mut().unwrap(),
        &fixture.store,
        current,
        20,
        0,
        0,
        "x",
        1,
        1,
    );
    assert!(matches!(
        slot.dispatch_selected_request(
            &fixture.store,
            selection,
            RangeTextInputRequest::Page(PageRequest::new(key)),
            Box::new([]),
            &CommandCancellation::new(),
        ),
        Err(MainWindowComposerDispatchError::StaleSelection)
    ));
}

#[test]
fn object_dispatch_preserves_canonical_marker_identity_order_and_fallback() {
    let fixture = Fixture::new("phase179-object", 23);
    let (claim, _) = fixture.claims();
    let mut host = fixture.activated_host(fixture.selected_thread, 24, 25, 5);
    let binding = host.binding().unwrap();
    let (binding, _, _) = composer::insert_marker(&mut host, &fixture.store, binding, 26, false);
    let mut slot = MainWindowComposerSlot::new(
        fixture.window_id,
        claim,
        host,
        fixture.storage,
        MainWindowComposerMarkerMetadataAuthority::new(fixture.assets()),
    )
    .unwrap();
    let selection = slot.selected_identity().unwrap();
    assert_eq!(selection.binding(), binding);
    let range_binding = binding.range_binding();
    let demand =
        ObjectDemandEnvelope::anchor(ByteOffset::new(0), None, ObjectDirection::Forward, 4, 4096)
            .unwrap();
    let key = ObjectRequestKey::new(
        ObjectRequestId::new(1),
        range_binding.binding(),
        range_binding.revision(),
        PresentationGeneration::new(5),
        ObjectPurpose::Viewport,
        demand,
    )
    .unwrap();
    let outcome = slot
        .dispatch_selected_request(
            &fixture.store,
            selection,
            RangeTextInputRequest::ObjectPage(ObjectRequest::new(key)),
            Box::new([]),
            &CommandCancellation::new(),
        )
        .unwrap();
    let MainWindowComposerDispatchOutcome::ObjectPage(page) = outcome else {
        panic!("unexpected dispatcher outcome")
    };
    assert_eq!(page.objects().len(), 1);
    let marker = &page.objects()[0];
    assert_eq!(marker.order(), InlineObjectOrder::new(1));
    assert_eq!(marker.fallback_copy(), "[Image A]");
    assert_eq!(marker.presentation().display().as_ref(), "[A]");
    assert!(marker.presentation().activation_eligible());
}

#[test]
fn authenticated_marker_insert_and_remove_keep_adopted_and_published_state_distinct() {
    let fixture = Fixture::new("phase179-marker-edit", 27);
    let (claim, _) = fixture.claims();
    let host = fixture.activated_host(fixture.selected_thread, 28, 29, 5);
    let mut slot = MainWindowComposerSlot::new(
        fixture.window_id,
        claim,
        host,
        fixture.storage,
        MainWindowComposerMarkerMetadataAuthority::new(fixture.assets()),
    )
    .unwrap();
    let original = slot.selected_identity().unwrap();
    let marker_id = InlineObjectId::new(0x8101);
    let order = InlineObjectOrder::new(1);
    let neighbor = InlineObjectNeighbor::new(marker_id, order);
    let before = SourcePosition::new(ByteOffset::new(0), InlineObjectGap::before(neighbor));
    let after = SourcePosition::new(ByteOffset::new(0), InlineObjectGap::after(neighbor));
    let admitted_asset = publish_image_asset(&fixture, b"phase179-admitted-marker");
    let metadata = ComposerHostImageMarkerMetadata::new(
        marker_id,
        ImageLabelOrdinal::new(1).unwrap(),
        admitted_asset,
    );
    let inserted = dispatch_edit(
        &mut slot,
        &fixture.store,
        original,
        30,
        SourceRange::new(composer::position(0), composer::position(0)).unwrap(),
        composer::position(0),
        vec![MutationPageItem::Object(ObjectChange::Insert {
            object: SuccessorObject::new(marker_id, ByteOffset::new(0), order, 17, 5),
        })],
        MutationPositions::collapsed(before),
        LogicalExtent::new(0, 1),
        vec![metadata].into_boxed_slice(),
        &CommandCancellation::new(),
    );
    let inserted = committed_binding(inserted);
    let state = slot
        .selected_draft_state(slot.selected_identity().unwrap())
        .unwrap();
    assert_eq!(state.adopted(), inserted);
    assert_published_binding(state, original.binding());
    assert!(state.is_dirty());

    let target =
        ObjectTarget::new(SourceRange::new(before, after).unwrap(), marker_id, order).unwrap();
    let selected = slot.selected_identity().unwrap();
    let removed = dispatch_edit(
        &mut slot,
        &fixture.store,
        selected,
        31,
        target.range(),
        before,
        vec![MutationPageItem::Object(ObjectChange::Remove { target })],
        MutationPositions::collapsed(composer::position(0)),
        LogicalExtent::new(0, 1),
        Box::new([]),
        &CommandCancellation::new(),
    );
    let removed = committed_binding(removed);
    let state = slot
        .selected_draft_state(slot.selected_identity().unwrap())
        .unwrap();
    assert_eq!(state.adopted(), removed);
    assert_published_binding(state, original.binding());
}

#[test]
fn unadmitted_marker_metadata_is_rejected_before_proposal_staging() {
    let fixture = Fixture::new("phase179-unadmitted-marker", 32);
    let (claim, _) = fixture.claims();
    let host = fixture.activated_host(fixture.selected_thread, 33, 34, 5);
    let mut slot = MainWindowComposerSlot::new(
        fixture.window_id,
        claim,
        host,
        fixture.storage,
        MainWindowComposerMarkerMetadataAuthority::new(fixture.assets()),
    )
    .unwrap();
    let selection = slot.selected_identity().unwrap();
    let range = selection.binding().range_binding();
    let key = MutationKey::new(range.binding(), range.revision(), OperationId::new(35));
    let position = composer::position(0);
    let begin = MutationBeginRequest::new(
        MutationProposal::new(
            key,
            MutationKind::Edit,
            MutationPositions::collapsed(position),
            SourceRange::new(position, position).unwrap(),
            0,
        ),
        MutationCursor::new(0),
        MutationCursor::new(0),
    );
    assert!(matches!(
        slot.dispatch_selected_request(
            &fixture.store,
            selection,
            RangeTextInputRequest::MutationBegin(begin),
            Box::new([]),
            &CommandCancellation::new(),
        )
        .unwrap(),
        MainWindowComposerDispatchOutcome::MutationBegan(actual) if actual == key
    ));

    let marker_id = InlineObjectId::new(0x8201);
    let page = MutationPage::new(
        MutationPageKey::new(
            key,
            MutationLane::Proposal,
            MutationCursor::new(0),
            0,
            MutationIdentity::ROOT,
        ),
        MutationCursor::new(1),
        vec![MutationPageItem::Object(ObjectChange::Insert {
            object: SuccessorObject::new(
                marker_id,
                ByteOffset::new(0),
                InlineObjectOrder::new(1),
                17,
                5,
            ),
        })],
    )
    .unwrap();
    let metadata = ComposerHostImageMarkerMetadata::new(
        marker_id,
        ImageLabelOrdinal::new(1).unwrap(),
        asset_id(marker_id),
    );
    assert!(matches!(
        slot.dispatch_selected_request(
            &fixture.store,
            selection,
            RangeTextInputRequest::MutationProposalPage(MutationPageRequest::new(page)),
            Box::new([metadata]),
            &CommandCancellation::new(),
        ),
        Err(MainWindowComposerDispatchError::MarkerMetadata(message))
            if message == "composer marker asset is not admitted"
    ));
    assert_eq!(slot.selected_identity(), Some(selection));
    let state = slot.selected_draft_state(selection).unwrap();
    assert_eq!(state.adopted(), selection.binding());
    assert_published_binding(state, selection.binding());
    assert_eq!(state.is_dirty(), slot.selected_host().unwrap().is_dirty());
}

#[test]
fn production_edit_dispatch_preserves_all_five_terminal_outcomes() {
    edit_terminal_outcomes_case();
}

#[test]
fn early_terminal_multi_item_page_advances_one_synthetic_ordinal() {
    let fixture = Fixture::new("phase181-early-terminal-page-ordinal", 191);
    let (claim, _) = fixture.claims();
    let host = fixture.activated_host(fixture.selected_thread, 192, 193, 18);
    let mut slot = MainWindowComposerSlot::new(
        fixture.window_id,
        claim,
        host,
        fixture.storage,
        MainWindowComposerMarkerMetadataAuthority::new(fixture.assets()),
    )
    .unwrap();
    let initial = slot.selected_identity().unwrap();
    let committed = dispatch_text(
        &mut slot,
        &fixture.store,
        initial,
        194,
        0,
        "a",
        &CommandCancellation::new(),
    );
    let adopted = committed_binding(committed);
    let selection = slot.selected_identity().unwrap();
    assert_eq!(selection.binding(), adopted);
    composer::direct_adopt(
        &fixture.store,
        fixture.storage,
        DraftHistoricalRootSelectionIntentV1::new(
            adopted.candidate(),
            support::operation_id(195),
            DraftHistoricalRootDirectionV1::Undo,
        ),
    );

    let outcome = dispatch_edit(
        &mut slot,
        &fixture.store,
        selection,
        196,
        SourceRange::new(composer::position(1), composer::position(1)).unwrap(),
        composer::position(1),
        vec![
            MutationPageItem::Utf8 {
                inserted_offset: 0,
                text: "b".into(),
            },
            MutationPageItem::Utf8 {
                inserted_offset: 1,
                text: "c".into(),
            },
        ],
        MutationPositions::collapsed(composer::position(3)),
        LogicalExtent::new(3, 1),
        Box::new([]),
        &CommandCancellation::new(),
    );

    assert_eq!(outcome, ComposerHostMutationOutcome::Conflict);
    assert_eq!(slot.selected_identity(), Some(selection));
}

fn edit_terminal_outcomes_case() {
    let fixture = Fixture::new("phase180-edit-cancel", 51);
    let (claim, _) = fixture.claims();
    let host = fixture.activated_host(fixture.selected_thread, 52, 53, 8);
    let mut slot = MainWindowComposerSlot::new(
        fixture.window_id,
        claim,
        host,
        fixture.storage,
        MainWindowComposerMarkerMetadataAuthority::new(fixture.assets()),
    )
    .unwrap();
    let selection = slot.selected_identity().unwrap();
    let cancelled = CommandCancellation::new();
    cancelled.cancel();
    assert_eq!(
        dispatch_text(
            &mut slot,
            &fixture.store,
            selection,
            54,
            0,
            "cancelled",
            &cancelled,
        ),
        ComposerHostMutationOutcome::Cancelled
    );
    assert_eq!(slot.selected_identity(), Some(selection));

    let fixture = Fixture::new("phase180-edit-reject", 55);
    let (claim, _) = fixture.claims();
    let host = fixture.activated_host(fixture.selected_thread, 56, 57, 9);
    let mut slot = MainWindowComposerSlot::new(
        fixture.window_id,
        claim,
        host,
        fixture.storage,
        MainWindowComposerMarkerMetadataAuthority::new(fixture.assets()),
    )
    .unwrap();
    let selection = slot.selected_identity().unwrap();
    let rejected = dispatch_edit(
        &mut slot,
        &fixture.store,
        selection,
        58,
        SourceRange::new(composer::position(0), composer::position(0)).unwrap(),
        composer::position(0),
        vec![
            MutationPageItem::Utf8 {
                inserted_offset: 0,
                text: "".into(),
            },
            MutationPageItem::Utf8 {
                inserted_offset: 0,
                text: "".into(),
            },
        ],
        MutationPositions::collapsed(composer::position(0)),
        LogicalExtent::new(0, 1),
        Box::new([]),
        &CommandCancellation::new(),
    );
    assert_eq!(rejected, ComposerHostMutationOutcome::Rejected);
    assert_eq!(slot.selected_identity(), Some(selection));

    let fixture = Fixture::new("phase180-edit-conflict", 59);
    let (claim, _) = fixture.claims();
    let host = fixture.activated_host(fixture.selected_thread, 60, 61, 10);
    let mut slot = MainWindowComposerSlot::new(
        fixture.window_id,
        claim,
        host,
        fixture.storage,
        MainWindowComposerMarkerMetadataAuthority::new(fixture.assets()),
    )
    .unwrap();
    let selection = slot.selected_identity().unwrap();
    let committed = dispatch_text(
        &mut slot,
        &fixture.store,
        selection,
        62,
        0,
        "a",
        &CommandCancellation::new(),
    );
    let adopted = committed_binding(committed);
    let selection = slot.selected_identity().unwrap();
    assert_eq!(selection.binding(), adopted);
    composer::direct_adopt(
        &fixture.store,
        fixture.storage,
        DraftHistoricalRootSelectionIntentV1::new(
            adopted.candidate(),
            support::operation_id(64),
            DraftHistoricalRootDirectionV1::Undo,
        ),
    );

    let (key, finish) = stage_edit(
        &mut slot,
        &fixture.store,
        selection,
        63,
        SourceRange::new(composer::position(1), composer::position(1)).unwrap(),
        composer::position(1),
        vec![MutationPageItem::Utf8 {
            inserted_offset: 0,
            text: "b".into(),
        }],
        MutationPositions::collapsed(composer::position(2)),
        LogicalExtent::new(2, 1),
        Box::new([]),
    );
    assert_eq!(
        settle_edit(
            &mut slot,
            &fixture.store,
            selection,
            key,
            finish,
            &CommandCancellation::new(),
        ),
        ComposerHostMutationOutcome::Conflict
    );
    assert_eq!(slot.selected_identity(), Some(selection));

    let fixture = Fixture::with_history_budget("phase180-edit-error", 65, 1_390);
    let (claim, _) = fixture.claims();
    let host = fixture.activated_host(fixture.selected_thread, 66, 67, 11);
    let mut slot = MainWindowComposerSlot::new(
        fixture.window_id,
        claim,
        host,
        fixture.storage,
        MainWindowComposerMarkerMetadataAuthority::new(fixture.assets()),
    )
    .unwrap();
    let selection = slot.selected_identity().unwrap();
    assert_eq!(
        dispatch_text(
            &mut slot,
            &fixture.store,
            selection,
            68,
            0,
            "x",
            &CommandCancellation::new(),
        ),
        ComposerHostMutationOutcome::Error
    );
    assert_eq!(slot.selected_identity(), Some(selection));
}

#[test]
fn production_history_dispatch_preserves_all_five_terminal_outcomes() {
    history_terminal_outcomes_case();
}

fn history_terminal_outcomes_case() {
    let fixture = Fixture::new("phase180-history-basic", 71);
    let (claim, _) = fixture.claims();
    let host = fixture.activated_host(fixture.selected_thread, 72, 73, 12);
    let mut slot = MainWindowComposerSlot::new(
        fixture.window_id,
        claim,
        host,
        fixture.storage,
        MainWindowComposerMarkerMetadataAuthority::new(fixture.assets()),
    )
    .unwrap();
    let selection = slot.selected_identity().unwrap();
    let rejected = composer::history_intent(
        selection.binding(),
        74,
        MutationKind::Undo,
        composer::position(0),
    );
    assert_eq!(
        dispatch_history(
            &mut slot,
            &fixture.store,
            selection,
            rejected,
            &CommandCancellation::new(),
        ),
        RangeHistoryOutcome::Rejected
    );
    assert_eq!(slot.selected_identity(), Some(selection));

    let committed = committed_binding(dispatch_text(
        &mut slot,
        &fixture.store,
        selection,
        75,
        0,
        "a",
        &CommandCancellation::new(),
    ));
    let selection = slot.selected_identity().unwrap();
    let cancelled_intent =
        composer::history_intent(committed, 76, MutationKind::Undo, composer::position(1));
    let cancelled = CommandCancellation::new();
    cancelled.cancel();
    assert_eq!(
        dispatch_history(
            &mut slot,
            &fixture.store,
            selection,
            cancelled_intent,
            &cancelled,
        ),
        RangeHistoryOutcome::Cancelled
    );
    assert_eq!(slot.selected_identity(), Some(selection));

    let committed_intent =
        composer::history_intent(committed, 77, MutationKind::Undo, composer::position(1));
    assert!(matches!(
        dispatch_history(
            &mut slot,
            &fixture.store,
            selection,
            committed_intent,
            &CommandCancellation::new(),
        ),
        RangeHistoryOutcome::Committed(_)
    ));
    assert_ne!(slot.selected_identity(), Some(selection));

    let fixture = Fixture::new("phase180-history-conflict", 81);
    let (claim, _) = fixture.claims();
    let host = fixture.activated_host(fixture.selected_thread, 82, 83, 13);
    let mut slot = MainWindowComposerSlot::new(
        fixture.window_id,
        claim,
        host,
        fixture.storage,
        MainWindowComposerMarkerMetadataAuthority::new(fixture.assets()),
    )
    .unwrap();
    let selection = slot.selected_identity().unwrap();
    let a = committed_binding(dispatch_text(
        &mut slot,
        &fixture.store,
        selection,
        84,
        0,
        "a",
        &CommandCancellation::new(),
    ));
    let selection = slot.selected_identity().unwrap();
    let ab = committed_binding(dispatch_text(
        &mut slot,
        &fixture.store,
        selection,
        85,
        1,
        "b",
        &CommandCancellation::new(),
    ));
    let selection = slot.selected_identity().unwrap();
    let conflict = composer::history_intent(ab, 86, MutationKind::Undo, composer::position(2));
    slot.test_selected_host_mut()
        .unwrap()
        .test_arm_history_before_execute_fault(move |store, storage| {
            composer::direct_adopt(
                store,
                storage,
                DraftHistoricalRootSelectionIntentV1::new(
                    ab.candidate(),
                    composer::operation_id(87),
                    DraftHistoricalRootDirectionV1::Undo,
                ),
            );
        });
    assert_eq!(
        dispatch_history(
            &mut slot,
            &fixture.store,
            selection,
            conflict,
            &CommandCancellation::new(),
        ),
        RangeHistoryOutcome::Conflict
    );
    assert_eq!(slot.selected_identity(), Some(selection));
    let _ = a;

    let fixture = Fixture::new("phase180-history-error", 91);
    let (claim, _) = fixture.claims();
    let host = fixture.activated_host(fixture.selected_thread, 92, 93, 14);
    let mut slot = MainWindowComposerSlot::new(
        fixture.window_id,
        claim,
        host,
        fixture.storage,
        MainWindowComposerMarkerMetadataAuthority::new(fixture.assets()),
    )
    .unwrap();
    let selection = slot.selected_identity().unwrap();
    let current = committed_binding(dispatch_text(
        &mut slot,
        &fixture.store,
        selection,
        94,
        0,
        "x",
        &CommandCancellation::new(),
    ));
    let selection = slot.selected_identity().unwrap();
    let error = composer::history_intent(current, 95, MutationKind::Undo, composer::position(1));
    slot.test_selected_host_mut()
        .unwrap()
        .test_arm_history_before_execute_fault(move |store, storage| {
            let DraftHistoricalRootSelectionV1::Prepared(prepared) = storage
                .prepare_draft_historical_root_selection(
                    store,
                    DraftHistoricalRootSelectionIntentV1::new(
                        current.candidate(),
                        composer::operation_id(95),
                        DraftHistoricalRootDirectionV1::Undo,
                    ),
                )
                .unwrap()
            else {
                panic!("history unexpectedly unavailable")
            };
            let mut command = HomeCommand::new(store.home_revision().unwrap());
            command
                .add(storage.error_draft_historical_root_adoption(
                    storage.revision(store).unwrap(),
                    prepared,
                    DraftHistoricalRootAdoptionErrorReasonV1::InvalidAuthority,
                ))
                .unwrap();
            assert!(matches!(
                store.execute(command),
                CommandOutcome::Committed { .. }
            ));
        });
    assert_eq!(
        dispatch_history(
            &mut slot,
            &fixture.store,
            selection,
            error,
            &CommandCancellation::new(),
        ),
        RangeHistoryOutcome::Error
    );
    assert_eq!(slot.selected_identity(), Some(selection));
}

#[test]
fn autosave_and_release_flush_publish_only_the_exact_captured_adopted_binding() {
    autosave_and_release_flush_case();
}

fn autosave_and_release_flush_case() {
    let fixture = Fixture::new("phase179-lifecycle", 35);
    let (claim, _) = fixture.claims();
    let host = fixture.activated_host(fixture.selected_thread, 36, 37, 6);
    let mut slot = MainWindowComposerSlot::new(
        fixture.window_id,
        claim,
        host,
        fixture.storage,
        MainWindowComposerMarkerMetadataAuthority::new(fixture.assets()),
    )
    .unwrap();
    let initial = slot.selected_identity().unwrap();
    let adopted = committed_binding(dispatch_text(
        &mut slot,
        &fixture.store,
        initial,
        38,
        0,
        "autosave",
        &CommandCancellation::new(),
    ));
    let selection = slot.selected_identity().unwrap();
    let timer = slot.selected_host().unwrap().autosave_timer().unwrap();
    let seals = fixture.marker_seals();
    let ComposerHostAutosaveCapture::Captured(ticket) = slot
        .fire_selected_autosave(
            &fixture.store,
            selection,
            timer,
            fixture.assets(),
            &seals,
            support::operation_id(39),
            None,
            syndic_storage::SyndicTimestamp::from_unix_millis(39),
            &CommandCancellation::new(),
        )
        .unwrap()
    else {
        panic!("dirty autosave was not captured")
    };
    for _ in 0..16 {
        if matches!(
            slot.advance_selected_autosave(&fixture.store, selection, ticket)
                .unwrap(),
            ComposerHostAutosaveAdvance::Saved { .. }
        ) {
            break;
        }
    }
    let selection = slot.selected_identity().unwrap();
    let state = slot.selected_draft_state(selection).unwrap();
    assert_eq!(state.adopted(), selection.binding());
    assert_published_binding(state, selection.binding());
    assert_eq!(state.adopted().root(), adopted.root());
    assert_eq!(
        state.adopted().candidate().candidate_generation(),
        adopted.candidate().candidate_generation()
    );

    let next = committed_binding(dispatch_text(
        &mut slot,
        &fixture.store,
        selection,
        40,
        8,
        "!",
        &CommandCancellation::new(),
    ));
    let selection = slot.selected_identity().unwrap();
    let ComposerHostFlushAdmission::Started { ticket, state } = slot
        .begin_selected_flush(selection, ComposerHostFlushPurpose::Release)
        .unwrap()
    else {
        panic!("release flush did not start")
    };
    assert_eq!(state, ComposerHostFlushState::CaptureRequired);
    assert!(matches!(
        slot.capture_selected_flush_publication(
            &fixture.store,
            selection,
            ticket,
            fixture.assets(),
            &seals,
            support::operation_id(41),
            None,
            syndic_storage::SyndicTimestamp::from_unix_millis(41),
            &CommandCancellation::new(),
        )
        .unwrap(),
        ComposerHostFlushCapture::Captured(_)
    ));
    for _ in 0..16 {
        if matches!(
            slot.advance_selected_flush(&fixture.store, selection, ticket)
                .unwrap(),
            ComposerHostFlushAdvance::Progress(ComposerHostFlushState::DisposalRequired)
        ) {
            break;
        }
    }
    let selection = slot.selected_identity().unwrap();
    let state = slot.selected_draft_state(selection).unwrap();
    assert_eq!(state.adopted(), selection.binding());
    assert_published_binding(state, selection.binding());
    assert_eq!(state.adopted().root(), next.root());
    assert_eq!(
        state.adopted().candidate().candidate_generation(),
        next.candidate().candidate_generation()
    );
    assert!(matches!(
        slot.capture_selected_flush_disposal(
            &fixture.store,
            selection,
            ticket,
            support::operation_id(42),
            &CommandCancellation::new(),
        )
        .unwrap(),
        ComposerHostFlushCapture::State(ComposerHostFlushState::DisposalRequired)
    ));
    assert_eq!(
        slot.advance_selected_flush(&fixture.store, selection, ticket)
            .unwrap(),
        ComposerHostFlushAdvance::Satisfied(ComposerHostFlushPurpose::Release)
    );
}

#[test]
fn activation_publish_advance_maps_the_captured_binding_before_widget_release() {
    let fixture = Fixture::new("phase179-activation-publish-mapping", 145);
    let (selected_claim, target_claim) = fixture.claims();
    let host = fixture.activated_host(fixture.selected_thread, 146, 147, 6);
    let mut slot = MainWindowComposerSlot::new(
        fixture.window_id,
        selected_claim,
        host,
        fixture.storage,
        MainWindowComposerMarkerMetadataAuthority::new(fixture.assets()),
    )
    .unwrap();
    let initial = slot.selected_identity().unwrap();
    let captured = committed_binding(dispatch_text(
        &mut slot,
        &fixture.store,
        initial,
        148,
        0,
        "switch",
        &CommandCancellation::new(),
    ));
    let captured_selection = slot.selected_identity().unwrap();
    let MainWindowComposerActivationAdvance::Ready(receipt) = slot
        .begin_activation(
            &fixture.store,
            target_claim,
            support::activation(fixture.target_thread, 149, 150, 4),
            support::operation_id(151),
            &CommandCancellation::new(),
        )
        .unwrap()
    else {
        panic!("target activation did not become coherent")
    };
    let ComposerHostFlushAdmission::Started { ticket, state } =
        slot.begin_publish(&fixture.store, receipt).unwrap()
    else {
        panic!("activation publication flush did not start")
    };
    assert_eq!(state, ComposerHostFlushState::CaptureRequired);
    assert!(matches!(
        slot.capture_selected_flush_publication(
            &fixture.store,
            captured_selection,
            ticket,
            fixture.assets(),
            &fixture.marker_seals(),
            support::operation_id(152),
            None,
            syndic_storage::SyndicTimestamp::from_unix_millis(152),
            &CommandCancellation::new(),
        )
        .unwrap(),
        ComposerHostFlushCapture::Captured(_)
    ));
    let mut disposal_required = false;
    for _ in 0..16 {
        match slot.advance_publish(&fixture.store, receipt).unwrap() {
            MainWindowComposerPublishAdvance::Progress(
                ComposerHostFlushState::DisposalRequired,
            ) => {
                disposal_required = true;
                break;
            }
            MainWindowComposerPublishAdvance::Progress(_)
            | MainWindowComposerPublishAdvance::ReconciliationPending => {}
            other => panic!("activation publication did not reach disposal: {other:?}"),
        }
    }
    assert!(disposal_required);
    let published_selection = slot.selected_identity().unwrap();
    assert_ne!(published_selection, captured_selection);
    let state = slot.selected_draft_state(published_selection).unwrap();
    assert_eq!(state.adopted(), published_selection.binding());
    assert_published_binding(state, published_selection.binding());
    assert_eq!(state.published().root(), captured.root());
    assert!(matches!(
        slot.capture_selected_flush_disposal(
            &fixture.store,
            published_selection,
            ticket,
            support::operation_id(153),
            &CommandCancellation::new(),
        )
        .unwrap(),
        ComposerHostFlushCapture::State(ComposerHostFlushState::DisposalRequired)
    ));
    assert_eq!(
        slot.advance_publish(&fixture.store, receipt).unwrap(),
        MainWindowComposerPublishAdvance::WidgetReleaseRequired(published_selection)
    );
}

#[test]
fn final_disposal_advance_maps_the_captured_binding_before_widget_release() {
    let fixture = Fixture::new("phase179-final-disposal-mapping", 155);
    let (claim, _) = fixture.claims();
    let host = fixture.activated_host(fixture.selected_thread, 156, 157, 6);
    let mut slot = MainWindowComposerSlot::new(
        fixture.window_id,
        claim,
        host,
        fixture.storage,
        MainWindowComposerMarkerMetadataAuthority::new(fixture.assets()),
    )
    .unwrap();
    let initial = slot.selected_identity().unwrap();
    let captured = committed_binding(dispatch_text(
        &mut slot,
        &fixture.store,
        initial,
        158,
        0,
        "dispose",
        &CommandCancellation::new(),
    ));
    let captured_selection = slot.selected_identity().unwrap();
    let ComposerHostFlushAdmission::Started { ticket, state } =
        slot.begin_disposal(&fixture.store).unwrap()
    else {
        panic!("final disposal flush did not start")
    };
    assert_eq!(state, ComposerHostFlushState::CaptureRequired);
    assert!(matches!(
        slot.capture_selected_flush_publication(
            &fixture.store,
            captured_selection,
            ticket,
            fixture.assets(),
            &fixture.marker_seals(),
            support::operation_id(159),
            None,
            syndic_storage::SyndicTimestamp::from_unix_millis(159),
            &CommandCancellation::new(),
        )
        .unwrap(),
        ComposerHostFlushCapture::Captured(_)
    ));
    let mut disposal_required = false;
    for _ in 0..16 {
        match slot.advance_disposal(&fixture.store).unwrap() {
            MainWindowComposerDisposalAdvance::Progress(
                ComposerHostFlushState::DisposalRequired,
            ) => {
                disposal_required = true;
                break;
            }
            MainWindowComposerDisposalAdvance::Progress(_)
            | MainWindowComposerDisposalAdvance::ReconciliationPending => {}
            other => panic!("final disposal did not reach disposal capture: {other:?}"),
        }
    }
    assert!(disposal_required);
    let published_selection = slot.selected_identity().unwrap();
    assert_ne!(published_selection, captured_selection);
    let state = slot.selected_draft_state(published_selection).unwrap();
    assert_eq!(state.adopted(), published_selection.binding());
    assert_published_binding(state, published_selection.binding());
    assert_eq!(state.published().root(), captured.root());
    assert!(matches!(
        slot.capture_selected_flush_disposal(
            &fixture.store,
            published_selection,
            ticket,
            support::operation_id(160),
            &CommandCancellation::new(),
        )
        .unwrap(),
        ComposerHostFlushCapture::State(ComposerHostFlushState::DisposalRequired)
    ));
    assert_eq!(
        slot.advance_disposal(&fixture.store).unwrap(),
        MainWindowComposerDisposalAdvance::WidgetReleaseRequired(published_selection)
    );
}

#[test]
fn concurrent_autosave_successor_keeps_newest_adopted_and_older_publication_dirty() {
    let fixture = Fixture::new("phase179-concurrent-autosave-successor", 135);
    let (claim, _) = fixture.claims();
    let host = fixture.activated_host(fixture.selected_thread, 136, 137, 6);
    let mut slot = MainWindowComposerSlot::new(
        fixture.window_id,
        claim,
        host,
        fixture.storage,
        MainWindowComposerMarkerMetadataAuthority::new(fixture.assets()),
    )
    .unwrap();
    let initial = slot.selected_identity().unwrap();
    let captured = committed_binding(dispatch_text(
        &mut slot,
        &fixture.store,
        initial,
        138,
        0,
        "captured",
        &CommandCancellation::new(),
    ));
    let captured_selection = slot.selected_identity().unwrap();
    let timer = slot.selected_host().unwrap().autosave_timer().unwrap();
    let ComposerHostAutosaveCapture::Captured(ticket) = slot
        .fire_selected_autosave(
            &fixture.store,
            captured_selection,
            timer,
            fixture.assets(),
            &fixture.marker_seals(),
            support::operation_id(139),
            None,
            syndic_storage::SyndicTimestamp::from_unix_millis(139),
            &CommandCancellation::new(),
        )
        .unwrap()
    else {
        panic!("dirty autosave was not captured")
    };
    let successor = committed_binding(dispatch_text(
        &mut slot,
        &fixture.store,
        captured_selection,
        140,
        8,
        "-successor",
        &CommandCancellation::new(),
    ));
    let successor_selection = slot.selected_identity().unwrap();
    let mut saved = false;
    for _ in 0..16 {
        match slot
            .advance_selected_autosave(&fixture.store, successor_selection, ticket)
            .unwrap()
        {
            ComposerHostAutosaveAdvance::Saved { dirty_successor } => {
                assert!(dirty_successor);
                saved = true;
                break;
            }
            ComposerHostAutosaveAdvance::Progress
            | ComposerHostAutosaveAdvance::Ready
            | ComposerHostAutosaveAdvance::ReconciliationPending => {}
            other => panic!("concurrent autosave did not save: {other:?}"),
        }
    }
    assert!(saved);
    let selection = slot.selected_identity().unwrap();
    let state = slot.selected_draft_state(selection).unwrap();
    assert_eq!(state.adopted(), selection.binding());
    assert_eq!(
        state.adopted().candidate().candidate_generation(),
        successor.candidate().candidate_generation()
    );
    assert_eq!(state.adopted().root(), successor.root());
    assert_eq!(
        state.published().candidate_generation(),
        captured.candidate().candidate_generation()
    );
    assert_eq!(state.published().root(), captured.root());
    assert_eq!(
        state.published().history().candidate_generation(),
        captured.history().candidate_generation()
    );
    assert_eq!(state.published().history().root(), captured.root());
    assert_ne!(state.published().history(), successor.history());
    assert!(state.is_dirty());
    assert!(slot.selected_host().unwrap().autosave_timer().is_some());
}

#[test]
fn marker_menu_and_preview_keep_canonical_anchor_and_focus_fallback() {
    let fixture = Fixture::new("phase179-marker", 31);
    let (claim, _) = fixture.claims();
    let host = fixture.activated_host(fixture.selected_thread, 32, 33, 7);
    let slot = MainWindowComposerSlot::new(
        fixture.window_id,
        claim,
        host,
        fixture.storage,
        MainWindowComposerMarkerMetadataAuthority::new(fixture.assets()),
    )
    .unwrap();
    let selection = slot.selected_identity().unwrap();
    let object_id = InlineObjectId::new(41);
    let anchor = RealizedInlineObjectAnchor {
        binding: selection.binding().range_binding(),
        object_id,
        order: InlineObjectOrder::new(2),
        presentation_generation: PresentationGeneration::new(7),
        layout_epoch: LayoutEpoch::new(1),
        bounds: Bounds::new(
            Point::new(px(10.0), px(11.0)),
            Size::new(px(24.0), px(22.0)),
        ),
    };
    let activation = InlineObjectActivation {
        anchor,
        origin: InlineObjectInputOrigin::Pointer {
            point: Point::new(px(12.0), px(12.0)),
        },
    };
    let mut surfaces = MainWindowComposerImageSurfaces::default();
    assert_eq!(
        surfaces.activate_marker(selection, activation).unwrap(),
        ComposerMarkerActivationDisposition::Opened
    );
    assert_eq!(
        surfaces.activate_marker(selection, activation).unwrap(),
        ComposerMarkerActivationDisposition::DuplicateSuppressed
    );
    assert_eq!(surfaces.menu().unwrap().anchor(), anchor);
    let moved_anchor = RealizedInlineObjectAnchor {
        layout_epoch: LayoutEpoch::new(2),
        bounds: Bounds::new(
            Point::new(px(30.0), px(31.0)),
            Size::new(px(24.0), px(22.0)),
        ),
        ..anchor
    };
    assert_eq!(
        surfaces
            .activate_marker(
                selection,
                InlineObjectActivation {
                    anchor: moved_anchor,
                    origin: activation.origin,
                },
            )
            .unwrap(),
        ComposerMarkerActivationDisposition::DuplicateSuppressed
    );
    assert_eq!(surfaces.menu().unwrap().anchor(), moved_anchor);
    assert_eq!(
        surfaces.menu().unwrap().commands(),
        &[ComposerMarkerCommand::View, ComposerMarkerCommand::Remove]
    );
    surfaces
        .invoke_view(selection, ComposerImagePresentationState::Pending)
        .unwrap();
    assert!(surfaces.menu().is_none());
    assert_eq!(
        surfaces.preview().unwrap().commands(),
        &[ComposerPreviewCommand::Copy, ComposerPreviewCommand::Save]
    );
    assert_eq!(
        surfaces.preview().unwrap().command_state(),
        ComposerImagePreviewCommandState::DisabledPending
    );
    assert_eq!(
        surfaces.dismiss_preview(false, true),
        Some(ComposerMarkerFocusTarget::ComposerEditor)
    );

    surfaces.activate_marker(selection, activation).unwrap();
    assert_eq!(
        surfaces.invoke_remove(selection).unwrap(),
        anchor,
        "Remove must target the exact activated marker"
    );
    assert!(surfaces.menu().is_none());

    surfaces.activate_marker(selection, activation).unwrap();
    assert_eq!(
        surfaces.dismiss_menu(true, true),
        Some(ComposerMarkerFocusTarget::OriginMarker(anchor))
    );
    surfaces.activate_marker(selection, activation).unwrap();
    surfaces
        .invoke_view(selection, ComposerImagePresentationState::LocalUnavailable)
        .unwrap();
    assert_eq!(
        surfaces.preview().unwrap().command_state(),
        ComposerImagePreviewCommandState::DisabledUnavailable
    );
    assert_eq!(
        surfaces.dismiss_preview(false, false),
        Some(ComposerMarkerFocusTarget::ThreadSelector)
    );
}

fn assert_published_binding(state: MainWindowComposerDraftState, binding: ComposerHostBinding) {
    assert_eq!(
        state.published().candidate_generation(),
        binding.candidate().candidate_generation()
    );
    assert_eq!(state.published().root(), binding.root());
    assert_eq!(
        state.published().history().candidate_generation(),
        binding.history().candidate_generation()
    );
    assert_eq!(state.published().history().root(), binding.root());
}

fn dispatch_text(
    slot: &mut MainWindowComposerSlot,
    store: &beryl_home_store::HomeStore,
    selection: beryl_app::main_window::MainWindowComposerSelectionIdentity,
    operation: u64,
    offset: u64,
    text: &str,
    cancellation: &CommandCancellation,
) -> ComposerHostMutationOutcome {
    dispatch_edit(
        slot,
        store,
        selection,
        operation,
        SourceRange::new(composer::position(offset), composer::position(offset)).unwrap(),
        composer::position(offset),
        vec![MutationPageItem::Utf8 {
            inserted_offset: 0,
            text: text.into(),
        }],
        MutationPositions::collapsed(composer::position(offset + text.len() as u64)),
        LogicalExtent::new(
            selection.binding().logical_extent().logical_utf8_bytes() + text.len() as u64,
            selection.binding().logical_extent().logical_line_count(),
        ),
        Box::new([]),
        cancellation,
    )
}

fn dispatch_history(
    slot: &mut MainWindowComposerSlot,
    store: &beryl_home_store::HomeStore,
    selection: beryl_app::main_window::MainWindowComposerSelectionIdentity,
    intent: gpui_text_input::RangeHistoryIntent,
    cancellation: &CommandCancellation,
) -> RangeHistoryOutcome {
    match slot
        .dispatch_selected_request(
            store,
            selection,
            RangeTextInputRequest::HistoryIntent(intent),
            Box::new([]),
            cancellation,
        )
        .unwrap()
    {
        MainWindowComposerDispatchOutcome::History { outcome, .. } => outcome,
        _ => panic!("composer history dispatch returned a non-history outcome"),
    }
}

#[allow(clippy::too_many_arguments)]
fn dispatch_edit(
    slot: &mut MainWindowComposerSlot,
    store: &beryl_home_store::HomeStore,
    selection: beryl_app::main_window::MainWindowComposerSelectionIdentity,
    operation: u64,
    replacement: SourceRange,
    predecessor: SourcePosition,
    items: Vec<MutationPageItem>,
    intended: MutationPositions,
    extent: LogicalExtent,
    marker_metadata: Box<[ComposerHostImageMarkerMetadata]>,
    cancellation: &CommandCancellation,
) -> ComposerHostMutationOutcome {
    let (key, finish) = stage_edit(
        slot,
        store,
        selection,
        operation,
        replacement,
        predecessor,
        items,
        intended,
        extent,
        marker_metadata,
    );
    settle_edit(slot, store, selection, key, finish, cancellation)
}

#[allow(clippy::too_many_arguments)]
fn stage_edit(
    slot: &mut MainWindowComposerSlot,
    store: &beryl_home_store::HomeStore,
    selection: beryl_app::main_window::MainWindowComposerSelectionIdentity,
    operation: u64,
    replacement: SourceRange,
    predecessor: SourcePosition,
    items: Vec<MutationPageItem>,
    intended: MutationPositions,
    extent: LogicalExtent,
    marker_metadata: Box<[ComposerHostImageMarkerMetadata]>,
) -> (MutationKey, MutationIdentity) {
    let range = selection.binding().range_binding();
    let key = MutationKey::new(
        range.binding(),
        range.revision(),
        OperationId::new(operation),
    );
    let begin = MutationBeginRequest::new(
        MutationProposal::new(
            key,
            MutationKind::Edit,
            MutationPositions::collapsed(predecessor),
            replacement,
            0,
        ),
        MutationCursor::new(0),
        MutationCursor::new(0),
    );
    assert!(matches!(
        slot.dispatch_selected_request(
            store,
            selection,
            RangeTextInputRequest::MutationBegin(begin),
            Box::new([]),
            &CommandCancellation::new(),
        )
        .unwrap(),
        MainWindowComposerDispatchOutcome::MutationBegan(actual) if actual == key
    ));
    let page = MutationPage::new(
        MutationPageKey::new(
            key,
            MutationLane::Proposal,
            MutationCursor::new(0),
            0,
            MutationIdentity::ROOT,
        ),
        MutationCursor::new(1),
        items,
    )
    .unwrap();
    let proposal_finish = MutationStreamFinish {
        next_cursor: page.next_cursor(),
        next_ordinal: 1,
        cumulative_identity: page.cumulative_identity(),
        totals: page.totals(),
    };
    slot.dispatch_selected_request(
        store,
        selection,
        RangeTextInputRequest::MutationProposalPage(MutationPageRequest::new(page)),
        marker_metadata,
        &CommandCancellation::new(),
    )
    .unwrap();
    let empty = MutationStreamFinish {
        next_cursor: MutationCursor::new(0),
        next_ordinal: 0,
        cumulative_identity: MutationIdentity::ROOT,
        totals: MutationTotals::default(),
    };
    let finish = MutationFinishInput::new(key, empty, proposal_finish, extent, intended);
    slot.dispatch_selected_request(
        store,
        selection,
        RangeTextInputRequest::MutationFinishInput(finish),
        Box::new([]),
        &CommandCancellation::new(),
    )
    .unwrap();
    (key, proposal_finish.cumulative_identity)
}

fn settle_edit(
    slot: &mut MainWindowComposerSlot,
    store: &beryl_home_store::HomeStore,
    selection: beryl_app::main_window::MainWindowComposerSelectionIdentity,
    key: MutationKey,
    finish: MutationIdentity,
    cancellation: &CommandCancellation,
) -> ComposerHostMutationOutcome {
    for _ in 0..32 {
        match slot.dispatch_selected_request(
            store,
            selection,
            RangeTextInputRequest::MutationCommit(MutationCommitRequest::new(key, finish)),
            Box::new([]),
            cancellation,
        ) {
            Ok(MainWindowComposerDispatchOutcome::Mutation { outcome, .. }) => return outcome,
            Err(MainWindowComposerDispatchError::Host(ComposerHostError::MutationWorkPending)) => {}
            Ok(_) => panic!("composer mutation returned a non-terminal outcome"),
            Err(error) => panic!("composer mutation did not settle: {error:?}"),
        }
    }
    panic!("composer mutation remained pending")
}

fn committed_binding(
    outcome: ComposerHostMutationOutcome,
) -> beryl_app::composer_host::ComposerHostBinding {
    match outcome {
        ComposerHostMutationOutcome::Committed { binding, .. } => binding,
        other => panic!("composer mutation did not commit: {other:?}"),
    }
}

fn publish_image_asset(fixture: &Fixture, bytes: &[u8]) -> AssetId {
    let sidecar = fixture
        .store
        .admit_sidecar(
            SidecarNamespace::new("images").unwrap(),
            bytes,
            SidecarByteLimit::new(NonZeroU64::new(1_024).unwrap()),
        )
        .unwrap();
    let asset = AssetId::sha256_v1(
        sidecar.address().digest().as_bytes(),
        NonZeroU64::new(sidecar.address().length()).unwrap(),
    );
    let assets = fixture.assets();
    let expected = assets.revision(&fixture.store).unwrap();
    let contribution = assets
        .publish_metadata(
            expected,
            sidecar,
            PublishAssetMetadata::new(
                asset,
                AssetMediaType::new("image/png").unwrap(),
                None,
                expected.checked_next().unwrap(),
            ),
        )
        .unwrap();
    let mut command = HomeCommand::new(fixture.store.home_revision().unwrap());
    contribution.add_to(&mut command).unwrap();
    assert!(matches!(
        fixture.store.execute(command),
        CommandOutcome::Committed { .. }
    ));
    asset
}

fn asset_id(object: InlineObjectId) -> AssetId {
    let bytes = object.get().to_be_bytes();
    let mut digest = [0; 32];
    digest[..16].copy_from_slice(&bytes);
    digest[16..].copy_from_slice(&bytes);
    AssetId::sha256_v1(digest, NonZeroU64::MIN)
}
