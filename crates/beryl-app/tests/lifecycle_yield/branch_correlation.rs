use beryl_app::{
    BranchDiscussionResolutionRequest, BranchDiscussionResolutionRequestHandler,
    cas_projection::{BranchDiscussionResolutionContext, OrdinaryDynamicToolHandlers},
};
use beryl_backend::DynamicToolCallResponse;

use super::*;

struct CorrelatedBranch {
    thread: beryl_model::SyndicThreadId,
    turn: SyndicTurnId,
    calls: usize,
}

impl BranchDiscussionResolutionRequestHandler for CorrelatedBranch {
    fn respond_branch_discussion_resolution(
        &mut self,
        context: BranchDiscussionResolutionContext,
        request: BranchDiscussionResolutionRequest,
    ) -> DynamicToolCallResponse {
        assert_eq!(context.ordinary().thread_id(), self.thread);
        assert_eq!(context.ordinary().turn_id(), self.turn);
        let identity = context.request_identity();
        assert_eq!(identity.cas_thread_id().as_str(), server::CAS_THREAD_ID);
        assert_eq!(identity.cas_turn_id().as_str(), server::CAS_TURN_ID);
        assert_eq!(
            identity.tool_call_id().as_str(),
            if self.calls == 0 {
                "resolution-901"
            } else {
                "resolution-903"
            }
        );
        assert_eq!(
            request.resolution(),
            "private resolution must not be retained or echoed"
        );
        self.calls += 1;
        DynamicToolCallResponse::success_text("correlated")
    }
}

#[test]
fn ordinary_branch_dispatch_preserves_exact_call_identity_and_closed_schema() {
    let mut fixture = Fixture::new(229);
    let submitted = fixture.submit_text(SUBMITTED_TEXT);
    let server = YieldServer::spawn();
    let (session, projection) = support::obtain(&fixture, &server);
    let pool = Arc::new(ProcessLifecycleAttentionPool::new());
    let mut lifecycle = fixture.store.lifecycle_yield_handler(&pool);
    let mut branch = CorrelatedBranch {
        thread: fixture.thread,
        turn: submitted.turn,
        calls: 0,
    };
    let result = thread::scope(|scope| {
        let worker = scope.spawn(|| {
            support::execute_with_handlers(
                &fixture,
                projection,
                OrdinaryDynamicToolHandlers::new(&mut lifecycle, &mut branch),
            )
        });
        server.wait_started();
        assert_eq!(server.resolve_branch()["result"]["success"], true);
        let invalid =
            server.resolve_branch_arguments(r#"{"resolution":"text","callId":"invented"}"#);
        assert_eq!(invalid["result"]["success"], false);
        assert_eq!(server.resolve_branch()["result"]["success"], true);
        server.finish(false);
        worker.join().unwrap().unwrap()
    });
    assert_eq!(branch.calls, 2);
    assert!(pool.snapshot().is_empty());
    drop(result);
    session.invalidate_connection();
    drop(session);
    server.join();
    let (directory, service) = fixture.into_service();
    service.close().unwrap();
    drop(directory);
}
