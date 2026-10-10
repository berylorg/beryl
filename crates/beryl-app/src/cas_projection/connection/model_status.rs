use super::*;

impl ProjectionConnection {
    pub(in crate::cas_projection) fn manual_projection(
        &self,
        owner: SyndicThreadId,
        cas_thread: &beryl_model::CasThreadId,
        binding: beryl_model::BindingRevision,
    ) -> Option<registry::ManualProjectionStamp> {
        if self.is_retired() || self.is_detached() {
            return None;
        }
        registry::read_manual_projection(self.authority.generation, owner, cas_thread, binding)
    }

    pub(in crate::cas_projection) fn manual_projection_current(
        &self,
        stamp: &registry::ManualProjectionStamp,
    ) -> bool {
        !self.is_retired() && self.try_forwarding_attached() && stamp.is_current()
    }

    pub(in crate::cas_projection) fn observed_context(
        &self,
        owner: SyndicThreadId,
        cas_thread: &beryl_model::CasThreadId,
        revision: beryl_model::BindingRevision,
    ) -> Option<registry::ContextSnapshot> {
        if self.is_retired() || self.is_detached() {
            return None;
        }
        registry::read_context(self.authority.generation, owner, cas_thread, revision)
            .ok()
            .flatten()
    }
    pub(in crate::cas_projection) fn context_is_current(
        &self,
        stamp: &registry::ContextStamp,
    ) -> bool {
        !self.is_retired() && self.try_forwarding_attached() && registry::context_is_current(stamp)
    }
    pub(in crate::cas_projection) fn observed_model_metadata(
        &self,
        owner: SyndicThreadId,
    ) -> Option<beryl_backend::ThreadSessionMetadata> {
        if self.is_retired() || self.is_detached() {
            return None;
        }
        registry::observed_owner_metadata(self.authority.generation, owner)
            .ok()
            .flatten()
    }
}
