use beryl_home_store::HomeStore;
use beryl_model::{SyndicThreadId, SyndicTurnId};

use crate::read::SyndicPointReadLimit;
use crate::{SyndicReadError, SyndicStorage, TurnDispatchAnchor};

impl SyndicStorage {
    pub(in crate::read) fn authenticated_cancelled_dispatch(
        &self,
        store: &HomeStore,
        thread: SyndicThreadId,
        turn: SyndicTurnId,
        anchor: TurnDispatchAnchor,
        limit: SyndicPointReadLimit,
    ) -> Result<bool, SyndicReadError> {
        let Some(binding) = self.binding(store, thread, anchor.binding_revision(), limit)? else {
            return Ok(false);
        };
        let Some(snapshot) = self.execution_snapshot(store, anchor.snapshot_id(), limit)? else {
            return Ok(false);
        };
        let Some(revision) = anchor.binding_revision().checked_next().ok() else {
            return Ok(false);
        };
        let Some(successor) = self.binding(store, thread, revision, limit)? else {
            return Ok(false);
        };
        Ok(crate::dispatch_provenance::activation_matches(
            thread, turn, anchor, &snapshot, &binding,
        ) && crate::dispatch_provenance::cancelled_successor_matches(&binding, &successor)
            && self
                .active_cas_turn(store, anchor.snapshot_id(), limit)?
                .is_none())
    }
}
