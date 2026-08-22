use super::*;

pub fn source_turn() -> SyndicTurnId {
    SyndicTurnId::from_bytes([32; 16])
}

pub fn source_item() -> SyndicItemId {
    SyndicItemId::from_bytes([33; 16])
}

pub(super) fn source_cas_thread() -> CasThreadId {
    CasThreadId::new("source-history-thread").unwrap()
}

pub(super) fn source_cas_turn() -> CasTurnId {
    CasTurnId::new("source-history-turn").unwrap()
}

pub(super) fn source_cas_item() -> CasItemId {
    CasItemId::new("source-history-item").unwrap()
}

pub(super) fn source_snapshot() -> SyndicExecutionSnapshotId {
    SyndicExecutionSnapshotId::from_bytes([35; 16])
}

pub fn source_projection() -> SyndicProjectionId {
    syndic_storage::test_faults::fixture_inline_paragraph_projection(
        source_item(),
        source_turn(),
        "assistant",
    )
    .id()
}

pub fn source_resource() -> SyndicResourceId {
    SyndicResourceId::from_bytes([35; 16])
}

pub fn source_resource_projection() -> SyndicProjectionId {
    SyndicProjectionId::from_bytes([34; 16])
}

pub fn active_turn() -> SyndicTurnId {
    SyndicTurnId::from_bytes([42; 16])
}

pub fn active_item() -> SyndicItemId {
    SyndicItemId::from_bytes([43; 16])
}

pub fn active_projection() -> SyndicProjectionId {
    syndic_storage::test_faults::fixture_inline_paragraph_projection(
        active_item(),
        active_turn(),
        "active",
    )
    .id()
}

pub fn suffix_item() -> SyndicItemId {
    SyndicItemId::from_bytes([60; 16])
}

pub fn build_item() -> SyndicItemId {
    SyndicItemId::from_bytes([61; 16])
}

pub fn activity_item() -> SyndicItemId {
    SyndicItemId::from_bytes([62; 16])
}

pub fn suffix_projection() -> SyndicProjectionId {
    syndic_storage::test_faults::fixture_empty_projection(suffix_item(), active_turn()).id()
}

pub fn active_snapshot() -> SyndicExecutionSnapshotId {
    SyndicExecutionSnapshotId::from_bytes([45; 16])
}

pub fn steering_input() -> SyndicAcceptedInputId {
    SyndicAcceptedInputId::from_bytes([46; 16])
}

pub fn next_input() -> SyndicAcceptedInputId {
    SyndicAcceptedInputId::from_bytes([47; 16])
}

pub fn cas_thread() -> CasThreadId {
    CasThreadId::new("populated-thread").unwrap()
}

pub fn cas_turn() -> CasTurnId {
    CasTurnId::new("populated-turn").unwrap()
}

pub fn cas_item() -> CasItemId {
    CasItemId::new("populated-item").unwrap()
}

pub(super) fn execution_binding() -> beryl_model::ExecutionBinding {
    let path = beryl_model::RuntimeNativePath::from_admitted(
        beryl_model::RuntimeMode::host(),
        beryl_model::PathFlavor::Windows,
        "C:\\populated",
    )
    .unwrap();
    beryl_model::ExecutionBinding::new(
        beryl_model::RuntimeId::from_bytes([48; 16]),
        beryl_model::RootId::from_bytes([49; 16]),
        path,
    )
}
