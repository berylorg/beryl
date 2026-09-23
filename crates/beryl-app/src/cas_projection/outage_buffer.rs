mod assembly;
mod encoding;
mod inventory;
mod slot;

pub use assembly::{OutageAssembly, OutageAssemblyError, OutageAssemblyLimits};
pub(in crate::cas_projection) use inventory::{
    OutageInventory, OutageInventoryAccess, OutageInventoryError,
};

#[cfg(feature = "test-faults")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutageCaptureState {
    Pending,
    Ready,
    Unavailable,
}

#[cfg(feature = "test-faults")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutageCaptureSnapshot {
    pub state: OutageCaptureState,
    pub targets: usize,
    pub facts: usize,
    pub gapped_targets: usize,
    pub encoded_bytes: usize,
}
pub use slot::OutageObservationSlot;

use super::ConnectionWorkTargetIdentity;
use beryl_backend::{
    NormalTurnTerminalStatus, ProviderObservationBegin, ProviderObservationControl,
    ProviderValueContext,
};
use beryl_model::{CasItemId, CasTurnId, ProviderObservationId, SyndicTurnId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutageBufferLimits {
    pub max_facts: usize,
    pub max_encoded_bytes: usize,
    pub max_field_bytes: usize,
    pub max_targets: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutageTarget {
    identity: ConnectionWorkTargetIdentity,
    turn: SyndicTurnId,
    cas_turn: CasTurnId,
}

impl OutageTarget {
    pub fn connection(&self) -> OutageConnectionIdentity {
        OutageConnectionIdentity {
            runtime: self.identity.runtime_id,
            process: self.identity.process_generation,
            connection: self.identity.connection_generation,
            home_generation: self.identity.home_generation,
        }
    }

    pub fn new(
        identity: ConnectionWorkTargetIdentity,
        turn: SyndicTurnId,
        cas_turn: CasTurnId,
    ) -> Self {
        Self {
            identity,
            turn,
            cas_turn,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutageConnectionIdentity {
    pub runtime: beryl_model::RuntimeId,
    pub process: beryl_model::CasProcessGeneration,
    pub connection: u64,
    pub home_generation: u64,
}

impl OutageConnectionIdentity {
    pub(in crate::cas_projection) fn record_gap(self, buffer: &mut OutageBuffer) {
        for state in &mut buffer.targets {
            if state.target.connection() == self {
                state.gap = true;
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum OutagePriority {
    Operational,
    MediaHandoff,
    UserCorrelation,
    TranscriptNarrative,
    AssistantFinal,
    Terminal,
    IdentityCorrelation,
}

#[derive(Clone, Copy, Debug)]
pub enum OutageTextKind {
    IdentityCorrelation,
    UserCorrelation,
    MediaHandoff,
    AssistantFinal,
    TranscriptNarrative,
    Operational,
}

#[derive(Clone, Copy, Debug)]
pub enum OutageFact<'a> {
    Identity {
        observation: ProviderObservationId,
        item: &'a CasItemId,
    },
    Lifecycle {
        observation: ProviderObservationId,
        begin: ProviderObservationBegin,
    },
    Control {
        observation: ProviderObservationId,
        ordinal: u64,
        priority: OutagePriority,
        control: ProviderObservationControl,
    },
    Terminal(NormalTurnTerminalStatus),
    CompleteField {
        observation: ProviderObservationId,
        ordinal: u64,
        context: ProviderValueContext,
        kind: OutageTextKind,
        text: &'a str,
    },
    UserCorrelation {
        item: &'a CasItemId,
        client: &'a beryl_backend::ClientUserMessageId,
    },
    MediaHandoff {
        item: &'a CasItemId,
        saved_path: &'a str,
    },
}

impl OutageFact<'_> {
    fn priority(self) -> OutagePriority {
        match self {
            Self::Identity { .. } | Self::Lifecycle { .. } => OutagePriority::IdentityCorrelation,
            Self::Control { priority, .. } => priority,
            Self::Terminal(_) => OutagePriority::Terminal,
            Self::CompleteField { kind, .. } => match kind {
                OutageTextKind::IdentityCorrelation => OutagePriority::IdentityCorrelation,
                OutageTextKind::UserCorrelation => OutagePriority::UserCorrelation,
                OutageTextKind::MediaHandoff => OutagePriority::MediaHandoff,
                OutageTextKind::AssistantFinal => OutagePriority::AssistantFinal,
                OutageTextKind::TranscriptNarrative => OutagePriority::TranscriptNarrative,
                OutageTextKind::Operational => OutagePriority::Operational,
            },
            Self::UserCorrelation { .. } => OutagePriority::UserCorrelation,
            Self::MediaHandoff { .. } => OutagePriority::MediaHandoff,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutageLoss {
    Partial,
    Unrepresentable,
    Dropped,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutageBufferError {
    TargetLimit,
    DuplicateTarget,
    FieldLimit,
    EncodedByteLimit,
    FactLimit,
    UnknownTarget,
}

struct TargetState {
    target: OutageTarget,
    gap: bool,
}
struct RetainedFact {
    target: usize,
    priority: OutagePriority,
    encoded: Vec<u8>,
}

pub struct OutageBuffer {
    limits: OutageBufferLimits,
    targets: Vec<TargetState>,
    facts: Vec<RetainedFact>,
    encoded_bytes: usize,
}

impl OutageBuffer {
    pub(in crate::cas_projection) fn capture_routed(
        &mut self,
        connection: OutageConnectionIdentity,
        thread: &beryl_model::CasThreadId,
        turn: &CasTurnId,
        fact: Option<OutageFact<'_>>,
        gap: bool,
    ) {
        let mut matches = self.targets.iter().enumerate().filter(|(_, state)| {
            state.target.connection() == connection
                && state.target.identity.cas_thread_id() == thread
                && &state.target.cas_turn == turn
        });
        let index = matches.next().map(|(index, _)| index);
        if index.is_none() || matches.next().is_some() {
            connection.record_gap(self);
            return;
        }
        let index = index.expect("unique exact route");
        self.targets[index].gap |= gap;
        if let Some(fact) = fact {
            let target = self.targets[index].target.clone();
            let _ = self.offer(&target, fact);
        }
    }

    pub fn new(
        limits: OutageBufferLimits,
        targets: &[OutageTarget],
    ) -> Result<Self, OutageBufferError> {
        if targets.len() > limits.max_targets {
            return Err(OutageBufferError::TargetLimit);
        }
        let mut encoded_bytes = 0usize;
        for (index, target) in targets.iter().enumerate() {
            if targets[..index].iter().any(|prior| {
                prior == target
                    || (prior.identity.thread_id() == target.identity.thread_id()
                        && prior.turn == target.turn)
            }) {
                return Err(OutageBufferError::DuplicateTarget);
            }
            let count = encoding::target_len(target, limits.max_field_bytes)?;
            encoded_bytes = encoded_bytes
                .checked_add(count)
                .filter(|bytes| *bytes <= limits.max_encoded_bytes)
                .ok_or(OutageBufferError::EncodedByteLimit)?;
        }
        Ok(Self {
            limits,
            targets: targets
                .iter()
                .map(|target| TargetState {
                    target: target.clone(),
                    gap: false,
                })
                .collect(),
            facts: Vec::new(),
            encoded_bytes,
        })
    }

    pub fn offer(
        &mut self,
        target: &OutageTarget,
        fact: OutageFact<'_>,
    ) -> Result<(), OutageBufferError> {
        let target_index = self.target_index(target)?;
        let result = self.offer_qualified(target_index, fact);
        if result.is_err() {
            self.targets[target_index].gap = true;
        }
        result
    }

    fn offer_qualified(
        &mut self,
        target: usize,
        fact: OutageFact<'_>,
    ) -> Result<(), OutageBufferError> {
        let length = encoding::fact_len(target, fact, self.limits.max_field_bytes)?;
        let mut bytes = self
            .encoded_bytes
            .checked_add(length)
            .ok_or(OutageBufferError::EncodedByteLimit)?;
        let mut count = self
            .facts
            .len()
            .checked_add(1)
            .ok_or(OutageBufferError::FactLimit)?;
        let mut victims: Vec<usize> = self
            .facts
            .iter()
            .enumerate()
            .filter_map(|(index, retained)| (retained.priority < fact.priority()).then_some(index))
            .collect();
        victims.sort_by_key(|index| (self.facts[*index].priority, *index));
        let mut removed = 0;
        while count > self.limits.max_facts || bytes > self.limits.max_encoded_bytes {
            let Some(index) = victims.get(removed) else {
                return Err(if count > self.limits.max_facts {
                    OutageBufferError::FactLimit
                } else {
                    OutageBufferError::EncodedByteLimit
                });
            };
            bytes -= self.facts[*index].encoded.len();
            count -= 1;
            removed += 1;
        }
        victims.truncate(removed);
        victims.sort_unstable_by(|left, right| right.cmp(left));
        for index in victims {
            let removed = self.facts.remove(index);
            self.targets[removed.target].gap = true;
        }
        let encoded = encoding::encode_fact(target, fact, length);
        self.facts.push(RetainedFact {
            target,
            priority: fact.priority(),
            encoded,
        });
        self.encoded_bytes = bytes;
        Ok(())
    }

    pub fn record_loss(
        &mut self,
        target: &OutageTarget,
        _loss: OutageLoss,
    ) -> Result<(), OutageBufferError> {
        let index = self.target_index(target)?;
        self.targets[index].gap = true;
        Ok(())
    }

    pub fn has_gap(&self, target: &OutageTarget) -> Result<bool, OutageBufferError> {
        self.target_index(target)
            .map(|index| self.targets[index].gap)
    }

    pub fn retained(&self) -> impl Iterator<Item = (&OutageTarget, OutagePriority, &[u8])> {
        self.facts.iter().map(|fact| {
            (
                &self.targets[fact.target].target,
                fact.priority,
                fact.encoded.as_slice(),
            )
        })
    }

    pub fn encoded_bytes(&self) -> usize {
        self.encoded_bytes
    }
    pub fn retire(self) {}

    fn target_index(&self, target: &OutageTarget) -> Result<usize, OutageBufferError> {
        self.targets
            .iter()
            .position(|state| state.target == *target)
            .ok_or(OutageBufferError::UnknownTarget)
    }
}
