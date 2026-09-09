mod pages;
pub use pages::*;

use super::*;
use crate::{NonIdleGateSourceRecord, record::non_idle_gate_source_matches};

impl SyndicStorage {
    pub(super) fn with_current_gate_source<T>(
        &self,
        store: &HomeStore,
        thread_id: SyndicThreadId,
        limit: SyndicPointReadLimit,
        read: impl FnOnce() -> Result<T, SyndicReadError>,
    ) -> Result<T, SyndicReadError> {
        let observed = (
            self.point::<InputGatesFamily>(store, thread_id, limit)?,
            self.point::<NonIdleGateSourcesFamily>(store, thread_id, limit)?,
        );
        let result = read();
        let confirmed = (
            self.point::<InputGatesFamily>(store, thread_id, limit)?,
            self.point::<NonIdleGateSourcesFamily>(store, thread_id, limit)?,
        );
        if observed != confirmed {
            return Err(SyndicReadError::ConcurrentChange {
                operation: "current gate/source reconciliation",
            });
        }
        if !non_idle_gate_source_matches(thread_id, confirmed.0.as_ref(), confirmed.1.as_ref()) {
            return Err(SyndicReadError::Invariant(
                "current input gate and non-idle source disagree",
            ));
        }
        result
    }

    pub fn non_idle_gate_source(
        &self,
        store: &HomeStore,
        thread_id: SyndicThreadId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<NonIdleGateSourceRecord>, SyndicReadError> {
        let revision = self.revision(store)?;
        let result = (|| {
            let source = self.point::<NonIdleGateSourcesFamily>(store, thread_id, limit)?;
            let gate = self.point::<InputGatesFamily>(store, thread_id, limit)?;
            if !non_idle_gate_source_matches(thread_id, gate.as_ref(), source.as_ref()) {
                return Err(SyndicReadError::Invariant(
                    "current input gate and non-idle source disagree",
                ));
            }
            Ok(source)
        })();
        if self.revision(store)? != revision {
            return Err(SyndicReadError::ConcurrentChange {
                operation: "non-idle gate source",
            });
        }
        result
    }
}
