//! Shell admission for one observer generation. No backend or queue mutations.
use super::compaction_observer::{
    ObserverTarget, ObserverUpdate, OperationIdentity, Outcome, UpdateKind,
};

pub(super) struct CompactionControl {
    pub(super) target: ObserverTarget,
    operation: Option<OperationIdentity>,
    turn_id: Option<String>,
    finished: bool,
}

impl CompactionControl {
    pub(super) fn new(target: ObserverTarget) -> Self {
        Self {
            target,
            operation: None,
            turn_id: None,
            finished: false,
        }
    }

    pub(super) fn accept(&mut self, update: ObserverUpdate) -> Option<UpdateKind> {
        if self.finished || !update.belongs_to(&self.target) {
            return None;
        }
        match &update.kind {
            UpdateKind::Prepared if self.operation.is_none() => {
                let operation = update.operation.as_ref()?;
                if operation.thread_id != self.target.thread_id {
                    return None;
                }
                self.operation = Some(operation.clone());
            }
            UpdateKind::Finished(Outcome::Rejected { .. }) if self.operation.is_none() => {
                if update
                    .operation
                    .as_ref()
                    .is_some_and(|operation| operation.thread_id != self.target.thread_id)
                {
                    return None;
                }
                // Preparation can retain identity before subscription fails;
                // that local rejection precedes the Prepared publication.
                self.operation = update.operation.clone();
            }
            _ if self.operation.is_none() => return None,
            _ if update.operation != self.operation => return None,
            _ => {}
        }
        if let UpdateKind::TurnKnown { turn_id } = &update.kind {
            if self.operation.is_none()
                || self.turn_id.as_ref().is_some_and(|known| known != turn_id)
            {
                return None;
            }
            self.turn_id = Some(turn_id.clone());
        }
        if matches!(update.kind, UpdateKind::Finished(_)) {
            self.finished = true;
        }
        Some(update.kind)
    }
}
