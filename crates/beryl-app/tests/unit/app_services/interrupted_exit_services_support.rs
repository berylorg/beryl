mod service_driver {
    use super::*;
    include!("recovery_service_driver_support.rs");
}

mod retry_delay {
    use super::*;
    include!("recovery_preparation_delay_support.rs");
}

mod preparation_attempt {
    use super::*;
    include!("recovery_preparation_attempt_support.rs");
}

mod attachment_driver {
    use super::*;
    include!("recovery_threadless_driver_support.rs");
}

mod threadless_attachment {
    use super::*;
    include!("prepared_threadless_attachment_support.rs");
}

mod appearance_attachment {
    use super::*;
    include!("prepared_appearance_attachment_support.rs");
}

mod service_publication {
    use super::*;
    include!("prepared_service_publication_support.rs");
}

mod session_validation {
    use super::*;
    include!("prepared_session_validation_support.rs");
}

pub(super) async fn verify(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    candidate: InterruptedExitCandidate,
    generation: beryl_home_store::HomeGeneration,
    window: beryl_model::WindowId,
    faults: &FaultController,
    previous_appearance: &gpui::Entity<crate::theme_runtime::GpuiAppearanceWindowSet>,
    publication_delivery: RecoveryPublicationDelivery,
    cx: &mut AsyncApp,
) -> gpui::Entity<crate::theme_runtime::GpuiAppearanceWindowSet> {
    use crate::app_services::recovery_graph::RecoveryServicePreparationError;
    use beryl_home_store::CommandCancellation;
    let mut candidate = Some(candidate);
    let foreign = request.test_foreign();
    let original = format!("{:?}", owner.borrow().interrupted_exit_session().unwrap());
    let mut stale_source = None;
    let mut attached_appearance = None;
    let mut previous_delay = None;
    for mode in [
        "validation_failure",
        "theme_failure",
        "success",
        "dropped",
        "cancel",
        "publish",
    ] {
        if stale_source.is_none() {
            let fresh = candidate.take().unwrap();
            let (fresh, source) = cx
                .background_executor()
                .spawn(async move {
                    let mut fresh = fresh;
                    let state = BerylState::reacquire_candidate(&fresh.candidate).unwrap();
                    let home = fresh.candidate.home_id();
                    let access = fresh.candidate.recovery_access().unwrap();
                    let source = ThreadlessRecoveryWindow::prepare(
                        &access, &state, home, generation, window,
                    )
                    .unwrap();
                    drop(access);
                    (fresh, source)
                })
                .await;
            candidate = Some(fresh);
            stale_source = Some(source);
        }
        let fresh = candidate.take().unwrap();
        let (fresh, appearance) = cx
            .background_executor()
            .spawn(async move {
                let state = BerylState::reacquire_candidate(&fresh.candidate).unwrap();
                let appearance = crate::theme_runtime::AppearanceCoordinator::new(
                    crate::theme_runtime::AppearanceCoordinatorConfig::new(
                        NonZeroUsize::new(4).unwrap(),
                    ),
                    native_appearance::system_font_appearance(&state),
                )
                .current();
                (fresh, appearance)
            })
            .await;
        candidate = Some(fresh);
        let appearance = cx
            .update(|app| {
                crate::theme_runtime::GpuiAppearanceWindowSet::new(
                    appearance,
                    NonZeroUsize::new(4).unwrap(),
                    app,
                )
            })
            .unwrap();
        let fresh = candidate.as_ref().unwrap();
        let home = fresh.candidate.home_id();
        let fresh_generation = fresh.candidate.generation();
        let requirement = configuration()
            .projection
            .turn_start_admission_requirement();
        let adapters = |request| {
            owner.borrow().interrupted_exit_composer_adapters(
                request,
                home,
                fresh_generation,
                requirement,
            )
        };
        cx.update(|app| {
            threadless_attachment::assert_unavailable(owner, request, app);
            appearance_attachment::assert_unavailable(owner, request, &appearance, app);
            assert!(
                RunningProcessOwner::prepare_interrupted_exit_threadless_window(
                    owner,
                    request,
                    home,
                    generation,
                    window,
                    app,
                    |_, _, _| panic!("missing graph callback"),
                )
                .is_err()
            );
        })
        .unwrap();
        assert!(adapters(request).is_err());
        let (sender, receiver) = futures_channel::oneshot::channel();
        cx.update(|app| {
            RunningProcessOwner::settle_interrupted_exit_candidate(
                owner,
                request,
                &mut candidate,
                app,
                move |_, _| {
                    sender.send(()).unwrap();
                },
            )
            .unwrap();
        })
        .unwrap();
        receiver.await.unwrap();
        owner
            .borrow()
            .interrupted_exit_candidate_result(request)
            .unwrap();
        let cancellation = CommandCancellation::new();
        if mode == "validation_failure" {
            faults.fail_next(FaultPoint::BeforeReadConfirmation);
        }
        if mode == "theme_failure" {
            faults.fail_next(FaultPoint::BeforeThemeWatchSpawn);
        }
        let canceller = if mode == "cancel" {
            let block = faults.block_next(FaultPoint::BeforeThemeWatchSpawn);
            let cancellation = cancellation.clone();
            Some(std::thread::spawn(move || {
                let reached = block.wait_until_reached(std::time::Duration::from_secs(10));
                cancellation.cancel();
                block.release();
                assert!(reached);
            }))
        } else {
            None
        };
        service_driver::verify(owner, request, generation, cancellation, mode, cx).await;
        if let Some(canceller) = canceller {
            canceller.join().unwrap();
        }
        assert!(adapters(request).is_err());
        assert!(
            owner
                .borrow()
                .interrupted_exit_services_result(request)
                .unwrap_err()
                .contains("request changed")
        );
        owner
            .borrow_mut()
            .test_replace_interrupted_exit_request(request);
        if mode == "validation_failure" {
            service_driver::verify_disposed_validation_failure(owner, request, generation, cx)
                .await;
            let deadline = owner
                .borrow()
                .interrupted_exit_reopen_deadline(request)
                .unwrap()
                .unwrap();
            previous_delay = Some(
                retry_delay::verify(owner, request, generation, deadline, previous_delay, cx).await,
            );
            RunningProcessOwner::construct_and_settle_interrupted_exit(
                owner,
                request,
                generation,
                CommandCancellation::new(),
                cx,
            )
            .await
            .unwrap();
            assert_eq!(
                original,
                format!("{:?}", owner.borrow().interrupted_exit_session().unwrap())
            );
            assert!(!RunningProcessOwner::finish_exit(owner, request));
            candidate = Some(owner.borrow().test_take_interrupted_exit_candidate());
            continue;
        }
        assert_eq!(
            owner
                .borrow()
                .interrupted_exit_services_result(request)
                .is_ok(),
            matches!(mode, "success" | "dropped" | "publish")
        );
        if matches!(mode, "success" | "dropped" | "publish") {
            session_validation::verify(owner, request, home, fresh_generation, generation, cx)
                .await;
            assert!(
                owner
                    .borrow_mut()
                    .take_interrupted_exit_preparation_failure(request, generation)
                    .is_err()
            );
        }
        assert!(adapters(&foreign).is_err());
        let substituted_appearance = appearance;
        let appearance = if matches!(mode, "success" | "dropped" | "publish") {
            let prepared = owner.borrow().interrupted_exit_appearance(request).unwrap();
            cx.update(|app| {
                crate::theme_runtime::GpuiAppearanceWindowSet::new(
                    prepared,
                    NonZeroUsize::new(4).unwrap(),
                    app,
                )
            })
            .unwrap()
        } else {
            assert!(owner.borrow().interrupted_exit_appearance(request).is_err());
            substituted_appearance.clone()
        };
        let retained_marker = if matches!(mode, "success" | "dropped" | "publish") {
            for retirement in [None, Some(Err("failed retirement".into()))] {
                owner
                    .borrow()
                    .test_set_resident_graph_retirement(retirement);
                assert!(adapters(request).is_err());
            }
            owner
                .borrow()
                .test_set_resident_graph_retirement(Some(Ok(())));
            assert!(
                owner
                    .borrow()
                    .interrupted_exit_composer_adapters(request, home, generation, requirement,)
                    .is_err()
            );
            assert!(
                owner
                    .borrow()
                    .interrupted_exit_composer_adapters(
                        request,
                        beryl_model::BerylHomeId::from_bytes([99; 16]),
                        fresh_generation,
                        requirement,
                    )
                    .is_err()
            );
            if mode == "publish" {
                cx.update(|app| {
                    attached_appearance
                        .as_ref()
                        .unwrap_or(previous_appearance)
                        .update(app, |set, _| set.retire());
                })
                .unwrap();
                attachment_driver::verify(
                    owner,
                    request,
                    home,
                    generation,
                    &appearance,
                    !matches!(
                        publication_delivery,
                        RecoveryPublicationDelivery::Driven
                            | RecoveryPublicationDelivery::DrivenCancelled
                            | RecoveryPublicationDelivery::DrivenStale
                    ),
                    cx,
                )
                .await;
            } else {
                for (source_home, source_generation, source_window, stale, succeeds) in [
                    (
                        beryl_model::BerylHomeId::from_bytes([99; 16]),
                        generation,
                        window,
                        false,
                        false,
                    ),
                    (home, fresh_generation, window, false, false),
                    (
                        home,
                        generation,
                        beryl_model::WindowId::from_bytes([99; 16]),
                        false,
                        false,
                    ),
                    (home, generation, window, true, false),
                    (home, generation, window, false, true),
                ] {
                    let (sender, receiver) = futures_channel::oneshot::channel();
                    cx.update(|app| {
                        let refused = |request, app: &mut gpui::App| {
                            RunningProcessOwner::prepare_interrupted_exit_threadless_window(
                                owner,
                                request,
                                home,
                                generation,
                                window,
                                app,
                                |_, _, _| panic!("refused window authentication callback"),
                            )
                        };
                        assert!(refused(&foreign, app).is_err());
                        for retirement in [None, Some(Err("failed retirement".into()))] {
                            owner
                                .borrow()
                                .test_set_resident_graph_retirement(retirement);
                            assert!(refused(request, app).is_err());
                        }
                        owner
                            .borrow()
                            .test_set_resident_graph_retirement(Some(Ok(())));
                        RunningProcessOwner::prepare_interrupted_exit_threadless_window(
                            owner,
                            request,
                            source_home,
                            source_generation,
                            source_window,
                            app,
                            move |owner, result, _| {
                                assert!(owner.borrow().interrupted_exit_session().is_some());
                                assert!(owner.borrow().test_services().graph().is_none());
                                assert!(sender.send(result).is_ok());
                            },
                        )
                        .unwrap();
                        threadless_attachment::assert_unavailable(owner, request, app);
                        appearance_attachment::assert_unavailable(owner, request, &appearance, app);
                        assert!(refused(request, app).is_err());
                        assert!(adapters(request).is_err());
                        assert!(!RunningProcessOwner::finish_exit(owner, request));
                        if stale {
                            owner
                                .borrow_mut()
                                .test_replace_interrupted_exit_request(&foreign);
                        }
                    })
                    .unwrap();
                    let result = receiver.await.unwrap();
                    assert_eq!(result.is_ok(), succeeds);
                    if let Ok(authenticated) = result {
                        assert_eq!(authenticated.home_id(), home);
                        assert_eq!(authenticated.generation(), fresh_generation);
                        assert_eq!(authenticated.window().window_id(), window);
                        cx.update(|app| {
                            threadless_attachment::verify(
                                owner,
                                request,
                                authenticated,
                                &mut stale_source,
                                app,
                            );
                            appearance_attachment::verify(
                                owner,
                                request,
                                &appearance,
                                &substituted_appearance,
                                attached_appearance.as_ref().unwrap_or(previous_appearance),
                                app,
                            );
                            attached_appearance = Some(appearance.clone());
                        })
                        .unwrap();
                    }
                    if stale {
                        owner
                            .borrow_mut()
                            .test_replace_interrupted_exit_request(request);
                    }
                    owner
                        .borrow()
                        .interrupted_exit_services_result(request)
                        .unwrap();
                }
            }
            let prepared = adapters(request).unwrap();
            assert!(prepared.matches(home, fresh_generation));
            let (_, marker, _, _) = prepared.into_parts();
            drop(adapters(request).unwrap());
            assert!(!marker.test_generation_retired());
            Some(marker)
        } else {
            assert!(adapters(request).is_err());
            None
        };
        assert_eq!(
            original,
            format!("{:?}", owner.borrow().interrupted_exit_session().unwrap())
        );
        assert!(owner.borrow().test_services().graph().is_none());
        assert!(!RunningProcessOwner::finish_exit(owner, request));
        if mode == "publish" {
            service_publication::verify(
                owner,
                request,
                generation,
                fresh_generation,
                &appearance,
                publication_delivery,
                cx,
            )
            .await;
            return appearance;
        }
        let (sender, receiver) = futures_channel::oneshot::channel();
        cx.update(|app| {
            let cancel = |request, app: &mut gpui::App| {
                RunningProcessOwner::cancel_interrupted_exit_services(
                    owner,
                    request,
                    app,
                    |_, _| panic!("refused cancellation callback"),
                )
            };
            assert!(cancel(&foreign, app).is_err());
            if !matches!(mode, "success" | "dropped" | "publish") {
                assert!(cancel(request, app).is_err());
                return;
            }
            for retirement in [None, Some(Err("failed retirement".into()))] {
                owner
                    .borrow()
                    .test_set_resident_graph_retirement(retirement);
                assert!(cancel(request, app).is_err());
            }
            owner
                .borrow()
                .test_set_resident_graph_retirement(Some(Ok(())));
            RunningProcessOwner::cancel_interrupted_exit_services(
                owner,
                request,
                app,
                move |owner, _| {
                    assert!(!owner.borrow().test_services_on_worker());
                    assert!(owner.borrow().interrupted_exit_session().is_some());
                    sender.send(()).unwrap();
                },
            )
            .unwrap();
            assert!(owner.borrow().test_services_on_worker());
            assert!(owner.borrow().interrupted_exit_session().is_none());
            assert!(cancel(request, app).is_err());
            assert!(!RunningProcessOwner::finish_exit(owner, request));
            owner
                .borrow_mut()
                .test_replace_interrupted_exit_request(&foreign);
        })
        .unwrap();
        if matches!(mode, "success" | "dropped" | "publish") {
            receiver.await.unwrap();
            assert!(
                owner
                    .borrow()
                    .interrupted_exit_services_result(request)
                    .unwrap_err()
                    .contains("request changed")
            );
            owner
                .borrow_mut()
                .test_replace_interrupted_exit_request(request);
            assert!(adapters(request).is_err());
            assert!(retained_marker.as_ref().unwrap().test_generation_retired());
            assert_eq!(
                original,
                format!("{:?}", owner.borrow().interrupted_exit_session().unwrap())
            );
            assert!(owner.borrow().test_services().graph().is_none());
            assert!(!RunningProcessOwner::finish_exit(owner, request));
        }
        cx.update(|app| {
            threadless_attachment::assert_unavailable(owner, request, app);
            appearance_attachment::assert_unavailable(owner, request, &appearance, app);
            assert!(
                RunningProcessOwner::prepare_interrupted_exit_threadless_window(
                    owner,
                    request,
                    home,
                    generation,
                    window,
                    app,
                    |_, _, _| panic!("failed graph callback"),
                )
                .is_err()
            );
        })
        .unwrap();
        let evidence = owner
            .borrow()
            .interrupted_exit_services_result(request)
            .unwrap_err();
        let returned_deadline = owner
            .borrow()
            .interrupted_exit_reopen_deadline(request)
            .unwrap();
        assert_eq!(
            returned_deadline.is_some(),
            matches!(mode, "theme_failure" | "cancel")
        );
        assert!(
            owner
                .borrow_mut()
                .take_interrupted_exit_preparation_failure(&foreign, generation)
                .is_err()
        );
        for retirement in [None, Some(Err("failed retirement".into()))] {
            owner
                .borrow()
                .test_set_resident_graph_retirement(retirement);
            assert!(
                owner
                    .borrow_mut()
                    .take_interrupted_exit_preparation_failure(request, generation)
                    .is_err()
            );
        }
        owner
            .borrow()
            .test_set_resident_graph_retirement(Some(Ok(())));
        assert!(
            owner
                .borrow_mut()
                .take_interrupted_exit_preparation_failure(request, fresh_generation)
                .is_err()
        );
        assert_eq!(
            owner
                .borrow()
                .interrupted_exit_reopen_deadline(request)
                .unwrap(),
            returned_deadline
        );
        let failure = owner
            .borrow_mut()
            .take_interrupted_exit_preparation_failure(request, generation)
            .unwrap();
        let deadline = owner
            .borrow()
            .interrupted_exit_reopen_deadline(request)
            .unwrap()
            .unwrap();
        if let Some(returned_deadline) = returned_deadline {
            assert_eq!(deadline, returned_deadline);
        }
        assert!(
            owner
                .borrow_mut()
                .take_interrupted_exit_preparation_failure(request, generation)
                .is_err()
        );
        assert_eq!(evidence, format!("{failure:?}"));
        assert_eq!(
            owner
                .borrow()
                .interrupted_exit_reopen_deadline(request)
                .unwrap(),
            Some(deadline)
        );
        assert_eq!(
            original,
            format!("{:?}", owner.borrow().interrupted_exit_session().unwrap())
        );
        assert!(!RunningProcessOwner::finish_exit(owner, request));
        match failure {
            RecoveryServicePreparationError::App(failure) => {
                if mode != "theme_failure" {
                    assert!(matches!(failure.error(), AppServiceOpenError::Cancelled));
                } else {
                    assert!(matches!(failure.error(), AppServiceOpenError::Theme(_)));
                }
                assert!(failure.into_retry_parts().is_err());
            }
            error => panic!("unexpected preparation failure: {error:?}"),
        }
        previous_delay = Some(
            retry_delay::verify(owner, request, generation, deadline, previous_delay, cx).await,
        );
        if mode == "theme_failure" {
            previous_delay = Some(
                preparation_attempt::verify(
                    owner,
                    request,
                    generation,
                    fresh_generation,
                    faults,
                    previous_delay.unwrap(),
                    cx,
                )
                .await,
            );
        }
        let (sender, receiver) = futures_channel::oneshot::channel();
        cx.update(|app| {
            RunningProcessOwner::construct_interrupted_exit_candidate(
                owner,
                request,
                generation,
                CommandCancellation::new(),
                app,
                move |_, _| {
                    sender.send(()).unwrap();
                },
            )
            .unwrap();
        })
        .unwrap();
        receiver.await.unwrap();
        owner
            .borrow()
            .interrupted_exit_construction_result(request)
            .unwrap();
        let (sender, receiver) = futures_channel::oneshot::channel();
        cx.update(|app| {
            RunningProcessOwner::settle_constructed_exit_candidate(
                owner,
                request,
                app,
                move |_, _| {
                    sender.send(()).unwrap();
                },
            )
            .unwrap();
        })
        .unwrap();
        receiver.await.unwrap();
        owner
            .borrow()
            .interrupted_exit_candidate_result(request)
            .unwrap();
        assert_eq!(
            original,
            format!("{:?}", owner.borrow().interrupted_exit_session().unwrap())
        );
        assert!(!RunningProcessOwner::finish_exit(owner, request));
        candidate = Some(owner.borrow().test_take_interrupted_exit_candidate());
    }
    unreachable!("the final fixture pass publishes its graph")
}
