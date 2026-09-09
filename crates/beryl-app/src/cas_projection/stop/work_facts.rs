use std::ops::Bound::{Excluded, Unbounded};

use super::StopCoordinator;
use crate::cas_projection::stop_work::{
    StopWorkError, StopWorkFact, StopWorkKey, StopWorkPageBuilder, StopWorkRecord,
};

impl StopCoordinator {
    pub(in crate::cas_projection) fn work_revision(&self) -> Result<u64, StopWorkError> {
        self.state
            .lock()
            .map_err(|_| StopWorkError::Poisoned)?
            .revision()
            .ok_or(StopWorkError::RevisionUnavailable)
    }

    pub(in crate::cas_projection) fn collect_work_records(
        &self,
        revision: u64,
        page: &mut StopWorkPageBuilder<'_>,
    ) -> Result<(), StopWorkError> {
        let state = self.state.lock().map_err(|_| StopWorkError::Poisoned)?;
        if state.revision().ok_or(StopWorkError::RevisionUnavailable)? != revision {
            return Err(StopWorkError::StaleRevision);
        }
        if !matches!(page.after, Some(StopWorkKey::Permission(_))) {
            let after = match page.after {
                Some(StopWorkKey::Stop(id)) => Some(*id),
                _ => None,
            };
            let first_thread = after.map(|id| id.thread_id());
            let mut retained = state
                .stops
                .range((
                    first_thread.map_or(Unbounded, std::ops::Bound::Included),
                    Unbounded,
                ))
                .map(|(_, local)| local.operation_id)
                .filter(|id| after.is_none_or(|after| *id > after))
                .peekable();
            let mut owned = state
                .live_custody
                .range((after.map_or(Unbounded, Excluded), Unbounded))
                .map(|(id, _)| *id)
                .peekable();
            loop {
                let operation_id = match (retained.peek(), owned.peek()) {
                    (Some(left), Some(right)) => *left.min(right),
                    (Some(id), None) | (None, Some(id)) => *id,
                    (None, None) => break,
                };
                if retained.peek() == Some(&operation_id) {
                    retained.next();
                }
                if owned.peek() == Some(&operation_id) {
                    owned.next();
                }
                let local = state
                    .stops
                    .get(&operation_id.thread_id())
                    .filter(|local| local.operation_id == operation_id);
                let custody = state.live_custody.get(&operation_id);
                let target = local
                    .map(|local| &local.target)
                    .or_else(|| custody.map(|custody| &custody.target))
                    .expect("selected stop source retains its exact target");
                let fact = StopWorkFact {
                    operation_id,
                    target: target.clone(),
                    local_dispatch: local.map(|local| local.dispatch),
                    primary_custody: custody.is_some_and(|custody| custody.primary),
                    driver_custody: custody.is_some_and(|custody| custody.driver),
                };
                if !page.push(StopWorkKey::Stop(operation_id), StopWorkRecord::Stop(fact))? {
                    return Ok(());
                }
            }
        }
        let after = match page.after {
            Some(StopWorkKey::Permission(serial)) => Excluded(*serial),
            _ => Unbounded,
        };
        for (serial, fact) in state.permissions.range((after, Unbounded)) {
            if !page.push(
                StopWorkKey::Permission(*serial),
                StopWorkRecord::Permission(fact.clone()),
            )? {
                break;
            }
        }
        Ok(())
    }
}
