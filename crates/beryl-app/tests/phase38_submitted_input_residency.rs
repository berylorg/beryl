#![cfg(feature = "test-faults")]

#[path = "phase10_projection/syndic.rs"]
mod syndic;

#[path = "phase38_submitted_input_residency/backpressure.rs"]
mod backpressure;
#[path = "phase38_submitted_input_residency/content.rs"]
mod content;
#[path = "phase38_submitted_input_residency/failure.rs"]
mod failure;
#[path = "phase38_submitted_input_residency/fixture.rs"]
mod fixture;
#[path = "phase38_submitted_input_residency/scale.rs"]
mod scale;
#[path = "phase38_submitted_input_residency/server.rs"]
mod server;
#[path = "phase38_submitted_input_residency/verification.rs"]
mod verification;
#[path = "phase38_submitted_input_residency/wire.rs"]
mod wire;

use beryl_app::{
    BranchDiscussionResolutionRequest, BranchDiscussionResolutionRequestHandler,
    LifecycleYieldRequest, LifecycleYieldRequestHandler,
    cas_projection::{OrdinaryDynamicToolContext, OrdinaryDynamicToolHandlers},
};
use beryl_backend::DynamicToolCallResponse;

pub(crate) const EXECUTION_ROOT: &str = r"C:\work\beryl";

static PHASE38_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[derive(Default)]
pub(crate) struct NoopLifecycle;

impl LifecycleYieldRequestHandler for NoopLifecycle {
    fn respond_lifecycle_yield(
        &mut self,
        _context: OrdinaryDynamicToolContext,
        _request: LifecycleYieldRequest,
    ) -> DynamicToolCallResponse {
        DynamicToolCallResponse::success_text("unused lifecycle handler")
    }
}

#[derive(Default)]
pub(crate) struct NoopBranch;

impl BranchDiscussionResolutionRequestHandler for NoopBranch {
    fn respond_branch_discussion_resolution(
        &mut self,
        _context: OrdinaryDynamicToolContext,
        _request: BranchDiscussionResolutionRequest,
    ) -> DynamicToolCallResponse {
        DynamicToolCallResponse::success_text("unused branch handler")
    }
}

pub(crate) fn noop_handlers<'a>(
    lifecycle: &'a mut NoopLifecycle,
    branch: &'a mut NoopBranch,
) -> OrdinaryDynamicToolHandlers<'a> {
    OrdinaryDynamicToolHandlers::new(lifecycle, branch)
}

#[test]
fn submitted_input_logical_work_scales_and_local_capacity_releases() {
    let _guard = PHASE38_TEST_LOCK.lock().unwrap();
    scale::run();
}

#[test]
fn submitted_input_backpressure_stays_capacity_one() {
    let _guard = PHASE38_TEST_LOCK.lock().unwrap();
    backpressure::run();
}

#[test]
fn submitted_input_failures_preserve_taxonomy_and_release() {
    let _guard = PHASE38_TEST_LOCK.lock().unwrap();
    failure::run();
}
