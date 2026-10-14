use super::code_long_line::{wait_until, wait_until_unminimized, with_native};
use super::narrative_native::{
    copy_both, drag, key, point_at, pointer, ready, record, require_fixture_ledger, writer,
};
use super::*;
use crate::syndic_transcript::{
    ResidentPresentationRecordKind, ResidentRecordSource, SyndicTranscriptPanel,
};
use beryl_model::SyndicThreadId;
use gpui::{Entity, EntityInputHandler, Focusable, PlatformInput, point, px};

#[path = "mounted_workflow/support.rs"]
mod workflow;
use workflow::*;

#[path = "mounted_workflow/composer.rs"]
mod composer_interaction;
use composer_interaction::*;

#[test]
fn native_mounted_mixed_replacement_clear_reentry_and_disposal() {
    require_fixture_ledger();
    run_mounted_with_prepared_home_retained_and_workers_and_deadline(
        1,
        0,
        9,
        Duration::from_secs(120),
        super::image_markers::fixture::seed,
        |owner, _, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let panel = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                leading(window, &panel, cx).await;
                ready(window, &panel, cx, &["before", "after"]).await;
                let target = edit(window, &panel, cx).await;
                let resident = composer(window, cx).await;
                replacement_ready(window, &resident, &panel, true, cx).await;
                let input = resident
                    .read_with(cx, |resident, _| resident.gpui_input())
                    .unwrap();
                let before = input
                    .read_with(cx, |input, _| {
                        assert_eq!(input.surface().unwrap().realized_objects().len(), 2);
                        input
                            .export_restoration(Some(input.history_frontier()))
                            .unwrap()
                    })
                    .unwrap();
                with_native(window, cx, |native, app| {
                    input.update(app, |input, _| input.focus(native));
                    key(native, app, "escape");
                })
                .unwrap();
                replacement_ready(window, &resident, &panel, false, cx).await;
                assert_eq!(
                    input
                        .read_with(cx, |input, _| input
                            .export_restoration(Some(input.history_frontier()))
                            .unwrap())
                        .unwrap(),
                    before
                );
                clear_editor(window, &resident, cx).await;
                resident
                    .read_with(cx, |resident, app| {
                        assert!(resident.last_error().is_none());
                        assert!(resident.last_mutation_admission_failure().is_none());
                        assert!(!resident.test_has_active_flight());
                        assert_eq!(
                            resident
                                .selection_identity()
                                .binding()
                                .logical_extent()
                                .logical_utf8_bytes(),
                            0
                        );
                        assert!(resident.test_edit_position_proof_ready(app));
                        let input = input.read(app);
                        assert!(input.is_quiescent());
                        assert!(input.is_surface_current_and_interactive());
                        let surface = input.surface().unwrap();
                        assert_eq!(
                            surface.binding(),
                            resident.selection_identity().binding().range_binding()
                        );
                        assert!(surface.realized_objects().is_empty());
                        let selection = surface.source_selection();
                        assert_eq!(selection.anchor, selection.head);
                        assert_eq!(selection.head.byte_offset.get(), 0);
                        assert_eq!(
                            selection.head.gap,
                            gpui_text_input::InlineObjectGap::NoObjects
                        );
                        assert_eq!(surface.source_caret(), selection.head);
                    })
                    .unwrap();
                leading(window, &panel, cx).await;
                assert_eq!(edit(window, &panel, cx).await, target);
                replacement_ready(window, &resident, &panel, true, cx).await;
                let reentered = input
                    .read_with(cx, |input, _| {
                        assert_eq!(input.surface().unwrap().realized_objects().len(), 2);
                        input
                            .export_restoration(Some(input.history_frontier()))
                            .unwrap()
                    })
                    .unwrap();
                with_native(window, cx, |native, app| {
                    input.update(app, |input, _| input.focus(native));
                    key(native, app, "escape");
                })
                .unwrap();
                replacement_ready(window, &resident, &panel, false, cx).await;
                assert_eq!(
                    input
                        .read_with(cx, |input, _| input
                            .export_restoration(Some(input.history_frontier()))
                            .unwrap())
                        .unwrap(),
                    reentered
                );
                dispose(window, &panel, cx).await;
            })
        },
    );
}
