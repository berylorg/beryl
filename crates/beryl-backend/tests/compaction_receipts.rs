use beryl_backend::{
    BackendLaunchSpec, BackendWebSocketEndpoint, CompactionCapabilityError,
    CompactionObservationCapability, CompactionReceiptState, CompactionUnknownReason,
    CompatibilitySnapshot, InitializeResponse, ManagedBackendSession,
};
use beryl_model::workspace::RuntimeMode;
use serde_json::{Value, json};
use std::{net::TcpListener, thread, time::Duration};
use tungstenite::{Message, accept};

const SESSION: &str = "01900000-0000-7000-8000-000000000001";
const THREAD: &str = "01900000-0000-7000-8000-000000000002";
const TURN: &str = "01900000-0000-7000-8000-000000000003";
const OTHER: &str = "01900000-0000-7000-8000-000000000004";
const TIMEOUT: Duration = Duration::from_secs(5);

fn initialize(cap: Value) -> Value {
    let mut result = json!({"userAgent":"receipt fixture", "codexHome":"C:/fixture", "platformFamily":"windows", "platformOs":"windows", "turnScopedDeveloperInstructionsVersion":1});
    result
        .as_object_mut()
        .unwrap()
        .extend(cap.as_object().unwrap().clone());
    result
}
fn supported(session: &str) -> Value {
    json!({"compactionObservationVersion":1,"compactionObservationSessionId":session})
}
fn step(method: &str, patch: Value) -> Value {
    json!({"method":method,"patch":patch})
}
fn marker() -> Value {
    step("model/list", json!({}))
}
fn result(request: &Value, step: &Value) -> Value {
    assert_eq!(request["method"], step["method"]);
    if step["method"] == "model/list" {
        return json!({"data":[]});
    }
    let p = &request["params"];
    let mut receipt = json!({"threadId":p["threadId"],"operationId":p["operationId"],"observationSessionId":p["observationSessionId"],"turnId":TURN,"state":{"status":"accepted"}});
    assert!(uuid::Uuid::parse_str(p["operationId"].as_str().unwrap()).is_ok());
    receipt
        .as_object_mut()
        .unwrap()
        .extend(step["patch"].as_object().unwrap().clone());
    if step["omit"] == true {
        return json!({});
    }
    if step["method"] == "thread/compact/start" {
        json!({"receipt":receipt})
    } else {
        receipt
    }
}

#[derive(Clone, Copy, Debug)]
enum Transport {
    WebSocket,
    #[cfg(all(windows, feature = "lifecycle-test-support"))]
    Stdio,
}
fn transports() -> Vec<Transport> {
    vec![
        Transport::WebSocket,
        #[cfg(all(windows, feature = "lifecycle-test-support"))]
        Transport::Stdio,
    ]
}
struct Fixture {
    client: ManagedBackendSession,
    server: Option<thread::JoinHandle<()>>,
}
impl Fixture {
    fn new(transport: Transport, cap: Value, steps: Vec<Value>) -> Self {
        let init = initialize(cap);
        match transport {
            Transport::WebSocket => {
                let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
                let endpoint =
                    BackendWebSocketEndpoint::loopback(listener.local_addr().unwrap().port());
                let server = thread::spawn(move || {
                    let (stream, _) = listener.accept().unwrap();
                    stream.set_read_timeout(Some(TIMEOUT)).unwrap();
                    stream.set_write_timeout(Some(TIMEOUT)).unwrap();
                    let mut socket = accept(stream).unwrap();
                    let request: Value =
                        serde_json::from_str(socket.read().unwrap().to_text().unwrap()).unwrap();
                    assert_eq!(request["method"], "initialize");
                    socket
                        .send(Message::text(
                            json!({"id":request["id"],"result":init}).to_string(),
                        ))
                        .unwrap();
                    let initialized: Value =
                        serde_json::from_str(socket.read().unwrap().to_text().unwrap()).unwrap();
                    assert_eq!(initialized["method"], "initialized");
                    for step in steps {
                        let request: Value =
                            serde_json::from_str(socket.read().unwrap().to_text().unwrap())
                                .unwrap();
                        if step["method"] == "approval/denied" {
                            assert_eq!(request["id"], 97);
                            assert_eq!(request["result"], json!({"decision":"cancel"}));
                            continue;
                        }
                        let response = result(&request, &step);
                        socket
                            .send(Message::text(
                                json!({"id":request["id"],"result":response}).to_string(),
                            ))
                            .unwrap();
                    }
                });
                let launch = BackendLaunchSpec::managed_websocket(
                    RuntimeMode::HostWindows,
                    "C:/fixture",
                    endpoint.clone(),
                    "C:/unused-token",
                );
                let client = ManagedBackendSession::connect_websocket(
                    launch,
                    endpoint,
                    "Bearer fixture-token".into(),
                    TIMEOUT,
                )
                .unwrap();
                Self {
                    client,
                    server: Some(server),
                }
            }
            #[cfg(all(windows, feature = "lifecycle-test-support"))]
            Transport::Stdio => {
                // A bounded scripted peer; Rust owns the scenarios and child lifetime.
                let script = format!(
                    r#"
$ErrorActionPreference = 'Stop'
$init = '{}' | ConvertFrom-Json
$steps = '{}' | ConvertFrom-Json
$request = [Console]::In.ReadLine() | ConvertFrom-Json
[Console]::Out.WriteLine((@{{id=$request.id;result=$init}} | ConvertTo-Json -Depth 30 -Compress))
[void][Console]::In.ReadLine()
foreach ($step in $steps) {{
 $request = [Console]::In.ReadLine() | ConvertFrom-Json
 if ($request.method -ne $step.method) {{ throw 'unexpected mutation or request' }}
 $p = $request.params
 $receipt = @{{threadId=$p.threadId;operationId=$p.operationId;observationSessionId=$p.observationSessionId;turnId='{}';state=@{{status='accepted'}}}}
 foreach ($property in $step.patch.PSObject.Properties) {{ $receipt[$property.Name]=$property.Value }}
 if ($step.method -eq 'model/list') {{ $result=@{{data=@()}} }}
 elseif ($step.omit) {{ $result=@{{}} }}
 elseif ($step.method -eq 'thread/compact/start') {{ $result=@{{receipt=$receipt}} }}
 else {{ $result=$receipt }}
 [Console]::Out.WriteLine((@{{id=$request.id;result=$result}} | ConvertTo-Json -Depth 30 -Compress))
}}
[void][Console]::In.ReadLine()
"#,
                    init.to_string().replace('\'', "''"),
                    serde_json::to_string(&steps).unwrap().replace('\'', "''"),
                    TURN
                );
                let mut command = std::process::Command::new("powershell.exe");
                command.args(["-NoProfile", "-NonInteractive", "-Command", &script]);
                let launch =
                    BackendLaunchSpec::managed_stdio(RuntimeMode::HostWindows, "C:/fixture");
                let client = ManagedBackendSession::launch_and_initialize_test_command(
                    launch,
                    command,
                    beryl_backend::ManagedBackendClientOptions::foreground(),
                    TIMEOUT,
                )
                .unwrap();
                Self {
                    client,
                    server: None,
                }
            }
        }
    }
    fn finish(mut self) {
        assert!(self.client.list_models(TIMEOUT).unwrap().is_empty());
        self.client.shutdown().unwrap();
        if let Some(server) = self.server.take() {
            server.join().unwrap();
        }
    }
}

#[test]
fn malformed_or_missing_capability_is_local_to_compaction() {
    for cap in [
        json!({}),
        json!({"compactionObservationVersion":1}),
        json!({"compactionObservationSessionId":SESSION}),
        json!({"compactionObservationVersion":"1","compactionObservationSessionId":SESSION}),
        json!({"compactionObservationVersion":2,"compactionObservationSessionId":SESSION}),
        json!({"compactionObservationVersion":1,"compactionObservationSessionId":true}),
        json!({"compactionObservationVersion":1,"compactionObservationSessionId":"not-uuid"}),
    ] {
        let response: InitializeResponse = serde_json::from_value(initialize(cap.clone())).unwrap();
        CompatibilitySnapshot::from_initialize_response(&response)
            .validate_runtime_mode(&RuntimeMode::HostWindows)
            .unwrap();
        assert!(response.compaction_observation.session_id().is_err());
        for transport in transports() {
            let mut fixture = Fixture::new(transport, cap.clone(), vec![marker()]);
            assert!(fixture.client.prepare_compaction(THREAD).is_err());
            assert!(fixture.client.compact_thread(THREAD, TIMEOUT).is_err());
            fixture.finish();
        }
    }
}

#[test]
fn capability_normalizes_exact_reasons_and_canonical_identity() {
    for (cap, reason) in [
        (json!({}), CompactionCapabilityError::Missing),
        (
            json!({"compactionObservationVersion":1}),
            CompactionCapabilityError::Incomplete,
        ),
        (
            json!({"compactionObservationVersion":2,"compactionObservationSessionId":SESSION}),
            CompactionCapabilityError::UnsupportedVersion(2),
        ),
        (
            json!({"compactionObservationVersion":true,"compactionObservationSessionId":SESSION}),
            CompactionCapabilityError::Malformed,
        ),
    ] {
        let response: InitializeResponse = serde_json::from_value(initialize(cap)).unwrap();
        assert_eq!(
            response.compaction_observation,
            CompactionObservationCapability::Unavailable(reason)
        );
    }
    let response: InitializeResponse =
        serde_json::from_value(initialize(supported(&SESSION.replace('-', "")))).unwrap();
    assert_eq!(
        response
            .compaction_observation
            .session_id()
            .unwrap()
            .to_string(),
        SESSION
    );
}

#[test]
fn observed_start_and_all_receipt_states_round_trip_on_both_transports() {
    let mut patches = vec![
        json!({"state":{"status":"accepted"}}),
        json!({"state":{"status":"running"}}),
        json!({"state":{"status":"completed"}}),
        json!({"state":{"status":"failed","error":{"classification":"coreTurnError","message":"é".repeat(2048),"truncated":true}}}),
    ];
    for reason in ["interrupted", "replaced", "reviewEnded", "budgetLimited"] {
        patches.push(json!({"state":{"status":"interrupted","reason":reason}}));
    }
    for reason in [
        "absentOrExpired",
        "runtimeChange",
        "observationGap",
        "incompleteTerminalEvidence",
        "submissionPending",
    ] {
        let mut patch = json!({"state":{"status":"unknown","reason":reason}});
        if ["absentOrExpired", "runtimeChange"].contains(&reason) {
            patch["turnId"] = Value::Null;
        }
        patches.push(patch);
    }
    for transport in transports() {
        let mut steps = vec![step("thread/compact/start", json!({}))];
        steps.extend(
            patches
                .iter()
                .map(|p| step("thread/compact/read", p.clone())),
        );
        steps.push(marker());
        let mut fixture = Fixture::new(transport, supported(SESSION), steps);
        let operation = fixture.client.prepare_compaction(THREAD).unwrap();
        assert!(fixture.client.prepare_compaction("invalid").is_err());
        let accepted = fixture
            .client
            .start_compaction(&operation, TIMEOUT)
            .unwrap();
        assert_eq!(accepted.turn_id.unwrap().to_string(), TURN);
        assert_eq!(accepted.state, CompactionReceiptState::Accepted);
        for patch in &patches {
            let receipt = fixture
                .client
                .read_compaction(&operation, Some(TURN), TIMEOUT)
                .unwrap();
            assert_eq!(receipt.operation_id, operation.operation_id());
            assert_eq!(
                receipt.state,
                serde_json::from_value::<CompactionReceiptState>(patch["state"].clone()).unwrap()
            );
        }
        fixture.finish();
    }
}

#[test]
fn malformed_or_mismatched_receipts_never_become_success() {
    let patches = vec![
        json!({"threadId":OTHER}),
        json!({"operationId":OTHER}),
        json!({"observationSessionId":OTHER}),
        json!({"turnId":OTHER}),
        json!({"turnId":"not-uuid"}),
        json!({"turnId":null}),
        json!({"state":{"status":"unknown","reason":"future"}}),
        json!({"state":{"status":"unknown","reason":"absentOrExpired"}}),
        json!({"state":{"status":"completed","error":{"message":"contradiction"}}}),
        json!({"state":{"status":"failed","error":{"classification":"coreTurnError","message":"é".repeat(2049),"truncated":false}}}),
        json!({"state":{"status":"failed","error":{"classification":"future","message":"error","truncated":false}}}),
        json!({"state":{"status":"failed","error":{"classification":"coreTurnError","message":"error","truncated":"yes"}}}),
        json!({"state":null}),
    ];
    for transport in transports() {
        let mut steps: Vec<_> = patches
            .iter()
            .map(|p| step("thread/compact/read", p.clone()))
            .collect();
        steps.push(marker());
        let mut fixture = Fixture::new(transport, supported(SESSION), steps);
        let operation = fixture.client.prepare_compaction(THREAD).unwrap();
        assert!(
            fixture
                .client
                .read_compaction(&operation, Some("bad"), TIMEOUT)
                .is_err()
        );
        for patch in &patches {
            assert!(
                fixture
                    .client
                    .read_compaction(&operation, Some(TURN), TIMEOUT)
                    .is_err(),
                "accepted invalid receipt {patch} on {transport:?}"
            );
        }
        fixture.finish();
    }
}

#[test]
fn missing_start_acknowledgement_keeps_operation_available_for_read_without_resubmit() {
    for transport in transports() {
        let mut missing = step("thread/compact/start", json!({}));
        missing["omit"] = json!(true);
        let mut fixture = Fixture::new(
            transport,
            supported(SESSION),
            vec![
                missing,
                step(
                    "thread/compact/read",
                    json!({"state":{"status":"completed"}}),
                ),
                marker(),
            ],
        );
        let operation = fixture.client.prepare_compaction(THREAD).unwrap();
        assert!(
            fixture
                .client
                .start_compaction(&operation, TIMEOUT)
                .is_err()
        );
        let receipt = fixture
            .client
            .read_compaction(&operation, None, TIMEOUT)
            .unwrap();
        assert_eq!(receipt.state, CompactionReceiptState::Completed);
        fixture.finish();
    }
}

#[test]
fn changed_runtime_never_adopts_a_previous_operation_or_terminal_result() {
    for transport in transports() {
        let first = Fixture::new(transport, supported(SESSION), vec![marker()]);
        let operation = first.client.prepare_compaction(THREAD).unwrap();
        first.finish();
        let mut second = Fixture::new(
            transport,
            supported(OTHER),
            vec![
                step(
                    "thread/compact/read",
                    json!({"turnId":null,"state":{"status":"unknown","reason":"runtimeChange"}}),
                ),
                step(
                    "thread/compact/read",
                    json!({"state":{"status":"completed"}}),
                ),
                marker(),
            ],
        );
        assert!(second.client.start_compaction(&operation, TIMEOUT).is_err());
        let unknown = second
            .client
            .read_compaction(&operation, Some(TURN), TIMEOUT)
            .unwrap();
        assert_eq!(
            unknown.state,
            CompactionReceiptState::Unknown {
                reason: CompactionUnknownReason::RuntimeChange
            }
        );
        assert!(
            second
                .client
                .read_compaction(&operation, Some(TURN), TIMEOUT)
                .is_err()
        );
        second.finish();
    }
}

#[test]
fn start_requires_accepted_receipt_with_exact_turn_identity() {
    let patches = [
        json!({"turnId":null}),
        json!({"threadId":OTHER}),
        json!({"state":{"status":"completed"}}),
        json!({"state":{"status":"unknown","reason":"submissionPending"}}),
    ];
    for transport in transports() {
        let mut steps: Vec<_> = patches
            .iter()
            .map(|p| step("thread/compact/start", p.clone()))
            .collect();
        steps.push(marker());
        let mut fixture = Fixture::new(transport, supported(SESSION), steps);
        for _ in &patches {
            let operation = fixture.client.prepare_compaction(THREAD).unwrap();
            assert!(
                fixture
                    .client
                    .start_compaction(&operation, TIMEOUT)
                    .is_err()
            );
        }
        fixture.finish();
    }
}

#[test]
fn approval_denial_zero_deadline_writes_nothing_and_keeps_session_reusable() {
    for transport in transports() {
        let mut fixture = Fixture::new(transport, supported(SESSION), vec![marker()]);
        let approval = beryl_backend::parse_approval_request(
            json!(97),
            "item/commandExecution/requestApproval",
            Some(json!({"threadId":THREAD,"turnId":TURN,"itemId":"item-one"})),
        )
        .unwrap();
        assert!(
            matches!(fixture.client.deny_approval_request_with_timeout(&approval, Duration::ZERO),
            Err(beryl_backend::ManagedBackendError::RequestTimeout { method, timeout })
                if method == "approval/deny" && timeout == Duration::ZERO)
        );
        fixture.finish();
    }
}

#[test]
fn bounded_approval_denial_preserves_the_wire_response_and_session() {
    let mut fixture = Fixture::new(
        Transport::WebSocket,
        supported(SESSION),
        vec![step("approval/denied", json!({})), marker()],
    );
    let approval = beryl_backend::parse_approval_request(
        json!(97),
        "item/commandExecution/requestApproval",
        Some(json!({"threadId":THREAD,"turnId":TURN,"itemId":"item-one"})),
    )
    .unwrap();
    fixture
        .client
        .deny_approval_request_with_timeout(&approval, TIMEOUT)
        .unwrap();
    fixture.finish();
}
