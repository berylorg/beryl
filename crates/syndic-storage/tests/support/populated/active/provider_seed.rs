use super::*;

pub(in crate::support::populated) fn seed_provider_records(
    store: &beryl_home_store::HomeStore,
    storage: &SyndicStorage,
    receipts: &mut Vec<beryl_home_store::CommitReceipt>,
) {
    let turn = super::super::active_turn();
    let cas_thread = cas_thread();
    let cas_turn = cas_turn();
    let source = CasTurnSource::new(cas_thread.clone(), cas_turn.clone());
    let mut seed = ProviderSeedTurn {
        thread: id(40),
        turn,
        source,
        state_revision: TurnStateRevision::FIRST,
        gate_revision: InputGateRevision::new(4).unwrap(),
        observed_at: timestamp(8),
    };
    agent_item_fixture(
        active_item(),
        turn,
        CasItemSource::new(
            CasTurnSource::new(cas_thread.clone(), cas_turn.clone()),
            cas_item(),
        ),
        SourceEventSequence::FIRST,
        ProviderMessagePhaseV1::Commentary,
        "active",
        AgentItemFixtureState::Completed,
    )
    .seed(store, storage, &mut seed, receipts);
    agent_item_fixture(
        suffix_item(),
        turn,
        CasItemSource::new(
            CasTurnSource::new(cas_thread.clone(), cas_turn.clone()),
            CasItemId::new("active-suffix-item").unwrap(),
        ),
        SourceEventSequence::new(4).unwrap(),
        ProviderMessagePhaseV1::Commentary,
        "",
        AgentItemFixtureState::Live,
    )
    .seed(store, storage, &mut seed, receipts);
    agent_item_fixture(
        build_item(),
        turn,
        CasItemSource::new(
            CasTurnSource::new(cas_thread.clone(), cas_turn.clone()),
            CasItemId::new("active-build-item").unwrap(),
        ),
        SourceEventSequence::new(5).unwrap(),
        ProviderMessagePhaseV1::Commentary,
        "",
        AgentItemFixtureState::Live,
    )
    .seed(store, storage, &mut seed, receipts);
    command_item_fixture(
        activity_item(),
        turn,
        CasItemSource::new(
            CasTurnSource::new(cas_thread, cas_turn),
            CasItemId::new("active-activity-item").unwrap(),
        ),
        SourceEventSequence::new(6).unwrap(),
    )
    .seed(store, storage, &mut seed, receipts);
}
