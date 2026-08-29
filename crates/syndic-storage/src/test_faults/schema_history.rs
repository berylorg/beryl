use beryl_home_store::{HomeStore, RecordCodec};
use beryl_model::{
    BindingRevision, CasThreadId, CasTurnId, InputGateRevision, SyndicAcceptedInputId,
    SyndicExecutionSnapshotId, SyndicThreadId, SyndicTurnId,
};

use crate::{SyndicStorage, codec::*, domain::SyndicDomain};

/// Awaiting-terminal family whose immediate predecessor must have no compatibility decoder.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AwaitingTerminalPredecessorFamily {
    InputGateV3,
    AcceptedRouteLeafV3,
    AcceptedRouteGenerationV2,
}

/// Canonical discriminants introduced by the awaiting-terminal schema.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AwaitingTerminalCodecTags {
    next_turn_reason: u8,
    input_gate_state: u8,
    route_target: u8,
    lost_target: u8,
}

impl AwaitingTerminalCodecTags {
    #[must_use]
    pub const fn next_turn_reason(self) -> u8 {
        self.next_turn_reason
    }

    #[must_use]
    pub const fn input_gate_state(self) -> u8 {
        self.input_gate_state
    }

    #[must_use]
    pub const fn route_target(self) -> u8 {
        self.route_target
    }

    #[must_use]
    pub const fn lost_target(self) -> u8 {
        self.lost_target
    }
}

/// Installs one immediate pre-awaiting-terminal envelope through the physical corruption seam.
pub fn inject_awaiting_terminal_predecessor(
    store: &HomeStore,
    storage: SyndicStorage,
    family: AwaitingTerminalPredecessorFamily,
) -> Result<(), beryl_home_store::test_faults::PersistedCorruptionError> {
    match family {
        AwaitingTerminalPredecessorFamily::InputGateV3 => {
            inject::<InputGatesFamily>(store, storage, SyndicThreadId::from_bytes([0xC1; 16]), 3)
        }
        AwaitingTerminalPredecessorFamily::AcceptedRouteLeafV3 => {
            inject::<AcceptedRouteLeavesFamily>(
                store,
                storage,
                SyndicAcceptedInputId::from_bytes([0xC2; 16]),
                3,
            )
        }
        AwaitingTerminalPredecessorFamily::AcceptedRouteGenerationV2 => {
            inject::<AcceptedRouteGenerationsFamily>(
                store,
                storage,
                ThreadRouteKey {
                    thread: SyndicThreadId::from_bytes([0xC3; 16]),
                    generation: crate::AcceptedRouteGeneration::FIRST,
                },
                2,
            )
        }
    }
}

/// Returns the exact new codec tags only when each complete value also round-trips canonically.
#[must_use]
pub fn awaiting_terminal_codec_tags() -> Option<AwaitingTerminalCodecTags> {
    let (next_turn_reason, input_gate_state) = awaiting_terminal_scalar_codec_tags()?;

    let target = steering_target();
    let route = empty_route(crate::AcceptedRouteTarget::AwaitingTerminal(target.clone()));
    let route_encoded = AcceptedRouteGenerationsFamily::encode_value(&route).ok()?;
    if AcceptedRouteGenerationsFamily::decode_value(&route_encoded).ok()? != route {
        return None;
    }

    let expected_route = crate::AcceptedRouteHeadProof::new(
        crate::AcceptedRouteGeneration::FIRST,
        crate::AcceptedRouteRevision::FIRST,
    );
    let lost = crate::AcceptedRouteProjectionLostProof::new(
        crate::AcceptedRouteLostTarget::AwaitingTerminal(target.clone()),
        crate::AcceptedRouteAbandonmentProof::new(
            BindingRevision::new(1).ok()?,
            InputGateRevision::new(1).ok()?,
            expected_route,
            crate::AcceptedRouteAbandonmentKind::Generic,
        ),
        BindingRevision::new(2).ok()?,
        target.pending().snapshot_id(),
        target.pending().cas_thread_id().clone(),
    );
    let lost_route = empty_route(crate::AcceptedRouteTarget::ProjectionLost(lost));
    let lost_encoded = AcceptedRouteGenerationsFamily::encode_value(&lost_route).ok()?;
    if AcceptedRouteGenerationsFamily::decode_value(&lost_encoded).ok()? != lost_route {
        return None;
    }

    Some(AwaitingTerminalCodecTags {
        next_turn_reason,
        input_gate_state,
        route_target: *route_encoded.get(32)?,
        lost_target: *lost_encoded.get(33)?,
    })
}

fn steering_target() -> crate::SteeringTargetProof {
    crate::SteeringTargetProof::new(
        crate::PendingSteeringTargetProof::new(
            BindingRevision::new(1).expect("one is nonzero"),
            SyndicExecutionSnapshotId::from_bytes([2; 16]),
            SyndicTurnId::from_bytes([3; 16]),
            CasThreadId::new("awaiting-terminal-codec-thread").expect("fixture id is valid"),
        ),
        CasTurnId::new("awaiting-terminal-codec-turn").expect("fixture id is valid"),
    )
}

fn empty_route(target: crate::AcceptedRouteTarget) -> crate::AcceptedRouteGenerationRecord {
    crate::AcceptedRouteGenerationRecord::new(
        SyndicThreadId::from_bytes([1; 16]),
        crate::AcceptedRouteGeneration::FIRST,
        crate::AcceptedRouteRevision::FIRST,
        target,
        None,
        None,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
    )
    .expect("empty codec fixture route is valid")
}

fn inject<F: Family>(
    store: &HomeStore,
    storage: SyndicStorage,
    key: F::Key,
    retired_version: u32,
) -> Result<(), beryl_home_store::test_faults::PersistedCorruptionError> {
    let encoded_key = <ExactCodec<F> as RecordCodec<SyndicDomain>>::encode_key(&key)
        .expect("retired schema-history fixture key must encode");
    store.inject_persisted_corrupt_record::<SyndicDomain, ExactCodec<F>>(
        &storage.handle,
        &encoded_key,
        &retired_version.to_be_bytes(),
    )
}
