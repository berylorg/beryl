#![cfg(feature = "test-faults")]

#[path = "support/generated_handoff.rs"]
mod generated_handoff;
#[path = "generated_input_execution/scheduler.rs"]
mod scheduler;
#[path = "normal_terminal/server.rs"]
mod server;
#[path = "../../syndic-storage/tests/support/mod.rs"]
mod support;
#[path = "projection/syndic.rs"]
mod syndic;

use beryl_app::cas_projection::*;
use beryl_app::{
    BranchDiscussionResolutionRequest, BranchDiscussionResolutionRequestHandler,
    LifecycleYieldRequest, LifecycleYieldRequestHandler,
};
use beryl_backend::{
    DynamicToolCallResponse, ManagedBackendClientConnector, ThreadStartOptions, TurnStartOptions,
};
use beryl_model::{CasNativeTurnCount, CasProcessGeneration, CasThreadId};
use std::path::Path;
use syndic_storage::*;

pub(crate) const EXECUTION_ROOT: &str = r"C:\populated";

struct Noop;
impl LifecycleYieldRequestHandler for Noop {
    fn respond_lifecycle_yield(
        &mut self,
        _: OrdinaryDynamicToolContext,
        _: LifecycleYieldRequest,
    ) -> DynamicToolCallResponse {
        panic!("unexpected lifecycle tool")
    }
}
impl BranchDiscussionResolutionRequestHandler for Noop {
    fn respond_branch_discussion_resolution(
        &mut self,
        _: BranchDiscussionResolutionContext,
        _: BranchDiscussionResolutionRequest,
    ) -> DynamicToolCallResponse {
        panic!("unexpected branch tool")
    }
}

#[test]
fn generated_resolution_replays_and_correlates_once_through_ordinary_cas_execution() {
    run_generated_execution(false, false);
}

#[test]
fn generated_rejection_requires_explicit_retry_of_the_same_parent_turn() {
    run_generated_execution(true, false);
}

#[test]
fn generated_unknown_dispatch_remains_ineligible_for_retry() {
    run_generated_execution(false, true);
}

fn run_generated_execution(reject_first: bool, lose_response: bool) {
    let fixture = syndic::Fixture::new(130);
    let resolution = format!("Literal [image:A] and \"quotes\"\n{}", "🦀".repeat(8_192));
    let expected = format!("Discussion resolution:\n\n{resolution}");
    let limit = SyndicPointReadLimit::new(400_000).unwrap();
    let (handoff, input, receipt) = generated_handoff::seed(&fixture, &resolution);
    let thread = input.thread_id();
    let execution = fixture
        .storage
        .thread_execution(&fixture.home(), thread, limit)
        .unwrap()
        .unwrap()
        .execution()
        .clone();
    let turn = fixture
        .storage
        .turn(&fixture.home(), receipt.parent_turn_id, limit)
        .unwrap()
        .unwrap();
    let parent = fixture
        .storage
        .turn(&fixture.home(), turn.parent().turn().unwrap(), limit)
        .unwrap()
        .unwrap();
    let selected = fixture.selected_path(thread);
    let represented = CasRepresentedPrefixProof::new(
        Some(parent.id()),
        selected.thread_revision(),
        parent.chain_digest(),
    );
    let current = fixture
        .storage
        .current_binding(&fixture.home(), thread, limit)
        .unwrap()
        .unwrap();
    let profile = beryl_app::conversation_tools::ConversationToolRegistry::canonical().profile();
    support::discussion_input::committed(
        &fixture.home(),
        fixture.storage.publish_valid_binding(
            fixture.storage.revision(&fixture.home()).unwrap(),
            PublishValidBinding::new(
                thread,
                current.binding().revision(),
                selected,
                execution.clone(),
                CasThreadId::new("generated-parent").unwrap(),
                represented,
                CasNativeTurnCount::new(parent.depth().get()),
                profile,
                CasLineageProof::native(NativeCasLineage::Resume, represented).unwrap(),
            ),
        ),
    );
    let server = if lose_response {
        server::NormalTerminalServer::spawn_generated_response_loss(
            "generated-parent",
            &expected,
            execution.root_path().as_str(),
        )
    } else if reject_first {
        server::NormalTerminalServer::spawn_generated_rejection_then_terminal(
            "generated-parent",
            &expected,
            execution.root_path().as_str(),
        )
    } else {
        server::NormalTerminalServer::spawn_generated_terminal(
            "generated-parent",
            &expected,
            execution.root_path().as_str(),
        )
    };
    let connector =
        ManagedBackendClientConnector::for_lifecycle_test(server.endpoint(), server::AUTHORIZATION);
    let mut session = fixture
        .store
        .admit_lifecycle_test_candidate(
            &connector,
            execution.runtime_id(),
            CasProcessGeneration::new(59_901).unwrap(),
            Path::new(execution.root_path().as_str()),
            server::TIMEOUT,
        )
        .unwrap();
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.home()).unwrap();
    let projection = coordinator
        .obtain_projection(
            &fixture.home(),
            &fixture.storage,
            &mut session,
            &CasProjectionRequest::new(
                thread,
                selected,
                execution,
                ThreadStartOptions::persistent(),
                Some(2_000_000),
                SyndicTimestamp::from_unix_millis(37_000),
                server::TIMEOUT,
            ),
            &fixture.cancellation,
        )
        .unwrap();
    server.wait_for_projection();
    let request = OrdinaryTurnExecutionRequest::new(TurnStartOptions::default(), server::TIMEOUT);
    let mut lifecycle = Noop;
    let mut branch = Noop;
    let failure = coordinator
        .execute_ordinary_turn(
            &fixture.home(),
            &fixture.storage,
            &fixture.state.assets(),
            None,
            projection,
            &fixture.cancellation,
            &request,
            OrdinaryDynamicToolHandlers::new(&mut lifecycle, &mut branch),
        )
        .unwrap_err();
    let OrdinaryTurnExecutionFailure::PreActivation {
        projection,
        source: OrdinaryTurnExecutionError::HandoffAuthorityUnavailable,
    } = failure
    else {
        panic!("missing handoff authority must reject before activation");
    };
    let projection = if !reject_first && !lose_response {
        let foreign = syndic::Fixture::new(134);
        let foreign_handoff = beryl_app::discussion_settlement::DiscussionSettlementService::new(
            beryl_app::discussion_settlement::DiscussionSettlementOperations::new(
                foreign.process_admission.clone(),
                std::num::NonZeroUsize::new(1).unwrap(),
            ),
            foreign.home().service_reference(),
            foreign.state.clone(),
            foreign.storage.clone(),
        );
        let failure = coordinator
            .execute_ordinary_turn(
                &fixture.home(),
                &fixture.storage,
                &fixture.state.assets(),
                Some(&foreign_handoff),
                *projection,
                &fixture.cancellation,
                &request,
                OrdinaryDynamicToolHandlers::new(&mut lifecycle, &mut branch),
            )
            .unwrap_err();
        let OrdinaryTurnExecutionFailure::PreActivation {
            projection,
            source: OrdinaryTurnExecutionError::HandoffAuthorityUnavailable,
        } = failure
        else {
            panic!("foreign authority must reject before activation");
        };
        drop(foreign_handoff);
        let (directory, service) = foreign.into_service();
        service.close().unwrap();
        drop(directory);
        projection
    } else {
        projection
    };
    let reserved = handoff
        .reserve_parent_dispatch(
            beryl_model::JobId::from_bytes(*input.id().as_bytes()),
            thread,
            receipt.parent_turn_id,
        )
        .unwrap();
    let failure = coordinator
        .execute_ordinary_turn(
            &fixture.home(),
            &fixture.storage,
            &fixture.state.assets(),
            Some(&handoff),
            *projection,
            &fixture.cancellation,
            &request,
            OrdinaryDynamicToolHandlers::new(&mut lifecycle, &mut branch),
        )
        .unwrap_err();
    let OrdinaryTurnExecutionFailure::PreActivation {
        projection,
        source:
            OrdinaryTurnExecutionError::HandoffSettlement(
                beryl_app::discussion_settlement::DiscussionSettlementError::DuplicateIdentity,
            ),
    } = failure
    else {
        panic!("reserved handoff must reject before activation");
    };
    assert!(
        fixture
            .storage
            .pending_dispatch_evidence(&fixture.home(), thread, limit,)
            .unwrap()
            .is_some()
    );
    drop(reserved);
    let mut result = coordinator
        .execute_ordinary_turn(
            &fixture.home(),
            &fixture.storage,
            &fixture.state.assets(),
            Some(&handoff),
            *projection,
            &fixture.cancellation,
            &request,
            OrdinaryDynamicToolHandlers::new(&mut lifecycle, &mut branch),
        )
        .unwrap();
    if lose_response {
        assert!(matches!(
            result,
            OrdinaryTurnExecutionOutcome::Incomplete { .. }
        ));
        assert!(
            fixture
                .storage
                .pending_dispatch_evidence(&fixture.home(), thread, limit)
                .unwrap()
                .is_none()
        );
        let job_id = beryl_model::JobId::from_bytes(*input.id().as_bytes());
        let job = fixture
            .state
            .durable_jobs()
            .job(&fixture.home(), job_id)
            .unwrap()
            .unwrap();
        assert_eq!(
            job.lifecycle(),
            beryl_state::BranchHandoffJobLifecycle::StartingParent
        );
        assert!(
            handoff
                .prepare_retry(
                    job_id,
                    job.revision(),
                    beryl_home_store::CommandCancellation::new()
                )
                .is_err()
        );
        assert_eq!(server.generated_turn_starts(), 1);
        session.invalidate_connection();
        drop(session);
        server.join();
        let (directory, service) = fixture.into_service();
        service.close().unwrap();
        drop(directory);
        return;
    }
    if reject_first {
        let OrdinaryTurnExecutionOutcome::NotStarted {
            projection: OrdinaryNotStartedProjection::Retained(projection),
            reason: OrdinaryTurnNotStarted::ExactRejection(_),
        } = result
        else {
            panic!("exact rejection must preserve the projection for retry: {result:?}");
        };
        let job_id = beryl_model::JobId::from_bytes(*input.id().as_bytes());
        let job = fixture
            .state
            .durable_jobs()
            .job(&fixture.home(), job_id)
            .unwrap()
            .unwrap();
        assert_eq!(
            job.lifecycle(),
            beryl_state::BranchHandoffJobLifecycle::RetryableFailed
        );
        let failure = coordinator
            .execute_ordinary_turn(
                &fixture.home(),
                &fixture.storage,
                &fixture.state.assets(),
                Some(&handoff),
                *projection,
                &fixture.cancellation,
                &request,
                OrdinaryDynamicToolHandlers::new(&mut lifecycle, &mut branch),
            )
            .unwrap_err();
        let OrdinaryTurnExecutionFailure::PreActivation {
            projection,
            source:
                OrdinaryTurnExecutionError::HandoffSettlement(
                    beryl_app::discussion_settlement::DiscussionSettlementError::IdentityMismatch,
                ),
        } = failure
        else {
            panic!("paused jobs cannot dispatch on an unrelated execution attempt");
        };
        assert!(matches!(
            handoff
                .prepare_retry(
                    job_id,
                    job.revision(),
                    beryl_home_store::CommandCancellation::new()
                )
                .unwrap()
                .execute(),
            beryl_app::discussion_settlement::DiscussionSettlementOutcome::Committed {
                later_failure: None,
                ..
            }
        ));
        result = coordinator
            .execute_ordinary_turn(
                &fixture.home(),
                &fixture.storage,
                &fixture.state.assets(),
                Some(&handoff),
                *projection,
                &fixture.cancellation,
                &request,
                OrdinaryDynamicToolHandlers::new(&mut lifecycle, &mut branch),
            )
            .unwrap();
    }
    let OrdinaryTurnExecutionOutcome::Terminal { projection, status } = result else {
        panic!("generated turn must settle");
    };
    assert_eq!(status, TurnEndStatus::complete());
    let item = fixture
        .storage
        .canonical_item(&fixture.home(), receipt.canonical_item_id, limit)
        .unwrap()
        .unwrap();
    assert_eq!(
        item.presentation(),
        &CanonicalItemPresentation::DiscussionHandoff {
            content: input.content(),
            accepted_input_id: input.id()
        }
    );
    assert_eq!(item.provider_lifecycle(), ProviderItemLifecycle::Completed);
    assert_eq!(
        fixture
            .storage
            .turn_state(&fixture.home(), receipt.parent_turn_id, limit)
            .unwrap()
            .unwrap()
            .item_count(),
        1
    );
    assert_eq!(
        fixture
            .storage
            .accepted_input(&fixture.home(), input.id(), limit)
            .unwrap(),
        Some(input)
    );
    fixture
        .home()
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    session.invalidate_connection();
    drop(projection);
    drop(session);
    server.join();
    let (directory, service) = fixture.into_service();
    service.close().unwrap();
    drop(directory);
}
