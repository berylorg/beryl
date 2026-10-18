use super::*;
use syndic_storage::{
    CasTurnSource, InputGateRecord, InputGateState, RepairCaptureGap, RepairCaptureGapReason,
    RepairRequestDisposition, RepairRequiredTarget, SourceEventPayload, SourceEventRecord,
    SourceEventSequence, TurnEndStatus, TurnIncompleteReason, TurnTerminalOutcome,
    test_faults::{FixtureBatch, FixtureRecord},
};

fn point_limit() -> syndic_storage::SyndicPointReadLimit {
    syndic_storage::SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn replace_gate(
    home: &beryl_home_store::HomeStore,
    storage: &syndic_storage::SyndicStorage,
    thread: SyndicThreadId,
    state: InputGateState,
) -> InputGateRecord {
    let previous = storage
        .input_gate(home, thread, point_limit())
        .unwrap()
        .unwrap();
    let updated = InputGateRecord::new(
        thread,
        previous.revision().checked_next().unwrap(),
        state,
        previous.accepted_high_water(),
        previous.route_generation_high_water(),
        previous.selected_route(),
        previous.live_steering_count(),
        previous.live_next_turn_count(),
        previous.live_logical_utf8_bytes(),
    )
    .unwrap();
    let mut batch = FixtureBatch::new();
    batch.put(FixtureRecord::InputGate(updated)).unwrap();
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command
        .add(storage.fixture_contribution(storage.revision(home).unwrap(), batch))
        .unwrap();
    assert!(matches!(
        home.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    previous
}

fn repair() -> InputGateState {
    let source = CasTurnSource::new(
        CasThreadId::new("authority-test-thread").unwrap(),
        CasTurnId::new("authority-test-turn").unwrap(),
    );
    let status = TurnEndStatus::new(
        TurnTerminalOutcome::Interrupted,
        Some(TurnIncompleteReason::StreamLost),
    )
    .unwrap();
    let event = SourceEventRecord::new(
        SyndicTurnId::from_bytes([204; 16]),
        SourceEventSequence::FIRST,
        Some(source.clone()),
        SourceEventPayload::TurnEnded(status),
    )
    .unwrap();
    InputGateState::RepairRequired(RepairRequiredTarget::new(
        event.turn_id(),
        source,
        RepairCaptureGap::new(
            event.repair_witness(),
            status,
            RepairCaptureGapReason::TerminalCaptureIncomplete,
            None,
        )
        .unwrap(),
        RepairRequestDisposition::Available,
    ))
}

pub(super) async fn qualify(
    window: gpui::WindowHandle<MainWindowShellRoot>,
    home: beryl_home_store::HomeServiceReference,
    storage: syndic_storage::SyndicStorage,
    worker: ExactStopWorker,
    selected: SyndicThreadId,
    other: SyndicThreadId,
    cx: &mut AsyncApp,
) {
    let identity = window
        .read_with(cx, |root, app| {
            root.test_submission_composer(app)
                .unwrap()
                .read(app)
                .selection_identity()
        })
        .unwrap();
    let stamp = window
        .read_with(cx, |root, app| root.test_submission_observation_stamp(app))
        .unwrap();
    let proof = window
        .read_with(cx, |root, app| root.test_submission_observation(app))
        .unwrap()
        .unwrap();
    let (home, storage, worker, other_original, original_binding) = join(
        support::worker(move || {
            let original_binding = storage
                .current_binding(&home, selected, point_limit())
                .unwrap()
                .unwrap()
                .binding()
                .revision();
            let original = replace_gate(&home, &storage, other, repair());
            assert_eq!(
                worker.selected_submission_availability(other),
                Err(SubmissionUnavailable::RepairRequired)
            );
            assert_eq!(worker.selected_submission_availability(selected), Ok(()));
            (home, storage, worker, original, original_binding)
        }),
        cx,
    )
    .await;
    window
        .update(cx, |root, window, app| {
            root.test_apply_submission_observation(stamp, Ok(proof), window, app);
            assert_eq!(
                root.test_submission_composer(app)
                    .unwrap()
                    .read(app)
                    .submission_unavailable(),
                None
            );
        })
        .unwrap();
    submission::wait_availability(window, Ok(()), cx).await;
    let (home, storage, worker, selected_original) = join(
        support::worker(move || {
            let original = replace_gate(&home, &storage, selected, repair());
            assert!(matches!(original.state(), InputGateState::Steerable(_)));
            assert_eq!(
                worker.selected_submission_availability(selected),
                Err(SubmissionUnavailable::RepairRequired)
            );
            (home, storage, worker, original)
        }),
        cx,
    )
    .await;
    submission::wait_availability(window, Err(SubmissionUnavailable::RepairRequired), cx).await;
    let stamp = window
        .read_with(cx, |root, app| root.test_submission_observation_stamp(app))
        .unwrap();
    let proof = window
        .read_with(cx, |root, app| root.test_submission_observation(app))
        .unwrap();
    assert!(matches!(proof, Err(SubmissionUnavailable::RepairRequired)));
    window
        .update(cx, |root, window, app| {
            root.test_submission_composer(app)
                .unwrap()
                .read(app)
                .gpui_input()
                .read(app)
                .focus(window);
        })
        .unwrap();
    cx.update_window(window.into(), |_, window, app| {
        window.dispatch_input_for_test(
            gpui::PlatformInput::KeyDown(gpui::KeyDownEvent {
                keystroke: gpui::Keystroke::parse("enter").unwrap(),
                is_held: false,
            }),
            app,
        );
        window.draw_and_present_for_test(app);
    })
    .unwrap();
    window
        .read_with(cx, |root, app| {
            let editor = root.test_submission_composer(app).unwrap();
            assert_eq!(editor.read(app).selection_identity(), identity);
            assert_eq!(
                editor.read(app).submission_disabled_outcome(),
                Some(SubmissionUnavailable::RepairRequired)
            );
            assert!(
                !root
                    .test_submission_mount()
                    .unwrap()
                    .read(app)
                    .test_submission_diagnostics()
                    .active_ticket()
            );
        })
        .unwrap();
    submission::wait_painted_outcome(
        window,
        SubmissionUnavailable::RepairRequired,
        Instant::now() + Duration::from_secs(10),
        cx,
    )
    .await;
    window
        .read_with(cx, |root, app| {
            let editor = root.test_submission_composer(app).unwrap();
            assert_eq!(editor.read(app).selection_identity(), identity);
            assert_eq!(
                editor.read(app).submission_disabled_outcome(),
                Some(SubmissionUnavailable::RepairRequired)
            );
            assert_eq!(
                editor
                    .read(app)
                    .test_submission_outcome_rendered()
                    .unwrap()
                    .0,
                SubmissionUnavailable::RepairRequired
            );
            assert!(
                !root
                    .test_submission_mount()
                    .unwrap()
                    .read(app)
                    .test_submission_diagnostics()
                    .active_ticket()
            );
        })
        .unwrap();
    join(
        support::worker(move || {
            let gate = storage
                .input_gate(&home, selected, point_limit())
                .unwrap()
                .unwrap();
            assert_eq!(
                gate.accepted_high_water(),
                selected_original.accepted_high_water()
            );
            assert_eq!(
                gate.live_steering_count(),
                selected_original.live_steering_count()
            );
            assert_eq!(
                gate.live_next_turn_count(),
                selected_original.live_next_turn_count()
            );
            replace_gate(&home, &storage, selected, selected_original.state().clone());
            replace_gate(&home, &storage, other, other_original.state().clone());
            assert_eq!(
                storage
                    .current_binding(&home, selected, point_limit())
                    .unwrap()
                    .unwrap()
                    .binding()
                    .revision(),
                original_binding
            );
            assert_eq!(worker.selected_submission_availability(selected), Ok(()));
        }),
        cx,
    )
    .await;
    submission::wait_availability(window, Ok(()), cx).await;
    window
        .read_with(cx, |root, app| {
            let editor = root.test_submission_composer(app).unwrap();
            assert_eq!(editor.read(app).selection_identity(), identity);
            assert_eq!(editor.read(app).submission_disabled_outcome(), None);
            assert_eq!(root.test_submission_observation_stamp(app).0, stamp.0);
            assert!(root.test_submission_observation(app).is_ok());
        })
        .unwrap();
}
