use super::*;
use beryl_app::main_window::{
    MainWindowComposerActivationReceipt, MainWindowComposerPublishAdvance,
};

#[gpui::test]
fn recovery_route_survives_switch_and_is_rediscovered_after_a_late_update(
    cx: &mut gpui::TestAppContext,
) {
    prove_recovery_after_failed_dirty_switch(cx, true);
}

#[gpui::test]
fn restored_recovery_composer_autosaves_the_retained_dirty_draft_without_another_edit(
    cx: &mut gpui::TestAppContext,
) {
    prove_recovery_after_failed_dirty_switch(cx, false);
}

fn prove_recovery_after_failed_dirty_switch(
    cx: &mut gpui::TestAppContext,
    switch_after_failure: bool,
) {
    cx.update(ensure_text_input_bindings);
    let fixture = Fixture::new("recovery-switch", 181);
    let (selected_claim, target_claim) = fixture.claims();
    let thread = fixture.selected_thread;
    let target_thread = fixture.target_thread;
    let window_id = fixture.window_id;
    let assets = fixture.assets();
    let marker_authority = MainWindowComposerMarkerMetadataAuthority::new(assets.clone());
    let marker_seals = fixture.marker_seals();
    let original_durable_root = fixture.current_draft(thread).draft().piece_root();
    let (_directory, store, storage) = fixture.into_store();
    let cancelled = CommandCancellation::new();
    let cancel_at_execution = cancelled.clone();
    let mut host = SyndicComposerHost::new(storage.clone());
    assert!(matches!(
        host.test_activate(
            &store,
            activation(thread, 182, 183, 1, 0),
            &CommandCancellation::new()
        )
        .unwrap(),
        ComposerHostActivationOutcome::Activated { .. }
    ));
    host.test_arm_publication_before_execute_fault(move |_, _| cancel_at_execution.cancel());
    let slot = MainWindowComposerSlot::new(
        window_id,
        selected_claim,
        host,
        storage.clone(),
        marker_authority,
    )
    .unwrap();
    let store = Arc::new(store);
    let service = Arc::new(MainWindowConversationComposerService::new(
        store.clone(),
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
    let (root, cx) = cx.add_window_view(|window, cx| {
        let mount = cx.new(|mount_cx| {
            MainWindowConversationComposerMount::new(
                service.clone(),
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
        NativeLineageMountRoot { mount }
    });
    drive(cx, 16);
    let mount = root.read_with(cx, |root, _| root.mount.clone());
    let original = mount
        .read_with(cx, |mount, _| mount.contribution())
        .unwrap();
    let original_input = original.read_with(cx, |composer, _| composer.gpui_input());
    let initial_selection = service.selected_identity().unwrap();
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
    cx.update(|window, app| {
        original_input.update(app, |input, input_cx| {
            input.replace_text_in_range(None, "saved before switch", window, input_cx)
        })
    });
    for _ in 0..64 {
        drive(cx, 1);
        if service.selected_identity() != Some(initial_selection) {
            break;
        }
    }
    assert_ne!(service.selected_identity(), Some(initial_selection));
    mount.update(cx, |mount, mount_cx| {
        mount.attach_native_lineage_recovery(control.clone(), mount_cx)
    });
    wait_for_native_lineage_prompt(cx, &mount, "switch source prompt");
    assert!(original_input.read_with(cx, |input, _| input.surface().is_none()));
    cx.simulate_keystrokes("enter");
    assert_eq!(
        control.take_command_for_test(key),
        Some(NativeLineageRecoveryCommand::Retry)
    );
    let MainWindowComposerActivationAdvance::Ready(retired) = mount
        .update(cx, |mount, mount_cx| {
            mount.begin_activation(
                target_claim,
                activation(target_thread, 184, 185, 2, 0),
                operation_id(186),
                &CommandCancellation::new(),
                mount_cx,
            )
        })
        .unwrap()
    else {
        panic!("target not ready")
    };
    let failed_flush =
        begin_switch_flush(cx, &mount, retired).expect("dirty prior requires a flush");
    let retained_selection = service.selected_identity().unwrap();
    assert!(matches!(
        mount
            .update(cx, |mount, _| mount.capture_flush_publication(
                retained_selection,
                failed_flush,
                assets.clone(),
                &marker_seals,
                operation_id(200),
                None,
                SyndicTimestamp::from_unix_millis(200),
                &cancelled
            ))
            .unwrap(),
        ComposerHostFlushCapture::Captured(_)
    ));
    let failure = cx
        .update(|window, app| {
            mount.update(app, |mount, mount_cx| {
                mount.advance_publish(retired, window, mount_cx)
            })
        })
        .unwrap();
    assert_eq!(
        failure,
        MainWindowConversationComposerMountPublishAdvance::Retained(
            MainWindowComposerPublishAdvance::PriorFlushFailed
        )
    );
    assert_eq!(service.selected_identity(), Some(retained_selection));
    assert_eq!(
        storage
            .current_draft(
                &store,
                thread,
                syndic_storage::SyndicPointReadLimit::new(65_536).unwrap()
            )
            .unwrap()
            .unwrap()
            .draft()
            .piece_root(),
        original_durable_root
    );
    assert!(mount.read_with(cx, |mount, _| mount.test_pending_contribution().is_none()));
    cx.executor().advance_clock(Duration::from_secs(10));
    drive(cx, 8);
    let autosave = mount.read_with(cx, |mount, _| mount.autosave_diagnostics());
    assert_eq!(
        autosave.phase(),
        MainWindowConversationComposerAutosavePhase::Idle,
        "{autosave:?}"
    );
    assert_eq!(autosave.retained_tasks(), 0, "{autosave:?}");
    assert_eq!(autosave.last_error(), None, "{autosave:?}");
    assert_eq!(control.snapshot_for_thread(thread).unwrap().key(), key);
    assert!(mount.read_with(cx, |mount, _| mount.contribution().is_none()));
    if !switch_after_failure {
        control.cancel(key).unwrap();
        wait_for_native_lineage_composer(cx, &mount, "dirty prior restored after failed switch");
        for _ in 0..128 {
            cx.executor().advance_clock(Duration::from_secs(1));
            drive(cx, 2);
            if storage
                .current_draft(
                    &store,
                    thread,
                    syndic_storage::SyndicPointReadLimit::new(65_536).unwrap(),
                )
                .unwrap()
                .unwrap()
                .draft()
                .piece_root()
                != original_durable_root
            {
                return;
            }
        }
        panic!(
            "restored dirty draft did not autosave: {:?}",
            mount.read_with(cx, |mount, _| mount.autosave_diagnostics())
        );
    }
    let MainWindowComposerActivationAdvance::Ready(receipt) = mount
        .update(cx, |mount, mount_cx| {
            mount.begin_activation(
                target_claim,
                activation(target_thread, 187, 188, 2, 0),
                operation_id(189),
                &CommandCancellation::new(),
                mount_cx,
            )
        })
        .unwrap()
    else {
        panic!("replacement target not ready")
    };
    assert!(
        cx.update(|window, app| mount.update(app, |mount, mount_cx| mount
            .begin_publish(retired, window, mount_cx)))
            .is_err()
    );
    let target_selection = publish_switch(cx, &mount, receipt, &assets, &marker_seals, 190);
    let target = mount
        .read_with(cx, |mount, _| mount.contribution())
        .unwrap();
    assert_eq!(service.selected_identity(), Some(target_selection));
    assert_ne!(
        storage
            .current_draft(
                &store,
                thread,
                syndic_storage::SyndicPointReadLimit::new(65_536).unwrap()
            )
            .unwrap()
            .unwrap()
            .draft()
            .piece_root(),
        original_durable_root
    );
    assert_eq!(control.snapshot_for_thread(thread).unwrap().key(), key);
    assert!(matches!(
        control.snapshot_for_thread(thread).unwrap().status(),
        NativeLineageRecoveryStatus::Running {
            command: NativeLineageRecoveryCommand::Retry
        }
    ));
    assert!(control.set_status_for_test(
        key,
        NativeLineageRecoveryStatus::Failed {
            command: NativeLineageRecoveryCommand::Retry,
            recovery_available: true
        }
    ));
    drive(cx, 8);
    assert_eq!(
        mount.read_with(cx, |mount, _| mount.contribution()),
        Some(target)
    );
    assert_eq!(service.selected_identity(), Some(target_selection));
    assert!(mount.read_with(cx, |mount, _| {
        mount.native_lineage_recovery_snapshot().is_none()
    }));
    let MainWindowComposerActivationAdvance::Ready(back) = mount
        .update(cx, |mount, mount_cx| {
            mount.begin_activation(
                selected_claim,
                activation(thread, 193, 194, 3, 19),
                operation_id(195),
                &CommandCancellation::new(),
                mount_cx,
            )
        })
        .unwrap()
    else {
        panic!("return target not ready")
    };
    publish_switch(cx, &mount, back, &assets, &marker_seals, 196);
    wait_for_native_lineage_prompt(cx, &mount, "same recovery after switch back");
    assert_eq!(
        mount.read_with(cx, |mount, _| mount
            .native_lineage_recovery_snapshot()
            .unwrap()
            .key()),
        key
    );
    assert!(matches!(
        control.snapshot_for_thread(thread).unwrap().status(),
        NativeLineageRecoveryStatus::Failed {
            command: NativeLineageRecoveryCommand::Retry,
            recovery_available: true
        }
    ));
    control.cancel(key).unwrap();
}

fn publish_switch(
    cx: &mut gpui::VisualTestContext,
    mount: &Entity<MainWindowConversationComposerMount>,
    receipt: MainWindowComposerActivationReceipt,
    assets: &beryl_state::AssetState,
    marker_seals: &beryl_app::composer_marker_seal::DraftMarkerSealService,
    seed: u8,
) -> MainWindowComposerSelectionIdentity {
    let ticket = begin_switch_flush(cx, mount, receipt);
    let selection = mount.read_with(cx, |mount, _| mount.selected_identity().unwrap());
    if let Some(ticket) = ticket {
        mount
            .update(cx, |mount, _| {
                mount.capture_flush_publication(
                    selection,
                    ticket,
                    assets.clone(),
                    marker_seals,
                    operation_id(seed),
                    None,
                    SyndicTimestamp::from_unix_millis(
                        SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap()
                            .as_millis()
                            .try_into()
                            .unwrap(),
                    ),
                    &CommandCancellation::new(),
                )
            })
            .unwrap();
    }
    let mut disposal_captured = false;
    let mut last_advance = None;
    for _ in 0..128 {
        drive(cx, 1);
        let advance = cx
            .update(|window, app| {
                mount.update(app, |mount, mount_cx| {
                    mount.advance_publish(receipt, window, mount_cx)
                })
            })
            .unwrap();
        last_advance = Some(advance);
        match advance {
            MainWindowConversationComposerMountPublishAdvance::Published(selection) => {
                return selection;
            }
            MainWindowConversationComposerMountPublishAdvance::Retained(
                MainWindowComposerPublishAdvance::Progress(ComposerHostFlushState::CaptureRequired),
            ) => {
                let selection = mount.read_with(cx, |mount, _| mount.selected_identity().unwrap());
                mount
                    .update(cx, |mount, _| {
                        mount.capture_flush_publication(
                            selection,
                            ticket.unwrap(),
                            assets.clone(),
                            marker_seals,
                            operation_id(seed),
                            None,
                            SyndicTimestamp::from_unix_millis(
                                SystemTime::now()
                                    .duration_since(UNIX_EPOCH)
                                    .unwrap()
                                    .as_millis()
                                    .try_into()
                                    .unwrap(),
                            ),
                            &CommandCancellation::new(),
                        )
                    })
                    .unwrap();
            }
            MainWindowConversationComposerMountPublishAdvance::Retained(
                MainWindowComposerPublishAdvance::Progress(
                    ComposerHostFlushState::DisposalRequired,
                ),
            ) if !disposal_captured => {
                let selection = mount.read_with(cx, |mount, _| mount.selected_identity().unwrap());
                mount
                    .update(cx, |mount, _| {
                        mount.capture_flush_disposal(
                            selection,
                            ticket.unwrap(),
                            operation_id(seed + 1),
                            &CommandCancellation::new(),
                        )
                    })
                    .unwrap();
                disposal_captured = true;
            }
            MainWindowConversationComposerMountPublishAdvance::WidgetReleasePending(_)
            | MainWindowConversationComposerMountPublishAdvance::TargetSurfacePending(_)
            | MainWindowConversationComposerMountPublishAdvance::Retained(
                MainWindowComposerPublishAdvance::Progress(_),
            ) => {}
            other => panic!("unexpected switch publication: {other:?}"),
        }
    }
    panic!("switch publication did not settle: {last_advance:?}")
}

fn begin_switch_flush(
    cx: &mut gpui::VisualTestContext,
    mount: &Entity<MainWindowConversationComposerMount>,
    receipt: MainWindowComposerActivationReceipt,
) -> Option<beryl_app::composer_host::ComposerHostFlushTicket> {
    let mut ticket = None;
    for _ in 0..64 {
        let start = cx
            .update(|window, app| {
                mount.update(app, |mount, mount_cx| {
                    mount.begin_publish(receipt, window, mount_cx)
                })
            })
            .unwrap();
        match start {
            MainWindowConversationComposerMountFlushStart::TargetPriming(_)
            | MainWindowConversationComposerMountFlushStart::WidgetFencePending(_) => drive(cx, 1),
            MainWindowConversationComposerMountFlushStart::Started(
                ComposerHostFlushAdmission::Started {
                    ticket: started, ..
                }
                | ComposerHostFlushAdmission::Joined {
                    ticket: started, ..
                },
            ) => {
                ticket = Some(started);
                break;
            }
            MainWindowConversationComposerMountFlushStart::Started(
                ComposerHostFlushAdmission::Satisfied(_),
            ) => return None,
        }
    }
    Some(ticket.expect("switch flush did not start"))
}
