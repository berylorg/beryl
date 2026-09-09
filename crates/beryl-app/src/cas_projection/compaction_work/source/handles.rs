use super::*;
use crate::cas_projection::ContextCompactionOutcome;
use syndic_storage::{
    CompactionAttemptNonce, CompactionOperationId, CompactionOperationTarget,
    CompactionRequestDisposition,
};

#[derive(Clone)]
pub(in crate::cas_projection) struct CompactionWorkHandle {
    pub(super) source: Weak<CompactionWorkSource>,
    pub(super) serial: u64,
}

impl CompactionWorkHandle {
    pub(in crate::cas_projection) fn unavailable() -> Self {
        Self {
            source: Weak::new(),
            serial: 0,
        }
    }

    fn update(&self, update: impl FnOnce(&mut CompactionWorkRecord)) {
        if let Some(source) = self.source.upgrade() {
            source.update(self.serial, update);
        }
    }

    fn update_checked(&self, update: impl FnOnce(&mut CompactionWorkRecord) -> bool) {
        if let Some(source) = self.source.upgrade() {
            source.update_checked(self.serial, update);
        }
    }

    pub(in crate::cas_projection) fn begin_shared_command(&self) -> CompactionCommandObservation {
        self.update_checked(|record| {
            if record.compaction.is_some() {
                return false;
            }
            let Some(continuation) = record.continuation.as_ref() else {
                return false;
            };
            record.compaction = Some(CompactionWorkFact::preparing(Some(
                continuation.yielding_turn_id,
            )));
            true
        });
        CompactionCommandObservation(self.clone())
    }

    pub(in crate::cas_projection) fn command_observation(&self) -> CompactionCommandObservation {
        CompactionCommandObservation(self.clone())
    }

    pub(in crate::cas_projection) fn continuation_observation(&self) -> ContinuationObservation {
        ContinuationObservation(self.clone())
    }

    pub(in crate::cas_projection) fn retire_slot(&self) {
        self.update(|record| {
            record.continuation = None;
            if let Some(compaction) = record.compaction.as_mut() {
                compaction.command = None;
            }
        });
    }

    pub(in crate::cas_projection) fn operation(
        &self,
        operation_id: CompactionOperationId,
        attempt: CompactionAttemptNonce,
        target: &CompactionOperationTarget,
    ) {
        self.update_checked(|record| {
            if record.thread_id != operation_id.thread_id()
                || target.thread_id() != record.thread_id
            {
                return false;
            }
            let Some(compaction) = record.compaction.as_mut() else {
                return false;
            };
            let operation = CompactionOperationWorkFact {
                operation_id,
                attempt,
                target: target.clone(),
            };
            if compaction
                .operation
                .as_ref()
                .is_some_and(|existing| existing != &operation)
            {
                return false;
            }
            compaction.operation = Some(operation);
            true
        });
    }

    pub(in crate::cas_projection) fn register_local(
        &self,
        replaced: Option<&Self>,
    ) -> CompactionLocalObservation {
        if let Some(source) = self.source.upgrade() {
            source.replace_local(self.serial, replaced);
        }
        CompactionLocalObservation(self.clone())
    }

    pub(in crate::cas_projection) fn stage(&self, stage: CompactionCommandWorkStage) {
        self.update(|record| {
            if let Some(compaction) = record.compaction.as_mut()
                && compaction.command.is_some()
                && compaction.result.is_none()
            {
                compaction.command = Some(stage);
            }
        });
    }

    pub(in crate::cas_projection) fn complete(&self, result: ContextCompactionOutcome) {
        self.update(|record| {
            if let Some(compaction) = record.compaction.as_mut() {
                compaction.result.get_or_insert(result);
                if compaction.command.is_some() {
                    compaction.command = Some(CompactionCommandWorkStage::Cleanup);
                }
            }
        });
    }

    pub(in crate::cas_projection) fn request(&self, disposition: CompactionRequestDisposition) {
        self.update(|record| {
            if let Some(compaction) = record.compaction.as_mut() {
                compaction.request_disposition = Some(disposition);
            }
        });
    }
}

pub(in crate::cas_projection) struct CompactionCommandObservation(CompactionWorkHandle);

impl CompactionCommandObservation {
    pub(in crate::cas_projection) fn handle(&self) -> CompactionWorkHandle {
        self.0.clone()
    }
}

impl Drop for CompactionCommandObservation {
    fn drop(&mut self) {
        self.0.update(|record| {
            if let Some(compaction) = record.compaction.as_mut() {
                compaction.command = None;
            }
        });
    }
}

pub(in crate::cas_projection) struct CompactionLocalObservation(CompactionWorkHandle);

impl Drop for CompactionLocalObservation {
    fn drop(&mut self) {
        self.0.update(|record| {
            if let Some(compaction) = record.compaction.as_mut() {
                compaction.local_registered = false;
            }
        });
    }
}

pub(in crate::cas_projection) struct ContinuationObservation(CompactionWorkHandle);

impl ContinuationObservation {
    pub(in crate::cas_projection) fn stage(&self, stage: ContinuationWorkStage) {
        self.0.update(|record| {
            if let Some(continuation) = record.continuation.as_mut() {
                continuation.stage = stage;
            }
        });
    }

    pub(in crate::cas_projection) fn cancel(&self) {
        self.0.update(|record| {
            if let Some(continuation) = record.continuation.as_mut() {
                continuation.pending = false;
            }
        });
    }

    pub(in crate::cas_projection) fn bind(&self, turn: SyndicTurnId) {
        self.0.update_checked(|record| {
            let Some(continuation) = record.continuation.as_mut() else {
                return false;
            };
            if continuation
                .compaction_turn_id
                .is_some_and(|existing| existing != turn)
            {
                return false;
            }
            continuation.compaction_turn_id = Some(turn);
            true
        });
    }
}

impl Drop for ContinuationObservation {
    fn drop(&mut self) {
        self.0.update(|record| {
            record.continuation = None;
        });
    }
}
