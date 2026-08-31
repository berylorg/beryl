use super::*;

impl DraftMarkerAdmissionAttachment {
    pub(crate) fn cancel_transient(
        &self,
        owner: DraftMarkerAdmissionOwnerV1,
    ) -> Result<CancelTransient, ()> {
        let mut state = self.state.lock().map_err(|_| ())?;
        if state.retired {
            return Err(());
        }
        let Some(index) = state
            .operations
            .iter()
            .position(|operation| operation.owner == owner)
        else {
            return Ok(CancelTransient::Absent);
        };
        let operation = &state.operations[index];
        if operation.durable_or_indeterminate
            || operation.disposition != OperationDisposition::Open
            || matches!(operation.attempt, OperationAttempt::Dispatched(_))
        {
            return Ok(CancelTransient::Protected);
        }
        state.operations.remove(index);
        Ok(CancelTransient::Released)
    }

    pub(crate) fn prepare_terminal_attempt(
        &self,
        owner: DraftMarkerAdmissionOwnerV1,
        command: DraftMarkerAdmissionCommandIdV1,
    ) -> Result<DraftMarkerAdmissionPreparedAttempt, ()> {
        let mut state = self.state.lock().map_err(|_| ())?;
        if state.retired {
            return Err(());
        }
        let operation = state
            .operations
            .iter_mut()
            .find(|operation| operation.owner == owner)
            .ok_or(())?;
        if operation.disposition != OperationDisposition::Open
            || operation.attempt != OperationAttempt::Idle
        {
            return Err(());
        }
        operation.attempt = OperationAttempt::Prepared(command);
        Ok(DraftMarkerAdmissionPreparedAttempt {
            state: Some(Arc::clone(&self.state)),
            owner,
            attempt: command,
            dispatched: false,
            was_present: true,
            allocation_range: operation.allocation_range,
        })
    }

    pub(crate) fn next_inert_cleanup(&self) -> Result<Option<DraftMarkerAdmissionOwnerV1>, ()> {
        let state = self.state.lock().map_err(|_| ())?;
        if state.retired {
            return Err(());
        }
        Ok(state.heads.iter().find_map(|head| {
            matches!(head.class, ReconstructedHeadClass::InertCleanup).then_some(head.owner)
        }))
    }

    pub(crate) fn is_reconstructed_inert_cleanup(
        &self,
        owner: DraftMarkerAdmissionOwnerV1,
    ) -> Result<bool, ()> {
        let state = self.state.lock().map_err(|_| ())?;
        if state.retired {
            return Err(());
        }
        Ok(state.heads.iter().any(|head| {
            head.owner == owner && matches!(head.class, ReconstructedHeadClass::InertCleanup)
        }))
    }

    pub(crate) fn finish_terminal(
        &self,
        owner: DraftMarkerAdmissionOwnerV1,
        command: DraftMarkerAdmissionCommandIdV1,
        exact_new: bool,
        uncertain: bool,
    ) -> Result<(), ()> {
        let mut state = self.state.lock().map_err(|_| ())?;
        if state.retired {
            return Err(());
        }
        let Some(index) = state
            .operations
            .iter()
            .position(|operation| operation.owner == owner)
        else {
            return if exact_new { Ok(()) } else { Err(()) };
        };
        if state.operations[index].attempt != OperationAttempt::Dispatched(command) {
            return Err(());
        }
        if exact_new {
            state.operations.remove(index);
        } else {
            state.operations[index].attempt = OperationAttempt::Idle;
            state.operations[index].disposition = if uncertain {
                OperationDisposition::UncertainClosed
            } else {
                OperationDisposition::Open
            };
        }
        Ok(())
    }

    pub(crate) fn resolve_terminal(
        &self,
        owner: DraftMarkerAdmissionOwnerV1,
        exact_new: bool,
        uncertain: bool,
    ) -> Result<(), ()> {
        let mut state = self.state.lock().map_err(|_| ())?;
        if state.retired {
            return Err(());
        }
        let Some(index) = state
            .operations
            .iter()
            .position(|operation| operation.owner == owner)
        else {
            return if exact_new { Ok(()) } else { Err(()) };
        };
        if state.operations[index].attempt != OperationAttempt::Idle {
            return Err(());
        }
        if exact_new {
            state.operations.remove(index);
        } else {
            state.operations[index].disposition = if uncertain {
                OperationDisposition::UncertainClosed
            } else {
                OperationDisposition::Open
            };
        }
        Ok(())
    }

    pub(crate) fn finish_cleanup(&self, owner: DraftMarkerAdmissionOwnerV1, retained: bool) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        if state.retired {
            return;
        }
        if retained {
            state.heads = Vec::from(std::mem::take(&mut state.heads))
                .into_iter()
                .filter(|head| head.owner != owner)
                .collect::<Vec<_>>()
                .into_boxed_slice();
        }
    }
}

pub(crate) enum CancelTransient {
    Released,
    Absent,
    Protected,
}
