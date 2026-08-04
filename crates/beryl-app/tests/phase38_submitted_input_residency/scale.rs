use beryl_app::cas_projection::{
    OrdinaryInputReplayDiagnosticsSnapshot, OrdinaryTurnExecutionOutcome,
    OrdinaryTurnExecutionRequest,
};
use beryl_backend::TurnStartOptions;

use crate::{
    content::{LogicalInput, SharedImage, publish_shared_image, seed_submitted_input},
    fixture::{CompletedExecution, PreparedExecution, close_execution},
    server::{RawCasServer, TIMEOUT},
    syndic::Fixture,
    verification::{assert_connection_released, assert_durable_success, assert_three_pass_work},
    wire::{RequestObservation, RequestOutcome},
};

const MARKER_FREE_SHAPES: [LogicalInput; 4] = [
    LogicalInput::marker_free(2_048),
    LogicalInput::marker_free(8_192),
    LogicalInput::marker_free(16_384),
    LogicalInput::marker_free(16_384),
];
const MARKER_AWARE_SHAPES: [LogicalInput; 4] = [
    LogicalInput::alternating_images(16, 128),
    LogicalInput::alternating_images(64, 128),
    LogicalInput::alternating_images(128, 128),
    LogicalInput::alternating_images(128, 128),
];

struct Evidence {
    input: OrdinaryInputReplayDiagnosticsSnapshot,
    request: RequestObservation,
}

pub fn run() {
    let mut fixture = Fixture::new(138);
    let marker_free = run_series(&mut fixture, 150, 1, &MARKER_FREE_SHAPES, None);
    assert_series(&marker_free);

    let image = publish_shared_image(&mut fixture);
    let marker_aware = run_series(&mut fixture, 154, 5, &MARKER_AWARE_SHAPES, Some(&image));
    assert_series(&marker_aware);

    let (directory, service) = fixture.into_service();
    service.close().unwrap();
    drop(directory);
}

fn run_series(
    fixture: &mut Fixture,
    first_thread_seed: u8,
    first_run_id: u64,
    shapes: &[LogicalInput; 4],
    image: Option<&SharedImage>,
) -> [Evidence; 4] {
    std::array::from_fn(|index| {
        let thread = fixture.create_ordinary(
            first_thread_seed
                .checked_add(u8::try_from(index).unwrap())
                .unwrap(),
        );
        run_one(
            fixture,
            thread,
            first_run_id.checked_add(index as u64).unwrap(),
            shapes[index],
            image,
        )
    })
}

fn run_one(
    fixture: &mut Fixture,
    thread: beryl_model::SyndicThreadId,
    run_id: u64,
    shape: LogicalInput,
    image: Option<&SharedImage>,
) -> Evidence {
    let seeded = seed_submitted_input(fixture, thread, shape, image);
    assert!(
        (syndic_storage::CONTENT_CHUNK_MAX_BYTES - 3..=syndic_storage::CONTENT_CHUNK_MAX_BYTES)
            .contains(&seeded.composer_max_buffer_bytes)
    );
    let server = RawCasServer::spawn(run_id, seeded.wire);
    let identity = server.identity();
    let prepared = PreparedExecution::new(fixture, thread, &server);
    let request = OrdinaryTurnExecutionRequest::new(TurnStartOptions::default(), TIMEOUT);
    let diagnostics = request.input_replay_diagnostics();
    let mut request_observation = None;
    let CompletedExecution { result, session } = prepared.execute(fixture, &request, |_| {
        let RequestOutcome::Complete(observation) = server.wait_for_request() else {
            panic!("successful scale request aborted")
        };
        request_observation = Some(observation);
        server.release_lifecycle();
    });
    let OrdinaryTurnExecutionOutcome::Terminal { projection, status } = result.unwrap() else {
        panic!("successful scale execution did not reach terminal")
    };
    assert_eq!(status, syndic_storage::TurnEndStatus::complete());
    assert_eq!(projection.cas_thread_id().as_str(), identity.thread_id());

    let input = diagnostics.snapshot();
    assert_three_pass_work(
        input,
        usize::try_from(seeded.descriptor_count).unwrap(),
        seeded.authored_logical_text_bytes,
    );
    assert_eq!(
        input.sidecar_verifications(),
        usize::try_from(shape.image_count().checked_mul(4).unwrap()).unwrap()
    );
    assert_connection_released(&session);
    assert_durable_success(fixture, thread, seeded.submitted.turn, status);

    let observation = request_observation.unwrap();
    assert!(observation.logical_bytes() > seeded.authored_logical_text_bytes);
    assert!(observation.frame_count() > 1);
    assert!(observation.maximum_frame_payload_bytes() <= 64 * 1_024);

    drop(projection);
    close_execution(session, server);
    Evidence {
        input,
        request: observation,
    }
}

fn assert_series(series: &[Evidence; 4]) {
    let baseline = &series[0];
    for larger in &series[1..] {
        assert_eq!(
            larger.input.passes_started(),
            baseline.input.passes_started()
        );
    }
    for pair in series[..3].windows(2) {
        let [smaller, larger] = pair else {
            unreachable!()
        };
        assert!(larger.input.logical_text_bytes() > smaller.input.logical_text_bytes());
        assert!(larger.input.text_page_requests() > smaller.input.text_page_requests());
        assert!(larger.request.logical_bytes() > smaller.request.logical_bytes());
        assert!(larger.request.frame_count() > smaller.request.frame_count());
    }
    let large = &series[2];
    let repeated = &series[3];
    assert_eq!(repeated.input, large.input);
    assert_eq!(repeated.request, large.request);
}
