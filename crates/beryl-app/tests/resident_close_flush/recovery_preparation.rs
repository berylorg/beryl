use super::{support, widget_support};
use beryl_app::main_window::{
    MainWindowComposerRecoveryPreparation as Preparation,
    MainWindowComposerRecoveryProgress as Progress,
    MainWindowConversationComposerCloseAdvance as CloseAdvance,
};
use beryl_home_store::test_faults::FaultPoint;
use gpui::{EntityInputHandler, TestAppContext, px};
use gpui_text_input::*;
use std::sync::Arc;

#[gpui::test]
fn reserved_candidate_realization_preserves_resident_and_drains(cx: &mut TestAppContext) {
    run(cx, Scenario::Ready);
}

#[gpui::test]
fn reserved_candidate_cancellation_settles_queued_effects(cx: &mut TestAppContext) {
    run(cx, Scenario::CancelQueued);
}

#[gpui::test]
fn reserved_candidate_read_failure_retains_cleanup(cx: &mut TestAppContext) {
    run(cx, Scenario::ReadFailure);
}

#[gpui::test]
fn reserved_candidate_cancellation_waits_for_actual_read(cx: &mut TestAppContext) {
    run(cx, Scenario::CancelReading);
}

#[gpui::test]
fn reserved_candidate_refuses_changed_protection(cx: &mut TestAppContext) {
    run(cx, Scenario::ProtectionChanged);
}

#[gpui::test]
fn reserved_candidate_refuses_changed_window_after_ready(cx: &mut TestAppContext) {
    run(cx, Scenario::WindowChanged);
}

#[gpui::test]
fn reserved_candidate_returns_authentication_refusal(cx: &mut TestAppContext) {
    run(cx, Scenario::AuthenticationFailure);
}

#[gpui::test]
fn prepared_adoption_preserves_fenced_resident_and_transfers_custody(cx: &mut TestAppContext) {
    run(cx, Scenario::Adopt);
}

#[gpui::test]
fn prepared_adoption_refuses_changed_protection_and_drains(cx: &mut TestAppContext) {
    run(cx, Scenario::AdoptionProtection);
}

#[gpui::test]
fn prepared_adoption_refuses_capacity_and_drains(cx: &mut TestAppContext) {
    run(cx, Scenario::AdoptionCapacity);
}

#[gpui::test]
fn prepared_adoption_refuses_changed_history_and_drains(cx: &mut TestAppContext) {
    run(cx, Scenario::AdoptionHistory);
}

#[derive(Clone, Copy, PartialEq)]
enum Scenario {
    Ready,
    CancelQueued,
    CancelReading,
    ReadFailure,
    ProtectionChanged,
    WindowChanged,
    AuthenticationFailure,
    Adopt,
    AdoptionProtection,
    AdoptionCapacity,
    AdoptionHistory,
}

struct Empty;
impl gpui::Render for Empty {
    fn render(
        &mut self,
        _: &mut gpui::Window,
        _: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        gpui::div()
    }
}

fn run(cx: &mut TestAppContext, scenario: Scenario) {
    let mut foreign_text_system = None;
    if scenario == Scenario::WindowChanged {
        cx.add_window_view(|window, _| {
            foreign_text_system = Some(window.text_system().clone());
            Empty
        });
    }
    let mut faults = None;
    let (fixture, cx) = support::mounted_observing_faults(
        cx,
        "resident-preparation",
        198,
        Box::new(support::configure),
        |_| support::submission_source(),
        |value| faults = Some(value),
    );
    let faults = faults.unwrap();
    let composer = fixture
        .mount
        .read_with(cx, |mount, _| mount.contribution())
        .unwrap();
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    let focus = cx.update(|window, app| {
        input.update(app, |input, cx| {
            input.focus(window);
            if scenario == Scenario::Adopt {
                input.replace_and_mark_text_in_range(
                    None,
                    "retained adoption pages",
                    None,
                    window,
                    cx,
                );
            }
        });
        window.focused(app)
    });
    let close = cx.update(|window, app| {
        fixture
            .mount
            .update(app, |mount, cx| mount.begin_window_close(window, cx))
            .unwrap()
    });
    support::drive_until(cx, "preparation close", |cx| {
        cx.update(|window, app| {
            fixture.mount.update(app, |mount, cx| {
                mount.advance_window_close(close.ticket, window, cx)
            })
        })
        .unwrap()
            == CloseAdvance::Ready
    });
    if scenario == Scenario::Adopt {
        cx.simulate_keystrokes("ctrl-home shift-right");
    }
    support::drive_until(cx, "preparation quiescence", |cx| {
        input.read_with(cx, |input, _| input.is_quiescent())
    });
    composer
        .update(cx, |composer, cx| {
            composer.test_set_shutdown_interaction_gated(true, cx)
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
    let (seed, protection) = composer.read_with(cx, |composer, _| {
        let snapshot = composer.recovery_snapshot().unwrap();
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
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let candidate = Arc::try_unwrap(store)
        .ok()
        .unwrap()
        .recover_same_home()
        .unwrap();
    let state = beryl_state::BerylState::reacquire_candidate(&candidate).unwrap();
    let storage = syndic_storage::SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let mut authentication_seed = seed;
    if scenario == Scenario::AuthenticationFailure {
        authentication_seed.history = None;
    }
    let mut preparation = cx.update(|_, app| {
        Preparation::prepare(
            candidate,
            retired,
            storage,
            state,
            authentication_seed,
            app,
            |_| {},
        )
    });
    assert!(preparation.authenticated_source().unwrap().is_none());
    cx.run_until_parked();
    if scenario == Scenario::AuthenticationFailure {
        assert!(preparation.authenticated_source().is_err());
        assert_resources_retained(&mut preparation);
        preparation.cancel();
        assert!(preparation.advance_cleanup().unwrap());
        close_resources(&mut preparation, Some(close.ticket));
        return;
    }
    let (fresh, selection) = preparation.authenticated_source().unwrap().unwrap();
    let mut current = RangePrepublicationCurrent {
        binding: fresh.binding,
        history: fresh.history,
        available_capacity: RangeSurfaceCharge { bytes: 0, items: 0 },
    };
    cx.update(|window, app| {
        input.update(app, |input, cx| {
            assert!(preparation.adopt(input, current, window, cx).is_err());
        });
    });
    assert_ne!(fresh.binding, seed.binding);
    let mut config =
        widget_support::widget_config(fresh.binding, selection.binding().presentation_generation());
    config.viewport_extent = px(96.);
    config.limits.max_realized_block_extent = config.viewport_extent;
    let capacity = RangeSurfaceCharge {
        bytes: config.limits.max_surface_bytes * 2,
        items: config.limits.max_surface_items * 2,
    };
    current.available_capacity = RangeSurfaceCharge {
        bytes: config.limits.max_surface_bytes,
        items: config.limits.max_surface_items,
    };
    let environment = cx.update(|window, _| {
        let cleanup = RangePrepublicationCleanupLedger::new(window.text_system(), 64).unwrap();
        RangePrepublicationEnvironment::new(9, config.clone(), window.text_system(), cleanup)
            .unwrap()
    });
    let mut wrong_config = config;
    wrong_config.binding = seed.binding;
    let wrong = cx.update(|window, _| {
        let cleanup = RangePrepublicationCleanupLedger::new(window.text_system(), 64).unwrap();
        RangePrepublicationEnvironment::new(10, wrong_config, window.text_system(), cleanup)
            .unwrap()
    });
    input.read_with(cx, |input, _| {
        assert!(
            preparation
                .admit(input, protection, wrong, capacity)
                .is_err()
        );
        assert!(
            preparation
                .admit(
                    input,
                    protection,
                    environment.clone(),
                    RangeSurfaceCharge { bytes: 0, items: 0 }
                )
                .is_err()
        );
        preparation
            .admit(input, protection, environment.clone(), capacity)
            .unwrap();
        assert!(
            preparation
                .admit(input, protection, environment.clone(), capacity)
                .is_err()
        );
    });
    if scenario == Scenario::ReadFailure {
        faults.fail_next(FaultPoint::BeforeReadConfirmation);
    }
    let mut finished = false;
    for step in 0..512 {
        let result = cx.update(|window, app| {
            input.update(app, |input, cx| {
                preparation.advance(input, window.text_system(), cx, |_| {})
            })
        });
        if scenario == Scenario::ProtectionChanged && step == 1 {
            input.update(cx, |input, cx| {
                assert!(
                    input
                        .set_presentation_generation(PresentationGeneration::new(99), cx)
                        .is_err()
                );
            });
        }
        if scenario == Scenario::CancelQueued && step == 0 {
            assert_eq!(result.unwrap(), Progress::Advancing);
            preparation.cancel();
            finished = true;
            break;
        }
        if scenario == Scenario::CancelReading && result == Ok(Progress::Waiting) {
            preparation.cancel();
            assert!(!preparation.advance_cleanup().unwrap());
            assert_resources_retained(&mut preparation);
            finished = true;
            break;
        }
        if matches!(
            scenario,
            Scenario::ReadFailure | Scenario::ProtectionChanged
        ) && result.is_err()
        {
            finished = true;
            break;
        }
        if result.unwrap() == Progress::Ready {
            assert!(!matches!(
                scenario,
                Scenario::ReadFailure | Scenario::ProtectionChanged
            ));
            if scenario == Scenario::WindowChanged {
                let changed = cx.update(|_, app| {
                    input.update(app, |input, cx| {
                        preparation.advance(
                            input,
                            foreign_text_system.as_ref().unwrap(),
                            cx,
                            |_| {},
                        )
                    })
                });
                assert!(changed.unwrap_err().contains("window changed"));
            }
            finished = true;
            break;
        }
        cx.run_until_parked();
    }
    assert!(finished);
    if matches!(
        scenario,
        Scenario::Adopt
            | Scenario::AdoptionProtection
            | Scenario::AdoptionCapacity
            | Scenario::AdoptionHistory
    ) {
        if scenario == Scenario::AdoptionProtection {
            input.update(cx, |input, cx| {
                assert!(
                    input
                        .set_presentation_generation(PresentationGeneration::new(99), cx)
                        .is_err()
                );
            });
        }
        if scenario == Scenario::AdoptionCapacity {
            current.available_capacity = RangeSurfaceCharge { bytes: 0, items: 0 };
        }
        if scenario == Scenario::AdoptionHistory {
            current.history = None;
        }
        let adopted = cx.update(|window, app| {
            input.update(app, |input, cx| {
                preparation.adopt(input, current, window, cx)
            })
        });
        cx.update(|window, app| {
            assert_eq!(window.focused(app), focus);
            input.update(app, |input, cx| {
                assert!(!input.is_enabled());
                assert!(preparation.adopt(input, current, window, cx).is_err());
            });
        });
        if scenario == Scenario::Adopt {
            let (candidate, source) = adopted.unwrap();
            assert_eq!(source.seed(), fresh);
            input.read_with(cx, |input, _| {
                assert_eq!(input.export_restoration(fresh.history).unwrap(), fresh);
                assert!(!input.resident_protection_is_current(protection));
            });
            assert_resources_retained(&mut preparation);
            drop((preparation, source));
            assert!(environment.cleanup().ownership().active > 0);
            cx.update(|window, app| {
                input.update(app, |input, cx| {
                    assert!(input.dispose(window, cx).is_empty());
                });
            });
            cx.update(|window, _| window.remove_window());
            drop((input, composer, root, mount));
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
            cx.run_until_parked();
            candidate.abort().close().unwrap();
            directory.close().unwrap();
            return;
        }
        assert!(adopted.is_err());
    }
    assert_resources_retained(&mut preparation);
    preparation.cancel();
    for _ in 0..128 {
        cx.run_until_parked();
        if preparation.advance_cleanup().unwrap() {
            break;
        }
    }
    assert!(preparation.advance_cleanup().unwrap());
    let ownership = environment.cleanup().ownership();
    assert_eq!(
        (
            ownership.active,
            ownership.ready,
            ownership.awaiting_acknowledgement
        ),
        (0, 0, 0)
    );
    input.read_with(cx, |input, _| {
        assert_eq!(
            input.resident_protection_is_current(protection),
            !matches!(
                scenario,
                Scenario::ProtectionChanged | Scenario::AdoptionProtection
            )
        );
        assert!(!input.is_enabled());
        assert_eq!(input.export_restoration(seed.history).unwrap(), seed);
    });
    close_resources(&mut preparation, None);
    assert_resources_retained(&mut preparation);
    drop((
        preparation,
        environment,
        input,
        composer,
        root,
        mount,
        directory,
    ));
}

fn assert_resources_retained(preparation: &mut Preparation) {
    assert!(preparation.take_cancelled_resources().is_none());
}

fn close_resources(
    preparation: &mut Preparation,
    refused_close: Option<beryl_app::main_window::MainWindowConversationComposerCloseTicket>,
) {
    let (candidate, source) = preparation.take_cancelled_resources().unwrap();
    if let Some(close) = refused_close {
        let (retired, _) = source.err().unwrap();
        assert_eq!(retired.close_ticket(), close);
        drop(retired);
    } else {
        assert!(source.is_ok());
        drop(source);
    }
    candidate.abort().close().unwrap();
}
