use super::*;
use crate::cas_projection::{
    OrdinaryDynamicToolAuthority, OrdinaryDynamicToolHandlers, OrdinaryTurnExecutionRequest,
    ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryExecutionLease,
    ScheduledOrdinaryRequestPolicy, ScheduledSessionRegistration,
};
use beryl_backend::{ManagedBackendClientConnector, ThreadStartOptions, TurnStartOptions};

#[path = "../../normal_terminal/server.rs"]
mod server;

struct NoTools;
impl OrdinaryDynamicToolAuthority for NoTools {
    fn handlers(&mut self) -> OrdinaryDynamicToolHandlers<'_> {
        panic!("selection must not dispatch dynamic tools")
    }
}

pub(super) struct LiveSession {
    server: server::NormalTerminalServer,
    lease: ScheduledOrdinaryExecutionLease,
    registration: ScheduledSessionRegistration,
}

impl LiveSession {
    pub(super) fn install(
        source: super::super::super::tests::Source,
        execution: beryl_model::ExecutionBinding,
        thread: SyndicThreadId,
        assets: beryl_state::AssetState,
        cx: &mut gpui::TestAppContext,
    ) -> (super::super::super::tests::Source, Self) {
        let server = server::NormalTerminalServer::spawn_admission_only_controlled_close();
        let endpoint = server.endpoint();
        let (source, lease, registration) = home_support::join(
            home_support::worker(move || {
                let mut source = source;
                let capacity = std::num::NonZeroUsize::new(8).unwrap();
                source.service.configure_runtime_interest(
                    crate::cas_projection::RuntimeInterestConfig::new(
                        capacity,
                        std::num::NonZeroUsize::new(32).unwrap(),
                        server::TIMEOUT,
                    ).unwrap(),
                    crate::runtime_activity_enrollment::RuntimeActivityEnrollmentOperations::new(
                        source.service.home_id(), capacity,
                    ),
                ).unwrap();
                let service = &source.service;
                let sessions = &source._sessions;
                let connector = ManagedBackendClientConnector::for_lifecycle_test(
                    endpoint,
                    server::AUTHORIZATION,
                );
                let session = service
                    .admit_runtime_lifecycle_test_candidate(
                        &connector,
                        execution.clone(),
                        beryl_model::CasProcessGeneration::new(73_001).unwrap(),
                        std::path::Path::new(r"C:\Work\Beryl"),
                        server::TIMEOUT,
                    )
                    .unwrap();
                let policy = ScheduledOrdinaryRequestPolicy::new(
                    ThreadStartOptions::persistent(),
                    Some(2_000_000),
                    server::TIMEOUT,
                    OrdinaryTurnExecutionRequest::new(TurnStartOptions::default(), server::TIMEOUT),
                );
                let registration = sessions
                    .register(
                        thread,
                        execution.clone(),
                        session,
                        policy,
                        assets,
                        Box::new(NoTools),
                    )
                    .unwrap();
                let ScheduledOrdinaryAdmissionResult::Issued(lease) = service
                    .checkout_scheduled_session_for_test(thread, execution)
                    .unwrap()
                else {
                    panic!("exact live session was not issued")
                };
                (source, lease, registration)
            }),
            cx,
        );
        server.wait_for_admission();
        assert_eq!(lease.thread_id(), thread);
        assert_eq!(source._sessions.diagnostics().checked_out, 1);
        (
            source,
            Self {
                server,
                lease,
                registration,
            },
        )
    }

    pub(super) fn verify(&mut self, thread: SyndicThreadId) {
        assert_eq!(self.lease.thread_id(), thread);
        assert_eq!(self.lease.process_generation().get(), 73_001);
        assert!(self.lease.session_authority_current_for_test());
        assert_eq!(self.server.generated_turn_starts(), 0);
    }

    pub(super) fn close(self, source: &super::super::super::tests::Source) {
        self.server.assert_quiet_and_close();
        self.server.join();
        drop(self.lease);
        source._sessions.retire(self.registration);
    }
}
