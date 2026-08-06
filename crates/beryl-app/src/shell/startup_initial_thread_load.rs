use super::thread_selection::ThreadSelectionRequest;

pub(super) trait StartupInitialThreadLoadAdapter {
    type Output;

    fn activate_exact(&mut self, request: &ThreadSelectionRequest) -> Self::Output;

    fn persisted_unavailable(&mut self, request: &ThreadSelectionRequest) -> Self::Output;

    fn restore_preferred(&mut self, request: &ThreadSelectionRequest) -> Self::Output;
}

pub(super) fn route_startup_initial_thread_load<A>(
    request: &ThreadSelectionRequest,
    adapter: &mut A,
) -> A::Output
where
    A: StartupInitialThreadLoadAdapter,
{
    match request {
        ThreadSelectionRequest::Exact { .. } => adapter.activate_exact(request),
        ThreadSelectionRequest::PersistedActiveRepairRequired { .. } => {
            adapter.persisted_unavailable(request)
        }
        ThreadSelectionRequest::RestorePreferred(_) => adapter.restore_preferred(request),
    }
}
