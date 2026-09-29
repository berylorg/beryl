use super::*;
use crate::main_window::{
    MainWindowComposerRecoveryPreparation as Preparation,
    MainWindowComposerRecoveryProgress as Progress,
};
use gpui_text_input::*;

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Adopt,
    Cancel,
    Refuse,
}

#[test]
fn native_prepared_graph_resident_adopts_and_returns_graph() {
    run(Mode::Adopt);
}
#[test]
fn native_prepared_graph_resident_cancels_queued_realization() {
    run(Mode::Cancel);
}
#[test]
fn native_prepared_graph_resident_returns_authentication_refusal() {
    run(Mode::Refuse);
}

fn run(mode: Mode) {
    let finished = Rc::new(Cell::new(false));
    let observed = finished.clone();
    Application::new()
        .with_quit_on_last_window_close(false)
        .run(move |app| {
            app.spawn(async move |cx: &mut AsyncApp| {
                let resident_fixture::Resident {
                    window,
                    composer,
                    close,
                    candidate,
                    retired,
                    directory,
                    mount,
                    drafts,
                    shell,
                } = resident_fixture::prepare(cx).await;
                eprintln!("graph preparation fixture: {}", directory.path().display());
                let home = candidate.home_id();
                let generation = candidate.generation();
                let reference = candidate.service_reference();
                let (mut seed, protection, input) = composer
                    .read_with(cx, |composer, _| {
                        let snapshot = composer.recovery_snapshot().unwrap();
                        (
                            *snapshot.restoration(),
                            snapshot.protection(),
                            composer.gpui_input(),
                        )
                    })
                    .unwrap();
                if mode == Mode::Refuse {
                    seed.history = None;
                }
                let graph = cx
                    .background_executor()
                    .spawn(async move { prepared(candidate) })
                    .await;
                let (notify, notified) = futures_channel::oneshot::channel();
                let mut preparation = cx
                    .update(|app| {
                        Preparation::prepare_graph(graph, retired, seed, app, move |_| {
                            notify.send(()).unwrap();
                        })
                    })
                    .unwrap();
                assert!(preparation.authenticated_source().unwrap().is_none());
                assert!(preparation.take_cancelled_resources().is_none());
                notified.await.unwrap();
                let mut cleanup = None;
                let (mut graph, source, service) = if mode == Mode::Refuse {
                    assert!(
                        preparation
                            .authenticated_source()
                            .unwrap_err()
                            .contains("seed does not match")
                    );
                    preparation.cancel();
                    assert!(preparation.advance_cleanup().unwrap());
                    let (graph, refused) = preparation.take_cancelled_resources().unwrap();
                    let Err((retired, _)) = refused else {
                        panic!("expected refusal")
                    };
                    assert_eq!(retired.close_ticket(), close);
                    (graph, None, None)
                } else {
                    let (fresh, selection) = preparation.authenticated_source().unwrap().unwrap();
                    let (environment, capacity) = window
                        .update(cx, |_, window, _| {
                            resident_fixture::environment(fresh, selection, window).unwrap()
                        })
                        .unwrap();
                    cleanup = Some(environment.cleanup().clone());
                    input
                        .read_with(cx, |input, _| {
                            preparation
                                .admit(input, protection, environment.clone(), capacity)
                                .unwrap()
                        })
                        .unwrap();
                    let mut ready = false;
                    for _ in 0..512 {
                        let (notify, notified) = futures_channel::oneshot::channel();
                        let progress = window
                            .update(cx, |_, window, app| {
                                input.update(app, |input, cx| {
                                    preparation.advance(
                                        input,
                                        window.text_system(),
                                        cx,
                                        move |_| {
                                            let _ = notify.send(());
                                        },
                                    )
                                })
                            })
                            .unwrap()
                            .unwrap();
                        if mode == Mode::Cancel {
                            assert_eq!(progress, Progress::Advancing);
                            break;
                        }
                        if progress == Progress::Ready {
                            ready = true;
                            break;
                        }
                        if progress == Progress::Waiting {
                            notified.await.unwrap();
                        }
                    }
                    if mode == Mode::Adopt {
                        assert!(ready);
                        let current = RangePrepublicationCurrent {
                            binding: fresh.binding,
                            history: fresh.history,
                            available_capacity: RangeSurfaceCharge {
                                bytes: environment.config().limits.max_surface_bytes,
                                items: environment.config().limits.max_surface_items,
                            },
                        };
                        let (graph, service, fresh_close) = window
                            .update(cx, |_, window, app| {
                                composer.update(app, |resident, cx| {
                                    preparation.adopt_resident(resident, close, current, window, cx)
                                })
                            })
                            .unwrap()
                            .unwrap();
                        assert_ne!(fresh_close, close);
                        composer
                            .read_with(cx, |resident, _| {
                                assert!(resident.recovery_snapshot().is_none())
                            })
                            .unwrap();
                        (graph, None, Some(service))
                    } else {
                        preparation.cancel();
                        for _ in 0..64 {
                            if preparation.advance_cleanup().unwrap() {
                                break;
                            }
                        }
                        assert!(preparation.advance_cleanup().unwrap());
                        let (graph, source) = preparation.take_cancelled_resources().unwrap();
                        let Ok(source) = source else {
                            panic!("authenticated source lost")
                        };
                        assert_eq!(source.predecessor(), close);
                        (graph, Some(source), None)
                    }
                };
                assert!(preparation.take_cancelled_resources().is_none());
                assert!(graph.matches_candidate(home, generation));
                assert_eq!(
                    reference.health().state(),
                    beryl_home_store::HomeHealthState::Reopening
                );
                assert!(reference.home_revision().is_err());
                drop(preparation);
                window
                    .update(cx, |root, window, _| {
                        assert_eq!(root.test_exit_presentation().0, "Exiting…");
                        window.remove_window();
                    })
                    .unwrap();
                drop((input, composer, mount, drafts, shell));
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(200))
                    .await;
                if let Some(cleanup) = cleanup {
                    let ownership = cleanup.ownership();
                    assert_eq!(
                        (
                            ownership.active,
                            ownership.ready,
                            ownership.awaiting_acknowledgement
                        ),
                        (0, 0, 0)
                    );
                }
                cx.background_executor()
                    .spawn(async move {
                        drop((source, service));
                        graph.cancel().close().unwrap();
                    })
                    .await;
                directory.close().unwrap();
                observed.set(true);
                cx.update(|app| app.quit()).unwrap();
            })
            .detach();
            app.spawn(async move |cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(15))
                    .await;
                panic!("prepared graph resident completion deadline");
            })
            .detach();
        });
    assert!(finished.get());
}
