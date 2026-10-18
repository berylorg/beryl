use crate::cas_projection::{
    AdmittedProjectionSession, CasProjectionCoordinator, CasProjectionRequest,
    OrdinaryDynamicToolHandlers, OrdinaryTurnExecutionRequest, ProjectionCancellationToken,
};
use beryl_backend::{ThreadStartOptions, TurnStartOptions};
use beryl_model::{ExecutionBinding, SyndicThreadId};
use syndic_storage::{SelectedPathProof, SyndicStorage, SyndicTimestamp};

#[allow(dead_code)]
pub(super) mod server {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/normal_terminal/server.rs"
    ));
}

struct Tools;

impl crate::LifecycleYieldRequestHandler for Tools {
    fn respond_lifecycle_yield(
        &mut self,
        _: crate::cas_projection::OrdinaryDynamicToolContext,
        _: crate::LifecycleYieldRequest,
    ) -> beryl_backend::DynamicToolCallResponse {
        panic!("Code capture must not invoke lifecycle tools")
    }
}

impl crate::BranchDiscussionResolutionRequestHandler for Tools {
    fn respond_branch_discussion_resolution(
        &mut self,
        _: crate::cas_projection::BranchDiscussionResolutionContext,
        _: crate::BranchDiscussionResolutionRequest,
    ) -> beryl_backend::DynamicToolCallResponse {
        panic!("Code capture must not invoke discussion tools")
    }
}

pub(super) struct Capture {
    server: Option<server::NormalTerminalServer>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl Capture {
    pub(super) fn start(
        server: server::NormalTerminalServer,
        mut session: AdmittedProjectionSession,
        home: beryl_home_store::HomeServiceReference,
        storage: SyndicStorage,
        assets: beryl_state::AssetState,
        thread: SyndicThreadId,
        binding: ExecutionBinding,
    ) -> Self {
        let worker = std::thread::spawn(move || {
            let cancellation = ProjectionCancellationToken::new();
            let coordinator = CasProjectionCoordinator::for_healthy_home(&home).unwrap();
            let record = storage
                .thread(
                    &home,
                    thread,
                    syndic_storage::SyndicPointReadLimit::new(1_000_000).unwrap(),
                )
                .unwrap()
                .unwrap();
            let selected = SelectedPathProof::new(
                record.committed_tail(),
                record.revision(),
                record.selected_path_digest(),
            );
            let projection = coordinator
                .obtain_projection(
                    &home,
                    &storage,
                    &mut session,
                    &CasProjectionRequest::new(
                        thread,
                        selected,
                        binding,
                        ThreadStartOptions::persistent(),
                        Some(2_000_000),
                        SyndicTimestamp::from_unix_millis(37_000),
                        server::TIMEOUT,
                    ),
                    &cancellation,
                )
                .unwrap();
            let mut lifecycle = Tools;
            let mut branch = Tools;
            let mut projection = projection;
            let mut preflight_failures = Vec::with_capacity(7);
            let mut attempt = 0;
            let outcome = loop {
                attempt += 1;
                let outcome = coordinator.execute_ordinary_turn(
                    &home,
                    &storage,
                    &assets,
                    None,
                    projection,
                    &cancellation,
                    &OrdinaryTurnExecutionRequest::new(
                        TurnStartOptions::default(),
                        server::TIMEOUT,
                    ),
                    OrdinaryDynamicToolHandlers::new(&mut lifecycle, &mut branch),
                );
                match outcome {
                    Err(crate::cas_projection::OrdinaryTurnExecutionFailure::PreActivation {
                        projection: retained,
                        source,
                    }) if attempt < 8
                        && matches!(&source,
                        crate::cas_projection::OrdinaryTurnExecutionError::ConcurrentChange {
                            thread_id,
                        } if *thread_id == thread) =>
                    {
                        preflight_failures.push(source);
                        projection = *retained;
                        std::thread::yield_now();
                    }
                    outcome => break outcome,
                }
            };
            session.invalidate_connection();
            assert!(
                outcome.is_ok(),
                "native capture terminal failed after {attempt} invocation(s), retained preflight failures={preflight_failures:?}: {outcome:?}"
            );
        });
        server.wait_for_projection();
        Self {
            server: Some(server),
            worker: Some(worker),
        }
    }

    pub(super) fn started(&self, text: &str) {
        self.send(serde_json::json!({"method":"item/started","params":{
            "item":{"type":"agentMessage","id":"native-streaming-code","text":text},
            "threadId":server::CAS_THREAD_ID,"turnId":server::CAS_TURN_ID,"startedAtMs":37012
        }}));
    }

    pub(super) fn worker_finished(&self) -> bool {
        self.worker
            .as_ref()
            .is_none_or(|worker| worker.is_finished())
    }

    pub(super) fn append(&self, text: &str) {
        self.send(
            serde_json::json!({"method":"item/agentMessage/delta","params":{
                "itemId":"native-streaming-code","delta":text,
                "threadId":server::CAS_THREAD_ID,"turnId":server::CAS_TURN_ID
            }}),
        );
    }

    pub(super) fn complete(mut self, text: &str) {
        self.send(serde_json::json!({"method":"item/completed","params":{
            "item":{"type":"agentMessage","id":"native-streaming-code","text":text},
            "threadId":server::CAS_THREAD_ID,"turnId":server::CAS_TURN_ID,"completedAtMs":37013
        }}));
        let mut terminal: serde_json::Value =
            serde_json::from_str(&server::terminal_wire()).unwrap();
        terminal["params"]["turn"]["completedAt"] = serde_json::json!(37014);
        terminal["params"]["turn"]["durationMs"] = serde_json::json!(4);
        self.send(terminal);
        self.worker.take().unwrap().join().unwrap();
        let server = self.server.take().unwrap();
        server.close_connection();
        server.join();
    }

    fn send(&self, value: serde_json::Value) {
        self.server
            .as_ref()
            .unwrap()
            .send_observation(value.to_string());
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        if let Some(server) = self.server.take() {
            server.close_connection();
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
            server.join();
        }
    }
}
