#![cfg(feature = "test-faults")]

#[path = "pending_composer_activation/support.rs"]
mod support;

use std::{
    num::NonZeroUsize,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use beryl_app::{
    cas_projection::{
        NativeLineageOperation, NativeLineageRecoveryControl, NativeLineageRecoveryKey,
    },
    composer_host::{ComposerHostActivationOutcome, SyndicComposerHost},
    main_window::{
        MainWindowComposerMarkerMetadataAuthority, MainWindowComposerSlot,
        MainWindowComposerSubmissionRequestSource, MainWindowConversationComposerConfig,
        MainWindowConversationComposerMount, MainWindowConversationComposerService,
    },
};
use beryl_home_store::CommandCancellation;
use beryl_model::BindingRevision;
use gpui::{AppContext, Entity, IntoElement, ParentElement, Render, Styled, div, px};
use gpui_text_input::{RangeRestorationSeed, ensure_text_input_bindings};

struct Root {
    mount: Entity<MainWindowConversationComposerMount>,
}

impl Render for Root {
    fn render(&mut self, _: &mut gpui::Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .w(px(320.))
            .h(px(64.))
            .child(self.mount.clone())
    }
}

struct Fixture {
    _directory: tempfile::TempDir,
    service: Arc<MainWindowConversationComposerService>,
    mount: Entity<MainWindowConversationComposerMount>,
    control: NativeLineageRecoveryControl,
    fail_config: Arc<AtomicBool>,
}

fn fixture(cx: &mut gpui::TestAppContext, seed: u8) -> (Fixture, &mut gpui::VisualTestContext) {
    cx.update(ensure_text_input_bindings);
    let fixture = support::fixture::Fixture::new("native-lineage-seed", seed);
    let claim = fixture.claims().0;
    let window_id = fixture.window_id;
    let thread = fixture.selected_thread;
    let marker_authority = MainWindowComposerMarkerMetadataAuthority::new(fixture.assets());
    let marker_seals = fixture.marker_seals();
    let (directory, store, storage) = fixture.into_store();
    let mut host = SyndicComposerHost::new(storage.clone());
    assert!(matches!(
        host.test_activate(
            &store,
            support::activation(thread, seed + 1, seed + 2, 1, 0),
            &CommandCancellation::new()
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
    let fail_config = Arc::new(AtomicBool::new(false));
    let factory_failure = fail_config.clone();
    let (root, cx) = cx.add_window_view(|window, cx| {
        let mount = cx.new(|mount_cx| {
            MainWindowConversationComposerMount::new(
                mounted_service,
                Box::new(move |selection| {
                    if factory_failure.swap(false, Ordering::SeqCst) {
                        return Err("injected post-validation configuration failure".to_owned());
                    }
                    MainWindowConversationComposerConfig::new(
                        selection,
                        support::widget_config(
                            selection.binding().range_binding(),
                            selection.binding().presentation_generation(),
                        ),
                    )
                    .map_err(|error| error.to_string())
                }),
                marker_seals,
                MainWindowComposerSubmissionRequestSource::new(
                    beryl_app::cas_projection::ProjectionServiceConfig::try_new(
                        1,
                        4,
                        beryl_home_store::MinimumTurnCaptureReserve::try_new(1).unwrap(),
                    )
                    .unwrap()
                    .turn_start_admission_requirement(),
                ),
                window,
                mount_cx,
            )
            .unwrap()
        });
        Root { mount }
    });
    let mount = root.read_with(cx, |root, _| root.mount.clone());
    let fixture = Fixture {
        _directory: directory,
        service,
        mount,
        control: NativeLineageRecoveryControl::for_test(NonZeroUsize::new(1).unwrap()),
        fail_config,
    };
    wait(cx, &fixture.mount, "initial editor", |cx| {
        fixture.mount.read_with(cx, |mount, app| {
            mount
                .contribution()
                .is_some_and(|composer| composer.read(app).gpui_input().read(app).is_quiescent())
        })
    });
    (fixture, cx)
}

fn wait(
    cx: &mut gpui::VisualTestContext,
    mount: &Entity<MainWindowConversationComposerMount>,
    stage: &str,
    mut ready: impl FnMut(&mut gpui::VisualTestContext) -> bool,
) {
    for _ in 0..256 {
        let _ = cx.executor().tick();
        cx.update(|window, app| window.draw(app).clear());
        if ready(cx) {
            return;
        }
    }
    panic!(
        "{stage}: {:?}",
        mount.read_with(cx, |mount, _| mount.test_native_lineage_mount_diagnostics())
    );
}

fn install(fixture: &Fixture) -> NativeLineageRecoveryKey {
    let thread = fixture
        .service
        .selected_identity()
        .unwrap()
        .claim()
        .thread_id();
    fixture
        .control
        .install_route_for_test(
            thread,
            thread,
            BindingRevision::new(1).unwrap(),
            NativeLineageOperation::Resume,
            1,
            true,
        )
        .unwrap()
}

fn attach(fixture: &Fixture, cx: &mut gpui::VisualTestContext) {
    cx.update(|_, app| {
        fixture.mount.update(app, |mount, cx| {
            mount.attach_native_lineage_recovery(fixture.control.clone(), cx)
        })
    });
}

fn seed(fixture: &Fixture, cx: &mut gpui::VisualTestContext) -> RangeRestorationSeed {
    let selection = fixture.service.selected_identity().unwrap();
    let input = fixture.mount.read_with(cx, |mount, app| {
        mount.contribution().unwrap().read(app).gpui_input()
    });
    input.read_with(cx, |input, _| {
        input
            .export_restoration(Some(selection.binding().range_history_frontier()))
            .unwrap()
    })
}

fn prompt(fixture: &Fixture, cx: &mut gpui::VisualTestContext) {
    wait(cx, &fixture.mount, "prompt publication", |cx| {
        cx.debug_bounds("native-lineage-recovery-prompt").is_some()
            && fixture
                .mount
                .read_with(cx, |mount, _| mount.contribution().is_none())
    });
    assert!(fixture.service.test_native_lineage_suspension_active());
}

fn cancel_and_restore(
    fixture: &Fixture,
    key: NativeLineageRecoveryKey,
    expected: RangeRestorationSeed,
    cx: &mut gpui::VisualTestContext,
) {
    fixture.control.cancel(key).unwrap();
    wait(cx, &fixture.mount, "editor restoration", |cx| {
        fixture.mount.read_with(cx, |mount, app| {
            mount.native_lineage_recovery_snapshot().is_none()
                && mount.contribution().is_some_and(|composer| {
                    composer.read(app).gpui_input().read(app).is_quiescent()
                })
        })
    });
    assert!(!fixture.service.test_native_lineage_suspension_active());
    assert_eq!(seed(fixture, cx), expected);
}

#[gpui::test]
fn readiness_loss_after_validation_retains_receipt_without_suspension(
    cx: &mut gpui::TestAppContext,
) {
    let (fixture, cx) = fixture(cx, 101);
    let expected = seed(&fixture, cx);
    let selection = fixture.service.selected_identity().unwrap();
    let original = fixture
        .mount
        .read_with(cx, |mount, _| mount.contribution().unwrap());
    let input = original.read_with(cx, |composer, _| composer.gpui_input());
    let gate = fixture
        .service
        .test_gate_next_native_lineage_seed_validation()
        .unwrap();
    let key = install(&fixture);
    attach(&fixture, cx);
    wait(cx, &fixture.mount, "validated seed gate", |_| {
        gate.is_blocked()
    });
    let mut config = support::widget_config(
        selection.binding().range_binding(),
        selection.binding().presentation_generation(),
    );
    config.layout.wrap_width = px(160.);
    cx.update(|_, app| {
        input.update(app, |input, cx| {
            input.set_layout(config.layout, config.style, cx).unwrap()
        })
    });
    assert!(!input.read_with(cx, |input, _| input.is_quiescent()));
    gate.release();
    for _ in 0..256 {
        let _ = cx.executor().tick();
        if fixture.mount.read_with(cx, |mount, _| {
            mount
                .test_native_lineage_mount_diagnostics()
                .validation_result_present
        }) {
            break;
        }
    }
    assert!(fixture.mount.read_with(cx, |mount, _| {
        mount
            .test_native_lineage_mount_diagnostics()
            .validation_result_present
    }));
    assert!(!fixture.service.test_native_lineage_suspension_active());
    assert_eq!(
        fixture.mount.read_with(cx, |mount, _| mount.contribution()),
        Some(original)
    );
    assert!(input.read_with(cx, |input, _| input.surface().is_some()
        && !input.is_enabled()));
    prompt(&fixture, cx);
    assert_eq!(
        fixture
            .mount
            .read_with(cx, |mount, _| mount.test_native_lineage_restoration_seed()),
        Some(expected)
    );
    assert_eq!(fixture.service.selected_identity(), Some(selection));
    cancel_and_restore(&fixture, key, expected, cx);
}

#[gpui::test]
fn post_validation_configuration_failure_preserves_editor_and_allows_retry(
    cx: &mut gpui::TestAppContext,
) {
    let (fixture, cx) = fixture(cx, 111);
    let expected = seed(&fixture, cx);
    let selection = fixture.service.selected_identity().unwrap();
    let original = fixture
        .mount
        .read_with(cx, |mount, _| mount.contribution().unwrap());
    let gate = fixture
        .service
        .test_gate_next_native_lineage_seed_validation()
        .unwrap();
    install(&fixture);
    attach(&fixture, cx);
    wait(cx, &fixture.mount, "validated seed gate", |_| {
        gate.is_blocked()
    });
    fixture.fail_config.store(true, Ordering::SeqCst);
    gate.release();
    wait(cx, &fixture.mount, "failed route cancellation", |cx| {
        fixture.mount.read_with(cx, |mount, _| {
            mount.native_lineage_recovery_snapshot().is_none()
        })
    });
    assert_eq!(
        fixture
            .control
            .snapshot_for_thread(selection.claim().thread_id()),
        None
    );
    assert!(!fixture.service.test_native_lineage_suspension_active());
    assert_eq!(
        fixture.mount.read_with(cx, |mount, _| mount.contribution()),
        Some(original.clone())
    );
    assert!(original.read_with(cx, |composer, app| {
        let input = composer.gpui_input();
        input.read(app).is_enabled() && input.read(app).surface().is_some()
    }));
    assert_eq!(seed(&fixture, cx), expected);
    assert_eq!(fixture.service.selected_identity(), Some(selection));
    let retry = install(&fixture);
    prompt(&fixture, cx);
    cancel_and_restore(&fixture, retry, expected, cx);
}

#[gpui::test]
fn canceled_validation_cannot_publish_or_cancel_replacement_route(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = fixture(cx, 121);
    let expected = seed(&fixture, cx);
    let selection = fixture.service.selected_identity().unwrap();
    let gate = fixture
        .service
        .test_gate_next_native_lineage_seed_validation()
        .unwrap();
    let old_key = install(&fixture);
    attach(&fixture, cx);
    wait(cx, &fixture.mount, "validated seed gate", |_| {
        gate.is_blocked()
    });
    assert!(!fixture.service.test_native_lineage_suspension_active());
    fixture.control.cancel(old_key).unwrap();
    let replacement = install(&fixture);
    assert_ne!(replacement, old_key);
    gate.release();
    prompt(&fixture, cx);
    assert_eq!(
        fixture
            .control
            .snapshot_for_thread(selection.claim().thread_id())
            .unwrap()
            .key(),
        replacement
    );
    assert_eq!(
        fixture.mount.read_with(cx, |mount, _| mount
            .native_lineage_recovery_snapshot()
            .unwrap()
            .key()),
        replacement
    );
    assert_eq!(
        fixture
            .mount
            .read_with(cx, |mount, _| mount.test_native_lineage_restoration_seed()),
        Some(expected)
    );
    assert_eq!(fixture.service.selected_identity(), Some(selection));
    cancel_and_restore(&fixture, replacement, expected, cx);
}
