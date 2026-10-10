use super::*;

pub(super) struct SelectedContext {
    connection: Weak<crate::cas_projection::connection::ProjectionConnection>,
    snapshot: crate::cas_projection::connection::registry::ContextSnapshot,
}

impl SelectedContext {
    pub(super) fn usage(&self) -> Option<beryl_backend::ContextTokenUsage> {
        self.snapshot.observation.usage()
    }

    pub(super) fn is_current(&self) -> bool {
        self.connection
            .upgrade()
            .is_some_and(|connection| connection.context_is_current(&self.snapshot.stamp))
    }
}

impl PartialEq for SelectedContext {
    fn eq(&self, other: &Self) -> bool {
        self.connection.ptr_eq(&other.connection) && self.snapshot.stamp == other.snapshot.stamp
    }
}

impl ExactStopRead {
    pub(super) fn selected_context(&self, thread: SyndicThreadId) -> Option<SelectedContext> {
        let home = self.home.as_deref()?;
        let limit = SyndicPointReadLimit::new(1_000_000).ok()?;
        let binding = self.storage.current_binding(home, thread, limit).ok()??;
        let valid = match binding.binding().state() {
            syndic_storage::BindingState::Valid(valid) => valid,
            syndic_storage::BindingState::Active(active) => active.usable(),
            _ => return None,
        };
        let mut observed = None;
        let connections = self.connections.lock().ok()?;
        for connection in connections.iter() {
            if connection.runtime_id() != valid.execution().runtime_id() {
                continue;
            }
            if let Some(value) = connection.observed_context(
                thread,
                valid.cas_thread_id(),
                binding.binding().revision(),
            ) {
                if observed.is_some() {
                    return None;
                }
                observed = Some(SelectedContext {
                    connection: Arc::downgrade(connection),
                    snapshot: value,
                });
            }
        }
        if self
            .storage
            .current_binding(home, thread, limit)
            .ok()?
            .as_ref()
            != Some(&binding)
            || !self.command_authorizer.is_open()
        {
            return None;
        }
        observed
    }
}
