use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

struct ReturnProbe {
    coordinator: Arc<crate::cas_projection::CasProjectionCoordinator>,
    thread: SyndicThreadId,
    observed: Arc<AtomicUsize>,
}

impl Drop for ReturnProbe {
    fn drop(&mut self) {
        assert!(
            matches!(
                self.coordinator.begin_projection(self.thread),
                Err(ProjectionCoordinatorError::ProjectionInFlight { .. })
            ),
            "session and tool returns must retain the exact thread flight"
        );
        self.observed.fetch_add(1, Ordering::SeqCst);
    }
}

struct ProbedSession {
    inner: Box<dyn ScheduledProjectionSessionAuthority>,
    _probe: ReturnProbe,
}

impl ScheduledProjectionSessionAuthority for ProbedSession {
    fn session(&mut self) -> &mut AdmittedProjectionSession {
        self.inner.session()
    }
}

struct ProbedTools {
    inner: Box<dyn OrdinaryDynamicToolAuthority>,
    _probe: ReturnProbe,
}

impl OrdinaryDynamicToolAuthority for ProbedTools {
    fn handlers(&mut self) -> OrdinaryDynamicToolHandlers<'_> {
        self.inner.handlers()
    }
}

struct Replacement;
impl ScheduledProjectionSessionAuthority for Replacement {
    fn session(&mut self) -> &mut AdmittedProjectionSession {
        unreachable!()
    }
}
impl OrdinaryDynamicToolAuthority for Replacement {
    fn handlers(&mut self) -> OrdinaryDynamicToolHandlers<'_> {
        unreachable!()
    }
}

pub(super) fn observe(
    mut lease: ScheduledOrdinaryExecutionLease,
    service: &ProjectionConnectionService,
) -> (ScheduledOrdinaryExecutionLease, Arc<AtomicUsize>) {
    let home = service.live_home_command().unwrap();
    let coordinator = Arc::new(
        crate::cas_projection::CasProjectionCoordinator::for_healthy_home(home.home()).unwrap(),
    );
    let observed = Arc::new(AtomicUsize::new(0));
    let probe = || ReturnProbe {
        coordinator: Arc::clone(&coordinator),
        thread: lease.thread_id,
        observed: Arc::clone(&observed),
    };
    let session = std::mem::replace(&mut lease.session, Box::new(Replacement));
    lease.session = Box::new(ProbedSession {
        inner: session,
        _probe: probe(),
    });
    let tools = std::mem::replace(&mut lease.tools, Box::new(Replacement));
    lease.tools = Box::new(ProbedTools {
        inner: tools,
        _probe: probe(),
    });
    (lease, observed)
}
