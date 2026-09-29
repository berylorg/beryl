use super::*;
use gpui::{Application, AsyncApp};
use std::{cell::Cell, rc::Rc};

#[allow(dead_code)]
#[path = "running_resident_recovery_support.rs"]
mod resident_fixture;

fn prepared(candidate: HomeRecoveryCandidate) -> PreparedRecoveryServiceGraph {
    let one = NonZeroUsize::new(1).unwrap();
    let owner = ProcessServiceOwner::new(candidate.home_id(), one, one);
    let state = BerylState::reacquire_candidate(&candidate).unwrap();
    let syndic = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let configuration = crate::app_services::tests::configuration();
    let (provider, sessions) = ProcessScheduledExecutionProvider::new();
    let attention = Arc::new(ProcessLifecycleAttentionPool::new());
    let cas = PreparedRecoveryCasServices::prepare(
        owner.process.clone(),
        candidate,
        syndic.clone(),
        configuration.projection.clone(),
        Box::new(provider),
        &ProjectionCancellationToken::new(),
    )
    .unwrap_or_else(|error| panic!("CAS preparation: {}", error.error()));
    let cas = cas
        .configure_managed_sessions(
            &sessions,
            configuration.runtime_interest.clone(),
            owner.enrollments.clone(),
            RuntimeSessionPreparationConfig {
                runtime_roots: state.runtime_roots(),
                assets: state.assets(),
                policy: configuration.session_policy.clone(),
                token_directories: Vec::new(),
            },
            &attention,
            &ProjectionCancellationToken::new(),
        )
        .unwrap_or_else(|error| panic!("session preparation: {}", error.error()));
    let cas = cas
        .prepare_handoff(
            owner.settlements.clone(),
            state.clone(),
            configuration.handoff,
            SyndicTimestamp::from_unix_millis(2),
            CommandCancellation::new(),
        )
        .unwrap_or_else(|error| panic!("handoff preparation: {}", error.error()));
    let services = PreparedRecoveryAppServices::prepare(
        cas,
        &state,
        syndic.clone(),
        configuration,
        &CommandCancellation::new(),
    )
    .unwrap();
    PreparedRecoveryServiceGraph {
        services: Some(services),
        sessions,
        attention,
        state,
        syndic,
    }
}

#[test]
fn native_prepared_graph_authenticates_resident_without_transferring_candidate() {
    let finished = Rc::new(Cell::new(false));
    let observed = finished.clone();
    Application::new()
        .with_quit_on_last_window_close(false)
        .run(move |app| {
            app.spawn(async move |cx: &mut AsyncApp| {
                let resident_fixture::Resident {
                    window,
                    composer,
                    close,
                    candidate,
                    retired,
                    directory,
                    mount,
                    drafts,
                    shell,
                } = resident_fixture::prepare(cx).await;
                let seed = composer
                    .read_with(cx, |composer, _| {
                        *composer.recovery_snapshot().unwrap().restoration()
                    })
                    .unwrap();
                cx.background_executor()
                    .spawn(async move {
                        let reference = candidate.service_reference();
                        let home = candidate.home_id();
                        let generation = candidate.generation();
                        let (foreign_directory, foreign, _, _, faults) =
                            crate::app_services::tests::fixture();
                        let foreign = foreign.publish().unwrap();
                        faults.fail_next(
                            beryl_home_store::test_faults::FaultPoint::BeforeReadConfirmation,
                        );
                        assert!(foreign.home_revision().is_err());
                        let mut foreign = prepared(foreign.recover_same_home().unwrap());
                        let (retired, _) = foreign
                            .composer_recovery_source(retired, seed)
                            .err()
                            .expect("another home must not adopt the retired resident");
                        assert_eq!(retired.close_ticket(), close);
                        foreign.cancel().close().unwrap();
                        foreign_directory.close().unwrap();
                        let mut graph = prepared(candidate);
                        let mut wrong_seed = seed;
                        wrong_seed.history = None;
                        let (retired, error) = graph
                            .composer_recovery_source(retired, wrong_seed)
                            .err()
                            .expect("mismatched restoration must be refused");
                        assert!(error.contains("seed does not match"));
                        assert_eq!(retired.close_ticket(), close);
                        assert!(graph.matches_candidate(home, generation));
                        let source = graph
                            .composer_recovery_source(retired, seed)
                            .unwrap_or_else(|(_, error)| {
                                panic!("resident authentication: {error}")
                            });
                        assert_eq!(source.predecessor(), close);
                        assert_eq!(source.selection().binding().home_id(), home);
                        assert_eq!(source.selection().binding().home_generation(), generation);
                        assert_ne!(source.seed().binding, seed.binding);
                        assert_eq!(source.seed().caret, seed.caret);
                        assert!(source.window().selected_thread().is_some());
                        assert!(graph.matches_candidate(home, generation));
                        assert_eq!(
                            reference.health().state(),
                            beryl_home_store::HomeHealthState::Reopening,
                        );
                        assert!(reference.home_revision().is_err());
                        drop(source);
                        graph.cancel().close().unwrap();
                    })
                    .await;
                window
                    .update(cx, |root, window, _| {
                        assert_eq!(root.test_exit_presentation().0, "Exiting…");
                        window.remove_window();
                    })
                    .unwrap();
                drop((composer, mount, drafts, shell));
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(200))
                    .await;
                directory.close().unwrap();
                observed.set(true);
                cx.update(|app| app.quit()).unwrap();
            })
            .detach();
            app.spawn(async move |cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(15))
                    .await;
                panic!("prepared resident test completion deadline");
            })
            .detach();
        });
    assert!(finished.get());
}
