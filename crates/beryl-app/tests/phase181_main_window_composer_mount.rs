#![cfg(feature = "test-faults")]

#[path = "phase177_main_window_composer_slot/support.rs"]
mod support;

use std::{
    num::NonZeroU64,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use beryl_app::{
    composer_host::{
        ComposerHostActivationOutcome, ComposerHostActivationRequest, ComposerHostAutosaveInterval,
        ComposerHostFlushAdmission, ComposerHostFlushCapture, ComposerHostFlushState,
        ComposerHostImageMarkerMetadata, ComposerHostInitialDemand, ComposerHostRequestId,
        ComposerHostRequestPurpose, SyndicComposerHost,
    },
    main_window::{
        MainWindowComposerActivationAdvance, MainWindowComposerMarkerMetadataAuthority,
        MainWindowComposerSlot, MainWindowConversationComposerAutosavePhase,
        MainWindowConversationComposerConfig, MainWindowConversationComposerMount,
        MainWindowConversationComposerMountDisposalAdvance,
        MainWindowConversationComposerMountEvent, MainWindowConversationComposerMountFlushStart,
        MainWindowConversationComposerMountPublishAdvance, MainWindowConversationComposerService,
    },
};
use beryl_home_store::{
    CommandCancellation, CommandOutcome, HomeCommand, SidecarByteLimit, SidecarNamespace,
};
use beryl_model::{AssetId, ImageLabelOrdinal};
use beryl_state::{AssetMediaType, PublishAssetMetadata};
use gpui::{
    AppContext, Entity, EntityInputHandler, IntoElement, ParentElement, Render, SharedString,
    StreamingLayoutBinding, StreamingLayoutLimits, StreamingLayoutPosition, TextRun, black, div,
    font, px,
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
fn mounted_commands_are_selection_qualified_and_shift_enter_stays_a_newline(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(ensure_text_input_bindings);
    let fixture = Fixture::new("phase183-mounted-commands", 21);
    let claim = fixture.claims().0;
    let window_id = fixture.window_id;
    let thread = fixture.selected_thread;
    let marker_authority = MainWindowComposerMarkerMetadataAuthority::new(fixture.assets());
    let marker_seals = fixture.marker_seals();
    let (_directory, store, storage) = fixture.into_store();
    let mut host = SyndicComposerHost::new(storage);
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

    cx.simulate_keystrokes("enter");
    drive(cx, 4);
    assert_eq!(
        events.lock().unwrap().as_slice(),
        &[MainWindowConversationComposerMountEvent::SubmitPropagated { selection }]
    );

    cx.simulate_keystrokes("shift-enter");
    drive(cx, 4);
    assert_eq!(events.lock().unwrap().len(), 1);
    let pasted_selection = service.selected_identity().unwrap();
    assert_ne!(pasted_selection, selection);

    cx.simulate_keystrokes("ctrl-v");
    drive(cx, 4);
    assert_eq!(
        events.lock().unwrap().as_slice(),
        &[
            MainWindowConversationComposerMountEvent::SubmitPropagated { selection },
            MainWindowConversationComposerMountEvent::RichPastePropagated {
                selection: pasted_selection,
            },
        ]
    );
}

#[gpui::test]
fn mount_retains_one_coherent_contribution_until_exact_publish_and_disposal(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(ensure_text_input_bindings);
    let fixture = Fixture::new("phase181-composer-mount", 31);
    let (selected_claim, target_claim) = fixture.claims();
    let window_id = fixture.window_id;
    let selected_thread = fixture.selected_thread;
    let target_thread = fixture.target_thread;
    let marker_authority = MainWindowComposerMarkerMetadataAuthority::new(fixture.assets());
    let assets = fixture.assets();
    let marker_seals = fixture.marker_seals();
    let image_asset = publish_image_asset(&fixture, b"phase181-mounted-marker");
    let (_directory, store, storage) = fixture.into_store();
    let mut selected_host = SyndicComposerHost::new(storage);
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
                assets,
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
    let fixture = Fixture::new("phase191-terminal-anchor-marker-run", 71);
    let (claim, _) = fixture.claims();
    let window_id = fixture.window_id;
    let thread = fixture.selected_thread;
    let marker_authority = MainWindowComposerMarkerMetadataAuthority::new(fixture.assets());
    let marker_seals = fixture.marker_seals();
    let image_asset = publish_image_asset(&fixture, b"phase191-terminal-anchor-marker");
    let (_directory, store, storage) = fixture.into_store();
    let mut host = SyndicComposerHost::new(storage);
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
    let fixture = Fixture::new("phase181-mounted-autosave-recoverable", 71);
    let (claim, _) = fixture.claims();
    let window_id = fixture.window_id;
    let thread = fixture.selected_thread;
    let assets = fixture.assets();
    let marker_seals = fixture.marker_seals();
    let image_asset = publish_image_asset(&fixture, b"phase181-recoverable-marker");
    let marker_authority = MainWindowComposerMarkerMetadataAuthority::new(assets);
    let (_directory, store, storage) = fixture.into_store();
    let mut host = SyndicComposerHost::new(storage);
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
    let fixture = Fixture::new("phase181-mounted-autosave-join", 61);
    let (claim, _) = fixture.claims();
    let window_id = fixture.window_id;
    let thread = fixture.selected_thread;
    let assets = fixture.assets();
    let marker_seals = fixture.marker_seals();
    let marker_authority = MainWindowComposerMarkerMetadataAuthority::new(assets);
    let (_directory, store, storage) = fixture.into_store();
    let mut host = SyndicComposerHost::new(storage);
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

fn drive(cx: &mut gpui::VisualTestContext, rounds: usize) {
    for _ in 0..rounds {
        cx.run_until_parked();
        cx.update(|window, app| window.draw(app).clear());
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
