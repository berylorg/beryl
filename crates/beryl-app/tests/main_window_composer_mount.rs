#![cfg(feature = "test-faults")]

#[path = "syndic_composer_history/support.rs"]
mod composer_support;
#[path = "native_lineage_gui/support.rs"]
mod native_lineage_support;
#[path = "main_window_composer_mount/recovery_switch.rs"]
mod recovery_switch;
#[path = "main_window_composer_slot/support.rs"]
mod support;

use std::{
    num::{NonZeroU64, NonZeroUsize},
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use beryl_app::{
    cas_projection::{
        NativeLineageOperation, NativeLineageRecoveryCommand, NativeLineageRecoveryControl,
        NativeLineageRecoveryKey, NativeLineageRecoveryStatus,
    },
    composer_host::{
        ComposerHostActivationOutcome, ComposerHostActivationRequest, ComposerHostAutosaveInterval,
        ComposerHostFlushAdmission, ComposerHostFlushCapture, ComposerHostFlushState,
        ComposerHostImageMarkerMetadata, ComposerHostInitialDemand, ComposerHostRequestId,
        ComposerHostRequestPurpose, SyndicComposerHost,
    },
    main_window::{
        MainWindowComposerActivationAdvance, MainWindowComposerMarkerMetadataAuthority,
        MainWindowComposerSelectionIdentity, MainWindowComposerSlot,
        MainWindowComposerSubmissionRequestSource, MainWindowConversationComposerAutosavePhase,
        MainWindowConversationComposerConfig, MainWindowConversationComposerMount,
        MainWindowConversationComposerMountDisposalAdvance,
        MainWindowConversationComposerMountEvent, MainWindowConversationComposerMountFlushStart,
        MainWindowConversationComposerMountPublishAdvance, MainWindowConversationComposerService,
        MainWindowNativeLineagePromptCommandPresentation,
    },
};
use beryl_home_store::{
    CommandCancellation, CommandOutcome, HomeCommand, SidecarByteLimit, SidecarNamespace,
};
use beryl_model::{AssetId, BindingRevision, ImageLabelOrdinal};
use beryl_state::{AssetMediaType, PublishAssetMetadata};
use gpui::{
    AppContext, Entity, EntityInputHandler, Focusable, InteractiveElement, IntoElement,
    ParentElement, Render, SharedString, StreamingLayoutBinding, StreamingLayoutLimits,
    StreamingLayoutPosition, Styled, TextRun, black, div, font, px,
};
use gpui_scrollbar::ScrollbarStyle;
use gpui_text_input::{
    ClipboardLimits, ExactGeometryLimits, InlineObjectId, InlineObjectOrder, MutationLimits,
    ObjectResidencyLimits, PresentationGeneration, RangeSettlementCoordinator,
    RangeTextInputConfig, RangeTextInputLimits, ResidencyLimits, SegmentationLimits,
    StreamingGeometryStyle, StreamingOversizePresentation, TextInputAtomClipboardPolicy,
    TextInputEnterKey, TextInputRichPastePolicy, TextInputTheme, ensure_text_input_bindings,
};
use syndic_storage::{
    DraftPieceMarkerDemandV1, DraftPieceMarkerDirectionV1, DraftPieceMarkerScopeV1,
    DraftPieceTextDemandV1, SyndicTimestamp,
};

use support::{Fixture, operation_id};

struct MountRoot {
    mount: Entity<MainWindowConversationComposerMount>,
}

struct NativeLineageMountRoot {
    mount: Entity<MainWindowConversationComposerMount>,
}

struct PendingNativeLineageMountRoot {
    mount: Entity<MainWindowConversationComposerMount>,
    pending_focus: gpui::FocusHandle,
}

#[derive(Clone, Copy, Debug)]
enum NativeLineageLateFlight {
    Validation,
    Page,
    ObjectPage,
}

impl Render for NativeLineageMountRoot {
    fn render(
        &mut self,
        _window: &mut gpui::Window,
        _cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        div()
            .flex()
            .w(px(320.))
            .h(px(64.))
            .child(self.mount.clone())
    }
}

impl Render for PendingNativeLineageMountRoot {
    fn render(
        &mut self,
        _window: &mut gpui::Window,
        _cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        div().child(self.mount.clone()).child(
            div()
                .id("native-lineage-pending-turn-focus")
                .track_focus(&self.pending_focus)
                .tab_stop(true),
        )
    }
}

impl Render for MountRoot {
    fn render(
        &mut self,
        _window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        div().children(self.mount.read(cx).contribution())
    }
}

#[gpui::test]
fn native_lineage_recovery_restores_the_exact_selected_composer(cx: &mut gpui::TestAppContext) {
    cx.update(ensure_text_input_bindings);
    let fixture = Fixture::new("native-lineage-restoration", 241);
    let claim = fixture.claims().0;
    let window_id = fixture.window_id;
    let thread = fixture.selected_thread;
    let marker_authority = MainWindowComposerMarkerMetadataAuthority::new(fixture.assets());
    let marker_seals = fixture.marker_seals();
    let (_directory, store, storage) = fixture.into_store();
    let mut host = SyndicComposerHost::new(storage.clone());
    assert!(matches!(
        host.test_activate(
            &store,
            activation(thread, 242, 243, 1, 0),
            &CommandCancellation::new(),
        )
        .unwrap(),
        ComposerHostActivationOutcome::Activated { .. }
    ));
    let slot =
        MainWindowComposerSlot::new(window_id, claim, host, storage, marker_authority).unwrap();
    let service = Arc::new(MainWindowConversationComposerService::new(
        Arc::new(store),
        slot,
    ));
    let control = NativeLineageRecoveryControl::for_test(NonZeroUsize::new(1).unwrap());
    let key = control
        .install_route_for_test(
            thread,
            thread,
            BindingRevision::new(1).unwrap(),
            NativeLineageOperation::Resume,
            1,
            true,
        )
        .unwrap();
    let mounted_service = service.clone();
    let (root, cx) = cx.add_window_view(|window, cx| {
        let mount = cx.new(|mount_cx| {
            MainWindowConversationComposerMount::new(
                mounted_service,
                Box::new(|selection| {
                    MainWindowConversationComposerConfig::new(
                        selection,
                        widget_config(
                            selection.binding().range_binding(),
                            selection.binding().presentation_generation(),
                        ),
                    )
                    .map_err(|error| error.to_string())
                }),
                marker_seals,
                submission_source(),
                window,
                mount_cx,
            )
            .unwrap()
        });
        NativeLineageMountRoot { mount }
    });
    drive(cx, 16);

    let mount = root.read_with(cx, |root, _| root.mount.clone());
    let selection = service.selected_identity().unwrap();
    let original = mount
        .read_with(cx, |mount, _| mount.contribution())
        .unwrap();
    let original_input = original.read_with(cx, |composer, _| composer.gpui_input());
    let original_seed = cx
        .update(|_, app| {
            original_input.update(app, |input, _| {
                input.export_restoration(Some(selection.binding().range_history_frontier()))
            })
        })
        .unwrap();
    cx.update(|_, app| {
        mount.update(app, |mount, mount_cx| {
            mount.attach_native_lineage_recovery(control.clone(), mount_cx)
        })
    });
    assert_eq!(
        mount.read_with(cx, |mount, _| mount.contribution()),
        Some(original.clone())
    );
    wait_for_native_lineage_prompt(cx, &mount, "exact restoration prompt");
    assert!(mount.read_with(cx, |mount, _| mount.contribution().is_none()));
    assert!(original_input.read_with(cx, |input, _| input.surface().is_none()));
    assert_eq!(service.selected_identity(), Some(selection));
    let prompt = mount.read_with(cx, |mount, _| {
        mount.test_native_lineage_prompt_diagnostics()
    });
    assert_eq!(
        prompt.retry,
        MainWindowNativeLineagePromptCommandPresentation::Enabled
    );
    assert_eq!(
        prompt.recover_from_syndic,
        MainWindowNativeLineagePromptCommandPresentation::Enabled
    );
    assert_eq!(prompt.failed_command, None);

    cx.simulate_keystrokes("enter");
    assert_eq!(
        control.take_command_for_test(key),
        Some(NativeLineageRecoveryCommand::Retry)
    );
    assert!(matches!(
        control.snapshot_for_thread(thread).unwrap().status(),
        NativeLineageRecoveryStatus::Running {
            command: NativeLineageRecoveryCommand::Retry,
        }
    ));
    drive(cx, 2);
    assert!(matches!(
        mount
            .read_with(cx, |mount, _| mount.native_lineage_recovery_snapshot())
            .unwrap()
            .status(),
        NativeLineageRecoveryStatus::Running {
            command: NativeLineageRecoveryCommand::Retry,
        }
    ));
    let prompt = mount.read_with(cx, |mount, _| {
        mount.test_native_lineage_prompt_diagnostics()
    });
    assert_eq!(
        prompt.retry,
        MainWindowNativeLineagePromptCommandPresentation::Running
    );
    assert_eq!(
        prompt.recover_from_syndic,
        MainWindowNativeLineagePromptCommandPresentation::Disabled
    );
    assert_eq!(prompt.retry_label, "Retrying…");
    assert_eq!(prompt.recover_label, "Recover from Syndic history");
    assert_eq!(prompt.failed_command, None);
    assert!(cx.debug_bounds("native-lineage-recovery-prompt").is_some());
    assert!(cx.debug_bounds("native-lineage-retry-command").is_some());
    assert!(
        cx.debug_bounds("native-lineage-recover-from-syndic")
            .is_some()
    );
    assert!(control.set_status_for_test(
        key,
        NativeLineageRecoveryStatus::Failed {
            command: NativeLineageRecoveryCommand::Retry,
            recovery_available: true,
        }
    ));
    cx.executor().advance_clock(Duration::from_millis(100));
    drive(cx, 2);
    assert!(matches!(
        mount
            .read_with(cx, |mount, _| mount.native_lineage_recovery_snapshot())
            .unwrap()
            .status(),
        NativeLineageRecoveryStatus::Failed {
            command: NativeLineageRecoveryCommand::Retry,
            recovery_available: true,
        }
    ));
    let prompt = mount.read_with(cx, |mount, _| {
        mount.test_native_lineage_prompt_diagnostics()
    });
    assert_eq!(
        prompt.retry,
        MainWindowNativeLineagePromptCommandPresentation::Enabled
    );
    assert_eq!(
        prompt.recover_from_syndic,
        MainWindowNativeLineagePromptCommandPresentation::Enabled
    );
    assert_eq!(
        prompt.failed_command,
        Some(NativeLineageRecoveryCommand::Retry)
    );
    assert_eq!(prompt.retry_label, "Retry");
    assert_eq!(prompt.recover_label, "Recover from Syndic history");
    assert!(!prompt.local_failure_present);

    cx.simulate_keystrokes("space");
    assert_eq!(
        control.take_command_for_test(key),
        Some(NativeLineageRecoveryCommand::Retry)
    );
    assert!(matches!(
        control.snapshot_for_thread(thread).unwrap().status(),
        NativeLineageRecoveryStatus::Running {
            command: NativeLineageRecoveryCommand::Retry,
        }
    ));
    assert!(control.set_status_for_test(
        key,
        NativeLineageRecoveryStatus::Failed {
            command: NativeLineageRecoveryCommand::Retry,
            recovery_available: true,
        }
    ));
    let mut recover = None;
    for _ in 0..64 {
        cx.executor().advance_clock(Duration::from_millis(100));
        drive(cx, 2);
        recover = cx.debug_bounds("native-lineage-recover-from-syndic");
        if recover.is_some() {
            break;
        }
    }
    let recover = recover.unwrap();
    cx.simulate_click(recover.center(), gpui::Modifiers::none());
    assert_eq!(
        control.take_command_for_test(key),
        Some(NativeLineageRecoveryCommand::RecoverFromSyndic)
    );
    assert!(matches!(
        control.snapshot_for_thread(thread).unwrap().status(),
        NativeLineageRecoveryStatus::Running {
            command: NativeLineageRecoveryCommand::RecoverFromSyndic,
        }
    ));
    drive(cx, 2);
    let prompt = mount.read_with(cx, |mount, _| {
        mount.test_native_lineage_prompt_diagnostics()
    });
    assert_eq!(
        prompt.retry,
        MainWindowNativeLineagePromptCommandPresentation::Disabled
    );
    assert_eq!(
        prompt.recover_from_syndic,
        MainWindowNativeLineagePromptCommandPresentation::Running
    );
    assert_eq!(prompt.retry_label, "Retry");
    assert_eq!(prompt.recover_label, "Recovering…");
    assert_eq!(prompt.failed_command, None);
    assert!(control.set_status_for_test(
        key,
        NativeLineageRecoveryStatus::Leaving {
            pending_turn_continues: false,
        }
    ));
    wait_for_native_lineage_composer(cx, &mount, "exact restoration publication");
    let restored = mount
        .read_with(cx, |mount, _| mount.contribution())
        .unwrap();
    assert_ne!(restored, original);
    assert_eq!(service.selected_identity(), Some(selection));
    assert_eq!(control.snapshot_for_thread(thread), None);
    let restored_input = restored.read_with(cx, |composer, _| composer.gpui_input());
    restored_input.read_with(cx, |input, _| {
        let surface = input.surface().unwrap();
        assert_eq!(surface.binding(), original_seed.binding);
        assert_eq!(surface.caret(), original_seed.caret);
        assert_eq!(surface.selection(), original_seed.selection);
        assert_eq!(surface.scroll_position(), original_seed.scroll.position);
        assert_eq!(input.history_frontier(), original_seed.history.unwrap());
    });
    assert_eq!(
        cx.update(|window, app| window.focused(app).unwrap()),
        restored_input.read_with(cx, |input, app| input.focus_handle(app))
    );
}

#[gpui::test]
fn native_lineage_routes_cycle_without_duplicate_or_stale_gui_custody(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(ensure_text_input_bindings);
    let fixture = Fixture::new("native-lineage-cycles", 245);
    let (claim, target_claim) = fixture.claims();
    let window_id = fixture.window_id;
    let thread = fixture.selected_thread;
    let unrelated_thread = fixture.target_thread;
    let marker_authority = MainWindowComposerMarkerMetadataAuthority::new(fixture.assets());
    let marker_seals = fixture.marker_seals();
    let (_directory, store, storage) = fixture.into_store();
    let mut host = SyndicComposerHost::new(storage.clone());
    assert!(matches!(
        host.test_activate(
            &store,
            activation(thread, 246, 247, 1, 0),
            &CommandCancellation::new(),
        )
        .unwrap(),
        ComposerHostActivationOutcome::Activated { .. }
    ));
    let slot =
        MainWindowComposerSlot::new(window_id, claim, host, storage, marker_authority).unwrap();
    let service = Arc::new(MainWindowConversationComposerService::new(
        Arc::new(store),
        slot,
    ));
    let control = NativeLineageRecoveryControl::for_test(NonZeroUsize::new(1).unwrap());
    let first_key = control
        .install_route_for_test(
            thread,
            thread,
            BindingRevision::new(1).unwrap(),
            NativeLineageOperation::Resume,
            1,
            false,
        )
        .unwrap();
    assert!(
        control
            .install_route_for_test(
                unrelated_thread,
                unrelated_thread,
                BindingRevision::new(1).unwrap(),
                NativeLineageOperation::Resume,
                1,
                true,
            )
            .is_none()
    );
    let mounted_service = service.clone();
    let (root, cx) = cx.add_window_view(|window, cx| {
        let mount = cx.new(|mount_cx| {
            MainWindowConversationComposerMount::new(
                mounted_service,
                Box::new(|selection| {
                    MainWindowConversationComposerConfig::new(
                        selection,
                        widget_config(
                            selection.binding().range_binding(),
                            selection.binding().presentation_generation(),
                        ),
                    )
                    .map_err(|error| error.to_string())
                }),
                marker_seals,
                submission_source(),
                window,
                mount_cx,
            )
            .unwrap()
        });
        NativeLineageMountRoot { mount }
    });
    drive(cx, 16);

    let mount = root.read_with(cx, |root, _| root.mount.clone());
    let first = mount
        .read_with(cx, |mount, _| mount.contribution())
        .unwrap();
    let first_input = first.read_with(cx, |composer, _| composer.gpui_input());
    let prior_binding = first_input.read_with(cx, |input, _| input.surface().unwrap().binding());
    cx.update(|window, app| {
        first_input.update(app, |input, input_cx| {
            input.replace_and_mark_text_in_range(None, "x", None, window, input_cx)
        })
    });
    cx.update(|_, app| {
        mount.update(app, |mount, mount_cx| {
            mount.attach_native_lineage_recovery(control.clone(), mount_cx)
        })
    });
    assert_eq!(
        mount.read_with(cx, |mount, _| mount.contribution()),
        Some(first.clone())
    );
    assert_eq!(
        first_input.read_with(cx, |input, _| input.surface().unwrap().binding()),
        prior_binding
    );
    wait_for_native_lineage_prompt(cx, &mount, "cycle one prompt after pending edit");
    assert!(first_input.read_with(cx, |input, _| input.surface().is_none()));
    assert_eq!(
        service
            .selected_identity()
            .unwrap()
            .binding()
            .logical_extent()
            .logical_utf8_bytes(),
        1
    );
    let disabled_recovery = cx
        .debug_bounds("native-lineage-recover-from-syndic")
        .unwrap();
    cx.simulate_click(disabled_recovery.center(), gpui::Modifiers::none());
    cx.simulate_keystrokes("escape");
    drive(cx, 2);
    assert_eq!(control.take_command_for_test(first_key), None);
    assert!(
        mount
            .read_with(cx, |mount, _| mount.native_lineage_recovery_snapshot())
            .is_some()
    );

    control.cancel(first_key).unwrap();
    wait_for_native_lineage_composer(cx, &mount, "cycle one cancellation restoration");
    let after_cancel = mount
        .read_with(cx, |mount, _| mount.contribution())
        .unwrap();
    assert_ne!(after_cancel, first);
    assert!(!control.set_status_for_test(
        first_key,
        NativeLineageRecoveryStatus::Leaving {
            pending_turn_continues: false,
        }
    ));

    let pending = mount
        .update(cx, |mount, mount_cx| {
            mount.begin_activation(
                target_claim,
                activation(unrelated_thread, 248, 249, 2, 0),
                operation_id(250),
                &CommandCancellation::new(),
                mount_cx,
            )
        })
        .unwrap();
    let MainWindowComposerActivationAdvance::Ready(pending_receipt) = pending else {
        panic!("replacement target did not become pending: {pending:?}")
    };
    assert_eq!(service.pending_receipt(), Some(pending_receipt));
    let validation_stale_key = control
        .install_route_for_test(
            thread,
            thread,
            BindingRevision::new(1).unwrap(),
            NativeLineageOperation::Resume,
            2,
            true,
        )
        .unwrap();
    for _ in 0..64 {
        cx.executor().advance_clock(Duration::from_millis(100));
        drive(cx, 2);
        if mount
            .read_with(cx, |mount, _| mount.native_lineage_recovery_snapshot())
            .is_none()
        {
            break;
        }
    }
    assert_eq!(
        control.snapshot_for_thread(thread).unwrap().key(),
        validation_stale_key
    );
    assert!(
        mount
            .read_with(cx, |mount, _| mount.native_lineage_recovery_snapshot())
            .is_none()
    );
    assert_eq!(
        mount.read_with(cx, |mount, _| mount.contribution()),
        Some(after_cancel.clone())
    );
    assert!(control.set_status_for_test(
        validation_stale_key,
        NativeLineageRecoveryStatus::Leaving {
            pending_turn_continues: false,
        }
    ));
    mount
        .update(cx, |mount, mount_cx| {
            mount.retire_pending(pending_receipt, mount_cx)
        })
        .unwrap();
    assert_eq!(service.pending_receipt(), None);
    control.cancel(validation_stale_key).unwrap();

    let unrelated_key = control
        .install_route_for_test(
            unrelated_thread,
            unrelated_thread,
            BindingRevision::new(1).unwrap(),
            NativeLineageOperation::Resume,
            1,
            true,
        )
        .unwrap();
    control.cancel(unrelated_key).unwrap();
    let second_key = control
        .install_route_for_test(
            thread,
            thread,
            BindingRevision::new(1).unwrap(),
            NativeLineageOperation::Resume,
            3,
            true,
        )
        .unwrap();
    assert_ne!(second_key, first_key);
    wait_for_native_lineage_prompt(cx, &mount, "cycle two prompt after capacity reuse");
    control.close_for_test();
    wait_for_native_lineage_composer(cx, &mount, "cycle two control-close restoration");
    assert_eq!(control.snapshot_for_thread(thread), None);

    let disposal_control = NativeLineageRecoveryControl::for_test(NonZeroUsize::new(1).unwrap());
    let disposal_gate = service.test_gate_next_native_lineage_validation().unwrap();
    let disposal_key = disposal_control
        .install_route_for_test(
            thread,
            thread,
            BindingRevision::new(1).unwrap(),
            NativeLineageOperation::Resume,
            3,
            true,
        )
        .unwrap();
    cx.update(|_, app| {
        mount.update(app, |mount, mount_cx| {
            mount.attach_native_lineage_recovery(disposal_control.clone(), mount_cx)
        })
    });
    wait_for_native_lineage_prompt(cx, &mount, "disposal-cycle prompt");
    assert!(disposal_control.set_status_for_test(
        disposal_key,
        NativeLineageRecoveryStatus::Leaving {
            pending_turn_continues: false,
        }
    ));
    for _ in 0..64 {
        cx.executor().advance_clock(Duration::from_millis(100));
        drive(cx, 2);
        if disposal_gate.is_blocked() {
            break;
        }
    }
    assert!(disposal_gate.is_blocked());
    let disposal_start = cx.update(|window, app| {
        mount
            .update(app, |mount, mount_cx| {
                mount.begin_disposal(window, mount_cx)
            })
            .unwrap()
    });
    assert!(matches!(
        disposal_start,
        MainWindowConversationComposerMountFlushStart::Started(_)
    ));
    assert_eq!(
        disposal_control.snapshot_for_thread(thread).unwrap().key(),
        disposal_key
    );
    assert!(disposal_control.set_status_for_test(
        disposal_key,
        NativeLineageRecoveryStatus::Leaving {
            pending_turn_continues: false,
        }
    ));
    let mut disposed = false;
    for _ in 0..64 {
        drive(cx, 2);
        match cx
            .update(|window, app| {
                mount.update(app, |mount, mount_cx| {
                    mount.advance_disposal(window, mount_cx)
                })
            })
            .unwrap()
        {
            MainWindowConversationComposerMountDisposalAdvance::Disposed => {
                disposed = true;
                break;
            }
            MainWindowConversationComposerMountDisposalAdvance::Retained(_) => {}
            MainWindowConversationComposerMountDisposalAdvance::WidgetReleasePending(_) => {}
        }
    }
    assert!(
        disposed,
        "{:?}",
        mount.read_with(cx, |mount, _| mount
            .test_native_lineage_disposal_diagnostics())
    );
    assert_eq!(service.selected_identity(), None);
    assert_eq!(service.pending_receipt(), None);
    assert!(mount.read_with(cx, |mount, _| mount.contribution().is_none()));
    let diagnostics = service.test_native_lineage_cleanup_diagnostics();
    assert_eq!(diagnostics.sources, 1, "{diagnostics:?}");
    assert_eq!(diagnostics.validation_flights, 1, "{diagnostics:?}");
    assert_eq!(diagnostics.pending_flights, 1, "{diagnostics:?}");
    assert!(
        diagnostics.cleanup_active
            + diagnostics.cleanup_ready
            + diagnostics.cleanup_awaiting_acknowledgement
            > 0,
        "{diagnostics:?}"
    );
    disposal_gate.release();
    wait_for_native_lineage_cleanup_drain(cx, &service);
    assert_eq!(
        disposal_control.snapshot_for_thread(thread).unwrap().key(),
        disposal_key
    );
    disposal_control.cancel(disposal_key).unwrap();
}

#[gpui::test]
fn native_lineage_capacity_denial_stays_visible_and_rearms_after_exact_retirement(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(ensure_text_input_bindings);
    let fixture = Fixture::new("native-lineage-capacity", 71);
    let claim = fixture.claims().0;
    let window_id = fixture.window_id;
    let thread = fixture.selected_thread;
    let marker_authority = MainWindowComposerMarkerMetadataAuthority::new(fixture.assets());
    let marker_seals = fixture.marker_seals();
    let (_directory, store, storage) = fixture.into_store();
    let mut host = SyndicComposerHost::new(storage.clone());
    assert!(matches!(
        host.test_activate(
            &store,
            activation(thread, 72, 73, 1, 0),
            &CommandCancellation::new(),
        )
        .unwrap(),
        ComposerHostActivationOutcome::Activated { .. }
    ));
    let slot =
        MainWindowComposerSlot::new(window_id, claim, host, storage, marker_authority).unwrap();
    let service = Arc::new(MainWindowConversationComposerService::new(
        Arc::new(store),
        slot,
    ));

    let first_gate = service.test_gate_next_native_lineage_validation().unwrap();
    let first_control = NativeLineageRecoveryControl::for_test(NonZeroUsize::new(1).unwrap());
    let first_key = first_control
        .install_route_for_test(
            thread,
            thread,
            BindingRevision::new(1).unwrap(),
            NativeLineageOperation::Resume,
            1,
            true,
        )
        .unwrap();
    let first_service = service.clone();
    let first_marker_seals = marker_seals.clone();
    let (first_root, mut first_cx) = cx.add_window_view(|window, cx| {
        let mount = cx.new(|mount_cx| {
            MainWindowConversationComposerMount::new(
                first_service,
                Box::new(|selection| {
                    MainWindowConversationComposerConfig::new(
                        selection,
                        widget_config(
                            selection.binding().range_binding(),
                            selection.binding().presentation_generation(),
                        ),
                    )
                    .map_err(|error| error.to_string())
                }),
                first_marker_seals,
                submission_source(),
                window,
                mount_cx,
            )
            .unwrap()
        });
        NativeLineageMountRoot { mount }
    });
    drive(&mut first_cx, 16);
    let first_mount = first_root.read_with(first_cx, |root, _| root.mount.clone());
    first_cx.update(|_, app| {
        first_mount.update(app, |mount, mount_cx| {
            mount.attach_native_lineage_recovery(first_control.clone(), mount_cx)
        })
    });
    wait_for_native_lineage_prompt(&mut first_cx, &first_mount, "first retained source prompt");
    assert!(first_control.set_status_for_test(
        first_key,
        NativeLineageRecoveryStatus::Leaving {
            pending_turn_continues: false,
        }
    ));
    wait_for_native_lineage_gate(&mut first_cx, "first retained source", || {
        first_gate.is_blocked()
    });
    first_cx.update(|window, _| window.remove_window());
    drop(first_mount);
    drop(first_root);
    first_cx.cx.update(|_| ());
    first_cx.run_until_parked();
    let first_retained = service.test_native_lineage_cleanup_diagnostics();
    assert_eq!(first_retained.sources, 1, "{first_retained:?}");
    assert_eq!(first_retained.owner_active_sources, 0, "{first_retained:?}");

    let second_gate = service.test_gate_next_native_lineage_validation().unwrap();
    let second_control = NativeLineageRecoveryControl::for_test(NonZeroUsize::new(1).unwrap());
    let second_key = second_control
        .install_route_for_test(
            thread,
            thread,
            BindingRevision::new(1).unwrap(),
            NativeLineageOperation::Resume,
            2,
            true,
        )
        .unwrap();
    let second_service = service.clone();
    let second_marker_seals = marker_seals.clone();
    let (second_root, mut second_cx) = first_cx.cx.add_window_view(|window, cx| {
        let mount = cx.new(|mount_cx| {
            MainWindowConversationComposerMount::new(
                second_service,
                Box::new(|selection| {
                    MainWindowConversationComposerConfig::new(
                        selection,
                        widget_config(
                            selection.binding().range_binding(),
                            selection.binding().presentation_generation(),
                        ),
                    )
                    .map_err(|error| error.to_string())
                }),
                second_marker_seals,
                submission_source(),
                window,
                mount_cx,
            )
            .unwrap()
        });
        NativeLineageMountRoot { mount }
    });
    drive(&mut second_cx, 16);
    let second_mount = second_root.read_with(second_cx, |root, _| root.mount.clone());
    second_cx.update(|_, app| {
        second_mount.update(app, |mount, mount_cx| {
            mount.attach_native_lineage_recovery(second_control.clone(), mount_cx)
        })
    });
    wait_for_native_lineage_prompt(
        &mut second_cx,
        &second_mount,
        "second retained source prompt",
    );
    assert!(second_control.set_status_for_test(
        second_key,
        NativeLineageRecoveryStatus::Leaving {
            pending_turn_continues: false,
        }
    ));
    wait_for_native_lineage_gate(&mut second_cx, "second retained source", || {
        second_gate.is_blocked()
    });
    second_cx.update(|window, _| window.remove_window());
    drop(second_mount);
    drop(second_root);
    second_cx.cx.update(|_| ());
    second_cx.run_until_parked();
    let saturated = service.test_native_lineage_cleanup_diagnostics();
    assert_eq!(saturated.sources, 2, "{saturated:?}");
    assert_eq!(saturated.owner_active_sources, 0, "{saturated:?}");
    assert_eq!(saturated.pending_flights, 2, "{saturated:?}");

    let third_control = NativeLineageRecoveryControl::for_test(NonZeroUsize::new(1).unwrap());
    let third_key = third_control
        .install_route_for_test(
            thread,
            thread,
            BindingRevision::new(1).unwrap(),
            NativeLineageOperation::Resume,
            3,
            true,
        )
        .unwrap();
    let third_service = service.clone();
    let (third_root, mut third_cx) = second_cx.cx.add_window_view(|window, cx| {
        let mount = cx.new(|mount_cx| {
            MainWindowConversationComposerMount::new(
                third_service,
                Box::new(|selection| {
                    MainWindowConversationComposerConfig::new(
                        selection,
                        widget_config(
                            selection.binding().range_binding(),
                            selection.binding().presentation_generation(),
                        ),
                    )
                    .map_err(|error| error.to_string())
                }),
                marker_seals,
                submission_source(),
                window,
                mount_cx,
            )
            .unwrap()
        });
        NativeLineageMountRoot { mount }
    });
    drive(&mut third_cx, 16);
    let third_mount = third_root.read_with(third_cx, |root, _| root.mount.clone());
    third_cx.update(|_, app| {
        third_mount.update(app, |mount, mount_cx| {
            mount.attach_native_lineage_recovery(third_control.clone(), mount_cx)
        })
    });
    wait_for_native_lineage_prompt(&mut third_cx, &third_mount, "capacity-denied prompt");
    assert!(third_control.set_status_for_test(
        third_key,
        NativeLineageRecoveryStatus::Leaving {
            pending_turn_continues: false,
        }
    ));
    for _ in 0..64 {
        third_cx
            .executor()
            .advance_clock(Duration::from_millis(100));
        drive(&mut third_cx, 2);
        if third_mount.read_with(third_cx, |mount, _| {
            mount
                .test_native_lineage_mount_diagnostics()
                .capacity_blocked
        }) {
            break;
        }
    }
    let blocked = third_mount.read_with(third_cx, |mount, _| {
        mount.test_native_lineage_mount_diagnostics()
    });
    assert!(blocked.prompt_published, "{blocked:?}");
    assert!(blocked.failure_present, "{blocked:?}");
    assert!(blocked.capacity_blocked, "{blocked:?}");
    assert!(third_mount.read_with(third_cx, |mount, _| mount.contribution().is_none()));
    assert!(
        third_cx
            .debug_bounds("native-lineage-recovery-prompt")
            .is_some()
    );
    let denied_epoch = service
        .test_native_lineage_cleanup_witness()
        .snapshot()
        .capacity_epoch;

    first_gate.release();
    for _ in 0..64 {
        third_cx
            .executor()
            .advance_clock(Duration::from_millis(100));
        drive(&mut third_cx, 2);
        if third_mount.read_with(third_cx, |mount, _| {
            mount.contribution().is_some() && mount.native_lineage_recovery_snapshot().is_none()
        }) {
            break;
        }
    }
    assert!(
        third_mount.read_with(third_cx, |mount, _| {
            mount.contribution().is_some() && mount.native_lineage_recovery_snapshot().is_none()
        }),
        "capacity retirement did not rearm exact restoration: mount={:?}, cleanup={:?}, witness={:?}",
        third_mount.read_with(third_cx, |mount, _| mount
            .test_native_lineage_mount_diagnostics()),
        service.test_native_lineage_cleanup_diagnostics(),
        service.test_native_lineage_cleanup_witness().snapshot(),
    );
    let rearmed = third_mount.read_with(third_cx, |mount, _| {
        mount.test_native_lineage_mount_diagnostics()
    });
    assert!(!rearmed.capacity_blocked, "{rearmed:?}");
    assert!(!rearmed.failure_present, "{rearmed:?}");
    assert_eq!(third_control.snapshot_for_thread(thread), None);
    assert!(
        service
            .test_native_lineage_cleanup_witness()
            .snapshot()
            .capacity_epoch
            > denied_epoch
    );

    second_gate.release();
    wait_for_native_lineage_cleanup_drain(&mut third_cx, &service);
}

#[gpui::test]
fn native_lineage_late_flights_drain_after_route_cancellation(cx: &mut gpui::TestAppContext) {
    cx.update(ensure_text_input_bindings);
    let fixture = Fixture::new("native-lineage-late-flights", 90);
    let claim = fixture.claims().0;
    let window_id = fixture.window_id;
    let thread = fixture.selected_thread;
    let marker_asset = publish_image_asset(&fixture, b"native-lineage-object-page");
    let marker_authority = MainWindowComposerMarkerMetadataAuthority::new(fixture.assets());
    let marker_seals = fixture.marker_seals();
    let (_directory, store, storage) = fixture.into_store();
    let mut host = SyndicComposerHost::new(storage.clone());
    assert!(matches!(
        host.test_activate(
            &store,
            ComposerHostActivationRequest::new(
                thread,
                syndic_storage::DraftEditorCandidateSessionIdV1::from_bytes([91; 16]),
                operation_id(92),
                NonZeroU64::new(1).unwrap(),
                None,
                Box::new([]),
            ),
            &CommandCancellation::new(),
        )
        .unwrap(),
        ComposerHostActivationOutcome::Activated { .. }
    ));
    let binding = host.binding().unwrap();
    let binding = composer_support::commit_text(&mut host, &store, binding, 93, 0, 0, "a", 1, 1);
    let binding = native_lineage_support::insert_published_marker_with_readiness(
        &mut host,
        &store,
        &storage,
        binding,
        94,
        marker_asset,
    );
    host.dispose_composer_service(&store).unwrap();
    let mut host = SyndicComposerHost::new(storage.clone());
    assert!(matches!(
        host.test_activate(
            &store,
            activation(thread, 91, 92, 1, 1),
            &CommandCancellation::new(),
        )
        .unwrap(),
        ComposerHostActivationOutcome::Activated {
            binding: rebound,
            ..
        } if rebound.root() == binding.root()
    ));
    assert_eq!(binding.root().summary().marker_count(), 1);
    let slot =
        MainWindowComposerSlot::new(window_id, claim, host, storage, marker_authority).unwrap();
    let service = Arc::new(MainWindowConversationComposerService::new(
        Arc::new(store),
        slot,
    ));
    let mounted_service = service.clone();
    let (root, cx) = cx.add_window_view(|window, cx| {
        let mount = cx.new(|mount_cx| {
            MainWindowConversationComposerMount::new(
                mounted_service,
                Box::new(|selection| {
                    MainWindowConversationComposerConfig::new(
                        selection,
                        widget_config(
                            selection.binding().range_binding(),
                            selection.binding().presentation_generation(),
                        ),
                    )
                    .map_err(|error| error.to_string())
                }),
                marker_seals,
                submission_source(),
                window,
                mount_cx,
            )
            .unwrap()
        });
        NativeLineageMountRoot { mount }
    });
    drive(cx, 16);
    let mount = root.read_with(cx, |root, _| root.mount.clone());

    prove_native_lineage_late_flight_cleanup(
        cx,
        &mount,
        &service,
        thread,
        NativeLineageLateFlight::Validation,
        1,
    );
    prove_native_lineage_late_flight_cleanup(
        cx,
        &mount,
        &service,
        thread,
        NativeLineageLateFlight::Page,
        2,
    );

    assert_eq!(
        service
            .selected_identity()
            .unwrap()
            .binding()
            .root()
            .summary()
            .marker_count(),
        1
    );
    prove_native_lineage_late_flight_cleanup(
        cx,
        &mount,
        &service,
        thread,
        NativeLineageLateFlight::ObjectPage,
        3,
    );
    prove_native_lineage_host_failure_cleanup(
        cx,
        &mount,
        &service,
        thread,
        NativeLineageLateFlight::Validation,
        4,
    );
    prove_native_lineage_host_failure_cleanup(
        cx,
        &mount,
        &service,
        thread,
        NativeLineageLateFlight::Page,
        5,
    );
}

#[gpui::test]
fn native_lineage_late_settlement_drains_after_actual_mount_and_service_drop(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(ensure_text_input_bindings);
    let fixture = Fixture::new("native-lineage-drop-cleanup", 111);
    let claim = fixture.claims().0;
    let window_id = fixture.window_id;
    let thread = fixture.selected_thread;
    let marker_authority = MainWindowComposerMarkerMetadataAuthority::new(fixture.assets());
    let marker_seals = fixture.marker_seals();
    let (_directory, store, storage) = fixture.into_store();
    let mut host = SyndicComposerHost::new(storage.clone());
    assert!(matches!(
        host.test_activate(
            &store,
            activation(thread, 112, 113, 1, 0),
            &CommandCancellation::new(),
        )
        .unwrap(),
        ComposerHostActivationOutcome::Activated { .. }
    ));
    let slot =
        MainWindowComposerSlot::new(window_id, claim, host, storage, marker_authority).unwrap();
    let service = Arc::new(MainWindowConversationComposerService::new(
        Arc::new(store),
        slot,
    ));
    let gate = service.test_gate_next_native_lineage_validation().unwrap();
    let witness = service.test_native_lineage_cleanup_witness();
    let weak_service = Arc::downgrade(&service);
    let control = NativeLineageRecoveryControl::for_test(NonZeroUsize::new(1).unwrap());
    let key = control
        .install_route_for_test(
            thread,
            thread,
            BindingRevision::new(1).unwrap(),
            NativeLineageOperation::Resume,
            1,
            true,
        )
        .unwrap();
    let mounted_service = service.clone();
    let (root, mut cx) = cx.add_window_view(|window, cx| {
        let mount = cx.new(|mount_cx| {
            MainWindowConversationComposerMount::new(
                mounted_service,
                Box::new(|selection| {
                    MainWindowConversationComposerConfig::new(
                        selection,
                        widget_config(
                            selection.binding().range_binding(),
                            selection.binding().presentation_generation(),
                        ),
                    )
                    .map_err(|error| error.to_string())
                }),
                marker_seals,
                submission_source(),
                window,
                mount_cx,
            )
            .unwrap()
        });
        NativeLineageMountRoot { mount }
    });
    drive(&mut cx, 16);
    let mount = root.read_with(cx, |root, _| root.mount.clone());
    cx.update(|_, app| {
        mount.update(app, |mount, mount_cx| {
            mount.attach_native_lineage_recovery(control.clone(), mount_cx)
        })
    });
    wait_for_native_lineage_prompt(&mut cx, &mount, "drop cleanup prompt");
    assert!(control.set_status_for_test(
        key,
        NativeLineageRecoveryStatus::Leaving {
            pending_turn_continues: false,
        }
    ));
    wait_for_native_lineage_gate(&mut cx, "drop cleanup admitted token", || gate.is_blocked());
    let retained = witness.snapshot();
    assert_eq!(retained.diagnostics.sources, 1, "{retained:?}");
    assert_eq!(retained.diagnostics.owner_active_sources, 1, "{retained:?}");
    assert_eq!(retained.diagnostics.pending_flights, 1, "{retained:?}");
    assert!(retained.driver_alive, "{retained:?}");

    cx.update(|window, _| window.remove_window());
    drop(mount);
    drop(root);
    cx.cx.update(|_| ());
    cx.run_until_parked();
    drop(service);
    gate.release();
    for _ in 0..64 {
        cx.executor().advance_clock(Duration::from_millis(100));
        cx.run_until_parked();
        let snapshot = witness.snapshot();
        if snapshot.diagnostics.sources == 0 && !snapshot.driver_alive {
            break;
        }
    }
    let drained = witness.snapshot();
    assert_eq!(drained.diagnostics.sources, 0, "{drained:?}");
    assert_eq!(drained.diagnostics.owner_active_sources, 0, "{drained:?}");
    assert_eq!(drained.diagnostics.pending_flights, 0, "{drained:?}");
    assert_eq!(drained.diagnostics.terminal_flights, 0, "{drained:?}");
    assert_eq!(drained.diagnostics.delivered_flights, 0, "{drained:?}");
    assert_eq!(drained.diagnostics.cleanup_active, 0, "{drained:?}");
    assert_eq!(drained.diagnostics.cleanup_ready, 0, "{drained:?}");
    assert_eq!(
        drained.diagnostics.cleanup_awaiting_acknowledgement, 0,
        "{drained:?}"
    );
    assert!(!drained.driver_alive, "{drained:?}");
    assert!(weak_service.upgrade().is_none());
    assert_eq!(control.snapshot_for_thread(thread).unwrap().key(), key);
    control.cancel(key).unwrap();
}

#[gpui::test]
fn native_lineage_disposal_reconciliation_drains_after_actual_mount_and_service_drop(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(ensure_text_input_bindings);
    let fixture = Fixture::new("native-lineage-disposal-drop", 115);
    let claim = fixture.claims().0;
    let window_id = fixture.window_id;
    let thread = fixture.selected_thread;
    let durable = fixture.current_draft(thread);
    let faults = fixture.faults.clone();
    let marker_authority = MainWindowComposerMarkerMetadataAuthority::new(fixture.assets());
    let marker_seals = fixture.marker_seals();
    let (_directory, store, storage) = fixture.into_store();
    let store = Arc::new(store);
    let mut host = SyndicComposerHost::new(storage.clone());
    assert!(matches!(
        host.test_activate(
            &store,
            activation(thread, 116, 117, 1, 0),
            &CommandCancellation::new(),
        )
        .unwrap(),
        ComposerHostActivationOutcome::Activated { .. }
    ));
    let resident = host.binding().unwrap();
    host.test_arm_publication_before_execute_fault(move |_, _| {
        faults.fail_next(beryl_home_store::test_faults::FaultPoint::AfterCommitBeforePersist);
    });
    let slot =
        MainWindowComposerSlot::new(window_id, claim, host, storage.clone(), marker_authority)
            .unwrap();
    let service = Arc::new(MainWindowConversationComposerService::new(
        store.clone(),
        slot,
    ));
    let weak_service = Arc::downgrade(&service);
    let control = NativeLineageRecoveryControl::for_test(NonZeroUsize::new(1).unwrap());
    control
        .install_route_for_test(
            thread,
            thread,
            BindingRevision::new(1).unwrap(),
            NativeLineageOperation::Resume,
            1,
            true,
        )
        .unwrap();
    let mounted_service = service.clone();
    let (root, mut cx) = cx.add_window_view(|window, cx| {
        let mount = cx.new(|mount_cx| {
            MainWindowConversationComposerMount::new(
                mounted_service,
                Box::new(|selection| {
                    MainWindowConversationComposerConfig::new(
                        selection,
                        widget_config(
                            selection.binding().range_binding(),
                            selection.binding().presentation_generation(),
                        ),
                    )
                    .map_err(|error| error.to_string())
                }),
                marker_seals,
                submission_source(),
                window,
                mount_cx,
            )
            .unwrap()
        });
        NativeLineageMountRoot { mount }
    });
    drive(&mut cx, 16);
    let mount = root.read_with(cx, |root, _| root.mount.clone());
    cx.update(|_, app| {
        mount.update(app, |mount, mount_cx| {
            mount.attach_native_lineage_recovery(control.clone(), mount_cx)
        })
    });
    wait_for_native_lineage_prompt(&mut cx, &mount, "disposal reconciliation drop prompt");
    assert!(matches!(
        cx.update(|window, app| mount.update(app, |mount, mount_cx| {
            mount.begin_disposal(window, mount_cx)
        }))
        .unwrap(),
        MainWindowConversationComposerMountFlushStart::Started(_)
    ));
    for _ in 0..64 {
        cx.update(|window, app| {
            mount.update(app, |mount, mount_cx| {
                mount.advance_disposal(window, mount_cx)
            })
        })
        .unwrap();
        drive(&mut cx, 2);
        if mount.read_with(cx, |mount, _| {
            mount
                .test_native_lineage_disposal_diagnostics()
                .mount_last_disposal_advance
        }) == Some(
            beryl_app::main_window::MainWindowComposerDisposalAdvance::ReconciliationPending,
        ) {
            break;
        }
    }
    let pending = mount.read_with(cx, |mount, _| {
        mount.test_native_lineage_disposal_diagnostics()
    });
    assert_eq!(
        pending.mount_last_disposal_advance,
        Some(beryl_app::main_window::MainWindowComposerDisposalAdvance::ReconciliationPending)
    );
    assert!(pending.mount_release_present, "{pending:?}");
    assert!(!pending.mount_contribution_present, "{pending:?}");
    assert!(pending.mount_flush_ticket_present, "{pending:?}");
    assert!(pending.slot_disposal_flushing, "{pending:?}");
    assert_eq!(pending.host_barriers, 1, "{pending:?}");

    cx.update(|window, _| window.remove_window());
    drop(mount);
    drop(root);
    cx.cx.update(|_| ());
    cx.run_until_parked();
    drop(service);
    for _ in 0..64 {
        cx.executor().advance_clock(Duration::from_millis(100));
        cx.run_until_parked();
        if weak_service.upgrade().is_none() {
            break;
        }
    }
    assert!(weak_service.upgrade().is_none());
    let syndic_storage::DraftEditorCandidateSessionReadOutcomeV1::Disposed(terminal) = storage
        .draft_editor_candidate_session(
            &store,
            resident.candidate().draft_id(),
            resident.candidate().session_id(),
        )
        .unwrap()
    else {
        panic!("detached disposal did not retain the exact terminal session");
    };
    assert_eq!(terminal.newest_root(), resident.root());
    assert_eq!(terminal.newest_history(), durable.draft().history());
    assert!(terminal.disposal_operation_id().is_some());
    assert_eq!(
        storage
            .current_draft(
                &store,
                thread,
                syndic_storage::SyndicPointReadLimit::new(65_536).unwrap()
            )
            .unwrap()
            .unwrap(),
        durable
    );
}

#[gpui::test]
fn native_lineage_prompt_survives_disposal_admission_and_advance_failures(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(ensure_text_input_bindings);
    let fixture = Fixture::new("native-lineage-disposal-failures", 121);
    let claim = fixture.claims().0;
    let window_id = fixture.window_id;
    let thread = fixture.selected_thread;
    let marker_authority = MainWindowComposerMarkerMetadataAuthority::new(fixture.assets());
    let marker_seals = fixture.marker_seals();
    let (_directory, store, storage) = fixture.into_store();
    let mut host = SyndicComposerHost::new(storage.clone());
    assert!(matches!(
        host.test_activate(
            &store,
            activation(thread, 122, 123, 1, 0),
            &CommandCancellation::new(),
        )
        .unwrap(),
        ComposerHostActivationOutcome::Activated { .. }
    ));
    let slot =
        MainWindowComposerSlot::new(window_id, claim, host, storage, marker_authority).unwrap();
    let service = Arc::new(MainWindowConversationComposerService::new(
        Arc::new(store),
        slot,
    ));
    let control = NativeLineageRecoveryControl::for_test(NonZeroUsize::new(1).unwrap());
    let route_key = control
        .install_route_for_test(
            thread,
            thread,
            BindingRevision::new(1).unwrap(),
            NativeLineageOperation::Resume,
            1,
            true,
        )
        .unwrap();
    let mounted_service = service.clone();
    let (root, mut cx) = cx.add_window_view(|window, cx| {
        let mount = cx.new(|mount_cx| {
            MainWindowConversationComposerMount::new(
                mounted_service,
                Box::new(|selection| {
                    MainWindowConversationComposerConfig::new(
                        selection,
                        widget_config(
                            selection.binding().range_binding(),
                            selection.binding().presentation_generation(),
                        ),
                    )
                    .map_err(|error| error.to_string())
                }),
                marker_seals,
                submission_source(),
                window,
                mount_cx,
            )
            .unwrap()
        });
        NativeLineageMountRoot { mount }
    });
    drive(&mut cx, 16);
    let mount = root.read_with(cx, |root, _| root.mount.clone());
    cx.update(|_, app| {
        mount.update(app, |mount, mount_cx| {
            mount.attach_native_lineage_recovery(control.clone(), mount_cx)
        })
    });
    wait_for_native_lineage_prompt(&mut cx, &mount, "disposal failure prompt");
    let preserved_selection = service.selected_identity().unwrap();

    service.test_fail_next_native_lineage_disposal_begin();
    let admission_error = cx.update(|window, app| {
        mount.update(app, |mount, mount_cx| {
            mount.begin_disposal(window, mount_cx)
        })
    });
    assert!(admission_error.is_err());
    drive(&mut cx, 2);
    assert_native_lineage_disposal_failure_preserved(
        &mut cx,
        &mount,
        &service,
        &control,
        route_key,
        preserved_selection,
        "admission failure",
    );

    service.test_fail_next_native_lineage_disposal_advance();
    let admitted = cx
        .update(|window, app| {
            mount.update(app, |mount, mount_cx| {
                mount.begin_disposal(window, mount_cx)
            })
        })
        .unwrap();
    assert!(matches!(
        admitted,
        MainWindowConversationComposerMountFlushStart::Started(_)
    ));
    let advance_error = cx.update(|window, app| {
        mount.update(app, |mount, mount_cx| {
            mount.advance_disposal(window, mount_cx)
        })
    });
    assert!(matches!(
        advance_error.unwrap(),
        MainWindowConversationComposerMountDisposalAdvance::Retained(_)
    ));
    for _ in 0..64 {
        drive(&mut cx, 1);
        if mount.read_with(cx, |mount, _| {
            mount
                .test_native_lineage_mount_diagnostics()
                .failure_present
        }) {
            break;
        }
    }
    assert_native_lineage_disposal_failure_preserved(
        &mut cx,
        &mount,
        &service,
        &control,
        route_key,
        preserved_selection,
        "post-admission advance failure",
    );
}

#[gpui::test]
fn native_lineage_pending_turn_leaves_without_remounting_or_focusing_a_composer(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(ensure_text_input_bindings);
    let fixture = Fixture::new("native-lineage-pending-focus", 252);
    let claim = fixture.claims().0;
    let window_id = fixture.window_id;
    let thread = fixture.selected_thread;
    let marker_authority = MainWindowComposerMarkerMetadataAuthority::new(fixture.assets());
    let marker_seals = fixture.marker_seals();
    let (_directory, store, storage) = fixture.into_store();
    let mut host = SyndicComposerHost::new(storage.clone());
    assert!(matches!(
        host.test_activate(
            &store,
            activation(thread, 253, 254, 1, 0),
            &CommandCancellation::new(),
        )
        .unwrap(),
        ComposerHostActivationOutcome::Activated { .. }
    ));
    let slot =
        MainWindowComposerSlot::new(window_id, claim, host, storage, marker_authority).unwrap();
    let service = Arc::new(MainWindowConversationComposerService::new(
        Arc::new(store),
        slot,
    ));
    let control = NativeLineageRecoveryControl::for_test(NonZeroUsize::new(1).unwrap());
    let key = control
        .install_route_for_test(
            thread,
            thread,
            BindingRevision::new(1).unwrap(),
            NativeLineageOperation::Resume,
            1,
            true,
        )
        .unwrap();
    let mounted_service = service.clone();
    let (root, cx) = cx.add_window_view(|window, cx| {
        let pending_focus = cx.focus_handle();
        let mount = cx.new(|mount_cx| {
            let mut mount = MainWindowConversationComposerMount::new(
                mounted_service,
                Box::new(|selection| {
                    MainWindowConversationComposerConfig::new(
                        selection,
                        widget_config(
                            selection.binding().range_binding(),
                            selection.binding().presentation_generation(),
                        ),
                    )
                    .map_err(|error| error.to_string())
                }),
                marker_seals,
                submission_source(),
                window,
                mount_cx,
            )
            .unwrap();
            mount.set_native_lineage_pending_focus(pending_focus.clone(), mount_cx);
            mount
        });
        PendingNativeLineageMountRoot {
            mount,
            pending_focus,
        }
    });
    drive(cx, 16);

    let (mount, pending_focus) = root.read_with(cx, |root, _| {
        (root.mount.clone(), root.pending_focus.clone())
    });
    let original = mount
        .read_with(cx, |mount, _| mount.contribution())
        .unwrap();
    let original_input = original.read_with(cx, |composer, _| composer.gpui_input());
    cx.update(|_, app| {
        mount.update(app, |mount, mount_cx| {
            mount.attach_native_lineage_recovery(control.clone(), mount_cx)
        })
    });
    wait_for_native_lineage_prompt(cx, &mount, "pending-turn prompt");
    assert!(control.set_status_for_test(
        key,
        NativeLineageRecoveryStatus::Leaving {
            pending_turn_continues: true,
        }
    ));
    for _ in 0..64 {
        cx.executor().advance_clock(Duration::from_millis(100));
        drive(cx, 2);
        if mount.read_with(cx, |mount, _| {
            mount.contribution().is_none() && mount.native_lineage_recovery_snapshot().is_none()
        }) {
            break;
        }
    }
    assert!(mount.read_with(cx, |mount, _| mount.contribution().is_none()));
    assert!(
        mount
            .read_with(cx, |mount, _| mount.native_lineage_recovery_snapshot())
            .is_none()
    );
    assert!(original_input.read_with(cx, |input, _| input.surface().is_none()));
    assert_eq!(control.snapshot_for_thread(thread), None);
    assert_eq!(
        cx.update(|window, app| window.focused(app).unwrap()),
        pending_focus
    );
}

#[gpui::test]
fn mounted_commands_are_selection_qualified_and_shift_enter_stays_a_newline(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(ensure_text_input_bindings);
    let fixture = Fixture::new("mounted-commands", 21);
    let claim = fixture.claims().0;
    let window_id = fixture.window_id;
    let thread = fixture.selected_thread;
    let marker_authority = MainWindowComposerMarkerMetadataAuthority::new(fixture.assets());
    let marker_seals = fixture.marker_seals();
    let (_directory, store, storage) = fixture.into_store();
    let mut host = SyndicComposerHost::new(storage.clone());
    assert!(matches!(
        host.test_activate(
            &store,
            activation(thread, 22, 23, 1, 0),
            &CommandCancellation::new(),
        )
        .unwrap(),
        ComposerHostActivationOutcome::Activated { .. }
    ));
    let slot =
        MainWindowComposerSlot::new(window_id, claim, host, storage, marker_authority).unwrap();
    let service = Arc::new(MainWindowConversationComposerService::new(
        Arc::new(store),
        slot,
    ));
    let mounted_service = service.clone();
    let (root, cx) = cx.add_window_view(|window, cx| {
        let mount = cx.new(|mount_cx| {
            MainWindowConversationComposerMount::new(
                mounted_service,
                Box::new(|selection| {
                    MainWindowConversationComposerConfig::new(
                        selection,
                        widget_config(
                            selection.binding().range_binding(),
                            selection.binding().presentation_generation(),
                        ),
                    )
                    .map_err(|error| error.to_string())
                }),
                marker_seals,
                submission_source(),
                window,
                mount_cx,
            )
            .unwrap()
        });
        MountRoot { mount }
    });
    drive(cx, 16);

    let mount = root.read_with(cx, |root, _| root.mount.clone());
    let selection = service.selected_identity().unwrap();
    let contribution = mount
        .read_with(cx, |mount, _| mount.contribution())
        .unwrap();
    let mount_diagnostics = mount
        .read_with(cx, |mount, app| mount.realization_diagnostics(app))
        .unwrap();
    let contribution_diagnostics =
        contribution.read_with(cx, |composer, app| composer.realization_diagnostics(app));
    assert_eq!(mount_diagnostics, contribution_diagnostics);
    assert_eq!(mount_diagnostics.max_realized_block_extent, px(80.));
    let events = Arc::new(Mutex::new(Vec::new()));
    let observed = events.clone();
    let _subscription = cx.update(|window, app| {
        window.subscribe(&mount, app, move |_, event, _, _| {
            observed.lock().unwrap().push(*event);
        })
    });
    let input = contribution.read_with(cx, |composer, _| composer.gpui_input());
    cx.update(|window, app| input.update(app, |input, _| input.focus(window)));

    cx.simulate_keystrokes("shift-enter");
    drive(cx, 4);
    assert_eq!(events.lock().unwrap().len(), 0);
    let pasted_selection = service.selected_identity().unwrap();
    assert_ne!(pasted_selection, selection);

    cx.simulate_keystrokes("ctrl-v");
    drive(cx, 4);
    assert_eq!(
        events.lock().unwrap().as_slice(),
        &[
            MainWindowConversationComposerMountEvent::RichPastePropagated {
                selection: pasted_selection,
            }
        ]
    );

    cx.simulate_keystrokes("enter");
    drive(cx, 4);
    assert_eq!(
        events.lock().unwrap().as_slice(),
        &[
            MainWindowConversationComposerMountEvent::RichPastePropagated {
                selection: pasted_selection,
            }
        ]
    );
}

#[gpui::test]
fn mount_retains_one_coherent_contribution_until_exact_publish_and_disposal(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(ensure_text_input_bindings);
    let fixture = Fixture::new("composer-mount", 31);
    let (selected_claim, target_claim) = fixture.claims();
    let window_id = fixture.window_id;
    let selected_thread = fixture.selected_thread;
    let target_thread = fixture.target_thread;
    let marker_authority = MainWindowComposerMarkerMetadataAuthority::new(fixture.assets());
    let assets = fixture.assets();
    let marker_seals = fixture.marker_seals();
    let image_asset = publish_image_asset(&fixture, b"mounted-marker");
    let (_directory, store, storage) = fixture.into_store();
    let mut selected_host = SyndicComposerHost::new(storage.clone());
    assert!(matches!(
        selected_host
            .test_activate(
                &store,
                activation(selected_thread, 32, 33, 1, 0),
                &CommandCancellation::new(),
            )
            .unwrap(),
        ComposerHostActivationOutcome::Activated { .. }
    ));
    let slot = MainWindowComposerSlot::new(
        window_id,
        selected_claim,
        selected_host,
        storage,
        marker_authority,
    )
    .unwrap();
    let mut initial_selection = slot.selected_identity().unwrap();
    let service = Arc::new(MainWindowConversationComposerService::new(
        Arc::new(store),
        slot,
    ));
    let mounted_service = service.clone();
    let (root, cx) = cx.add_window_view(|window, cx| {
        let mount = cx.new(|mount_cx| {
            MainWindowConversationComposerMount::new(
                mounted_service,
                Box::new(|selection| {
                    MainWindowConversationComposerConfig::new(
                        selection,
                        widget_config(
                            selection.binding().range_binding(),
                            selection.binding().presentation_generation(),
                        ),
                    )
                    .map_err(|error| error.to_string())
                }),
                marker_seals.clone(),
                submission_source(),
                window,
                mount_cx,
            )
            .unwrap()
        });
        MountRoot { mount }
    });
    drive(cx, 16);

    let mount = root.read_with(cx, |root, _| root.mount.clone());
    let initial = mount
        .read_with(cx, |mount, _| mount.contribution())
        .unwrap();
    let initial_entity = initial.entity_id();
    assert_eq!(
        initial.read_with(cx, |composer, _| composer.selection_identity()),
        initial_selection
    );
    initial.read_with(cx, |composer, _| {
        composer
            .gpui_input()
            .read_with(cx, |input, _| assert!(input.is_quiescent()))
    });
    assert!(
        cx.update(|window, app| initial.update(app, |composer, composer_cx| {
            composer.release_widget(window, composer_cx)
        }))
        .is_err()
    );
    let initial_input = initial.read_with(cx, |composer, _| composer.gpui_input());
    cx.update(|window, app| initial_input.update(app, |input, _| input.focus(window)));
    cx.update(|window, app| {
        initial_input.update(app, |input, input_cx| {
            input.replace_and_mark_text_in_range(None, "a", None, window, input_cx)
        })
    });
    drive(cx, 32);
    initial_selection = service.selected_identity().unwrap();
    assert_eq!(
        initial.read_with(cx, |composer, _| composer.selection_identity()),
        initial_selection
    );
    initial
        .update(cx, |composer, composer_cx| {
            composer.insert_authenticated_image_marker(
                ComposerHostImageMarkerMetadata::new(
                    InlineObjectId::new(0x181),
                    ImageLabelOrdinal::new(1).unwrap(),
                    image_asset,
                ),
                InlineObjectOrder::new(1),
                composer_cx,
            )
        })
        .unwrap();
    drive(cx, 48);
    initial_selection = service.selected_identity().unwrap();
    assert_eq!(
        initial_selection.binding().root().summary().marker_count(),
        1
    );
    assert_eq!(
        initial.read_with(cx, |composer, _| composer.selection_identity()),
        initial_selection
    );
    let first_timer_generation = mount.read_with(cx, |mount, _| {
        let diagnostics = mount.autosave_diagnostics();
        assert_eq!(
            diagnostics.phase(),
            MainWindowConversationComposerAutosavePhase::Waiting
        );
        assert_eq!(diagnostics.retained_tasks(), 1);
        assert_eq!(diagnostics.last_error(), None);
        diagnostics.generation()
    });
    cx.update(|window, app| {
        mount.update(app, |mount, mount_cx| {
            mount.publish_autosave_interval(
                1,
                ComposerHostAutosaveInterval::new(5).unwrap(),
                window,
                mount_cx,
            )
        })
    })
    .unwrap();
    assert!(
        mount.read_with(cx, |mount, _| mount.autosave_diagnostics().generation())
            > first_timer_generation
    );
    cx.executor().advance_clock(Duration::from_secs(5));
    for _ in 0..64 {
        drive(cx, 1);
        if mount.read_with(cx, |mount, _| {
            mount.autosave_diagnostics().phase()
                == MainWindowConversationComposerAutosavePhase::Idle
        }) {
            break;
        }
    }
    let autosave = mount.read_with(cx, |mount, _| mount.autosave_diagnostics());
    assert_eq!(
        autosave.phase(),
        MainWindowConversationComposerAutosavePhase::Idle
    );
    assert_eq!(autosave.last_error(), None);
    initial_selection = service.selected_identity().unwrap();
    assert_eq!(
        initial.read_with(cx, |composer, _| composer.selection_identity()),
        initial_selection
    );

    let cancelled = CommandCancellation::new();
    cancelled.cancel();
    assert!(matches!(
        mount
            .update(cx, |mount, mount_cx| mount.begin_activation(
                target_claim,
                activation(target_thread, 35, 36, 2, 0),
                operation_id(37),
                &cancelled,
                mount_cx,
            ))
            .unwrap(),
        MainWindowComposerActivationAdvance::Cancelled
    ));
    assert_eq!(
        mount
            .read_with(cx, |mount, _| mount.contribution())
            .unwrap()
            .entity_id(),
        initial_entity
    );

    let ready = mount
        .update(cx, |mount, mount_cx| {
            mount.begin_activation(
                target_claim,
                activation(target_thread, 38, 39, 2, 0),
                operation_id(40),
                &CommandCancellation::new(),
                mount_cx,
            )
        })
        .unwrap();
    let MainWindowComposerActivationAdvance::Ready(retired_receipt) = ready else {
        panic!("target did not become pending: {ready:?}")
    };
    assert_eq!(
        mount
            .read_with(cx, |mount, _| mount.contribution())
            .unwrap()
            .entity_id(),
        initial_entity
    );
    mount
        .update(cx, |mount, mount_cx| {
            mount.retire_pending(retired_receipt, mount_cx)
        })
        .unwrap();
    assert_eq!(
        mount
            .read_with(cx, |mount, _| mount.contribution())
            .unwrap()
            .entity_id(),
        initial_entity
    );

    let ready = mount
        .update(cx, |mount, mount_cx| {
            mount.begin_activation(
                target_claim,
                activation(target_thread, 41, 42, 2, 0),
                operation_id(43),
                &CommandCancellation::new(),
                mount_cx,
            )
        })
        .unwrap();
    let MainWindowComposerActivationAdvance::Ready(receipt) = ready else {
        panic!("successor target did not become pending: {ready:?}")
    };
    assert!(
        cx.update(|window, app| {
            mount.update(app, |mount, mount_cx| {
                mount.begin_publish(retired_receipt, window, mount_cx)
            })
        })
        .is_err()
    );
    assert!(
        cx.update(|window, app| {
            mount.update(app, |mount, mount_cx| {
                mount.advance_publish(receipt, window, mount_cx)
            })
        })
        .is_err()
    );
    assert_eq!(
        mount
            .read_with(cx, |mount, _| mount.contribution())
            .unwrap()
            .entity_id(),
        initial_entity
    );
    let flush = match cx
        .update(|window, app| {
            mount.update(app, |mount, mount_cx| {
                mount.begin_publish(receipt, window, mount_cx)
            })
        })
        .unwrap()
    {
        MainWindowConversationComposerMountFlushStart::Started(
            ComposerHostFlushAdmission::Started { ticket, .. },
        ) => ticket,
        start => panic!("prior flush did not start: {start:?}"),
    };
    assert!(matches!(
        mount
            .update(cx, |mount, _| mount.capture_flush_publication(
                initial_selection,
                flush,
                assets.clone(),
                &marker_seals,
                operation_id(50),
                None,
                SyndicTimestamp::from_unix_millis(50),
                &CommandCancellation::new(),
            ))
            .unwrap(),
        ComposerHostFlushCapture::State(ComposerHostFlushState::DisposalRequired)
    ));
    let mut prior_disposal_required = false;
    for _ in 0..16 {
        let advance = cx.update(|window, app| {
            mount.update(app, |mount, mount_cx| {
                mount.advance_publish(receipt, window, mount_cx)
            })
        });
        if matches!(
            advance,
            Ok(MainWindowConversationComposerMountPublishAdvance::Retained(
                beryl_app::main_window::MainWindowComposerPublishAdvance::Progress(
                    ComposerHostFlushState::DisposalRequired
                )
            ))
        ) {
            prior_disposal_required = true;
            break;
        }
    }
    assert!(prior_disposal_required);
    let prior_disposal = mount
        .update(cx, |mount, _| {
            mount.capture_flush_disposal(
                mount.selected_identity().unwrap(),
                flush,
                operation_id(51),
                &CommandCancellation::new(),
            )
        })
        .unwrap();
    assert!(
        matches!(
            prior_disposal,
            ComposerHostFlushCapture::State(ComposerHostFlushState::DisposalRequired)
        ),
        "unexpected prior disposal capture: {prior_disposal:?}"
    );
    let prior_release_selection =
        initial.read_with(cx, |composer, _| composer.selection_identity());
    assert_eq!(service.selected_identity(), Some(prior_release_selection));

    let published = loop {
        drive(cx, 4);
        let advance = cx.update(|window, app| {
            mount.update(app, |mount, mount_cx| {
                mount.advance_publish(receipt, window, mount_cx)
            })
        });
        match advance.unwrap() {
            MainWindowConversationComposerMountPublishAdvance::WidgetReleasePending(_) => continue,
            advance => break advance,
        }
    };
    let MainWindowConversationComposerMountPublishAdvance::Published(target_selection) = published
    else {
        panic!("target was not atomically published: {published:?}")
    };
    let target = mount
        .read_with(cx, |mount, _| mount.contribution())
        .unwrap();
    assert_ne!(target.entity_id(), initial_entity);
    assert_eq!(service.selected_identity(), Some(target_selection));
    assert_eq!(
        target.read_with(cx, |composer, _| composer.selection_identity()),
        target_selection
    );
    assert_eq!(
        cx.update(|window, app| {
            initial.update(app, |composer, composer_cx| {
                composer.release_widget(window, composer_cx)
            })
        })
        .unwrap()
        .selection(),
        prior_release_selection
    );

    drive(cx, 16);
    let target_input = target.read_with(cx, |composer, _| composer.gpui_input());
    cx.update(|window, app| target_input.update(app, |input, _| input.focus(window)));
    cx.update(|window, app| {
        target_input.update(app, |input, input_cx| {
            input.replace_and_mark_text_in_range(None, "x", None, window, input_cx)
        })
    });
    drive(cx, 32);
    let disposal_selection = service.selected_identity().unwrap();
    assert_ne!(disposal_selection, target_selection);
    assert_eq!(
        target.read_with(cx, |composer, _| composer.selection_identity()),
        disposal_selection
    );
    let disposal_flush = match cx
        .update(|window, app| {
            mount.update(app, |mount, mount_cx| {
                mount.begin_disposal(window, mount_cx)
            })
        })
        .unwrap()
    {
        MainWindowConversationComposerMountFlushStart::Started(
            ComposerHostFlushAdmission::Started { ticket, .. },
        ) => ticket,
        start => panic!("selected disposal flush did not start: {start:?}"),
    };
    assert!(matches!(
        mount
            .update(cx, |mount, _| mount.capture_flush_publication(
                disposal_selection,
                disposal_flush,
                assets,
                &marker_seals,
                operation_id(52),
                None,
                SyndicTimestamp::from_unix_millis(52),
                &CommandCancellation::new(),
            ))
            .unwrap(),
        ComposerHostFlushCapture::Captured(_)
    ));
    let mut selected_disposal_required = false;
    for _ in 0..16 {
        let advance = cx.update(|window, app| {
            mount.update(app, |mount, mount_cx| {
                mount.advance_disposal(window, mount_cx)
            })
        });
        if matches!(
            advance,
            Ok(
                MainWindowConversationComposerMountDisposalAdvance::Retained(
                    beryl_app::main_window::MainWindowComposerDisposalAdvance::Progress(
                        ComposerHostFlushState::DisposalRequired
                    )
                )
            )
        ) {
            selected_disposal_required = true;
            break;
        }
    }
    assert!(selected_disposal_required);
    let disposal_release_selection = service.selected_identity().unwrap();
    assert_eq!(
        target.read_with(cx, |composer, _| composer.selection_identity()),
        disposal_release_selection
    );
    assert!(matches!(
        mount
            .update(cx, |mount, _| mount.capture_flush_disposal(
                disposal_release_selection,
                disposal_flush,
                operation_id(53),
                &CommandCancellation::new(),
            ))
            .unwrap(),
        ComposerHostFlushCapture::State(ComposerHostFlushState::DisposalRequired)
    ));
    let disposed = loop {
        drive(cx, 4);
        let advance = cx.update(|window, app| {
            mount.update(app, |mount, mount_cx| {
                mount.advance_disposal(window, mount_cx)
            })
        });
        match advance.unwrap() {
            MainWindowConversationComposerMountDisposalAdvance::WidgetReleasePending(_) => continue,
            advance => break advance,
        }
    };
    assert_eq!(
        disposed,
        MainWindowConversationComposerMountDisposalAdvance::Disposed
    );
    assert!(
        mount
            .read_with(cx, |mount, _| mount.contribution())
            .is_none()
    );
    assert_eq!(service.selected_identity(), None);
}

#[gpui::test]
fn mounted_terminal_anchor_marker_run_remains_proven_for_successive_edits(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(ensure_text_input_bindings);
    let fixture = Fixture::new("terminal-anchor-marker-run", 71);
    let (claim, _) = fixture.claims();
    let window_id = fixture.window_id;
    let thread = fixture.selected_thread;
    let marker_authority = MainWindowComposerMarkerMetadataAuthority::new(fixture.assets());
    let marker_seals = fixture.marker_seals();
    let image_asset = publish_image_asset(&fixture, b"terminal-anchor-marker");
    let (_directory, store, storage) = fixture.into_store();
    let mut host = SyndicComposerHost::new(storage.clone());
    assert!(matches!(
        host.test_activate(
            &store,
            activation(thread, 72, 73, 1, 0),
            &CommandCancellation::new(),
        )
        .unwrap(),
        ComposerHostActivationOutcome::Activated { .. }
    ));
    let slot =
        MainWindowComposerSlot::new(window_id, claim, host, storage, marker_authority).unwrap();
    let service = Arc::new(MainWindowConversationComposerService::new(
        Arc::new(store),
        slot,
    ));
    let mounted_service = service.clone();
    let (root, cx) = cx.add_window_view(|window, cx| {
        let mount = cx.new(|mount_cx| {
            MainWindowConversationComposerMount::new(
                mounted_service,
                Box::new(|selection| {
                    MainWindowConversationComposerConfig::new(
                        selection,
                        widget_config(
                            selection.binding().range_binding(),
                            selection.binding().presentation_generation(),
                        ),
                    )
                    .map_err(|error| error.to_string())
                }),
                marker_seals,
                submission_source(),
                window,
                mount_cx,
            )
            .unwrap()
        });
        MountRoot { mount }
    });
    drive(cx, 16);
    let mount = root.read_with(cx, |root, _| root.mount.clone());
    let composer = mount
        .read_with(cx, |mount, _| mount.contribution())
        .unwrap();
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    cx.update(|window, app| input.update(app, |input, _| input.focus(window)));
    cx.update(|window, app| {
        input.update(app, |input, input_cx| {
            input.replace_and_mark_text_in_range(None, "a", None, window, input_cx)
        })
    });
    drive(cx, 32);

    for (id, order) in [(0x191_u128, 1_u128), (0x192_u128, 2_u128)] {
        composer
            .update(cx, |composer, composer_cx| {
                composer.insert_authenticated_image_marker(
                    ComposerHostImageMarkerMetadata::new(
                        InlineObjectId::new(id),
                        ImageLabelOrdinal::new(1).unwrap(),
                        image_asset,
                    ),
                    InlineObjectOrder::new(order),
                    composer_cx,
                )
            })
            .unwrap();
        drive(cx, 48);
        assert_eq!(
            service
                .selected_identity()
                .unwrap()
                .binding()
                .root()
                .summary()
                .marker_count(),
            order as u64
        );
        assert_eq!(
            composer.read_with(cx, |composer, _| composer.last_error().map(str::to_owned)),
            None
        );
    }
    input.read_with(cx, |input, _| {
        assert!(input.is_surface_current_and_interactive());
        assert!(input.is_quiescent());
        let surface = input.surface().unwrap();
        let resident = surface
            .object_pages()
            .iter()
            .map(|page| page.objects().len())
            .sum::<usize>();
        assert!((1..=32).contains(&resident));
    });
}

#[gpui::test]
fn recoverable_mounted_autosave_releases_rearms_and_does_not_spin(cx: &mut gpui::TestAppContext) {
    cx.update(ensure_text_input_bindings);
    let fixture = Fixture::new("mounted-autosave-recoverable", 71);
    let (claim, _) = fixture.claims();
    let window_id = fixture.window_id;
    let thread = fixture.selected_thread;
    let assets = fixture.assets();
    let marker_seals = fixture.marker_seals();
    let image_asset = publish_image_asset(&fixture, b"recoverable-marker");
    let marker_authority = MainWindowComposerMarkerMetadataAuthority::new(assets.clone());
    let (_directory, store, storage) = fixture.into_store();
    let mut host = SyndicComposerHost::new(storage.clone());
    assert!(matches!(
        host.test_activate(
            &store,
            activation(thread, 72, 73, 1, 0),
            &CommandCancellation::new(),
        )
        .unwrap(),
        ComposerHostActivationOutcome::Activated { .. }
    ));
    let slot =
        MainWindowComposerSlot::new(window_id, claim, host, storage, marker_authority).unwrap();
    let service = Arc::new(MainWindowConversationComposerService::new(
        Arc::new(store),
        slot,
    ));
    let mounted_service = service.clone();
    let mounted_seals = marker_seals.clone();
    let (root, cx) = cx.add_window_view(|window, cx| {
        let mount = cx.new(|mount_cx| {
            MainWindowConversationComposerMount::new(
                mounted_service,
                Box::new(|selection| {
                    MainWindowConversationComposerConfig::new(
                        selection,
                        widget_config(
                            selection.binding().range_binding(),
                            selection.binding().presentation_generation(),
                        ),
                    )
                    .map_err(|error| error.to_string())
                }),
                mounted_seals,
                submission_source(),
                window,
                mount_cx,
            )
            .unwrap()
        });
        MountRoot { mount }
    });
    drive(cx, 16);

    let mount = root.read_with(cx, |root, _| root.mount.clone());
    let contribution = mount
        .read_with(cx, |mount, _| mount.contribution())
        .unwrap();
    let input = contribution.read_with(cx, |composer, _| composer.gpui_input());
    cx.update(|window, app| input.update(app, |input, _| input.focus(window)));
    cx.update(|window, app| {
        input.update(app, |input, input_cx| {
            input.replace_and_mark_text_in_range(None, "a", None, window, input_cx)
        })
    });
    drive(cx, 32);
    contribution
        .update(cx, |composer, composer_cx| {
            composer.insert_authenticated_image_marker(
                ComposerHostImageMarkerMetadata::new(
                    InlineObjectId::new(0x182),
                    ImageLabelOrdinal::new(1).unwrap(),
                    image_asset,
                ),
                InlineObjectOrder::new(1),
                composer_cx,
            )
        })
        .unwrap();
    drive(cx, 48);
    let dirty_selection = service.selected_identity().unwrap();
    assert_eq!(dirty_selection.binding().root().summary().marker_count(), 1);

    cx.update(|window, app| {
        mount.update(app, |mount, mount_cx| {
            mount.publish_autosave_interval(
                1,
                ComposerHostAutosaveInterval::new(5).unwrap(),
                window,
                mount_cx,
            )
        })
    })
    .unwrap();
    let armed_generation =
        mount.read_with(cx, |mount, _| mount.autosave_diagnostics().generation());
    marker_seals.test_fail_next_drive_operationally();
    cx.executor().advance_clock(Duration::from_secs(5));
    for _ in 0..64 {
        drive(cx, 1);
        let diagnostics = mount.read_with(cx, |mount, _| mount.autosave_diagnostics());
        if diagnostics.phase() == MainWindowConversationComposerAutosavePhase::Waiting
            && diagnostics.generation() > armed_generation
        {
            break;
        }
    }
    let settled = mount.read_with(cx, |mount, _| mount.autosave_diagnostics());
    assert_eq!(
        settled.phase(),
        MainWindowConversationComposerAutosavePhase::Waiting
    );
    assert!(settled.generation() > armed_generation);
    assert_eq!(settled.retained_tasks(), 1);
    assert!(!settled.fenced());
    assert_eq!(settled.last_error(), None);

    let settled_generation = settled.generation();
    drive(cx, 32);
    let quiescent = mount.read_with(cx, |mount, _| mount.autosave_diagnostics());
    assert_eq!(quiescent.generation(), settled_generation);
    assert_eq!(quiescent.retained_tasks(), 1);
    assert!(!quiescent.fenced());
    assert_eq!(quiescent.last_error(), None);
    assert_eq!(
        contribution.read_with(cx, |composer, _| composer.last_error().map(str::to_owned)),
        None
    );
    assert!(input.read_with(cx, |input, _| input.is_enabled()));
    assert!(input.read_with(cx, |input, _| input.is_quiescent()));
    let resumed = mount.read_with(cx, |mount, _| mount.autosave_diagnostics());
    assert_eq!(
        resumed.phase(),
        MainWindowConversationComposerAutosavePhase::Waiting
    );
    assert_eq!(resumed.retained_tasks(), 1);
    assert!(!resumed.fenced());
}

#[gpui::test]
fn disposal_flush_joins_mounted_autosave_and_publishes_live_dirty_successor(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(ensure_text_input_bindings);
    let fixture = Fixture::new("mounted-autosave-join", 61);
    let (claim, _) = fixture.claims();
    let window_id = fixture.window_id;
    let thread = fixture.selected_thread;
    let assets = fixture.assets();
    let marker_seals = fixture.marker_seals();
    let marker_authority = MainWindowComposerMarkerMetadataAuthority::new(assets.clone());
    let (_directory, store, storage) = fixture.into_store();
    let mut host = SyndicComposerHost::new(storage.clone());
    assert!(matches!(
        host.test_activate(
            &store,
            activation(thread, 62, 63, 1, 0),
            &CommandCancellation::new(),
        )
        .unwrap(),
        ComposerHostActivationOutcome::Activated { .. }
    ));
    let slot =
        MainWindowComposerSlot::new(window_id, claim, host, storage, marker_authority).unwrap();
    let service = Arc::new(MainWindowConversationComposerService::new(
        Arc::new(store),
        slot,
    ));
    let mounted_service = service.clone();
    let (root, cx) = cx.add_window_view(|window, cx| {
        let mount = cx.new(|mount_cx| {
            MainWindowConversationComposerMount::new(
                mounted_service,
                Box::new(|selection| {
                    MainWindowConversationComposerConfig::new(
                        selection,
                        widget_config(
                            selection.binding().range_binding(),
                            selection.binding().presentation_generation(),
                        ),
                    )
                    .map_err(|error| error.to_string())
                }),
                marker_seals.clone(),
                submission_source(),
                window,
                mount_cx,
            )
            .unwrap()
        });
        MountRoot { mount }
    });
    drive(cx, 16);
    let mount = root.read_with(cx, |root, _| root.mount.clone());
    let contribution = mount
        .read_with(cx, |mount, _| mount.contribution())
        .unwrap();
    let entity_id = contribution.entity_id();
    let input = contribution.read_with(cx, |composer, _| composer.gpui_input());
    cx.update(|window, app| input.update(app, |input, _| input.focus(window)));
    cx.update(|window, app| {
        input.update(app, |input, input_cx| {
            input.replace_and_mark_text_in_range(None, "a", None, window, input_cx)
        })
    });
    drive(cx, 32);
    let captured_selection = service.selected_identity().unwrap();
    cx.update(|window, app| {
        mount.update(app, |mount, mount_cx| {
            mount.test_hold_next_autosave_ready();
            mount.publish_autosave_interval(
                1,
                ComposerHostAutosaveInterval::new(5).unwrap(),
                window,
                mount_cx,
            )
        })
    })
    .unwrap();
    cx.executor().advance_clock(Duration::from_secs(5));
    for _ in 0..32 {
        drive(cx, 1);
        if mount.read_with(cx, |mount, _| {
            mount.autosave_diagnostics().phase()
                == MainWindowConversationComposerAutosavePhase::Ready
        }) {
            break;
        }
    }
    assert_eq!(
        mount.read_with(cx, |mount, _| mount.autosave_diagnostics().phase()),
        MainWindowConversationComposerAutosavePhase::Ready
    );

    cx.update(|window, app| {
        input.update(app, |input, input_cx| {
            input.replace_and_mark_text_in_range(None, "b", None, window, input_cx)
        })
    });
    drive(cx, 32);
    let successor_selection = service.selected_identity().unwrap();
    assert_ne!(successor_selection, captured_selection);
    assert_eq!(
        contribution.read_with(cx, |composer, _| composer.selection_identity()),
        successor_selection
    );
    assert_eq!(
        mount.read_with(cx, |mount, _| mount.autosave_diagnostics().phase()),
        MainWindowConversationComposerAutosavePhase::Ready
    );

    let flush = match cx
        .update(|window, app| {
            mount.update(app, |mount, mount_cx| {
                mount.begin_disposal(window, mount_cx)
            })
        })
        .unwrap()
    {
        MainWindowConversationComposerMountFlushStart::Started(
            ComposerHostFlushAdmission::Started {
                ticket,
                state: ComposerHostFlushState::PublicationPending,
            },
        ) => ticket,
        start => panic!("disposal did not join the admitted autosave: {start:?}"),
    };
    let mut capture_required = false;
    for _ in 0..32 {
        let advance = cx.update(|window, app| {
            mount.update(app, |mount, mount_cx| {
                mount.advance_disposal(window, mount_cx)
            })
        });
        if matches!(
            advance,
            Ok(
                MainWindowConversationComposerMountDisposalAdvance::Retained(
                    beryl_app::main_window::MainWindowComposerDisposalAdvance::Progress(
                        ComposerHostFlushState::CaptureRequired
                    )
                )
            )
        ) {
            capture_required = true;
            break;
        }
    }
    assert!(capture_required);
    let successor_selection = service.selected_identity().unwrap();
    assert_eq!(
        contribution.read_with(cx, |composer, _| composer.selection_identity()),
        successor_selection
    );
    assert_eq!(
        mount
            .read_with(cx, |mount, _| mount.contribution())
            .unwrap()
            .entity_id(),
        entity_id
    );
    let published_at = SyndicTimestamp::from_unix_millis(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis()
            .try_into()
            .unwrap(),
    );
    assert!(matches!(
        mount
            .update(cx, |mount, _| mount.capture_flush_publication(
                successor_selection,
                flush,
                assets,
                &marker_seals,
                operation_id(64),
                None,
                published_at,
                &CommandCancellation::new(),
            ))
            .unwrap(),
        ComposerHostFlushCapture::Captured(_)
    ));
    let mut disposal_required = false;
    let mut disposal_advances = Vec::new();
    for _ in 0..32 {
        let advance = cx.update(|window, app| {
            mount.update(app, |mount, mount_cx| {
                mount.advance_disposal(window, mount_cx)
            })
        });
        disposal_advances.push(format!("{advance:?}"));
        if matches!(
            advance,
            Ok(
                MainWindowConversationComposerMountDisposalAdvance::Retained(
                    beryl_app::main_window::MainWindowComposerDisposalAdvance::Progress(
                        ComposerHostFlushState::DisposalRequired
                    )
                )
            )
        ) {
            disposal_required = true;
            break;
        }
    }
    assert!(disposal_required, "{disposal_advances:?}");
    let release_selection = service.selected_identity().unwrap();
    assert!(matches!(
        mount
            .update(cx, |mount, _| mount.capture_flush_disposal(
                release_selection,
                flush,
                operation_id(65),
                &CommandCancellation::new(),
            ))
            .unwrap(),
        ComposerHostFlushCapture::State(ComposerHostFlushState::DisposalRequired)
    ));
    let disposed = loop {
        drive(cx, 2);
        match cx
            .update(|window, app| {
                mount.update(app, |mount, mount_cx| {
                    mount.advance_disposal(window, mount_cx)
                })
            })
            .unwrap()
        {
            MainWindowConversationComposerMountDisposalAdvance::WidgetReleasePending(_) => {}
            advance => break advance,
        }
    };
    assert_eq!(
        disposed,
        MainWindowConversationComposerMountDisposalAdvance::Disposed
    );
    assert_eq!(service.selected_identity(), None);
}

fn submission_source() -> MainWindowComposerSubmissionRequestSource {
    MainWindowComposerSubmissionRequestSource::new(
        beryl_app::cas_projection::ProjectionServiceConfig::try_new(
            1,
            4,
            beryl_home_store::MinimumTurnCaptureReserve::try_new(1).unwrap(),
        )
        .unwrap()
        .turn_start_admission_requirement(),
    )
}

fn drive(cx: &mut gpui::VisualTestContext, rounds: usize) {
    for _ in 0..rounds {
        cx.run_until_parked();
        cx.update(|window, app| window.draw(app).clear());
    }
}

fn wait_for_native_lineage_prompt(
    cx: &mut gpui::VisualTestContext,
    mount: &Entity<MainWindowConversationComposerMount>,
    stage: &str,
) {
    for _ in 0..64 {
        for _ in 0..2 {
            let _ = cx.executor().tick();
            cx.update(|window, app| window.draw(app).clear());
        }
        if cx.debug_bounds("native-lineage-recovery-prompt").is_some()
            && mount.read_with(cx, |mount, _| mount.contribution().is_none())
        {
            return;
        }
    }
    let contribution = mount.read_with(cx, |mount, _| mount.contribution());
    let contribution_state = contribution.map(|contribution| {
        contribution.read_with(cx, |composer, app| {
            let input = composer.gpui_input();
            (
                composer.last_error().map(str::to_owned),
                composer.test_has_active_flight(),
                input.read(app).is_quiescent(),
                input.read(app).surface().is_some(),
                input.read(app).realization_diagnostics(),
            )
        })
    });
    let explicit_refresh = cx.update(|window, app| {
        mount.update(app, |mount, mount_cx| {
            mount.refresh_native_lineage_recovery(window, mount_cx)
        })
    });
    panic!(
        "{stage}: native-lineage recovery prompt did not reach its coherent publication cut: snapshot={:?}, contribution={contribution_state:?}, mount={:?}, explicit_refresh={explicit_refresh:?}",
        mount.read_with(cx, |mount, _| mount.native_lineage_recovery_snapshot()),
        mount.read_with(cx, |mount, _| mount.test_native_lineage_mount_diagnostics()),
    );
}

fn wait_for_native_lineage_composer(
    cx: &mut gpui::VisualTestContext,
    mount: &Entity<MainWindowConversationComposerMount>,
    stage: &str,
) {
    for _ in 0..64 {
        for _ in 0..2 {
            let _ = cx.executor().tick();
            cx.update(|window, app| window.draw(app).clear());
        }
        if mount.read_with(cx, |mount, _| {
            mount.contribution().is_some() && mount.native_lineage_recovery_snapshot().is_none()
        }) {
            return;
        }
    }
    panic!(
        "{stage}: native-lineage recovery did not publish a coherent composer: snapshot={:?}, contribution_present={}",
        mount.read_with(cx, |mount, _| mount.native_lineage_recovery_snapshot()),
        mount.read_with(cx, |mount, _| mount.contribution().is_some()),
    );
}

fn wait_for_native_lineage_gate(
    cx: &mut gpui::VisualTestContext,
    stage: &str,
    mut blocked: impl FnMut() -> bool,
) {
    for _ in 0..64 {
        cx.executor().advance_clock(Duration::from_millis(100));
        drive(cx, 2);
        if blocked() {
            return;
        }
    }
    panic!("{stage}: native-lineage host operation did not reach its admitted-token gate");
}

fn prove_native_lineage_late_flight_cleanup(
    cx: &mut gpui::VisualTestContext,
    mount: &Entity<MainWindowConversationComposerMount>,
    service: &Arc<MainWindowConversationComposerService>,
    thread: beryl_model::SyndicThreadId,
    flight: NativeLineageLateFlight,
    failed_attempts: u8,
) {
    let gate = match flight {
        NativeLineageLateFlight::Validation => service.test_gate_next_native_lineage_validation(),
        NativeLineageLateFlight::Page => service.test_gate_next_native_lineage_page(),
        NativeLineageLateFlight::ObjectPage => service.test_gate_next_native_lineage_object_page(),
    }
    .unwrap();
    let control = NativeLineageRecoveryControl::for_test(NonZeroUsize::new(1).unwrap());
    let key = control
        .install_route_for_test(
            thread,
            thread,
            BindingRevision::new(1).unwrap(),
            NativeLineageOperation::Resume,
            failed_attempts,
            true,
        )
        .unwrap();
    cx.update(|_, app| {
        mount.update(app, |mount, mount_cx| {
            mount.attach_native_lineage_recovery(control.clone(), mount_cx)
        })
    });
    wait_for_native_lineage_prompt(cx, mount, "late-flight prompt");
    assert!(control.set_status_for_test(
        key,
        NativeLineageRecoveryStatus::Leaving {
            pending_turn_continues: false,
        }
    ));
    for _ in 0..64 {
        cx.executor().advance_clock(Duration::from_millis(100));
        drive(cx, 2);
        if gate.is_blocked() {
            break;
        }
    }
    assert!(
        gate.is_blocked(),
        "{flight:?} flight did not reach its gate: control={:?}, mount={:?}, cleanup={:?}",
        control.snapshot_for_thread(thread),
        mount.read_with(cx, |mount, _| mount.native_lineage_recovery_snapshot()),
        service.test_native_lineage_cleanup_diagnostics(),
    );
    let diagnostics = service.test_native_lineage_cleanup_diagnostics();
    assert_eq!(diagnostics.sources, 1, "{flight:?}: {diagnostics:?}");
    assert_eq!(
        diagnostics.pending_flights, 1,
        "{flight:?}: {diagnostics:?}"
    );
    match flight {
        NativeLineageLateFlight::Validation => {
            assert_eq!(diagnostics.validation_flights, 1, "{diagnostics:?}")
        }
        NativeLineageLateFlight::Page => {
            assert_eq!(diagnostics.page_flights, 1, "{diagnostics:?}")
        }
        NativeLineageLateFlight::ObjectPage => {
            assert_eq!(diagnostics.object_page_flights, 1, "{diagnostics:?}")
        }
    }

    control.cancel(key).unwrap();
    wait_for_native_lineage_composer(cx, mount, "late-flight cancellation restoration");
    wait_for_native_lineage_source_count(cx, service, 1);
    let diagnostics = service.test_native_lineage_cleanup_diagnostics();
    assert_eq!(diagnostics.sources, 1, "{flight:?}: {diagnostics:?}");
    assert_eq!(
        diagnostics.pending_flights, 1,
        "{flight:?}: {diagnostics:?}"
    );
    assert!(
        diagnostics.cleanup_active
            + diagnostics.cleanup_ready
            + diagnostics.cleanup_awaiting_acknowledgement
            > 0,
        "{flight:?}: cleanup ownership was lost before late settlement: {diagnostics:?}"
    );
    assert!(!control.set_status_for_test(
        key,
        NativeLineageRecoveryStatus::Leaving {
            pending_turn_continues: false,
        }
    ));

    gate.release();
    wait_for_native_lineage_cleanup_drain(cx, service);
}

fn prove_native_lineage_host_failure_cleanup(
    cx: &mut gpui::VisualTestContext,
    mount: &Entity<MainWindowConversationComposerMount>,
    service: &Arc<MainWindowConversationComposerService>,
    thread: beryl_model::SyndicThreadId,
    flight: NativeLineageLateFlight,
    failed_attempts: u8,
) {
    let gate = match flight {
        NativeLineageLateFlight::Validation => {
            service.test_fail_next_native_lineage_validation();
            service.test_gate_next_native_lineage_validation()
        }
        NativeLineageLateFlight::Page => {
            service.test_fail_next_native_lineage_page();
            service.test_gate_next_native_lineage_page()
        }
        NativeLineageLateFlight::ObjectPage => {
            panic!("object-page failure injection is not part of this acceptance seam")
        }
    }
    .unwrap();
    let control = NativeLineageRecoveryControl::for_test(NonZeroUsize::new(1).unwrap());
    let key = control
        .install_route_for_test(
            thread,
            thread,
            BindingRevision::new(1).unwrap(),
            NativeLineageOperation::Resume,
            failed_attempts,
            true,
        )
        .unwrap();
    cx.update(|_, app| {
        mount.update(app, |mount, mount_cx| {
            mount.attach_native_lineage_recovery(control.clone(), mount_cx)
        })
    });
    wait_for_native_lineage_prompt(cx, mount, "host-failure prompt");
    assert!(control.set_status_for_test(
        key,
        NativeLineageRecoveryStatus::Leaving {
            pending_turn_continues: false,
        }
    ));
    wait_for_native_lineage_gate(cx, "host-failure admitted token", || gate.is_blocked());
    let admitted = service.test_native_lineage_cleanup_diagnostics();
    assert_eq!(admitted.sources, 1, "{flight:?}: {admitted:?}");
    assert_eq!(admitted.owner_active_sources, 1, "{flight:?}: {admitted:?}");
    assert_eq!(admitted.pending_flights, 1, "{flight:?}: {admitted:?}");
    match flight {
        NativeLineageLateFlight::Validation => {
            assert_eq!(admitted.validation_flights, 1, "{admitted:?}")
        }
        NativeLineageLateFlight::Page => assert_eq!(admitted.page_flights, 1, "{admitted:?}"),
        NativeLineageLateFlight::ObjectPage => unreachable!(),
    }

    gate.release();
    wait_for_native_lineage_composer(cx, mount, "host-failure exact restoration");
    assert_eq!(control.snapshot_for_thread(thread), None);
    assert!(
        mount
            .read_with(cx, |mount, _| mount.native_lineage_recovery_snapshot())
            .is_none()
    );
    wait_for_native_lineage_cleanup_drain(cx, service);
}

fn assert_native_lineage_disposal_failure_preserved(
    cx: &mut gpui::VisualTestContext,
    mount: &Entity<MainWindowConversationComposerMount>,
    service: &Arc<MainWindowConversationComposerService>,
    control: &NativeLineageRecoveryControl,
    route_key: NativeLineageRecoveryKey,
    selection: MainWindowComposerSelectionIdentity,
    stage: &str,
) {
    let diagnostics = mount.read_with(cx, |mount, _| mount.test_native_lineage_mount_diagnostics());
    assert!(diagnostics.snapshot_present, "{stage}: {diagnostics:?}");
    assert!(diagnostics.selection_current, "{stage}: {diagnostics:?}");
    assert!(diagnostics.seed_present, "{stage}: {diagnostics:?}");
    assert!(diagnostics.config_present, "{stage}: {diagnostics:?}");
    assert!(diagnostics.prompt_published, "{stage}: {diagnostics:?}");
    assert!(diagnostics.failure_present, "{stage}: {diagnostics:?}");
    assert!(diagnostics.disposal_active, "{stage}: {diagnostics:?}");
    assert!(mount.read_with(cx, |mount, _| mount.contribution().is_none()));
    assert_eq!(service.selected_identity(), Some(selection));
    assert!(
        cx.debug_bounds("native-lineage-recovery-prompt").is_some(),
        "{stage}: prompt surface disappeared"
    );
    let prompt = mount.read_with(cx, |mount, _| {
        mount.test_native_lineage_prompt_diagnostics()
    });
    assert!(prompt.local_failure_present, "{stage}: {prompt:?}");
    assert!(prompt.disposal_failure_present, "{stage}: {prompt:?}");
    assert_eq!(
        prompt.retry,
        MainWindowNativeLineagePromptCommandPresentation::Disabled,
        "{stage}: {prompt:?}"
    );
    assert_eq!(
        prompt.recover_from_syndic,
        MainWindowNativeLineagePromptCommandPresentation::Disabled,
        "{stage}: {prompt:?}"
    );
    assert_eq!(prompt.failed_command, None, "{stage}: {prompt:?}");
    assert_eq!(
        prompt.retry_disabled_explanation,
        "Retry is unavailable because composer disposal did not complete. Your preserved draft has not been discarded.",
        "{stage}: {prompt:?}"
    );
    assert_eq!(
        prompt.recover_disabled_explanation,
        "Syndic-history recovery is unavailable because composer disposal did not complete. Your preserved draft has not been discarded.",
        "{stage}: {prompt:?}"
    );
    assert!(
        prompt.retry_disabled_explanation.len() <= 128
            && prompt.recover_disabled_explanation.len() <= 128,
        "{stage}: {prompt:?}"
    );
    assert_eq!(control.take_command_for_test(route_key), None);
    for command_id in [
        "native-lineage-retry-command",
        "native-lineage-recover-from-syndic",
    ] {
        let bounds = cx
            .debug_bounds(command_id)
            .unwrap_or_else(|| panic!("{stage}: {command_id} disappeared"));
        cx.simulate_click(bounds.center(), gpui::Modifiers::none());
        cx.simulate_keystrokes("enter");
        cx.simulate_keystrokes("space");
        drive(cx, 2);
        assert_eq!(
            control.take_command_for_test(route_key),
            None,
            "{stage}: disabled {command_id} submitted"
        );
    }
    let after_attempts = mount.read_with(cx, |mount, _| {
        mount.test_native_lineage_prompt_diagnostics()
    });
    assert_eq!(
        after_attempts, prompt,
        "{stage}: disabled activation mutated prompt"
    );
}

fn wait_for_native_lineage_cleanup_drain(
    cx: &mut gpui::VisualTestContext,
    service: &Arc<MainWindowConversationComposerService>,
) {
    for _ in 0..64 {
        cx.executor().advance_clock(Duration::from_millis(100));
        drive(cx, 2);
        if service.test_native_lineage_cleanup_diagnostics().sources == 0 {
            break;
        }
    }
    let diagnostics = service.test_native_lineage_cleanup_diagnostics();
    assert_eq!(diagnostics.sources, 0, "{diagnostics:?}");
    assert_eq!(diagnostics.owner_active_sources, 0, "{diagnostics:?}");
    assert_eq!(diagnostics.validation_flights, 0, "{diagnostics:?}");
    assert_eq!(diagnostics.page_flights, 0, "{diagnostics:?}");
    assert_eq!(diagnostics.object_page_flights, 0, "{diagnostics:?}");
    assert_eq!(diagnostics.pending_flights, 0, "{diagnostics:?}");
    assert_eq!(diagnostics.terminal_flights, 0, "{diagnostics:?}");
    assert_eq!(diagnostics.delivered_flights, 0, "{diagnostics:?}");
    assert_eq!(diagnostics.cleanup_active, 0, "{diagnostics:?}");
    assert_eq!(diagnostics.cleanup_ready, 0, "{diagnostics:?}");
    assert_eq!(
        diagnostics.cleanup_awaiting_acknowledgement, 0,
        "{diagnostics:?}"
    );
}

fn wait_for_native_lineage_source_count(
    cx: &mut gpui::VisualTestContext,
    service: &Arc<MainWindowConversationComposerService>,
    expected: usize,
) {
    for _ in 0..64 {
        cx.executor().advance_clock(Duration::from_millis(100));
        drive(cx, 2);
        if service.test_native_lineage_cleanup_diagnostics().sources == expected {
            return;
        }
    }
    let diagnostics = service.test_native_lineage_cleanup_diagnostics();
    panic!("native-lineage cleanup did not reach {expected} retained sources: {diagnostics:?}");
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

fn activation(
    thread: beryl_model::SyndicThreadId,
    session: u8,
    operation: u8,
    presentation: u64,
    end: u64,
) -> ComposerHostActivationRequest {
    ComposerHostActivationRequest::new(
        thread,
        syndic_storage::DraftEditorCandidateSessionIdV1::from_bytes([session; 16]),
        operation_id(operation),
        NonZeroU64::new(presentation).unwrap(),
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
                    DraftPieceMarkerScopeV1::Range { start: 0, end },
                    DraftPieceMarkerDirectionV1::Forward,
                    None,
                    32,
                    65_536,
                ),
            },
        ]
        .into_boxed_slice(),
    )
}

fn widget_config(
    binding: gpui_text_input::RangeBinding,
    presentation: NonZeroU64,
) -> RangeTextInputConfig {
    let layout = StreamingLayoutBinding {
        input_id: 11,
        segment_policy_id: 13,
        start_position: StreamingLayoutPosition::at(0),
        wrap_width: px(320.),
        font_size: px(12.),
        line_height: px(16.),
        limits: StreamingLayoutLimits {
            segment_bytes: 32,
            runs: 8,
            decorations: 8,
            glyphs: 256,
            wraps: 128,
            maps: 257,
            fragments: 1,
            retained_items: 4096,
            retained_bytes: 256 * 1024,
        },
    };
    let run = TextRun {
        len: 0,
        font: font(".SystemUIFont"),
        color: black(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    RangeTextInputConfig {
        binding,
        presentation_generation: PresentationGeneration::new(presentation.get()),
        enter_key: TextInputEnterKey::Propagate,
        atom_clipboard_policy: TextInputAtomClipboardPolicy::Propagate,
        rich_paste_policy: TextInputRichPastePolicy::Propagate,
        layout,
        style: StreamingGeometryStyle::new(
            run,
            StreamingOversizePresentation::new(
                SharedString::new_static(""),
                vec![],
                px(12.),
                px(16.),
                px(12.),
                None,
            ),
        ),
        geometry_limits: ExactGeometryLimits::new(32, 8, 512 * 1024, 8192).unwrap(),
        residency_limits: ResidencyLimits::new(8, 128 * 1024, 8, 256).unwrap(),
        object_residency_limits: ObjectResidencyLimits::new(
            4,
            32,
            65_536,
            32 * 1024,
            4,
            32,
            65_536,
        )
        .unwrap(),
        mutation_limits: MutationLimits::new(8, 256).unwrap(),
        clipboard_limits: ClipboardLimits::new(1024, 32).unwrap(),
        segmentation_limits: SegmentationLimits::new(32, 64).unwrap(),
        limits: RangeTextInputLimits::new(2 * 1024 * 1024, 32768, 32, px(80.), 32, 32, px(16.))
            .unwrap(),
        settlement_coordinator: RangeSettlementCoordinator::new(4).unwrap(),
        viewport_extent: px(80.),
        overscan: px(32.),
        placeholder: SharedString::new_static("Message"),
        theme: TextInputTheme::default(),
        scrollbar_style: ScrollbarStyle::default(),
    }
}
