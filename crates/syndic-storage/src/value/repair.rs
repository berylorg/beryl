use beryl_model::InputGateRevision;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RepairRequestAttemptNonce([u8; 16]);

impl RepairRequestAttemptNonce {
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConsumedRepairRequest {
    attempt_nonce: RepairRequestAttemptNonce,
    source_gate_revision: InputGateRevision,
    successor_gate_revision: InputGateRevision,
}

impl ConsumedRepairRequest {
    pub fn new(
        attempt_nonce: RepairRequestAttemptNonce,
        source_gate_revision: InputGateRevision,
        successor_gate_revision: InputGateRevision,
    ) -> Result<Self, RepairRequestDispositionError> {
        if source_gate_revision.checked_next().ok() != Some(successor_gate_revision) {
            return Err(RepairRequestDispositionError::InvalidRevisionTransition);
        }
        Ok(Self {
            attempt_nonce,
            source_gate_revision,
            successor_gate_revision,
        })
    }

    #[must_use]
    pub const fn attempt_nonce(self) -> RepairRequestAttemptNonce {
        self.attempt_nonce
    }

    #[must_use]
    pub const fn source_gate_revision(self) -> InputGateRevision {
        self.source_gate_revision
    }

    #[must_use]
    pub const fn successor_gate_revision(self) -> InputGateRevision {
        self.successor_gate_revision
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepairRequestDisposition {
    Available,
    Consumed(ConsumedRepairRequest),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum RepairRequestDispositionError {
    #[error("repair request claim must advance the gate revision exactly once")]
    InvalidRevisionTransition,
}
