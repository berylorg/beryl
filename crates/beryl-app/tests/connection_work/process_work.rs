use super::*;
use beryl_app::lifecycle_attention::ProcessLifecycleAttentionPool;
use std::sync::Arc;

struct ObservationOnlyProvider(ProcessScheduledExecutionProvider);

impl ScheduledOrdinaryExecutionProvider for ObservationOnlyProvider {
    fn attach(&mut self, context: ScheduledExecutionProviderContext) {
        self.0.attach(context);
    }

    fn try_issue(
        &mut self,
        admission: ScheduledOrdinaryAdmission,
    ) -> Result<ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryAdmissionError> {
        Ok(admission.decline(ScheduledOrdinaryExecutionUnavailable::RuntimeNotReady))
    }

    fn shutdown(&mut self) {
        self.0.shutdown();
    }
}

pub(super) fn fixture(seed: u8) -> (Fixture, ScheduledExecutionSessions) {
    let (provider, sessions) = ProcessScheduledExecutionProvider::new();
    (
        Fixture::new_with_scheduled_provider(seed, |_| Box::new(ObservationOnlyProvider(provider))),
        sessions,
    )
}

pub(super) fn register(
    fixture: &Fixture,
    sessions: &ScheduledExecutionSessions,
    session: AdmittedProjectionSession,
) -> ScheduledSessionRegistration {
    let attention = Arc::new(ProcessLifecycleAttentionPool::new());
    sessions
        .register(
            fixture.thread,
            execution_binding(),
            session,
            ScheduledOrdinaryRequestPolicy::backend_defaults(
                fixture.state.settings(),
                Some(2_000_000),
                TIMEOUT,
                TIMEOUT,
            ),
            fixture.state.assets(),
            Box::new(fixture.store.ordinary_dynamic_tool_authority(&attention)),
        )
        .unwrap()
}

pub(super) fn assert_request_work(
    fixture: &Fixture,
    sessions: &ScheduledExecutionSessions,
    expected: bool,
) {
    let attention = ProcessLifecycleAttentionPool::new();
    let inventory = fixture.store.process_work_inventory(sessions, &attention);
    let cancellation = ProjectionCancellationToken::new();
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let home_before = fixture.home().home_revision().unwrap();
        let revision = inventory.revision().unwrap();
        let public = inventory.page(
            &revision,
            None,
            ProcessWorkPageLimits::new(256, 65_536).unwrap(),
            &cancellation,
        );
        let targeted = fixture
            .store
            .required_session_work_for_test(sessions, &cancellation);
        if let (Ok(public), Ok(targeted)) = (public, targeted) {
            assert_eq!(targeted.len(), 1);
            let public = public
                .records()
                .iter()
                .find(|row| row.thread_id == fixture.thread)
                .unwrap();
            assert_eq!(targeted[0].0, fixture.thread);
            assert_eq!(public.facts, targeted[0].2);
            assert_eq!(public.facts.request_handling, expected);
            assert_eq!(fixture.home().home_revision().unwrap(), home_before);
            return;
        }
        assert!(Instant::now() < deadline, "work observation did not settle");
        thread::yield_now();
    }
}

#[test]
fn paused_response_writer_alone_remains_required_until_successful_write() {
    let (fixture, server, session, target, sessions) = live_fixture(225);
    let registration = register(&fixture, &sessions, session);
    server.send_requests(1);
    let LiveEventPoll::DynamicTool(call) = target.poll(TIMEOUT) else {
        panic!("missing call");
    };
    let barrier = test_faults::install_response_write_barrier(fixture.thread);
    let writer = thread::spawn(move || {
        test_faults::respond_routed_dynamic_tool(
            &target,
            call,
            DynamicToolCallResponse::success_text("complete"),
        )
        .unwrap();
        target
    });
    barrier.wait();
    let paused = page(&fixture.store);
    let requests: Vec<_> = paused
        .records()
        .iter()
        .filter_map(|row| match row {
            ConnectionWorkRecord::Request(fact) => Some(fact),
            _ => None,
        })
        .collect();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].stage(),
        ConnectionRequestWorkStage::ResponseAdmitted
    );
    assert!(!requests[0].response().response_written());
    assert_request_work(&fixture, &sessions, true);
    barrier.release();
    let target = writer.join().unwrap();
    server.wait_for_response();
    assert_request_work(&fixture, &sessions, false);
    drop(target);
    assert!(sessions.retire(registration));
    let (directory, service) = fixture.into_service();
    let _ = service.close().unwrap();
    server.join();
    drop(directory);
}
