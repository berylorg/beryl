use crate::{RepairResolution, ResolvedRepair, SyndicRecordError, TurnStateRecord};

impl TurnStateRecord {
    #[must_use]
    pub const fn resolved_repair(&self) -> Option<&ResolvedRepair> {
        self.resolved_repair.as_ref()
    }

    pub fn with_resolved_repair(
        mut self,
        resolved: ResolvedRepair,
    ) -> Result<Self, SyndicRecordError> {
        let target = resolved.target();
        let RepairResolution::Incomplete(reason) = resolved.resolution();
        if self
            .resolved_repair
            .as_ref()
            .is_some_and(|old| old != &resolved)
            || target.turn_id() != self.turn_id
            || target.gap().terminal().sequence().get() != self.source_event_count
            || self.lifecycle != target.gap().status().lifecycle()
            || self.terminal_outcome() != Some(target.gap().status().outcome())
            || self.incomplete_reason() != Some(reason)
            || self.dispatch_provenance == crate::TurnDispatchProvenance::ProviderOperation
        {
            return Err(SyndicRecordError::InvalidResolvedRepair);
        }
        self.resolved_repair = Some(resolved);
        Ok(self)
    }

    pub(crate) fn preserve_resolved_repair(
        self,
        previous: &Self,
    ) -> Result<Self, SyndicRecordError> {
        match previous.resolved_repair() {
            Some(resolved) => self.with_resolved_repair(resolved.clone()),
            None => Ok(self),
        }
    }
}
