use super::*;

#[derive(Clone)]
pub(crate) struct DraftMarkerAdmissionLiveAuthorityV1 {
    pub(crate) authority: DraftMarkerLabelReadinessRequestAuthorityV1,
    pub(crate) allocation_range: Option<DraftMarkerLabelAllocationRangeV1>,
}

pub(crate) struct DraftMarkerAdmissionAttemptReservation {
    pub(crate) was_present: bool,
}

pub(crate) struct DraftMarkerAdmissionPreparedAttempt {
    pub(super) state: Option<Arc<Mutex<AttachmentState>>>,
    pub(super) owner: DraftMarkerAdmissionOwnerV1,
    pub(super) attempt: DraftMarkerAdmissionCommandIdV1,
    pub(super) dispatched: bool,
    pub(super) was_present: bool,
    pub(super) allocation_range: Option<DraftMarkerLabelAllocationRangeV1>,
}

impl DraftMarkerAdmissionPreparedAttempt {
    pub(crate) const fn was_present(&self) -> bool {
        self.was_present
    }

    pub(crate) const fn allocation_range(&self) -> Option<DraftMarkerLabelAllocationRangeV1> {
        self.allocation_range
    }

    pub(crate) fn disarm(mut self) -> Result<DraftMarkerAdmissionAttemptReservation, ()> {
        let state = self.state.as_ref().ok_or(())?;
        let mut state = state.lock().map_err(|_| ())?;
        let operation = state
            .operations
            .iter_mut()
            .find(|operation| operation.owner == self.owner)
            .ok_or(())?;
        if self.dispatched {
            if operation.attempt != OperationAttempt::Dispatched(self.attempt) {
                return Err(());
            }
        } else {
            if operation.attempt != OperationAttempt::Prepared(self.attempt) {
                return Err(());
            }
            operation.attempt = OperationAttempt::Dispatched(self.attempt);
        }
        drop(state);
        self.state = None;
        Ok(DraftMarkerAdmissionAttemptReservation {
            was_present: self.was_present,
        })
    }

    #[cfg(feature = "test-faults")]
    pub(crate) fn dispatch_for_test(&mut self) -> Result<(), ()> {
        if self.dispatched {
            return Ok(());
        }
        let state = self.state.as_ref().ok_or(())?;
        let mut state = state.lock().map_err(|_| ())?;
        let operation = state
            .operations
            .iter_mut()
            .find(|operation| operation.owner == self.owner)
            .ok_or(())?;
        if operation.attempt != OperationAttempt::Prepared(self.attempt) {
            return Err(());
        }
        operation.attempt = OperationAttempt::Dispatched(self.attempt);
        self.dispatched = true;
        Ok(())
    }
}

impl Drop for DraftMarkerAdmissionPreparedAttempt {
    fn drop(&mut self) {
        let Some(state) = self.state.take() else {
            return;
        };
        let Ok(mut state) = state.lock() else {
            return;
        };
        let Some(index) = state
            .operations
            .iter()
            .position(|entry| entry.owner == self.owner)
        else {
            return;
        };
        let expected = if self.dispatched {
            OperationAttempt::Dispatched(self.attempt)
        } else {
            OperationAttempt::Prepared(self.attempt)
        };
        if state.operations[index].attempt != expected {
            return;
        }
        if self.was_present {
            state.operations[index].attempt = OperationAttempt::Idle;
        } else {
            state.operations.remove(index);
        }
    }
}

impl DraftMarkerAdmissionAttachment {
    pub(crate) fn prepare_attempt(
        &self,
        owner: DraftMarkerAdmissionOwnerV1,
        attempt: DraftMarkerAdmissionCommandIdV1,
        frontier: u64,
        authority: &DraftMarkerLabelReadinessRequestAuthorityV1,
        allocation_count: Option<u64>,
    ) -> Result<DraftMarkerAdmissionPreparedAttempt, ()> {
        let mut state = self.state.lock().map_err(|_| ())?;
        if state.retired {
            return Err(());
        }
        if let Some(index) = state
            .operations
            .iter()
            .position(|entry| entry.owner == owner)
        {
            let operation = &state.operations[index];
            if operation.disposition != OperationDisposition::Open
                || operation.attempt != OperationAttempt::Idle
                || operation.destination != authority.session.thread_id()
                || operation.authority != *authority
            {
                return Err(());
            }
            if reserve_allocation_if_needed(&mut state, owner, authority, allocation_count).is_err()
            {
                state.operations[index].disposition = OperationDisposition::UncertainClosed;
                return Err(());
            }
            state.operations[index].attempt = OperationAttempt::Prepared(attempt);
            return Ok(DraftMarkerAdmissionPreparedAttempt {
                state: Some(Arc::clone(&self.state)),
                owner,
                attempt,
                dispatched: false,
                was_present: true,
                allocation_range: state.operations[index].allocation_range,
            });
        }
        if state.operations.len() >= DRAFT_MARKER_ADMISSION_MAX_HEADS as usize {
            return Err(());
        }
        let allocation_range = allocation_range(&state, authority, allocation_count)?;
        state.operations.push(OperationReservation {
            owner,
            frontier,
            attempt: OperationAttempt::Prepared(attempt),
            durable_or_indeterminate: false,
            disposition: OperationDisposition::Open,
            destination: authority.session.thread_id(),
            authority: authority.clone(),
            allocation_range,
        });
        Ok(DraftMarkerAdmissionPreparedAttempt {
            state: Some(Arc::clone(&self.state)),
            owner,
            attempt,
            dispatched: false,
            was_present: false,
            allocation_range,
        })
    }

    pub(crate) fn finish_submission(
        &self,
        owner: DraftMarkerAdmissionOwnerV1,
        attempt: DraftMarkerAdmissionCommandIdV1,
        retain_operation: bool,
        uncertain_closed: bool,
        frontier: u64,
    ) -> Result<(), ()> {
        let mut state = self.state.lock().map_err(|_| ())?;
        if state.retired {
            return Err(());
        }
        let index = state
            .operations
            .iter()
            .position(|entry| entry.owner == owner)
            .ok_or(())?;
        if state.operations[index].attempt != OperationAttempt::Dispatched(attempt) {
            return Err(());
        }
        if retain_operation {
            let operation = &mut state.operations[index];
            operation.attempt = OperationAttempt::Idle;
            operation.durable_or_indeterminate = true;
            operation.frontier = operation.frontier.max(frontier);
            if uncertain_closed {
                operation.disposition = OperationDisposition::UncertainClosed;
            }
        } else {
            state.operations.remove(index);
        }
        Ok(())
    }

    pub(crate) fn resolve_submission(
        &self,
        owner: DraftMarkerAdmissionOwnerV1,
        retain_operation: bool,
        uncertain_closed: bool,
        frontier: u64,
    ) -> Result<(), ()> {
        let mut state = self.state.lock().map_err(|_| ())?;
        if state.retired {
            return Err(());
        }
        let Some(index) = state
            .operations
            .iter()
            .position(|entry| entry.owner == owner)
        else {
            return Ok(());
        };
        if state.operations[index].attempt != OperationAttempt::Idle {
            return Err(());
        }
        if retain_operation {
            let operation = &mut state.operations[index];
            operation.durable_or_indeterminate = true;
            operation.frontier = operation.frontier.max(frontier);
            operation.disposition = if uncertain_closed {
                OperationDisposition::UncertainClosed
            } else {
                OperationDisposition::Open
            };
        } else {
            state.operations.remove(index);
        }
        Ok(())
    }

    pub(crate) fn prepare_assignment_attempt(
        &self,
        owner: DraftMarkerAdmissionOwnerV1,
        attempt: DraftMarkerAdmissionCommandIdV1,
    ) -> Result<
        (
            DraftMarkerAdmissionPreparedAttempt,
            DraftMarkerAdmissionLiveAuthorityV1,
        ),
        (),
    > {
        let mut state = self.state.lock().map_err(|_| ())?;
        if state.retired {
            return Err(());
        }
        let operation = state
            .operations
            .iter_mut()
            .find(|entry| entry.owner == owner)
            .ok_or(())?;
        if operation.disposition != OperationDisposition::Open
            || operation.attempt != OperationAttempt::Idle
        {
            return Err(());
        }
        operation.attempt = OperationAttempt::Prepared(attempt);
        Ok((
            DraftMarkerAdmissionPreparedAttempt {
                state: Some(Arc::clone(&self.state)),
                owner,
                attempt,
                dispatched: false,
                was_present: true,
                allocation_range: operation.allocation_range,
            },
            DraftMarkerAdmissionLiveAuthorityV1 {
                authority: operation.authority.clone(),
                allocation_range: operation.allocation_range,
            },
        ))
    }

    pub(crate) fn live_authority(
        &self,
        owner: DraftMarkerAdmissionOwnerV1,
    ) -> Result<DraftMarkerAdmissionLiveAuthorityV1, ()> {
        let state = self.state.lock().map_err(|_| ())?;
        if state.retired {
            return Err(());
        }
        let operation = state
            .operations
            .iter()
            .find(|entry| entry.owner == owner && entry.disposition == OperationDisposition::Open)
            .ok_or(())?;
        Ok(DraftMarkerAdmissionLiveAuthorityV1 {
            authority: operation.authority.clone(),
            allocation_range: operation.allocation_range,
        })
    }

    #[cfg(feature = "test-faults")]
    pub(crate) fn invalidate_prepared_attempt_for_test(
        &self,
        owner: DraftMarkerAdmissionOwnerV1,
        attempt: DraftMarkerAdmissionCommandIdV1,
    ) -> Result<bool, ()> {
        let mut state = self.state.lock().map_err(|_| ())?;
        if state.retired {
            return Err(());
        }
        let Some(index) = state
            .operations
            .iter()
            .position(|operation| operation.owner == owner)
        else {
            return Ok(false);
        };
        if state.operations[index].attempt != OperationAttempt::Prepared(attempt) {
            return Ok(false);
        }
        state.operations.remove(index);
        Ok(true)
    }
}
