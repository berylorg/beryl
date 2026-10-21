use super::*;

impl WslGeneratedReader {
    pub(crate) fn current(&self) -> bool {
        let state = lock(&self.registry.state);
        !state.namespace_closed && !state.failed
    }
    fn send(&self, frame: &Frame) -> Result<(), GeneratedOutputError> {
        let bytes = encode_frame(&self.nonce, frame).map_err(|_| GeneratedOutputError::Protocol)?;
        lock(&self.sender)
            .as_ref()
            .ok_or(GeneratedOutputError::CleanupPending)?
            .try_send(bytes)
            .map_err(|error| match error {
                TrySendError::Full(_) => GeneratedOutputError::Busy,
                TrySendError::Disconnected(_) => GeneratedOutputError::CleanupPending,
            })
    }
    pub(super) fn wait(
        &self,
        cancelled: Option<&AtomicBool>,
    ) -> Result<Frame, GeneratedOutputError> {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut state = lock(&self.slot.state);
        loop {
            if let Some(error) = state.error {
                return Err(error);
            }
            if let Some(reply) = state.reply.take() {
                return Ok(reply);
            }
            if state.failed {
                return Err(GeneratedOutputError::Protocol);
            }
            if cancelled.is_some_and(|cancelled| cancelled.load(Ordering::Acquire)) {
                return Err(GeneratedOutputError::Cancelled);
            }
            if Instant::now() >= deadline {
                return Err(GeneratedOutputError::CleanupPending);
            }
            state = self
                .slot
                .event
                .wait_timeout(state, Duration::from_millis(20))
                .unwrap_or_else(|error| error.into_inner())
                .0;
        }
    }
    pub(crate) fn open(
        &self,
        path: &str,
        cancelled: &AtomicBool,
    ) -> Result<u64, GeneratedOutputError> {
        if let Err(error) = self.send(&Frame::ReaderOpen {
            id: self.slot.id,
            path: path.to_owned(),
        }) {
            lock(&self.slot.state).closed = true;
            self.release();
            return Err(error);
        }
        match self.wait(Some(cancelled))? {
            Frame::ReaderOpened { length, .. } => Ok(length),
            _ => Err(GeneratedOutputError::Protocol),
        }
    }
    pub(crate) fn read(
        &self,
        offset: u64,
        destination: &mut [u8],
        cancelled: &AtomicBool,
    ) -> Result<usize, GeneratedOutputError> {
        let _operation = self
            .operation
            .try_lock()
            .map_err(|_| GeneratedOutputError::Busy)?;
        let allowance = u32::try_from(destination.len().min(beryl_wsl_supervisor::MAX_READER_PAGE))
            .map_err(|_| GeneratedOutputError::Protocol)?;
        let (sequence, previous) = {
            let mut state = lock(&self.slot.state);
            if state.closed
                || state.failed
                || state.reply.is_some()
                || state.request_in_flight
                || state.opening
                || state.error.is_some()
            {
                return Err(GeneratedOutputError::CleanupPending);
            }
            let sequence = state.next_sequence;
            state.next_sequence = sequence
                .checked_add(1)
                .ok_or(GeneratedOutputError::Protocol)?;
            let previous = state.pending;
            state.pending = Pending::Read {
                sequence,
                offset,
                allowance,
            };
            state.request_in_flight = true;
            (sequence, previous)
        };
        if let Err(error) = self.send(&Frame::ReaderRead {
            id: self.slot.id,
            sequence,
            offset,
            allowance,
        }) {
            let mut state = lock(&self.slot.state);
            state.next_sequence = sequence;
            state.pending = previous;
            state.request_in_flight = false;
            return Err(error);
        }
        match self.wait(Some(cancelled))? {
            Frame::ReaderPage { bytes, .. } => {
                destination[..bytes.len()].copy_from_slice(&bytes);
                Ok(bytes.len())
            }
            _ => Err(GeneratedOutputError::Protocol),
        }
    }
    pub(crate) fn close(&self, cancelled: bool) -> Result<(), GeneratedOutputError> {
        let _operation = self
            .operation
            .try_lock()
            .map_err(|_| GeneratedOutputError::Busy)?;
        if self.closed() {
            self.release();
            return Ok(());
        }
        let sequence = {
            let mut state = lock(&self.slot.state);
            match state.pending {
                Pending::Close { sequence } => {
                    if state.close_sent {
                        None
                    } else {
                        state.close_sent = true;
                        Some(sequence)
                    }
                }
                pending => {
                    if let Pending::Read {
                        sequence,
                        offset,
                        allowance,
                    } = pending
                    {
                        state.previous_read = Some((sequence, offset, allowance));
                    }
                    let sequence = state.next_sequence;
                    state.next_sequence = sequence
                        .checked_add(1)
                        .ok_or(GeneratedOutputError::Protocol)?;
                    state.pending = Pending::Close { sequence };
                    state.reply = None;
                    state.close_sent = true;
                    Some(sequence)
                }
            }
        };
        if let Some(sequence) = sequence {
            let request = if cancelled {
                Frame::ReaderCancel {
                    id: self.slot.id,
                    sequence,
                }
            } else {
                Frame::ReaderClose {
                    id: self.slot.id,
                    sequence,
                }
            };
            if let Err(error) = self.send(&request) {
                let mut state = lock(&self.slot.state);
                state.close_sent = false;
                return Err(error);
            }
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while !self.closed() {
            if Instant::now() >= deadline {
                return Err(GeneratedOutputError::CleanupPending);
            }
            let state = lock(&self.slot.state);
            let _guard = self
                .slot
                .event
                .wait_timeout(state, Duration::from_millis(20))
                .unwrap_or_else(|error| error.into_inner());
        }
        self.release();
        Ok(())
    }
    pub(super) fn closed(&self) -> bool {
        if lock(&self.slot.state).closed {
            return true;
        }
        let state = lock(&self.registry.state);
        state.namespace_closed && state.joined
    }
    fn release(&self) {
        lock(&self.registry.state).readers.remove(&self.slot.id);
    }
}
