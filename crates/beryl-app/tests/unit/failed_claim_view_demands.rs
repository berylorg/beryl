use super::*;
use std::num::NonZeroU64;

use crate::{
    composer_host::{ComposerHostActivationOutcome, SyndicComposerHost},
    main_window::{
        MainWindowComposerDispatchOutcome, MainWindowComposerMarkerMetadataAuthority,
        MainWindowComposerSuccessorProofLimits,
    },
};
use beryl_home_store::CommandCancellation;
use gpui_text_input::{
    BindingId, ByteOffset, InlineObjectGap, MutationBeginRequest, MutationCursor, MutationKey,
    MutationKind, MutationPositions, MutationProposal, RangeBinding, SourcePosition, SourceRange,
};

#[path = "../pending_composer_activation/support.rs"]
mod support;

fn original_slot(seed: u8) -> (support::fixture::Fixture, MainWindowComposerSlot) {
    let fixture = support::fixture::Fixture::new("failed-view-demand", seed);
    let marker_authority = MainWindowComposerMarkerMetadataAuthority::new(fixture.assets());
    let mut host = SyndicComposerHost::new(fixture.storage.clone());
    assert!(matches!(
        host.test_activate(
            &fixture.store,
            support::activation_with_marker_proof(fixture.selected_thread, 1, 2, 1, 0),
            &CommandCancellation::new(),
        )
        .unwrap(),
        ComposerHostActivationOutcome::Activated { .. }
    ));
    let slot = MainWindowComposerSlot::new(
        fixture.window_id,
        fixture.claims().0,
        host,
        fixture.storage.clone(),
        marker_authority,
    )
    .unwrap();
    (fixture, slot)
}

#[gpui::test]
fn failed_geometry_page_settles_original_demand_preserving_existing_presentation(
    cx: &mut gpui::TestAppContext,
) {
    let (_fixture, slot) = original_slot(31);
    let binding = slot.selected_identity().unwrap().binding().range_binding();
    let (input, cx) = cx.add_window_view(|window, cx| {
        RangeTextInput::new(
            support::widget_config(binding, NonZeroU64::new(1).unwrap()),
            window,
            cx,
        )
        .unwrap()
    });
    input.update(cx, |input, cx| {
        let before = input.realization_diagnostics().current;
        let presentation = input
            .surface()
            .map(|surface| (surface.binding(), surface.caret(), surface.selection()));
        assert_eq!(before.pending_page_requests, 1);
        assert_eq!(before.dispatched_page_requests, 0);
        let mut key = None;
        assert!(
            input
                .take_request_if(|request| {
                    let RangeTextInputRequest::Page(page) = request else {
                        panic!("original geometry Page")
                    };
                    key = Some(page.key());
                    false
                })
                .is_none()
        );
        assert_eq!(input.realization_diagnostics().current, before);
        assert!(settle_failed_input_view_demands(input, binding, cx).unwrap());
        assert!(input.is_quiescent());
        assert_eq!(
            input.surface().map(|surface| (
                surface.binding(),
                surface.caret(),
                surface.selection()
            )),
            presentation
        );
        let after = input.realization_diagnostics().current;
        assert_eq!(after.queued_requests, 0);
        assert_eq!(after.pending_page_requests, 0);
        assert_eq!(after.dispatched_page_requests, 0);
        assert_eq!(after.active_geometry_jobs, 0);
        assert!(matches!(
            input.fail_page(key.unwrap(), gpui_text_input::PageFailure::Unavailable, cx),
            Err(gpui_text_input::RangeTextInputError::Stale)
        ));
    });
}

#[gpui::test]
fn failed_geometry_object_settles_exact_cleanup_and_generated_release_custody(
    cx: &mut gpui::TestAppContext,
) {
    let (fixture, mut slot) = original_slot(41);
    let selection = slot.selected_identity().unwrap();
    let binding = selection.binding().range_binding();
    let (input, cx) = cx.add_window_view(|window, cx| {
        RangeTextInput::new(
            support::widget_config(binding, NonZeroU64::new(1).unwrap()),
            window,
            cx,
        )
        .unwrap()
    });
    let request = input.update(cx, |input, _| input.take_request().unwrap());
    assert!(matches!(&request, RangeTextInputRequest::Page(_)));
    let MainWindowComposerDispatchOutcome::Page(page) = slot
        .dispatch_selected_request(
            &fixture.store,
            selection,
            request,
            Box::new([]),
            &CommandCancellation::new(),
        )
        .unwrap()
    else {
        panic!("source-owned geometry Page")
    };
    cx.update(|window, app| {
        input.update(app, |input, cx| {
            input.deliver_page(page, window, cx).unwrap()
        });
    });
    input.update(cx, |input, cx| {
        for _ in 0..input.realization_diagnostics().max_queued_requests {
            if input
                .take_request_if(MainWindowComposerSlot::widget_release_request_is_settled)
                .is_none()
            {
                break;
            }
        }
        let before = input.realization_diagnostics().current;
        let presentation = input
            .surface()
            .map(|surface| (surface.binding(), surface.caret(), surface.selection()));
        assert_eq!(before.pending_geometry_objects, 1);
        assert_eq!(before.pending_object_requests, 1);
        assert_eq!(before.dispatched_object_requests, 0);
        let mut key = None;
        assert!(
            input
                .take_request_if(|request| {
                    let RangeTextInputRequest::ObjectPage(page) = request else {
                        panic!("original geometry Object")
                    };
                    key = Some(page.key());
                    false
                })
                .is_none()
        );
        assert_eq!(input.realization_diagnostics().current, before);
        assert_eq!(presentation.map(|identity| identity.0), Some(binding));
        let settled = settle_failed_input_view_demands(input, binding, cx).unwrap();
        let after = input.realization_diagnostics().current;
        assert!(
            settled,
            "original Object cleanup remains pending: {after:?}"
        );
        assert_eq!(after.active_geometry_jobs, 0);
        assert_eq!(after.pending_geometry_objects, 0);
        assert_eq!(after.pending_object_requests, 0);
        assert_eq!(after.dispatched_object_requests, 0);
        assert_eq!(after.queued_requests, 0);
        assert!(input.is_quiescent());
        assert_eq!(
            input.surface().map(|surface| (
                surface.binding(),
                surface.caret(),
                surface.selection()
            )),
            presentation
        );
        assert!(matches!(
            input.fail_object_page(
                key.unwrap(),
                gpui_text_input::ObjectPageFailure::Unavailable,
                cx
            ),
            Err(gpui_text_input::RangeTextInputError::Stale)
        ));
    });
    assert_eq!(slot.selected_identity(), Some(selection));
}

#[gpui::test]
fn failed_view_rejects_foreign_front_without_taking_its_original_request(
    cx: &mut gpui::TestAppContext,
) {
    let (_fixture, slot) = original_slot(51);
    let binding = slot.selected_identity().unwrap().binding().range_binding();
    let foreign = RangeBinding::new(
        BindingId::new(binding.binding().get() + 1),
        binding.revision(),
        binding.extent(),
    );
    let (input, cx) = cx.add_window_view(|window, cx| {
        RangeTextInput::new(
            support::widget_config(binding, NonZeroU64::new(1).unwrap()),
            window,
            cx,
        )
        .unwrap()
    });
    input.update(cx, |input, cx| {
        let before = input.realization_diagnostics().current;
        let mut original = None;
        assert!(input.take_request_if(|request| {
            let RangeTextInputRequest::Page(page) = request else { panic!("original Page") };
            original = Some(page.key());
            false
        }).is_none());
        assert_eq!(settle_failed_input_view_demands(input, foreign, cx), Err("original failed view request remains unsupported".into()));
        assert_eq!(input.realization_diagnostics().current, before);
        assert!(input.take_request_if(|request| {
            assert!(matches!(request, RangeTextInputRequest::Page(page) if Some(page.key()) == original));
            false
        }).is_none());
        assert_eq!(input.realization_diagnostics().current, before);
        assert!(settle_failed_input_view_demands(input, binding, cx).unwrap());
    });
}

#[gpui::test]
fn failed_view_keeps_semantic_mutation_front_and_original_operation_custody(
    cx: &mut gpui::TestAppContext,
) {
    let (fixture, mut slot) = original_slot(61);
    let selection = slot.selected_identity().unwrap();
    let binding = selection.binding().range_binding();
    let config = support::widget_config(binding, NonZeroU64::new(1).unwrap());
    let point = SourcePosition::new(ByteOffset::new(0), InlineObjectGap::NoObjects);
    let positions = MutationPositions::collapsed(point);
    let proof = slot
        .build_selected_successor_proof(
            &fixture.store,
            selection,
            positions,
            MainWindowComposerSuccessorProofLimits {
                text: config.residency_limits,
                text_page_bytes: config.limits.page_bytes,
                objects: config.object_residency_limits,
                presentation_generation: config.presentation_generation,
            },
        )
        .unwrap();
    let (input, cx) =
        cx.add_window_view(|window, cx| RangeTextInput::new(config, window, cx).unwrap());
    input.update(cx, |input, cx| {
        assert!(settle_failed_input_view_demands(input, binding, cx).unwrap());
        let operation = input.lease_host_operation().unwrap();
        let key = MutationKey::new(binding.binding(), binding.revision(), operation.operation());
        let begin = MutationBeginRequest::new(
            MutationProposal::new(key, MutationKind::Edit, positions, SourceRange::new(point, point).unwrap(), 0),
            MutationCursor::new(0),
            MutationCursor::new(0),
        );
        input.begin_host_mutation(operation, begin, &[point], &proof.text, &proof.objects, cx).unwrap();
        let before = input.realization_diagnostics().current;
        assert!(!input.is_semantically_quiescent());
        assert!(!settle_failed_input_view_demands(input, binding, cx).unwrap());
        assert_eq!(input.realization_diagnostics().current, before);
        assert!(input.take_request_if(|request| {
            assert!(matches!(request, RangeTextInputRequest::MutationBegin(request) if *request == begin));
            false
        }).is_none());
        assert_eq!(input.realization_diagnostics().current, before);
        assert!(!input.is_semantically_quiescent());
        assert!(matches!(input.take_request(), Some(RangeTextInputRequest::MutationBegin(request)) if request == begin));
        input.cancel_mutation(key, cx).unwrap();
        assert!(matches!(input.take_request(), Some(RangeTextInputRequest::CancelMutation(request)) if request.key() == key));
        assert!(input.is_quiescent());
    });
    assert_eq!(slot.selected_identity(), Some(selection));
}
