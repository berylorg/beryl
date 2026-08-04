use beryl_model::{SyndicPathDigest, SyndicThreadId};

use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn records(
    source_thread: SyndicThreadId,
    binding_one: BindingRevision,
    binding_two: BindingRevision,
    binding_three: BindingRevision,
    binding_four: BindingRevision,
    source_selected: SelectedPathProof,
    source_usable: UsableCasBinding,
    source_active: ActiveCasBinding,
    terminal_usable: UsableCasBinding,
    source_digest: SyndicPathDigest,
    source: SyndicTurnId,
    source_cas_thread: CasThreadId,
    represented_parent: CasRepresentedPrefixProof,
    lineage: CasLineageProof,
    source_cas_turn: CasTurnId,
) -> Vec<FixtureRecord> {
    vec![
        FixtureRecord::Binding(BindingRecord::new(
            source_thread,
            binding_one,
            source_selected,
            BindingState::unbound("source fixture").unwrap(),
        )),
        FixtureRecord::Binding(BindingRecord::new(
            source_thread,
            binding_two,
            source_selected,
            BindingState::valid(source_usable),
        )),
        FixtureRecord::Binding(BindingRecord::new(
            source_thread,
            binding_three,
            source_selected,
            BindingState::active(source_active),
        )),
        FixtureRecord::Binding(BindingRecord::new(
            source_thread,
            binding_four,
            source_selected,
            BindingState::valid(terminal_usable),
        )),
        FixtureRecord::BindingHead(BindingHeadRecord::new(
            source_thread,
            binding_four,
            BindingLifecycle::Valid,
            source_digest,
        )),
        FixtureRecord::ExecutionSnapshot(ExecutionSnapshotRecord::new(
            source_snapshot(),
            source_thread,
            binding_three,
            InputGateRevision::new(1).unwrap(),
            source,
            source_cas_thread.clone(),
            source_selected,
            represented_parent,
            CasNativeTurnCount::ZERO,
            test_tool_profile(),
            lineage,
            execution_binding(),
            CasLoadedSessionGeneration::new(
                CasProcessGeneration::new(1).unwrap(),
                CasLoadedThreadGeneration::new(1).unwrap(),
            ),
            timestamp(3),
        )),
        FixtureRecord::ActiveCasTurn(ActiveCasTurnRecord::new(
            source_snapshot(),
            source_thread,
            source,
            binding_three,
            source_cas_thread.clone(),
            source_cas_turn.clone(),
            timestamp(3),
        )),
        FixtureRecord::CasThread(CasThreadIndexRecord::with_latest(
            source_cas_thread.clone(),
            source_thread,
            binding_two,
            binding_four,
        )),
        FixtureRecord::CasThreadBinding(CasThreadBindingIndexRecord::new(
            source_cas_thread.clone(),
            source_thread,
            binding_two,
        )),
        FixtureRecord::CasThreadBinding(CasThreadBindingIndexRecord::new(
            source_cas_thread.clone(),
            source_thread,
            binding_three,
        )),
        FixtureRecord::CasThreadBinding(CasThreadBindingIndexRecord::new(
            source_cas_thread.clone(),
            source_thread,
            binding_four,
        )),
        FixtureRecord::CasTurn(CasTurnIndexRecord::new(
            source_cas_thread,
            source_cas_turn,
            source_thread,
            source,
            binding_three,
            source_snapshot(),
            CasNativeTurnCount::new(1),
        )),
    ]
}
