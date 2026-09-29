use super::*;
use crate::composer_marker_seal::{
    DraftMarkerSealServiceLimits, initial_preparation::PreparedMarkerServices,
};
use crate::main_window::{
    MainWindowComposerRecoveryPreparation as Preparation,
    MainWindowComposerRecoveryProgress as Progress,
    MainWindowConversationComposerCloseAdvance as CloseAdvance,
    MainWindowConversationComposerCloseTicket as CloseTicket,
};
use beryl_home_store::test_faults::FaultPoint;
use gpui::{EntityInputHandler, TestAppContext};
use gpui_text_input::*;
use std::{num::NonZeroUsize, sync::Arc};
use syndic_storage::SyndicStorage;

#[path = "../syndic_composer_history/support.rs"]
mod composer;
#[path = "../resident_close_flush/support.rs"]
mod support;
#[path = "../pending_composer_activation/support.rs"]
mod widget_support;

#[derive(Clone, Copy, PartialEq)]
enum Scenario {
    Adopt,
    StaleClose,
    ForeignAdapters,
    StaleGeneration,
    MissingAdapters,
    MissingConfigurator,
    Capacity,
}

#[gpui::test]
fn mount_attachment_preserves_editor_focus_and_recovery_fences(cx: &mut TestAppContext) {
    run(cx, Scenario::Adopt);
}

#[gpui::test]
fn mount_attachment_refuses_stale_close_without_consuming_resources(cx: &mut TestAppContext) {
    run(cx, Scenario::StaleClose);
}

#[gpui::test]
fn mount_attachment_refuses_foreign_adapters_without_partial_association(cx: &mut TestAppContext) {
    run(cx, Scenario::ForeignAdapters);
}

#[gpui::test]
fn mount_attachment_requires_configurator_before_adoption(cx: &mut TestAppContext) {
    run(cx, Scenario::MissingConfigurator);
}

#[gpui::test]
fn mount_attachment_refuses_stale_adapter_generation(cx: &mut TestAppContext) {
    run(cx, Scenario::StaleGeneration);
}

#[gpui::test]
fn mount_attachment_requires_adapters_before_adoption(cx: &mut TestAppContext) {
    run(cx, Scenario::MissingAdapters);
}

#[gpui::test]
fn mount_attachment_retains_resources_when_widget_adoption_refuses(cx: &mut TestAppContext) {
    run(cx, Scenario::Capacity);
}

fn run(cx: &mut TestAppContext, scenario: Scenario) {
    let mut faults = None;
    let (fixture, cx) = support::mounted_observing_faults(
        cx,
        "recovery-mount-attachment",
        201,
        Box::new(support::configure),
        |_| support::submission_source(),
        |value| faults = Some(value),
    );
    let resident = fixture
        .mount
        .read_with(cx, |mount, _| mount.contribution())
        .unwrap();
    let input = resident.read_with(cx, |resident, _| resident.gpui_input());
    let focus = cx.update(|window, app| {
        input.update(app, |input, cx| {
            input.focus(window);
            input.replace_and_mark_text_in_range(None, "preserved mount", None, window, cx);
        });
        window.focused(app)
    });
    let close = cx.update(|window, app| {
        fixture
            .mount
            .update(app, |mount, cx| mount.begin_window_close(window, cx))
            .unwrap()
    });
    support::drive_until(cx, "attachment close", |cx| {
        cx.update(|window, app| {
            fixture.mount.update(app, |mount, cx| {
                mount.advance_window_close(close.ticket, window, cx)
            })
        })
        .unwrap()
            == CloseAdvance::Ready
    });
    cx.simulate_keystrokes("ctrl-home shift-right");
    support::drive_until(cx, "attachment quiescence", |cx| {
        input.read_with(cx, |input, _| input.is_quiescent())
    });
    resident
        .update(cx, |resident, cx| {
            resident.test_set_shutdown_interaction_gated(true, cx)
        })
        .unwrap();
    let resources = fixture.mount.update(cx, |mount, cx| {
        assert!(
            mount
                .fence_interrupted_exit_resident(close.ticket, cx)
                .unwrap()
        );
        mount
            .detach_interrupted_exit_resources(close.ticket, cx)
            .unwrap()
            .unwrap()
    });
    let (seed, protection) = resident.read_with(cx, |resident, _| {
        let snapshot = resident.recovery_snapshot().unwrap();
        (*snapshot.restoration(), snapshot.protection())
    });
    let support::Mounted {
        root,
        mount,
        service,
        store,
        storage,
        assets,
        seals,
        directory,
    } = fixture;
    drop(service);
    let retired = resources.retire().ok().unwrap();
    drop((storage, assets, seals));
    faults
        .unwrap()
        .fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let mut candidate = Arc::try_unwrap(store)
        .ok()
        .unwrap()
        .recover_same_home()
        .unwrap();
    let state = beryl_state::BerylState::reacquire_candidate(&candidate).unwrap();
    let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let marker = PreparedMarkerServices::prepare_recovery(
        &mut candidate,
        storage.clone(),
        state.assets(),
        DraftMarkerSealServiceLimits::new(
            NonZeroUsize::new(2).unwrap(),
            NonZeroUsize::new(1).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let mut adapters = Some(PreparedComposerRecoveryAdapters {
        home: candidate.home_id(),
        generation: candidate.generation(),
        assets: state.assets(),
        marker: marker.into_service(),
        submission: support::submission_source(),
        native: NativeLineageRecoveryControl::for_test(NonZeroUsize::new(1).unwrap()),
    });
    if scenario == Scenario::ForeignAdapters {
        adapters.as_mut().unwrap().home = BerylHomeId::from_bytes([99; 16]);
    }
    if scenario == Scenario::StaleGeneration {
        adapters.as_mut().unwrap().generation = resident.read_with(cx, |resident, _| {
            resident.selection_identity().binding().home_generation()
        });
    }
    let withheld = if scenario == Scenario::MissingAdapters {
        adapters.take()
    } else {
        None
    };
    let mut configurator: Option<crate::main_window::MainWindowConversationComposerConfigurator> =
        (scenario != Scenario::MissingConfigurator).then(|| Box::new(support::configure) as _);
    let mut preparation = cx.update(|_, app| {
        Preparation::prepare(candidate, retired, storage, state, seed, app, |_| {})
    });
    cx.run_until_parked();
    let (fresh, selection) = preparation.authenticated_source().unwrap().unwrap();
    let mut config =
        widget_support::widget_config(fresh.binding, selection.binding().presentation_generation());
    config.viewport_extent = gpui::px(96.);
    config.limits.max_realized_block_extent = config.viewport_extent;
    let current = RangePrepublicationCurrent {
        binding: fresh.binding,
        history: fresh.history,
        available_capacity: RangeSurfaceCharge {
            bytes: config.limits.max_surface_bytes,
            items: config.limits.max_surface_items,
        },
    };
    let combined = RangeSurfaceCharge {
        bytes: current.available_capacity.bytes * 2,
        items: current.available_capacity.items * 2,
    };
    let environment = cx.update(|window, _| {
        RangePrepublicationEnvironment::new(
            11,
            config,
            window.text_system(),
            RangePrepublicationCleanupLedger::new(window.text_system(), 64).unwrap(),
        )
        .unwrap()
    });
    input.read_with(cx, |input, _| {
        preparation
            .admit(input, protection, environment.clone(), combined)
            .unwrap()
    });
    let mut ready = false;
    for _ in 0..512 {
        let progress = cx
            .update(|window, app| {
                input.update(app, |input, cx| {
                    preparation.advance(input, window.text_system(), cx, |_| {})
                })
            })
            .unwrap();
        if progress == Progress::Ready {
            ready = true;
            break;
        }
        cx.run_until_parked();
    }
    assert!(ready);
    let mut attempted = current;
    if scenario == Scenario::Capacity {
        attempted.available_capacity = RangeSurfaceCharge { bytes: 0, items: 0 };
    }
    let ticket = if scenario == Scenario::StaleClose {
        CloseTicket::for_test(
            mount.entity_id(),
            u64::MAX,
            resident.read_with(cx, |resident, _| resident.selection_identity()),
        )
    } else {
        close.ticket
    };
    let result = cx.update(|window, app| {
        mount.update(app, |mount, cx| {
            mount.adopt_interrupted_exit_resident(
                &resident,
                ticket,
                &mut preparation,
                &mut adapters,
                &mut configurator,
                attempted,
                window,
                cx,
            )
        })
    });
    let candidate = if scenario == Scenario::Adopt {
        let (candidate, fresh_close) = result.unwrap();
        assert!(adapters.is_none());
        assert!(configurator.is_none());
        assert_ne!(fresh_close, close.ticket);
        cx.update(|window, app| {
            mount.update(app, |mount, cx| {
                assert_eq!(mount.contribution().as_ref(), Some(&resident));
                assert_eq!(mount.selected_identity(), Some(selection));
                assert!(mount.window_close_flush_ticket(fresh_close).is_none());
                assert_eq!(
                    mount
                        .advance_window_close(close.ticket, window, cx)
                        .unwrap(),
                    CloseAdvance::Stale
                );
                assert!(mount.begin_window_close(window, cx).is_err());
                assert!(
                    mount
                        .adopt_interrupted_exit_resident(
                            &resident,
                            fresh_close,
                            &mut preparation,
                            &mut adapters,
                            &mut configurator,
                            current,
                            window,
                            cx,
                        )
                        .is_err()
                );
            })
        });
        resident.update(cx, |resident, cx| {
            assert_eq!(resident.selection_identity(), selection);
            assert_eq!(resident.gpui_input(), input);
            assert!(resident.recovery_snapshot().is_none());
            assert!(
                resident
                    .test_set_shutdown_interaction_gated(false, cx)
                    .is_err()
            );
        });
        input.read_with(cx, |input, _| {
            assert_eq!(input.export_restoration(fresh.history).unwrap(), fresh)
        });
        candidate
    } else {
        assert!(result.is_err());
        assert_eq!(adapters.is_some(), scenario != Scenario::MissingAdapters);
        assert_eq!(
            configurator.is_some(),
            scenario != Scenario::MissingConfigurator
        );
        mount.read_with(cx, |mount, _| assert_eq!(mount.selected_identity(), None));
        resident.read_with(cx, |resident, _| {
            assert_eq!(
                resident.recovery_snapshot().unwrap().close_ticket(),
                close.ticket
            )
        });
        input.read_with(cx, |input, _| {
            assert_eq!(input.export_restoration(seed.history).unwrap(), seed)
        });
        preparation.cancel();
        for _ in 0..128 {
            cx.run_until_parked();
            if preparation.advance_cleanup().unwrap() {
                break;
            }
        }
        assert!(preparation.advance_cleanup().unwrap());
        let (candidate, source) = preparation.take_cancelled_resources().unwrap();
        assert!(source.is_ok());
        drop(source);
        candidate
    };
    cx.update(|window, app| {
        assert_eq!(window.focused(app), focus);
        input.update(app, |input, cx| {
            assert!(!input.is_enabled());
            assert!(input.dispose(window, cx).is_empty());
        });
    });
    drop((preparation, adapters, withheld, configurator));
    cx.update(|window, _| window.remove_window());
    drop((input, resident, root, mount));
    cx.run_until_parked();
    for _ in 0..4 {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(50));
        cx.run_until_parked();
    }
    let ownership = environment.cleanup().ownership();
    assert_eq!(
        (
            ownership.active,
            ownership.ready,
            ownership.awaiting_acknowledgement
        ),
        (0, 0, 0)
    );
    drop(environment);
    candidate.abort().close().unwrap();
    directory.close().unwrap();
}
