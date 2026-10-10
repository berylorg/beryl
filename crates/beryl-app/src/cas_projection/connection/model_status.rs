use super::*;

impl ProjectionConnection {
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
