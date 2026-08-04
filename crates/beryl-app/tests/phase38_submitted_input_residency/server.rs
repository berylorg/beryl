#[path = "server/admission.rs"]
mod admission;

use std::{
    net::{Shutdown, TcpListener},
    num::NonZeroU64,
    sync::mpsc::{self, Receiver, SyncSender},
    thread,
    time::Duration,
};

use beryl_backend::BackendWebSocketEndpoint;

use super::wire::{
    ExpectedTurnStart, InputSpec, LifecycleMessage, LifecycleStage, RequestCutoff, RequestOutcome,
    RunIdentity, TerminalMessage, TurnStartResponse, await_masked_client_close,
    verify_masked_text_message, write_unmasked_text_message,
};

pub const AUTHORIZATION: &str = "Bearer phase38-submitted-input-residency";
pub const TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ServerScenario {
    #[default]
    Complete,
    CloseRequestAfterBytes(NonZeroU64),
    CloseRequestAfterFrames(NonZeroU64),
    PauseAfterStarted,
    HoldOpenAfterResponse,
    ObserveTailAfterTerminal,
}

impl ServerScenario {
    #[must_use]
    pub fn close_request_after_bytes(bytes: u64) -> Self {
        Self::CloseRequestAfterBytes(
            NonZeroU64::new(bytes).expect("request byte cutoff must be nonzero"),
        )
    }

    #[must_use]
    pub fn close_request_after_frames(frames: u64) -> Self {
        Self::CloseRequestAfterFrames(
            NonZeroU64::new(frames).expect("request frame cutoff must be nonzero"),
        )
    }

    const fn request_cutoff(self) -> RequestCutoff {
        match self {
            Self::Complete
            | Self::PauseAfterStarted
            | Self::HoldOpenAfterResponse
            | Self::ObserveTailAfterTerminal => RequestCutoff::None,
            Self::CloseRequestAfterBytes(bytes) => RequestCutoff::Bytes(bytes),
            Self::CloseRequestAfterFrames(frames) => RequestCutoff::Frames(frames),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ServerEvent {
    ProjectionReady,
    Request(RequestOutcome),
    StartedEmitted,
    ResponseEmitted,
    TailEmitted,
    Closed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ServerControl {
    EmitLifecycle,
    CloseAfterStarted,
}

pub struct RawCasServer {
    endpoint: BackendWebSocketEndpoint,
    identity: RunIdentity,
    events: Receiver<ServerEvent>,
    controls: SyncSender<ServerControl>,
    handle: Option<thread::JoinHandle<()>>,
}

impl RawCasServer {
    #[must_use]
    pub fn spawn(run_id: u64, input: InputSpec) -> Self {
        Self::spawn_scenario(run_id, input, ServerScenario::Complete)
    }

    #[must_use]
    pub fn spawn_scenario(run_id: u64, input: InputSpec, scenario: ServerScenario) -> Self {
        let identity = RunIdentity::new(run_id);
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let endpoint = BackendWebSocketEndpoint::loopback(listener.local_addr().unwrap().port());
        let (event_sender, events) = mpsc::sync_channel(1);
        let (controls, control_receiver) = mpsc::sync_channel(0);
        let handle = thread::Builder::new()
            .name(format!("phase38-raw-cas-{run_id}"))
            .spawn(move || {
                run_server(
                    listener,
                    identity,
                    input,
                    scenario,
                    event_sender,
                    control_receiver,
                )
            })
            .unwrap();
        Self {
            endpoint,
            identity,
            events,
            controls,
            handle: Some(handle),
        }
    }

    pub fn endpoint(&self) -> BackendWebSocketEndpoint {
        self.endpoint.clone()
    }

    pub const fn identity(&self) -> RunIdentity {
        self.identity
    }

    pub fn wait_for_projection(&self) {
        self.expect(ServerEvent::ProjectionReady);
    }

    pub fn wait_for_request(&self) -> RequestOutcome {
        let ServerEvent::Request(outcome) = self.receive() else {
            panic!("phase38 server did not report its request outcome")
        };
        outcome
    }

    pub fn assert_request_pending(&self) {
        match self.events.recv_timeout(Duration::from_millis(25)) {
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                panic!("phase38 server disconnected while its request should remain pending")
            }
            Ok(event) => panic!("phase38 server advanced unexpectedly while blocked: {event:?}"),
        }
    }

    pub fn release_lifecycle(&self) {
        self.send_control(ServerControl::EmitLifecycle);
    }

    pub fn wait_for_started(&self) {
        self.expect(ServerEvent::StartedEmitted);
    }

    pub fn close_after_started(&self) {
        self.send_control(ServerControl::CloseAfterStarted);
    }

    pub fn wait_for_response(&self) {
        self.expect(ServerEvent::ResponseEmitted);
    }

    pub fn wait_for_tail(&self) {
        self.expect(ServerEvent::TailEmitted);
    }

    pub fn join(mut self) {
        self.expect(ServerEvent::Closed);
        self.handle.take().unwrap().join().unwrap();
    }

    fn send_control(&self, control: ServerControl) {
        self.controls
            .send(control)
            .unwrap_or_else(|error| panic!("phase38 server did not accept {control:?}: {error}"));
    }

    fn expect(&self, expected: ServerEvent) {
        assert_eq!(self.receive(), expected);
    }

    fn receive(&self) -> ServerEvent {
        self.events
            .recv_timeout(TIMEOUT)
            .unwrap_or_else(|error| panic!("timed out waiting for phase38 server: {error}"))
    }
}

fn run_server(
    listener: TcpListener,
    identity: RunIdentity,
    input: InputSpec,
    scenario: ServerScenario,
    events: SyncSender<ServerEvent>,
    controls: Receiver<ServerControl>,
) {
    let (mut stream, request_id) = admission::accept_and_prepare(listener, identity);
    events.send(ServerEvent::ProjectionReady).unwrap();
    let expected = ExpectedTurnStart::new(identity, request_id, input);
    let outcome =
        verify_masked_text_message(&mut stream, request_id, expected, scenario.request_cutoff())
            .unwrap();
    events.send(ServerEvent::Request(outcome)).unwrap();
    if !matches!(outcome, RequestOutcome::Complete(_)) {
        let _ = stream.shutdown(Shutdown::Both);
        let _ = events.send(ServerEvent::Closed);
        return;
    }

    expect_control(&controls, ServerControl::EmitLifecycle);
    if write_unmasked_text_message(
        &mut stream,
        LifecycleMessage::new(identity, LifecycleStage::Started, input),
    )
    .is_err()
    {
        finish_closed(&mut stream, &events);
        return;
    }
    if scenario == ServerScenario::PauseAfterStarted {
        events.send(ServerEvent::StartedEmitted).unwrap();
        expect_control(&controls, ServerControl::CloseAfterStarted);
        finish_closed(&mut stream, &events);
        return;
    }

    if write_unmasked_text_message(
        &mut stream,
        LifecycleMessage::new(identity, LifecycleStage::Completed, input),
    )
    .is_err()
        || write_unmasked_text_message(&mut stream, TurnStartResponse::new(identity, request_id))
            .is_err()
    {
        finish_closed(&mut stream, &events);
        return;
    }
    if scenario == ServerScenario::HoldOpenAfterResponse {
        events.send(ServerEvent::ResponseEmitted).unwrap();
        let _ = await_masked_client_close(&mut stream);
        let _ = events.send(ServerEvent::Closed);
        return;
    }
    if write_unmasked_text_message(&mut stream, TerminalMessage::new(identity)).is_err() {
        finish_closed(&mut stream, &events);
        return;
    }
    if scenario == ServerScenario::ObserveTailAfterTerminal {
        events.send(ServerEvent::TailEmitted).unwrap();
    }
    let _ = await_masked_client_close(&mut stream);
    let _ = events.send(ServerEvent::Closed);
}

fn finish_closed(stream: &mut std::net::TcpStream, events: &SyncSender<ServerEvent>) {
    let _ = stream.shutdown(Shutdown::Both);
    let _ = events.send(ServerEvent::Closed);
}

fn expect_control(controls: &Receiver<ServerControl>, expected: ServerControl) {
    let actual = controls.recv_timeout(TIMEOUT).unwrap_or_else(|error| {
        panic!("timed out waiting for phase38 control {expected:?}: {error}")
    });
    assert_eq!(actual, expected);
}
