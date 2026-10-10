#[allow(dead_code)]
#[path = "../accepted_next_scheduler/support.rs"]
mod scheduler_support;
#[path = "../attempt_policy/server.rs"]
mod server;
#[path = "../attempt_policy/support.rs"]
mod support;
#[path = "../projection/syndic.rs"]
mod syndic;

use super::*;
use crate::cas_projection::{
    OrdinaryTurnExecutionRequest, ProcessScheduledExecutionProvider,
    ScheduledOrdinaryRequestPolicy, test_faults::install_scheduled_promotion_barrier,
};
use beryl_backend::{ProtocolIdentity, ReasoningEffort};
use serde_json::json;
use server::{AttemptServer, SUBMITTED_TEXT, TIMEOUT, hidden_context};
use support::{admit, apply_instructions, execute, obtain, rejected_projection};
use syndic::Fixture;
use syndic_storage::TurnLifecycle;

const EXECUTION_ROOT: &str = crate::EXECUTION_ROOT;

fn choice(reasoning: Option<ReasoningEffort>) -> ThreadModelChoice {
    ThreadModelChoice {
        model: ProtocolIdentity::try_new("selected-model").unwrap(),
        reasoning,
    }
}

fn expected(instructions: &str) -> serde_json::Value {
    let mut value = hidden_context("selected-model", Some("low"), Some(instructions));
    value["model"] = json!("selected-model");
    value["effort"] = json!("low");
    value
}

#[test]
fn accepted_choice_gives_way_to_metadata_reloaded_from_the_same_backend_thread() {
    use crate::cas_projection::{
        CasProjectionCoordinator, CasProjectionRequest, OrdinaryTurnExecutionOutcome,
    };
    use beryl_backend::ThreadStartOptions;
    use syndic_storage::SyndicTimestamp;

    let (provider, sessions) = ProcessScheduledExecutionProvider::new();
    let mut fixture = Fixture::new_with_scheduled_provider(216, move |_| Box::new(provider));
    eprintln!("model-choice Home: {}", fixture.home_path().display());
    let thread = fixture.thread;
    let binding = syndic::execution_binding();
    sessions
        .select_next_turn_model_elected(thread, &binding, choice(Some(ReasoningEffort::Low)))
        .unwrap();
    fixture.submit_text(SUBMITTED_TEXT);
    apply_instructions(&fixture, "accepted instructions");
    let pending = sessions.pending_model_choice(thread, &binding).unwrap();
    let request = OrdinaryTurnExecutionRequest::backend_defaults(fixture.state.settings(), TIMEOUT)
        .with_pending_model_choice(pending);
    let server = AttemptServer::spawn_reloading(
        "accepted-choice-metadata".into(),
        expected("accepted instructions"),
    );
    let mut admitted = admit(&fixture, &server, 950_816);
    let projection = obtain(&fixture, &mut admitted, thread);
    assert_eq!(
        projection
            .observed_thread_metadata()
            .unwrap()
            .unwrap()
            .model
            .as_deref(),
        Some("prior-model")
    );
    let OrdinaryTurnExecutionOutcome::Terminal { projection, status } =
        execute(&fixture, projection, &request).unwrap()
    else {
        panic!("the actual chosen ordinary turn must complete");
    };
    assert_eq!(status, syndic_storage::TurnEndStatus::complete());
    assert!(sessions.pending_model_choice(thread, &binding).is_none());
    assert_eq!(
        projection.observed_thread_metadata().unwrap(),
        Some(beryl_backend::ThreadSessionMetadata::default())
    );
    assert!(matches!(
        projection.release().unwrap(),
        crate::cas_projection::LoadedProjectionReleaseOutcome::Unsubscribe(
            beryl_backend::ThreadUnsubscribeStatus::Unsubscribed
        )
    ));

    let loaded = CasProjectionCoordinator::for_healthy_home(&fixture.home())
        .unwrap()
        .obtain_completed_projection_for_test(
            &fixture.home(),
            &fixture.storage,
            &mut admitted,
            &CasProjectionRequest::new(
                thread,
                fixture.selected_path(thread),
                binding.clone(),
                ThreadStartOptions::persistent(),
                Some(1_000_000),
                SyndicTimestamp::from_unix_millis(96_000),
                TIMEOUT,
            ),
            &fixture.cancellation,
        )
        .unwrap();
    let metadata = loaded.observed_thread_metadata().unwrap().unwrap();
    assert_eq!(metadata.model.as_deref(), Some("selected-model"));
    assert_eq!(metadata.reasoning_effort.as_deref(), Some("low"));
    let registration = sessions
        .register(
            thread,
            binding.clone(),
            admitted,
            ScheduledOrdinaryRequestPolicy::backend_defaults(
                fixture.state.settings(),
                Some(1_000_000),
                TIMEOUT,
                TIMEOUT,
            ),
            fixture.state.assets(),
            scheduler_support::tool_authority(),
        )
        .unwrap();
    assert_eq!(
        sessions.observed_model_metadata(thread, &binding),
        Some(metadata)
    );
    assert!(sessions.pending_model_choice(thread, &binding).is_none());
    assert!(matches!(
        loaded.release().unwrap(),
        crate::cas_projection::LoadedProjectionReleaseOutcome::Unsubscribe(
            beryl_backend::ThreadUnsubscribeStatus::Unsubscribed
        )
    ));
    sessions.retire(registration);
    let (directory, service) = fixture.into_service();
    assert!(matches!(
        service.close().unwrap(),
        crate::cas_projection::ProjectionConnectionServiceCloseOutcome::Closed
    ));
    server.join();
    drop(directory);
}

#[test]
fn scheduled_choice_reaches_real_turn_with_latest_instructions_and_retires_on_acceptance() {
    let (provider, sessions) = ProcessScheduledExecutionProvider::new();
    let mut fixture = Fixture::new_with_scheduled_provider(211, move |_| Box::new(provider));
    eprintln!("model-choice Home: {}", fixture.home_path().display());
    let ids = scheduler_support::seed_runtime_next_input_without_wake(&mut fixture, 211);
    sessions
        .select_next_turn_model_elected(
            ids.thread,
            &syndic::execution_binding(),
            choice(Some(ReasoningEffort::Low)),
        )
        .unwrap();
    let policy = ScheduledOrdinaryRequestPolicy::backend_defaults(
        fixture.state.settings(),
        Some(1_000_000),
        TIMEOUT,
        TIMEOUT,
    );
    let cas_thread =
        scheduler_support::current_cas_thread_id(&*fixture.home(), &fixture.storage, ids.thread);
    let server = AttemptServer::spawn(
        "thread/resume",
        cas_thread,
        "prior-model",
        Some("high"),
        vec![expected("latest instructions")],
        true,
    );
    let session = admit(&fixture, &server, 950_811);
    let barrier = install_scheduled_promotion_barrier(ids.thread);
    sessions
        .register(
            ids.thread,
            syndic::execution_binding(),
            session,
            policy,
            fixture.state.assets(),
            scheduler_support::tool_authority(),
        )
        .unwrap();
    assert!(barrier.wait_until_paused(TIMEOUT));
    assert!(
        sessions
            .pending_model_choice(ids.thread, &syndic::execution_binding())
            .is_some()
    );
    assert_eq!(
        sessions.select_next_turn_model_elected(
            ids.thread,
            &syndic::execution_binding(),
            choice(None)
        ),
        Err(ThreadModelChoiceError::Active)
    );
    apply_instructions(&fixture, "latest instructions");
    barrier.release();
    scheduler_support::wait_until("selected scheduled turn completes", || {
        let home = fixture.home();
        let record = fixture
            .storage
            .thread(&home, ids.thread, scheduler_support::point_limit())
            .ok()??;
        let tail = record.committed_tail()?;
        let state = fixture
            .storage
            .turn_state(&home, tail, scheduler_support::point_limit())
            .ok()??;
        (tail != ids.parent
            && state.lifecycle() == TurnLifecycle::Complete
            && sessions.diagnostics().available == 1)
            .then_some(())
    });
    assert!(
        sessions
            .pending_model_choice(ids.thread, &syndic::execution_binding())
            .is_none()
    );
    let (directory, service) = fixture.into_service();
    assert!(matches!(
        service.close().unwrap(),
        crate::cas_projection::ProjectionConnectionServiceCloseOutcome::Closed
    ));
    server.join();
    assert_eq!(sessions.diagnostics().retained, 0);
    drop(directory);
}

#[test]
fn exact_rejection_preserves_choice_for_explicit_retry_with_new_instructions() {
    let (provider, sessions) = ProcessScheduledExecutionProvider::new();
    let mut fixture = Fixture::new_with_scheduled_provider(212, move |_| Box::new(provider));
    eprintln!("model-choice Home: {}", fixture.home_path().display());
    sessions
        .select_next_turn_model_elected(
            fixture.thread,
            &syndic::execution_binding(),
            choice(Some(ReasoningEffort::Low)),
        )
        .unwrap();
    fixture.submit_text(SUBMITTED_TEXT);
    let pending = sessions
        .pending_model_choice(fixture.thread, &syndic::execution_binding())
        .unwrap();
    let request = OrdinaryTurnExecutionRequest::backend_defaults(fixture.state.settings(), TIMEOUT)
        .with_pending_model_choice(pending);
    let server = AttemptServer::spawn_attempts(
        "thread/start",
        "model-choice-retry".into(),
        "prior-model",
        Some("high"),
        vec![expected("first"), expected("second")],
        vec![false, true],
    );
    let mut session = admit(&fixture, &server, 950_812);
    let projection = obtain(&fixture, &mut session, fixture.thread);
    apply_instructions(&fixture, "first");
    let projection = rejected_projection(execute(&fixture, projection, &request).unwrap());
    assert!(
        sessions
            .pending_model_choice(fixture.thread, &syndic::execution_binding())
            .is_some()
    );
    apply_instructions(&fixture, "second");
    let outcome = execute(&fixture, projection, &request).unwrap();
    assert!(
        sessions
            .pending_model_choice(fixture.thread, &syndic::execution_binding())
            .is_none()
    );
    drop(outcome);
    session.invalidate_connection();
    drop(session);
    let (directory, service) = fixture.into_service();
    assert!(matches!(
        service.close().unwrap(),
        crate::cas_projection::ProjectionConnectionServiceCloseOutcome::Closed
    ));
    server.join();
    drop(directory);
}

#[test]
fn old_acknowledgement_cannot_retire_a_newer_thread_choice() {
    let (provider, sessions) = ProcessScheduledExecutionProvider::new();
    let fixture = Fixture::new_with_scheduled_provider(213, move |_| Box::new(provider));
    eprintln!("model-choice Home: {}", fixture.home_path().display());
    let binding = syndic::execution_binding();
    sessions
        .select_next_turn_model_elected(
            fixture.thread,
            &binding,
            choice(Some(ReasoningEffort::Low)),
        )
        .unwrap();
    let prior = sessions
        .pending_model_choice(fixture.thread, &binding)
        .unwrap();
    sessions
        .select_next_turn_model_elected(fixture.thread, &binding, choice(None))
        .unwrap();
    prior.accepted();
    assert_eq!(
        sessions
            .pending_model_choice(fixture.thread, &binding)
            .unwrap()
            .choice,
        choice(None)
    );
    let (directory, service) = fixture.into_service();
    assert!(matches!(
        service.close().unwrap(),
        crate::cas_projection::ProjectionConnectionServiceCloseOutcome::Closed
    ));
    assert!(
        sessions
            .pending_model_choice(prior.thread, &binding)
            .is_none()
    );
    drop(directory);
}

#[test]
fn pre_choice_status_cannot_publish_after_choice_acceptance_restores_absence() {
    let (provider, sessions) = ProcessScheduledExecutionProvider::new();
    let fixture = Fixture::new_with_scheduled_provider(215, move |_| Box::new(provider));
    eprintln!("model-choice Home: {}", fixture.home_path().display());
    let binding = syndic::execution_binding();
    let (absent, epoch) = sessions
        .model_choice_snapshot(fixture.thread, &binding)
        .unwrap();
    assert!(absent.is_none());
    sessions
        .select_next_turn_model_elected(
            fixture.thread,
            &binding,
            choice(Some(ReasoningEffort::Low)),
        )
        .unwrap();
    sessions
        .pending_model_choice(fixture.thread, &binding)
        .unwrap()
        .accepted();
    let mut published = false;
    assert!(
        sessions
            .with_current_model_choice_elected(fixture.thread, &binding, None, epoch, || {
                published = true
            })
            .is_err()
    );
    assert!(!published);
    let (absent, current_epoch) = sessions
        .model_choice_snapshot(fixture.thread, &binding)
        .unwrap();
    assert!(absent.is_none());
    sessions
        .with_current_model_choice_elected(fixture.thread, &binding, None, current_epoch, || {
            published = true
        })
        .unwrap();
    assert!(published);
    let (directory, service) = fixture.into_service();
    assert!(matches!(
        service.close().unwrap(),
        crate::cas_projection::ProjectionConnectionServiceCloseOutcome::Closed
    ));
    drop(directory);
}

#[test]
fn fixed_policy_keeps_instructions_and_omits_unselected_reasoning_on_real_dispatch() {
    let (provider, sessions) = ProcessScheduledExecutionProvider::new();
    let mut fixture = Fixture::new_with_scheduled_provider(214, move |_| Box::new(provider));
    eprintln!("model-choice Home: {}", fixture.home_path().display());
    let binding = syndic::execution_binding();
    sessions
        .select_next_turn_model_elected(fixture.thread, &binding, choice(None))
        .unwrap();
    fixture.submit_text(SUBMITTED_TEXT);
    let pending = sessions
        .pending_model_choice(fixture.thread, &binding)
        .unwrap();
    let options = beryl_backend::TurnStartOptions::default()
        .with_model("prior-model")
        .with_reasoning_effort("high")
        .with_developer_instructions_context(
            Some("fixed instructions".to_owned()),
            "prior-model",
            Some("high".to_owned()),
        );
    let request =
        OrdinaryTurnExecutionRequest::new(options, TIMEOUT).with_pending_model_choice(pending);
    let mut expected = hidden_context("selected-model", None, Some("fixed instructions"));
    expected["model"] = json!("selected-model");
    let server = AttemptServer::spawn(
        "thread/start",
        "model-choice-fixed".into(),
        "prior-model",
        Some("high"),
        vec![expected],
        false,
    );
    let mut session = admit(&fixture, &server, 950_814);
    let projection = obtain(&fixture, &mut session, fixture.thread);
    let projection = rejected_projection(execute(&fixture, projection, &request).unwrap());
    assert!(
        sessions
            .pending_model_choice(fixture.thread, &binding)
            .is_some()
    );
    session.invalidate_connection();
    drop(projection);
    drop(session);
    let (directory, service) = fixture.into_service();
    assert!(matches!(
        service.close().unwrap(),
        crate::cas_projection::ProjectionConnectionServiceCloseOutcome::Closed
    ));
    server.join();
    drop(directory);
}
