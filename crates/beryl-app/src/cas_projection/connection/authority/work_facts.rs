use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) struct ConnectionAuthorityWorkFact {
    pub(in crate::cas_projection) generation: ConnectionGeneration,
    pub(in crate::cas_projection) session_owner_live: bool,
    pub(in crate::cas_projection) promotion: Option<u64>,
    pub(in crate::cas_projection) cleanup_owners: usize,
    pub(in crate::cas_projection) next_promotion_id: u64,
    pub(in crate::cas_projection) next_cleanup_id: u64,
    pub(in crate::cas_projection) retired: bool,
    pub(in crate::cas_projection) retirement_complete: bool,
}

impl ConnectionAuthorityWorkFact {
    pub(in crate::cas_projection) fn requires_cleanup(self, detached: bool) -> bool {
        self.promotion.is_some()
            || self.cleanup_owners != 0
            || (self.retired && (!self.retirement_complete || !detached))
    }
}

impl ConnectionRegistryAuthority {
    pub(in crate::cas_projection) fn work_fact(
        &self,
    ) -> Result<ConnectionAuthorityWorkFact, ProjectionCoordinatorError> {
        let state = self.lock()?;
        Ok(ConnectionAuthorityWorkFact {
            generation: self.generation,
            session_owner_live: state.session_owner_live,
            promotion: state.scheduled_promotion.map(|promotion| promotion.id.0),
            cleanup_owners: state.cleanup_owners.len(),
            next_promotion_id: state.next_promotion_id,
            next_cleanup_id: state.next_cleanup_id,
            retired: self.is_retired(),
            retirement_complete: state.retirement_complete,
        })
    }
}
