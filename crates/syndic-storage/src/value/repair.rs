use beryl_model::{InputGateRevision, SyndicTurnId};

use crate::{CasTurnSource, SourceEventSequence, TurnEndStatus, TurnTerminalOutcome};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RepairSourceEventDigest([u8; 32]);

impl RepairSourceEventDigest {
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RepairSourceEventWitness {
    sequence: SourceEventSequence,
    digest: RepairSourceEventDigest,
}

impl RepairSourceEventWitness {
    #[must_use]
    pub const fn new(sequence: SourceEventSequence, digest: RepairSourceEventDigest) -> Self {
        Self { sequence, digest }
    }

    #[must_use]
    pub const fn sequence(self) -> SourceEventSequence {
        self.sequence
    }

    #[must_use]
    pub const fn digest(self) -> RepairSourceEventDigest {
        self.digest
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepairCaptureGapReason {
    TerminalCaptureIncomplete,
    ForcedAbortOrderingUnproven,
    ProviderObservationIssue,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RepairCaptureGap {
    terminal: RepairSourceEventWitness,
    status: TurnEndStatus,
    reason: RepairCaptureGapReason,
    issue: Option<RepairSourceEventWitness>,
}

impl RepairCaptureGap {
    pub fn new(
        terminal: RepairSourceEventWitness,
        status: TurnEndStatus,
        reason: RepairCaptureGapReason,
        issue: Option<RepairSourceEventWitness>,
    ) -> Result<Self, RepairProvenanceError> {
        if !status.lifecycle().is_proven_terminal() {
            return Err(RepairProvenanceError::UnknownTerminal);
        }
        match reason {
            RepairCaptureGapReason::TerminalCaptureIncomplete
                if status.incomplete_reason().is_none() =>
            {
                return Err(RepairProvenanceError::MissingIncompleteReason);
            }
            RepairCaptureGapReason::ForcedAbortOrderingUnproven
                if status.outcome() != TurnTerminalOutcome::Interrupted =>
            {
                return Err(RepairProvenanceError::RequiresInterruptedTerminal);
            }
            RepairCaptureGapReason::ProviderObservationIssue if issue.is_none() => {
                return Err(RepairProvenanceError::MissingObservationIssue);
            }
            _ => {}
        }
        if issue.is_some_and(|issue| issue.sequence() >= terminal.sequence()) {
            return Err(RepairProvenanceError::InvalidIssueOrder);
        }
        Ok(Self {
            terminal,
            status,
            reason,
            issue,
        })
    }

    #[must_use]
    pub const fn terminal(self) -> RepairSourceEventWitness {
        self.terminal
    }

    #[must_use]
    pub const fn status(self) -> TurnEndStatus {
        self.status
    }

    #[must_use]
    pub const fn reason(self) -> RepairCaptureGapReason {
        self.reason
    }

    #[must_use]
    pub const fn issue(self) -> Option<RepairSourceEventWitness> {
        self.issue
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepairRequiredTarget {
    turn_id: SyndicTurnId,
    source: CasTurnSource,
    gap: RepairCaptureGap,
    request: RepairRequestDisposition,
}

impl RepairRequiredTarget {
    #[must_use]
    pub const fn new(
        turn_id: SyndicTurnId,
        source: CasTurnSource,
        gap: RepairCaptureGap,
        request: RepairRequestDisposition,
    ) -> Self {
        Self {
            turn_id,
            source,
            gap,
            request,
        }
    }

    #[must_use]
    pub const fn turn_id(&self) -> SyndicTurnId {
        self.turn_id
    }

    #[must_use]
    pub const fn source(&self) -> &CasTurnSource {
        &self.source
    }

    #[must_use]
    pub const fn gap(&self) -> RepairCaptureGap {
        self.gap
    }

    #[must_use]
    pub const fn request(&self) -> RepairRequestDisposition {
        self.request
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum RepairProvenanceError {
    #[error("repair requires a proven terminal outcome")]
    UnknownTerminal,
    #[error("terminal capture gap requires an incomplete reason")]
    MissingIncompleteReason,
    #[error("forced-abort ordering gap requires an interrupted terminal")]
    RequiresInterruptedTerminal,
    #[error("provider-observation gap requires an issue witness")]
    MissingObservationIssue,
    #[error("repair issue witness must precede the terminal witness")]
    InvalidIssueOrder,
}

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
