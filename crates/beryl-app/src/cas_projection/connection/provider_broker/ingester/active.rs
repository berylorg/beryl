use beryl_backend::{DynamicToolCall, SteeringUserMessageSelection, StreamedInputHeader};
use beryl_home_store::HomeGeneration;
use beryl_model::ProviderObservationId;
use syndic_storage::{ProviderCompactionMarkerStager, ProviderObservationStager, SyndicStorage};

use super::super::steering_result::SteeringSequenceProof;

pub(super) struct ActiveDurableObservation {
    pub(super) stager: ProviderObservationStager,
    pub(super) identity: ProviderObservationId,
    pub(super) home_generation: HomeGeneration,
    pub(super) storage: SyndicStorage,
}

pub(super) enum ActiveObservation {
    Durable(ActiveDurableObservation),
    Compaction(ProviderCompactionMarkerStager),
}

impl ActiveObservation {
    pub(super) fn abandon(self) {
        match self {
            Self::Durable(observation) => observation.stager.abandon(),
            Self::Compaction(marker) => marker.abandon(),
        }
    }
}

pub(super) enum ActiveIngress {
    Provider(ActiveObservation),
    Dynamic(ActiveDynamicTool),
    Steering(ActiveSteeringLifecycle),
}

pub(super) struct ActiveDynamicTool {
    pub(super) call: DynamicToolCall,
    pub(super) builder: crate::conversation_tools::InstalledArgumentBuilder,
    pub(super) permit: crate::cas_projection::connection::router::DynamicToolTargetPermit,
}

pub(super) struct ActiveSteeringLifecycle {
    pub(super) selection: SteeringUserMessageSelection,
    pub(super) header: StreamedInputHeader,
    pub(super) proof: SteeringSequenceProof,
    pub(super) permit: crate::cas_projection::connection::router::DelayedSteeringLifecyclePermit,
}
