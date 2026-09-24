#![cfg(feature = "test-faults")]

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
    let fixture = syndic::Fixture::new(130);
    let resolution = format!("Literal [image:A] and \"quotes\"\n{}", "🦀".repeat(8_192));
    let expected = format!("Discussion resolution:\n\n{resolution}");
    let limit = SyndicPointReadLimit::new(400_000).unwrap();
    let (input, receipt, _) = support::generated_input::seed(
        &fixture.home(),
        fixture.storage.clone(),
        &resolution,
    );
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
    let server = server::NormalTerminalServer::spawn_generated_terminal(
        "generated-parent",
        &expected,
        execution.root_path().as_str(),
    );
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
    let result = coordinator
        .execute_ordinary_turn(
            &fixture.home(),
            &fixture.storage,
            &fixture.state.assets(),
            projection,
            &fixture.cancellation,
            &request,
            OrdinaryDynamicToolHandlers::new(&mut lifecycle, &mut branch),
        )
        .unwrap();
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
