use std::{
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Duration,
};

use beryl_backend::{
    ManagedBackendClientConnector, ManagedBackendError, TurnSteerResponse, UserInput,
    active_turn_not_steerable_error,
};

use super::execution_detail::{TranscriptImageMarkerSpec, UserInputFragment};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SteeringInputFragment {
    pub turn_index: usize,
    pub fragment_id: u64,
    pub text: String,
    backend_input: Vec<UserInput>,
    image_markers: Vec<TranscriptImageMarkerSpec>,
}

impl SteeringInputFragment {
    pub(super) fn from_user_input_fragment(
        turn_index: usize,
        fragment: &UserInputFragment,
    ) -> Self {
        Self {
            turn_index,
            fragment_id: fragment.id,
            text: fragment.text.clone(),
            backend_input: fragment.backend_input().to_vec(),
            image_markers: fragment.image_marker_specs(),
        }
    }

    pub(super) fn into_user_input_fragment(self) -> UserInputFragment {
        UserInputFragment::from_backend_input_with_image_markers(
            self.text,
            self.backend_input,
            self.image_markers,
        )
    }

    pub(super) fn retained_payload_bytes_lower_bound(&self) -> usize {
        self.text
            .len()
            .saturating_add(
                self.backend_input
                    .iter()
                    .map(user_input_payload_bytes)
                    .sum::<usize>(),
            )
            .saturating_add(self.image_markers.len().saturating_mul(32))
    }
}

fn user_input_payload_bytes(input: &UserInput) -> usize {
    match input {
        UserInput::Text { text } => text.len(),
        UserInput::Image { url } => url.len(),
        UserInput::LocalImage { path } => path.len(),
        UserInput::Skill { name, path } | UserInput::Mention { name, path } => {
            name.len().saturating_add(path.len())
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum TurnSteeringUpdate {
    Finished(TurnSteeringOutcome),
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum TurnSteeringOutcome {
    Steered,
    QueueForNextTurn {
        thread_id: String,
        fragments: Vec<SteeringInputFragment>,
        message: String,
    },
}

pub(super) fn spawn_turn_steering_worker(
    connector: ManagedBackendClientConnector,
    thread_id: String,
    expected_turn_id: String,
    fragments: Vec<SteeringInputFragment>,
    timeout: Duration,
) -> Receiver<TurnSteeringUpdate> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        run_turn_steering_worker(
            connector,
            thread_id,
            expected_turn_id,
            fragments,
            timeout,
            sender,
        )
    });
    receiver
}

fn run_turn_steering_worker(
    connector: ManagedBackendClientConnector,
    thread_id: String,
    expected_turn_id: String,
    fragments: Vec<SteeringInputFragment>,
    timeout: Duration,
    sender: Sender<TurnSteeringUpdate>,
) {
    let mut session = match connector.connect_client(timeout) {
        Ok(session) => session,
        Err(error) => {
            send_queue_for_next_turn(
                sender,
                thread_id,
                fragments,
                format!(
                    "Beryl could not connect to the managed backend to steer the active turn: {error}"
                ),
            );
            return;
        }
    };

    let input = backend_input_for_steering_fragments(&fragments);
    let result = session.steer_turn_with_user_input(&thread_id, &expected_turn_id, input, timeout);
    let outcome =
        turn_steering_outcome_from_result(thread_id, &expected_turn_id, fragments, result);
    let _ = sender.send(TurnSteeringUpdate::Finished(outcome));
}

pub(super) fn backend_input_for_steering_fragments(
    fragments: &[SteeringInputFragment],
) -> Vec<UserInput> {
    fragments
        .iter()
        .flat_map(|fragment| fragment.backend_input.iter().cloned())
        .collect()
}

pub(super) fn turn_steering_outcome_from_result(
    thread_id: String,
    expected_turn_id: &str,
    fragments: Vec<SteeringInputFragment>,
    result: Result<TurnSteerResponse, ManagedBackendError>,
) -> TurnSteeringOutcome {
    match result {
        Ok(response) if response.turn_id == expected_turn_id => TurnSteeringOutcome::Steered,
        Ok(response) => TurnSteeringOutcome::QueueForNextTurn {
            thread_id,
            fragments,
            message: format!(
                "Beryl could not steer the active turn because the backend accepted steering for turn {} instead of expected turn {}.",
                response.turn_id, expected_turn_id
            ),
        },
        Err(ManagedBackendError::RequestFailed { method, error }) if method == "turn/steer" => {
            let message = if let Some(non_steerable) = active_turn_not_steerable_error(&error) {
                format!(
                    "Beryl could not steer the active turn because the backend reported that {} turns cannot accept steering.",
                    non_steerable.turn_kind
                )
            } else {
                format!(
                    "Beryl could not steer the active turn because the backend rejected the request: {error}"
                )
            };
            TurnSteeringOutcome::QueueForNextTurn {
                thread_id,
                fragments,
                message,
            }
        }
        Err(error) => TurnSteeringOutcome::QueueForNextTurn {
            thread_id,
            fragments,
            message: format!("Beryl could not steer the active turn: {error}"),
        },
    }
}

fn send_queue_for_next_turn(
    sender: Sender<TurnSteeringUpdate>,
    thread_id: String,
    fragments: Vec<SteeringInputFragment>,
    message: String,
) {
    let _ = sender.send(TurnSteeringUpdate::Finished(
        TurnSteeringOutcome::QueueForNextTurn {
            thread_id,
            fragments,
            message,
        },
    ));
}
