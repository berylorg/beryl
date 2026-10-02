use super::{composer, support, widget_support};
use beryl_app::main_window::{
    MainWindowComposerRecoveryProgress as Progress,
    MainWindowFailedResidentCandidateSource as Source,
    MainWindowFailedResidentPreparation as Preparation,
};
use beryl_home_store::{CommandCancellation, HomeHealthState, test_faults::FaultPoint};
use gpui::{EntityInputHandler, TestAppContext, px};
use gpui_text_input::*;
use std::sync::Arc;
use syndic_storage::{DraftEditorCandidatePublicationEvidenceV1 as Evidence, SyndicTimestamp};

#[gpui::test]
fn unfinished_marker_flight_refuses_failed_mount_detachment_without_losing_custody(
    cx: &mut TestAppContext,
) {
    let mut faults = None;
    let (fixture, cx) = support::mounted_observing_faults(
        cx,
        "failed-marker-flight-refusal",
        171,
        Box::new(support::configure),
        |_| support::submission_source(),
        |value| faults = Some(value),
    );
    let selection = fixture.service.selected_identity().unwrap();
    let request = beryl_app::composer_marker_seal::DraftMarkerSealFlightRequest::new(
        selection.binding().candidate(),
        syndic_storage::DraftMarkerSealOperationIdV1::from_bytes([172; 16]),
        beryl_state::AssetReferenceSetStagingAuthority::new(
            beryl_model::AssetReferenceSetId::from_bytes([173; 16]),
            [174; 32],
        ),
    );
    assert!(matches!(
        fixture
            .seals
            .admit(&fixture.store, request, &CommandCancellation::new())
            .unwrap(),
        beryl_app::composer_marker_seal::DraftMarkerSealAdmission::Admitted(_)
    ));
    assert_eq!(fixture.seals.diagnostics().current_flights(), 1);
    faults
        .unwrap()
        .fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(fixture.store.home_revision().is_err());
    let ticket = fixture
        .mount
        .update(cx, |mount, cx| mount.begin_failed_resident(cx))
        .unwrap();
    let mut capture = None;
    support::drive_until(cx, "marker flight resident capture", |cx| {
        capture = fixture
            .mount
            .update(cx, |mount, cx| mount.capture_failed_resident(ticket, cx))
            .unwrap();
        capture.is_some()
    });
    let capture = capture.unwrap();
    assert!(
        fixture
            .mount
            .update(cx, |mount, cx| mount
                .detach_failed_resident_resources(&capture, cx))
            .is_err()
    );
    assert_eq!(fixture.seals.diagnostics().current_flights(), 1);
    assert_eq!(fixture.service.selected_identity(), Some(selection));
    let resources = beryl_app::main_window::MainWindowFailedResidentMountResources {
        resident: beryl_app::main_window::MainWindowComposerRecoveryResources {
            service: Some(fixture.service.clone()),
            clipboard_writer: None,
            mutation_failure: None,
        },
        service: Some(fixture.service.clone()),
        publication_adapters: Some((fixture.assets.clone(), fixture.seals.clone())),
        configurator: None,
        submission_source: None,
        native_lineage_control: None,
    };
    let resources = resources
        .retire()
        .err()
        .expect("owned unfinished seal authority must refuse retirement before dropping adapters");
    assert!(
        resources
            .service
            .as_ref()
            .is_some_and(|service| Arc::ptr_eq(service, &fixture.service))
    );
    assert_eq!(
        resources
            .publication_adapters
            .as_ref()
            .unwrap()
            .1
            .diagnostics()
            .current_flights(),
        1
    );
    assert!(
        fixture
            .mount
            .read_with(cx, |mount, _| mount.contribution().is_some())
    );
}

#[gpui::test]
fn saved_failed_editor_reconstructs_same_widget_without_save(cx: &mut TestAppContext) {
    run(cx, false, Scenario::Adopt);
}

#[gpui::test]
fn genuinely_dirty_failed_editor_saves_then_reconstructs_exact_view_and_history(
    cx: &mut TestAppContext,
) {
    run(cx, true, Scenario::Adopt);
}

#[gpui::test]
fn failed_editor_cancellation_retains_actual_worker_and_queued_cleanup_custody(
    cx: &mut TestAppContext,
) {
    run(cx, true, Scenario::CancelReading);
}

#[gpui::test]
fn failed_editor_capacity_refusal_keeps_protected_predecessor_and_result_custody(
    cx: &mut TestAppContext,
) {
    run(cx, true, Scenario::Capacity);
}

#[gpui::test]
fn failed_editor_read_failure_retains_candidate_source_and_protected_predecessor(
    cx: &mut TestAppContext,
) {
    run(cx, true, Scenario::ReadFailure);
}

#[derive(Clone, Copy, PartialEq)]
enum Scenario {
    Adopt,
    CancelReading,
    Capacity,
    ReadFailure,
}

fn run(cx: &mut TestAppContext, dirty: bool, scenario: Scenario) {
    let mut faults = None;
    let (fixture, cx) = support::mounted_observing_faults(
        cx,
        "failed-resident-widget",
        111,
        Box::new(support::configure),
        |_| support::submission_source(),
        |value| faults = Some(value),
    );
    let faults = faults.unwrap();
    let resident = fixture
        .mount
        .read_with(cx, |mount, _| mount.contribution())
        .unwrap();
    let input = resident.read_with(cx, |resident, _| resident.gpui_input());
    support::drive_until(cx, "actual editor interactive", |cx| {
        input.read_with(cx, |input, _| input.is_enabled() && input.is_quiescent())
    });
    let focus = cx.update(|window, app| {
        input.update(app, |input, cx| {
            input.focus(window);
            if dirty {
                input.replace_and_mark_text_in_range(None, "retained live edit", None, window, cx);
            }
        });
        window.focused(app)
    });
    if dirty {
        support::drive_until(cx, "actual dirty edit adopted", |cx| {
            input.read_with(cx, |input, _| {
                input
                    .surface()
                    .is_some_and(|surface| surface.binding().extent().byte_len() == 18)
                    && input.is_quiescent()
            })
        });
        cx.simulate_keystrokes("ctrl-home shift-right");
        support::drive_until(cx, "view restored before failure", |cx| {
            input.read_with(cx, |input, _| input.is_quiescent())
        });
    }
    let old = resident.read_with(cx, |resident, _| resident.selection_identity());
    if dirty {
        let current = fixture
            .storage
            .current_draft(
                &fixture.store,
                old.claim().thread_id(),
                syndic_storage::SyndicPointReadLimit::new(65_536).unwrap(),
            )
            .unwrap()
            .unwrap();
        let selector = syndic_storage::DraftEditorCurrentSelectorV1::new(
            current.thread().id(),
            current.thread().revision(),
            current.draft().id(),
            current.draft().revision(),
            current.draft().piece_root(),
            current.draft().history(),
        );
        assert!(
            !fixture
                .storage
                .draft_editor_candidate_is_saved(
                    &fixture.store,
                    old.binding().candidate(),
                    selector
                )
                .unwrap()
        );
    }
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(fixture.store.home_revision().is_err());
    assert_eq!(fixture.store.health().state(), HomeHealthState::Failed);
    let ticket = fixture
        .mount
        .update(cx, |mount, cx| mount.begin_failed_resident(cx))
        .unwrap();
    let worker = fixture.mount.read_with(cx, |mount, _| {
        mount.test_pending_cleanup_worker(std::future::pending())
    });
    assert!(
        fixture
            .mount
            .update(cx, |mount, cx| mount.capture_failed_resident(ticket, cx))
            .unwrap()
            .is_none()
    );
    drop(worker);
    let mut capture = None;
    support::drive_until(cx, "failed capture drain", |cx| {
        capture = fixture
            .mount
            .update(cx, |mount, cx| mount.capture_failed_resident(ticket, cx))
            .unwrap();
        capture.is_some()
    });
    let capture = capture.unwrap();
    let seed = capture.restoration();
    assert_eq!(capture.selection(), old);
    assert!(input.read_with(cx, |input, _| !input.is_enabled()
        && input.resident_protection_is_current(capture.protection())));
    assert!(
        fixture
            .mount
            .update(cx, |mount, cx| mount.capture_failed_resident(ticket, cx))
            .is_err()
    );
    assert!(
        cx.update(|window, app| fixture
            .mount
            .update(app, |mount, cx| mount.begin_window_close(window, cx)))
            .is_err()
    );
    let resources = fixture
        .mount
        .update(cx, |mount, cx| {
            mount.detach_failed_resident_resources(&capture, cx)
        })
        .unwrap();
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
    let resources = resources
        .retire()
        .err()
        .expect("external service reference must retain retirement custody");
    drop(service);
    let retired = resources
        .retire()
        .ok()
        .expect("complete drained resources retire failed host");
    drop((storage, assets, seals));
    let mut candidate = Arc::try_unwrap(store)
        .ok()
        .unwrap()
        .recover_same_home()
        .unwrap();
    let state = beryl_state::BerylState::reacquire_candidate(&candidate).unwrap();
    let storage = syndic_storage::SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let mut retired = retired;
    let revision = candidate
        .recovery_access()
        .unwrap()
        .home_revision()
        .unwrap();
    assert_eq!(
        retired
            .qualify_saved(&mut candidate, &storage, &state, None)
            .unwrap(),
        !dirty
    );
    if dirty {
        retired
            .publish_candidate(
                &mut candidate,
                &storage,
                &state,
                None,
                composer::operation_id(501),
                SyndicTimestamp::from_unix_millis(501),
                Evidence::UnchangedEmpty,
                CommandCancellation::new(),
            )
            .unwrap();
    }
    let source = Source::new(&mut candidate, retired, storage.clone(), &state, seed, None)
        .ok()
        .expect("same exact resident authenticates fresh source");
    if !dirty {
        assert_eq!(
            candidate
                .recovery_access()
                .unwrap()
                .home_revision()
                .unwrap(),
            revision
        );
    }
    let mut preparation = Preparation::new(candidate, source, capture).ok().unwrap();
    let (fresh_seed, selection) = preparation.authenticated_source().unwrap();
    assert_eq!(fresh_seed.caret, seed.caret);
    assert_eq!(fresh_seed.selection, seed.selection);
    assert_eq!(fresh_seed.scroll, seed.scroll);
    assert_eq!(
        selection.binding().candidate().session_id(),
        old.binding().candidate().session_id()
    );
    let mut config = widget_support::widget_config(
        fresh_seed.binding,
        selection.binding().presentation_generation(),
    );
    config.viewport_extent = px(96.);
    config.limits.max_realized_block_extent = config.viewport_extent;
    let capacity = RangeSurfaceCharge {
        bytes: config.limits.max_surface_bytes * 2,
        items: config.limits.max_surface_items * 2,
    };
    let mut current = RangePrepublicationCurrent {
        binding: fresh_seed.binding,
        history: fresh_seed.history,
        available_capacity: RangeSurfaceCharge {
            bytes: config.limits.max_surface_bytes,
            items: config.limits.max_surface_items,
        },
    };
    let environment = cx.update(|window, _| {
        let cleanup = RangePrepublicationCleanupLedger::new(window.text_system(), 64).unwrap();
        RangePrepublicationEnvironment::new(13, config, window.text_system(), cleanup).unwrap()
    });
    input
        .read_with(cx, |input, _| {
            preparation.admit(input, environment, capacity)
        })
        .unwrap();
    if scenario == Scenario::ReadFailure {
        faults.fail_next(FaultPoint::BeforeReadConfirmation);
    }
    let mut ready = false;
    for _ in 0..512 {
        let result = cx.update(|window, app| {
            input.update(app, |input, cx| {
                preparation.advance(input, window.text_system(), cx, |_| {})
            })
        });
        if scenario == Scenario::CancelReading && result == Ok(Progress::Waiting) {
            preparation.cancel();
            assert!(preparation.take_cancelled_resources().is_none());
            break;
        }
        if scenario == Scenario::ReadFailure && result.is_err() {
            break;
        }
        if result.unwrap() == Progress::Ready {
            ready = true;
            break;
        }
        cx.run_until_parked();
    }
    if scenario == Scenario::Adopt {
        assert!(ready);
        let (candidate, service, adoption) = cx
            .update(|window, app| {
                resident.update(app, |resident, cx| {
                    preparation.adopt_resident(resident, current, window, cx)
                })
            })
            .unwrap();
        assert_eq!(
            resident
                .read_with(cx, |resident, _| resident.gpui_input())
                .entity_id(),
            input.entity_id()
        );
        assert_eq!(
            resident.read_with(cx, |resident, _| resident.selection_identity()),
            selection
        );
        assert!(input.read_with(cx, |input, _| !input.is_enabled()
            && input.resident_protection_is_current(adoption.protection())));
        assert_eq!(
            adoption.recovery_known_commit(),
            if dirty { Some(true) } else { None }
        );
        assert_eq!(cx.update(|window, app| window.focused(app)), focus);
        let store = candidate.publish().unwrap();
        resident
            .update(cx, |resident, cx| {
                resident.publish_failed_resident(&adoption, &store, cx)
            })
            .unwrap();
        support::drive(cx, 4);
        assert!(input.read_with(cx, |input, _| input.is_enabled()));
        if dirty {
            assert_eq!(
                composer::candidate_text(storage, &store, selection.binding()),
                b"retained live edit"
            );
            assert_eq!(
                input.read_with(cx, |input, _| input
                    .export_restoration(Some(input.history_frontier()))
                    .unwrap()
                    .selection),
                seed.selection
            );
        }
        drop((service, store));
    } else {
        if scenario == Scenario::Capacity {
            assert!(ready);
            current.available_capacity = RangeSurfaceCharge { bytes: 0, items: 0 };
            assert!(
                cx.update(
                    |window, app| resident.update(app, |resident, cx| preparation
                        .adopt_resident(resident, current, window, cx))
                )
                .is_err()
            );
        }
        preparation.cancel();
        support::drive_until(cx, "failed preparation complete cleanup", |cx| {
            cx.run_until_parked();
            preparation.advance_cleanup().unwrap()
        });
        let (candidate, source, capture) = preparation.take_cancelled_resources().expect(
            "cancelled completion returns exact candidate source and protected predecessor",
        );
        assert_eq!(capture.selection(), old);
        assert_eq!(source.predecessor(), seed);
        assert_eq!(
            resident.read_with(cx, |resident, _| resident.selection_identity()),
            old
        );
        assert!(input.read_with(cx, |input, _| !input.is_enabled()
            && input.resident_protection_is_current(capture.protection())));
        drop(source);
        drop(candidate.abort());
    }
    drop((preparation, root, mount, resident, input));
    drop(directory);
}
