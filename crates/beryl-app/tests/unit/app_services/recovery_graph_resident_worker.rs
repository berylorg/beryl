use super::*;
use crate::main_window::MainWindowComposerCandidateWorker as Worker;
use gpui_text_input::*;

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Read,
    Refuse,
    Cancel,
}

#[test]
fn native_prepared_graph_worker_reads_and_returns_custody() {
    run(Mode::Read);
}

#[test]
fn native_prepared_graph_worker_returns_refused_resident() {
    run(Mode::Refuse);
}

#[test]
fn native_prepared_graph_worker_retains_cancelled_authentication() {
    run(Mode::Cancel);
}

fn run(mode: Mode) {
    let finished = Rc::new(Cell::new(false));
    let observed = finished.clone();
    Application::new()
        .with_quit_on_last_window_close(false)
        .run(move |app| {
            let progress = Rc::new(Cell::new("resident fixture"));
            let deadline_progress = progress.clone();
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
                eprintln!("graph worker fixture: {}", directory.path().display());
                progress.set("graph preparation");
                let reference = candidate.service_reference();
                let home = candidate.home_id();
                let generation = candidate.generation();
                let mut seed = composer
                    .read_with(cx, |composer, _| {
                        *composer.recovery_snapshot().unwrap().restoration()
                    })
                    .unwrap();
                if mode == Mode::Refuse {
                    seed.history = None;
                }
                let graph = cx
                    .background_executor()
                    .spawn(async move { prepared(candidate) })
                    .await;
                progress.set("authentication delivery");
                let (notify, notified) = futures_channel::oneshot::channel();
                let (mut worker, mut custody) = if mode == Mode::Cancel {
                    let (release, wait) = futures_channel::oneshot::channel();
                    let (mut worker, mut custody) = cx
                        .update(|app| {
                            Worker::prepare_graph_with(
                                graph,
                                retired,
                                seed,
                                app,
                                move |_| {
                                    notify.send(()).unwrap();
                                },
                                async move {
                                    wait.await.unwrap();
                                },
                            )
                        })
                        .unwrap();
                    assert!(custody.pending());
                    worker.cancel();
                    assert!(custody.take_resources().is_none());
                    assert!(custody.take_refused_resources().is_none());
                    assert!(!custody.cleanup_drained());
                    release.send(()).unwrap();
                    (worker, custody)
                } else {
                    cx.update(|app| {
                        Worker::prepare_graph(graph, retired, seed, app, move |_| {
                            notify.send(()).unwrap();
                        })
                    })
                    .unwrap()
                };
                notified.await.unwrap();
                progress.set("source read or refusal");
                assert!(!custody.pending());
                let (mut graph, source) = if mode == Mode::Refuse {
                    assert!(custody.take_resources().is_none());
                    let (graph, retired, error) = custody.take_refused_resources().unwrap();
                    assert_eq!(retired.close_ticket(), close);
                    assert!(error.contains("seed does not match"));
                    assert!(custody.take_refused_resources().is_none());
                    drop(retired);
                    (graph, None)
                } else {
                    assert_eq!(custody.source().unwrap().predecessor(), close);
                    if mode == Mode::Read {
                        let (mut session, environment, text_system) = window
                            .update(cx, |_, window, _| {
                                let source = custody.source().unwrap();
                                let (environment, _) = resident_fixture::environment(
                                    source.seed(),
                                    source.selection(),
                                    window,
                                )
                                .unwrap();
                                let session = RangePrepublicationSession::new(
                                    source.seed(),
                                    environment.clone(),
                                )
                                .unwrap();
                                (session, environment, window.text_system().clone())
                            })
                            .unwrap();
                        worker
                            .bind_prepublication(session.generation(), &environment)
                            .unwrap();
                        let mut counts = [0; 3];
                        for _ in 0..256 {
                            let step = session.service(&text_system);
                            for effect in step.effects {
                                counts[match &effect {
                                    RangePrepublicationEffect::ValidateOwner(_) => 0,
                                    RangePrepublicationEffect::Page { .. } => 1,
                                    RangePrepublicationEffect::ObjectPage { .. } => 2,
                                }] += 1;
                                let (notify, notified) = futures_channel::oneshot::channel();
                                cx.update(|app| {
                                    worker
                                        .start(effect, app, move |_| {
                                            notify.send(()).unwrap();
                                        })
                                        .unwrap()
                                })
                                .unwrap();
                                assert!(custody.pending());
                                assert!(custody.take_resources().is_none());
                                notified.await.unwrap();
                                assert!(!custody.pending());
                                assert!(custody.take_resources().is_none());
                                assert_eq!(
                                    custody.deliver_completion(&mut session).unwrap(),
                                    Some(RangePrepublicationDelivery::Accepted)
                                );
                                custody.drive_cleanup(2);
                            }
                            if step.status == RangePrepublicationStatus::Ready {
                                break;
                            }
                            assert!(!matches!(
                                step.status,
                                RangePrepublicationStatus::Failed(_)
                                    | RangePrepublicationStatus::Stale
                                    | RangePrepublicationStatus::Cancelled
                            ));
                        }
                        assert_eq!(session.status(), RangePrepublicationStatus::Ready);
                        assert!(counts.into_iter().all(|count| count > 0));
                        drop(session.take_candidate().unwrap());
                        drop(session);
                        worker.cancel();
                        for _ in 0..64 {
                            custody.drive_cleanup(2);
                            if custody.cleanup_drained() {
                                break;
                            }
                        }
                        assert!(custody.cleanup_drained());
                        let ownership = environment.cleanup().ownership();
                        assert_eq!(
                            (
                                ownership.active,
                                ownership.ready,
                                ownership.awaiting_acknowledgement
                            ),
                            (0, 0, 0)
                        );
                    } else {
                        assert!(custody.cancelled());
                        assert!(custody.cleanup_drained());
                    }
                    let (graph, source) = custody.take_resources().unwrap();
                    assert!(custody.take_resources().is_none());
                    (graph, Some(source))
                };
                assert!(graph.matches_candidate(home, generation));
                assert_eq!(
                    reference.health().state(),
                    beryl_home_store::HomeHealthState::Reopening
                );
                assert!(reference.home_revision().is_err());
                drop((worker, custody));
                progress.set("graph disposal");
                cx.background_executor()
                    .spawn(async move {
                        drop(source);
                        graph.cancel().close().unwrap();
                    })
                    .await;
                progress.set("native cleanup");
                window
                    .update(cx, |root, window, _| {
                        assert_eq!(root.test_exit_presentation().0, "Exiting…");
                        window.remove_window();
                    })
                    .unwrap();
                drop((composer, mount, drafts, shell));
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(200))
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
                panic!(
                    "prepared graph worker completion deadline: {}",
                    deadline_progress.get()
                );
            })
            .detach();
        });
    assert!(finished.get());
}
