use crate::turn::{ThreadUnsubscribeResponse, ThreadUnsubscribeStatus};

use super::{ConfigReadResponse, InitializeResponse, ModelPage};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompatibilityProbe {
    ConfigRead,
    ModelList,
    ThreadCompactStart,
    ThreadFork,
    ThreadInjectItems,
    ThreadResume,
    ThreadRollback,
    ThreadUnsubscribe,
    TurnInterrupt,
    TurnStart,
    TurnSteer,
}

impl CompatibilityProbe {
    pub const ALL: [Self; 11] = [
        Self::ConfigRead,
        Self::ModelList,
        Self::ThreadCompactStart,
        Self::ThreadFork,
        Self::ThreadInjectItems,
        Self::ThreadResume,
        Self::ThreadRollback,
        Self::ThreadUnsubscribe,
        Self::TurnInterrupt,
        Self::TurnStart,
        Self::TurnSteer,
    ];

    #[must_use]
    pub const fn method(self) -> &'static str {
        match self {
            Self::ConfigRead => "config/read",
            Self::ModelList => "model/list",
            Self::ThreadCompactStart => "thread/compact/start",
            Self::ThreadFork => "thread/fork",
            Self::ThreadInjectItems => "thread/inject_items",
            Self::ThreadResume => "thread/resume",
            Self::ThreadRollback => "thread/rollback",
            Self::ThreadUnsubscribe => "thread/unsubscribe",
            Self::TurnInterrupt => "turn/interrupt",
            Self::TurnStart => "turn/start",
            Self::TurnSteer => "turn/steer",
        }
    }

    const fn bit(self) -> u16 {
        1 << (self as u8)
    }

    const fn is_mutating_schema(self) -> bool {
        matches!(
            self,
            Self::ThreadCompactStart
                | Self::ThreadFork
                | Self::ThreadInjectItems
                | Self::ThreadResume
                | Self::ThreadRollback
                | Self::TurnInterrupt
                | Self::TurnStart
                | Self::TurnSteer
        )
    }
}

const REQUIRED_COMPATIBILITY_PROBE_BITS: u16 = (1 << CompatibilityProbe::ALL.len()) - 1;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CompatibilityProbeSet(u16);

impl CompatibilityProbeSet {
    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }

    pub fn insert(&mut self, probe: CompatibilityProbe) {
        self.0 |= probe.bit();
    }

    #[must_use]
    pub const fn contains(self, probe: CompatibilityProbe) -> bool {
        self.0 & probe.bit() != 0
    }

    #[must_use]
    pub const fn is_complete(self) -> bool {
        self.0 == REQUIRED_COMPATIBILITY_PROBE_BITS
    }

    #[must_use]
    pub const fn bits(self) -> u16 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmptyAcknowledgement {
    ThreadCompactStart,
    ThreadBackgroundTerminalsClean,
    ThreadInjectItems,
    TurnInterrupt,
}

#[derive(Debug, PartialEq, Eq)]
pub enum CompatibilityProbeResult {
    ConfigRead(ConfigReadResponse),
    ModelList(Box<ModelPage>),
    ThreadUnsubscribe(ThreadUnsubscribeStatus),
    UnexpectedMutatingSuccess(CompatibilityProbe),
}

impl CompatibilityProbeResult {
    #[must_use]
    pub const fn unexpected_mutating_success(probe: CompatibilityProbe) -> Option<Self> {
        if probe.is_mutating_schema() {
            Some(Self::UnexpectedMutatingSuccess(probe))
        } else {
            None
        }
    }

    #[must_use]
    pub const fn probe(&self) -> CompatibilityProbe {
        match self {
            Self::ConfigRead(_) => CompatibilityProbe::ConfigRead,
            Self::ModelList(_) => CompatibilityProbe::ModelList,
            Self::ThreadUnsubscribe(_) => CompatibilityProbe::ThreadUnsubscribe,
            Self::UnexpectedMutatingSuccess(probe) => *probe,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum BoundedResponseResult {
    Initialize(InitializeResponse),
    ConfigRead(ConfigReadResponse),
    ModelList(Box<ModelPage>),
    ThreadStart(crate::ThreadLineageResponse),
    ThreadRead(crate::ThreadReadMetadata),
    ThreadResume(crate::ThreadLineageResponse),
    ThreadFork(crate::ThreadLineageResponse),
    TurnStart(crate::turn::TurnStartResponseWire),
    TurnSteer(crate::TurnSteerResponseWire),
    EmptyAcknowledgement(EmptyAcknowledgement),
    ThreadUnsubscribe(ThreadUnsubscribeStatus),
    Compatibility(CompatibilityProbeResult),
}

impl ThreadUnsubscribeStatus {
    #[must_use]
    pub fn from_wire(value: &str) -> Option<Self> {
        Some(match value {
            "notLoaded" => Self::NotLoaded,
            "notSubscribed" => Self::NotSubscribed,
            "unsubscribed" => Self::Unsubscribed,
            _ => return None,
        })
    }
}

impl ThreadUnsubscribeResponse {
    #[must_use]
    pub const fn new(status: ThreadUnsubscribeStatus) -> Self {
        Self { status }
    }
}
