use beryl_model::{
    CasItemId, CasLoadedSessionGeneration, CasThreadId, CasTurnId, RuntimeId, SyndicThreadId,
};
use syndic_storage::{StopOperationId, StopOperationTarget};

pub use crate::cas_projection::stop::StopDispatchWorkState;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StopWorkFact {
    pub operation_id: StopOperationId,
    pub target: StopOperationTarget,
    pub local_dispatch: Option<StopDispatchWorkState>,
    pub primary_custody: bool,
    pub driver_custody: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PermissionInterruptionWorkStage {
    Reserved,
    Prepared,
    Pending,
    Driver,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PermissionInterruptionWorkFact {
    pub serial: u64,
    pub operation_id: Option<StopOperationId>,
    pub runtime_id: RuntimeId,
    pub connection_generation: u64,
    pub registration_serial: u64,
    pub thread_id: SyndicThreadId,
    pub loaded_generation: CasLoadedSessionGeneration,
    pub cas_thread_id: CasThreadId,
    pub cas_turn_id: CasTurnId,
    pub cas_item_id: Option<CasItemId>,
    pub stage: PermissionInterruptionWorkStage,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StopWorkRecord {
    Stop(StopWorkFact),
    Permission(PermissionInterruptionWorkFact),
}

impl StopWorkRecord {
    pub fn thread_id(&self) -> SyndicThreadId {
        match self {
            Self::Stop(fact) => fact.operation_id.thread_id(),
            Self::Permission(fact) => fact.thread_id,
        }
    }

    pub(super) fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + match self {
                Self::Stop(fact) => {
                    fact.target.cas_thread_id().as_str().len()
                        + fact.target.cas_turn_id().as_str().len()
                }
                Self::Permission(fact) => {
                    fact.cas_thread_id.as_str().len()
                        + fact.cas_turn_id.as_str().len()
                        + fact
                            .cas_item_id
                            .as_ref()
                            .map_or(0, |item| item.as_str().len())
                }
            }
    }
}
