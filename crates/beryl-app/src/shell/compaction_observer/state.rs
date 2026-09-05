use super::*;

pub(super) struct Observation {
    pub(super) target: ObserverTarget,
    pub(super) operation: Option<OperationIdentity>,
    pub(super) turn_id: Option<String>,
    pub(super) item_completed: bool,
    pub(super) turn_completed: bool,
    pub(super) success_proven: bool,
    pub(super) last_uncertainty: Option<UnconfirmedReason>,
    pub(super) diagnostics: diagnostics::Capture,
}

impl Observation {
    pub(super) fn emit(
        &self,
        kind: UpdateKind,
        sink: &mut impl UpdateSink,
        cancel: &Cancellation,
    ) -> bool {
        if !cancel.is_cancelled() {
            self.diagnostics.update(&kind, self.turn_id.as_deref());
        }
        !cancel.is_cancelled()
            && sink.publish(
                ObserverUpdate {
                    target: self.target.clone(),
                    operation: self.operation.clone(),
                    kind,
                },
                cancel,
            )
    }

    pub(super) fn uncertain(
        &mut self,
        reason: UnconfirmedReason,
        sink: &mut impl UpdateSink,
        cancel: &Cancellation,
    ) -> bool {
        if self.last_uncertainty == Some(reason) {
            return !cancel.is_cancelled();
        }
        self.last_uncertainty = Some(reason);
        self.emit(UpdateKind::Unconfirmed(reason), sink, cancel)
    }

    pub(super) fn receipt(
        &mut self,
        receipt: Receipt,
        source: Stage,
        sink: &mut impl UpdateSink,
        cancel: &Cancellation,
    ) -> Result<Option<Outcome>, ()> {
        let absent = matches!(
            receipt.state,
            CompactionReceiptState::Unknown {
                reason: CompactionUnknownReason::AbsentOrExpired
                    | CompactionUnknownReason::RuntimeChange
            }
        );
        if self.operation.as_ref() != Some(&receipt.operation)
            || absent != receipt.turn_id.is_none()
            || self
                .turn_id
                .as_ref()
                .zip(receipt.turn_id.as_ref())
                .is_some_and(|(known, got)| known != got)
        {
            self.diagnostics
                .record(source, Category::Invalid, self.turn_id.as_deref());
            if source == Stage::Start {
                self.diagnostics.acceptance(Acceptance::Indeterminate);
            }
            return self
                .uncertain(UnconfirmedReason::InvalidEvidence, sink, cancel)
                .then_some(None)
                .ok_or(());
        }
        if !matches!(receipt.state, CompactionReceiptState::Unknown { .. }) {
            self.diagnostics.acceptance(Acceptance::Accepted);
        } else if source == Stage::Start {
            self.diagnostics.acceptance(Acceptance::Indeterminate);
        }
        self.diagnostics.record(
            source,
            match &receipt.state {
                CompactionReceiptState::Accepted => Category::Accepted,
                CompactionReceiptState::Running => Category::Started,
                CompactionReceiptState::Completed => Category::Completed,
                CompactionReceiptState::Failed { .. } => Category::Failed,
                CompactionReceiptState::Interrupted { .. } => Category::Interrupted,
                CompactionReceiptState::Unknown { .. } => Category::Unknown,
            },
            receipt.turn_id.as_deref(),
        );
        if self.turn_id.is_none()
            && let Some(turn_id) = receipt.turn_id
        {
            self.turn_id = Some(turn_id.clone());
            if !self.emit(UpdateKind::TurnKnown { turn_id }, sink, cancel) {
                return Err(());
            }
        }
        match receipt.state {
            CompactionReceiptState::Completed => self.success_proven = true,
            CompactionReceiptState::Failed { error } => {
                return Ok(Some(Outcome::Failed {
                    message: bounded(error.message),
                }));
            }
            CompactionReceiptState::Interrupted { .. } => return Ok(Some(Outcome::Interrupted)),
            CompactionReceiptState::Unknown { reason } => {
                if !self.uncertain(UnconfirmedReason::Receipt(reason), sink, cancel) {
                    return Err(());
                }
            }
            CompactionReceiptState::Accepted | CompactionReceiptState::Running => {}
        }
        Ok(None)
    }
}
