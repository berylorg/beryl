use super::*;
use crate::draft_piece::build_mapping::{DraftPieceBuildMappingFamily, model::MapRoot};
use crate::draft_piece::mutation::mapping_custody;

#[derive(Default)]
struct Evidence {
    mapping: Vec<([u8; 64], MapRoot)>,
    sequence: Vec<DraftPieceSequenceDescriptorV1>,
    identity: Vec<DraftPieceIdentityDescriptorV1>,
    order: Vec<DraftPieceBuildRootsV1>,
    fragments: Vec<DraftPieceBuildFragmentV1>,
}

impl Evidence {
    fn mapping(
        &mut self,
        storage: &SyndicStorage,
        store: &HomeStore,
        receipt: &DraftPieceBuildProgressReceiptV1,
        limit: crate::SyndicPointReadLimit,
    ) -> Result<bool, SyndicReadError> {
        let Some(mapping) = receipt.mapping() else {
            return Ok(false);
        };
        let owner = DraftPieceSettlementKeyV1::new(
            receipt.key().draft_id(),
            receipt.key().session_id(),
            receipt.key().operation_id(),
        );
        for root in [
            Some(mapping.current_map),
            mapping_custody::pending_root(mapping),
        ]
        .into_iter()
        .flatten()
        {
            let Ok(key) = mapping_custody::stored_key(owner, root) else {
                return Ok(false);
            };
            let Some(key) = key else {
                continue;
            };
            if let Some((_, prior)) = self.mapping.iter().find(|(prior_key, _)| *prior_key == key) {
                if *prior != root {
                    return Ok(false);
                }
                continue;
            }
            if self.mapping.len() >= 3 {
                return Ok(false);
            }
            let Some(node) = storage.point::<DraftPieceBuildMappingFamily>(store, key, limit)?
            else {
                return Ok(false);
            };
            if mapping_custody::validate_node(key, root, &node).is_err() {
                return Ok(false);
            }
            self.mapping.push((key, root));
        }
        Ok(true)
    }

    fn sequence(
        &mut self,
        storage: &SyndicStorage,
        store: &HomeStore,
        draft: SyndicDraftId,
        value: DraftPieceSequenceDescriptorV1,
        limit: crate::SyndicPointReadLimit,
    ) -> Result<bool, SyndicReadError> {
        if !value.is_locally_exact() {
            return Ok(false);
        }
        let Some(id) = value.root_node_id else {
            return Ok(true);
        };
        if let Some(prior) = self
            .sequence
            .iter()
            .find(|prior| prior.root_node_id == Some(id))
        {
            return Ok(*prior == value);
        }
        if self.sequence.len() + self.identity.len() + self.order.len() >= 9 {
            return Ok(false);
        }
        let key = DraftPieceRecordKeyV1::new(draft, id);
        if storage
            .point::<DraftPieceNodesFamily>(store, key, limit)?
            .is_none_or(|node| {
                node.key() != key || validate_sequence_root_node(node, value.summary).is_err()
            })
        {
            return Ok(false);
        }
        self.sequence.push(value);
        Ok(true)
    }

    fn identity(
        &mut self,
        storage: &SyndicStorage,
        store: &HomeStore,
        draft: SyndicDraftId,
        value: DraftPieceIdentityDescriptorV1,
        limit: crate::SyndicPointReadLimit,
    ) -> Result<bool, SyndicReadError> {
        if !value.is_locally_exact() {
            return Ok(false);
        }
        let Some(id) = value.root_node_id else {
            return Ok(true);
        };
        if let Some(prior) = self
            .identity
            .iter()
            .find(|prior| prior.root_node_id == Some(id))
        {
            return Ok(*prior == value);
        }
        if self.sequence.len() + self.identity.len() + self.order.len() >= 9 {
            return Ok(false);
        }
        let key = DraftMarkerIdentityRecordKeyV1::new(
            draft,
            DraftMarkerIdentityRecordKindV1::Internal,
            id,
        );
        if storage
            .point::<DraftMarkerIdentityIndexFamily>(store, key, limit)?
            .is_none_or(|node| {
                node.key() != key || validate_index_root_record(node, value.summary).is_err()
            })
        {
            return Ok(false);
        }
        self.identity.push(value);
        Ok(true)
    }

    fn roots(
        &mut self,
        storage: &SyndicStorage,
        store: &HomeStore,
        draft: SyndicDraftId,
        roots: DraftPieceBuildRootsV1,
        limit: crate::SyndicPointReadLimit,
    ) -> Result<bool, SyndicReadError> {
        if !draft_piece_build_roots_are_locally_exact_v1(roots)
            || !self.sequence(
                storage,
                store,
                draft,
                DraftPieceSequenceDescriptorV1 {
                    root_node_id: roots.sequence_root(),
                    summary: roots.sequence_summary(),
                },
                limit,
            )?
            || !self.identity(
                storage,
                store,
                draft,
                DraftPieceIdentityDescriptorV1 {
                    root_node_id: roots.marker_index_root(),
                    summary: roots.marker_index_summary(),
                },
                limit,
            )?
        {
            return Ok(false);
        }
        let Some(id) = roots.marker_order_root() else {
            return Ok(true);
        };
        if let Some(prior) = self
            .order
            .iter()
            .find(|prior| prior.marker_order_root() == Some(id))
        {
            return Ok(prior.marker_order_height() == roots.marker_order_height()
                && prior.marker_commitment() == roots.marker_commitment());
        }
        if self.sequence.len() + self.identity.len() + self.order.len() >= 9 {
            return Ok(false);
        }
        let key =
            DraftMarkerOrderRecordKeyV1::new(draft, DraftMarkerOrderRecordKindV1::Internal, id);
        if storage
            .point::<DraftMarkerOrderCommitmentsFamily>(store, key, limit)?
            .is_none_or(|node| {
                node.key() != key || validate_marker_order_root_record(node, roots).is_err()
            })
        {
            return Ok(false);
        }
        self.order.push(roots);
        Ok(true)
    }

    fn fragment(
        &mut self,
        storage: &SyndicStorage,
        store: &HomeStore,
        key: DraftPieceBuildFragmentKeyV1,
        limit: crate::SyndicPointReadLimit,
    ) -> Result<Option<&DraftPieceBuildFragmentV1>, SyndicReadError> {
        if let Some(index) = self
            .fragments
            .iter()
            .position(|fragment| fragment.key() == key)
        {
            return Ok(self.fragments.get(index));
        }
        if self.fragments.len() >= 4 {
            return Ok(None);
        }
        let Some(fragment) = storage.point::<DraftPieceBuildFragmentsFamily>(store, key, limit)?
        else {
            return Ok(None);
        };
        if fragment.key() != key {
            return Ok(None);
        }
        self.fragments.push(fragment);
        Ok(self.fragments.last())
    }

    fn effects(
        &mut self,
        storage: &SyndicStorage,
        store: &HomeStore,
        receipt: &DraftPieceBuildProgressReceiptV1,
        limit: crate::SyndicPointReadLimit,
    ) -> Result<bool, SyndicReadError> {
        if !progress_receipt_is_exact(receipt) || !self.mapping(storage, store, receipt, limit)? {
            return Ok(false);
        }
        if let Some(endpoint) = receipt.fragment_endpoint() {
            if self
                .fragment(storage, store, endpoint.key(), limit)?
                .is_none_or(|fragment| canonical_fragment_endpoint(fragment) != endpoint)
            {
                return Ok(false);
            }
        }
        let draft = receipt.key().draft_id();
        if !self.roots(storage, store, draft, receipt.working_roots(), limit)? {
            return Ok(false);
        }
        let Some(active) = receipt.marker_effect_continuation().active() else {
            return Ok(true);
        };
        if self
            .fragment(storage, store, active.fragment_key(), limit)?
            .is_none_or(|fragment| {
                canonical_fragment_endpoint(fragment).digest() != active.fragment_digest()
                    || fragment.replacement().marker_effect() != Some(active.effect())
            })
            || active.source_roots() != receipt.working_roots()
            || !self.roots(storage, store, draft, active.working_roots(), limit)?
        {
            return Ok(false);
        }
        use DraftPieceMarkerPendingV1 as Pending;
        let (sequence, identity) = match active.pending() {
            Pending::RemoveIdentity { sequence_target }
            | Pending::InsertIdentity {
                sequence_target, ..
            } => (Some(sequence_target), None),
            Pending::RemoveOrder {
                sequence_target,
                identity_target,
            }
            | Pending::InsertOrder {
                sequence_target,
                identity_target,
                ..
            } => (Some(sequence_target), Some(identity_target)),
            _ => (None, None),
        };
        if let Some(sequence) = sequence {
            if !self.sequence(storage, store, draft, sequence, limit)? {
                return Ok(false);
            }
        }
        if let Some(identity) = identity {
            if !self.identity(storage, store, draft, identity, limit)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn transition(
        &mut self,
        storage: &SyndicStorage,
        store: &HomeStore,
        previous: &DraftPieceBuildProgressReceiptV1,
        current: &DraftPieceBuildProgressReceiptV1,
        limit: crate::SyndicPointReadLimit,
    ) -> Result<bool, SyndicReadError> {
        let key = mapping_custody::transition_fragment_key(previous, current);
        let fragment = match key {
            Some(key) => self.fragment(storage, store, key, limit)?,
            None => None,
        };
        if !marker_effect_progress_transition_is_exact(previous, current, fragment) {
            return Ok(false);
        }
        let preceding_chain = fragment.map(|fragment| fragment.preceding_chain());
        let prior_key =
            super::super::mutation::marker_advance::previous_proof_fragment_key(previous, current);
        let prior = match prior_key {
            Some(key) => self.fragment(storage, store, key, limit)?,
            None => None,
        };
        Ok(
            super::super::mutation::marker_advance::previous_proof_transition_is_exact(
                previous, current, prior,
            ) && prior.is_none_or(|prior| Some(prior.chain_digest()) == preceding_chain),
        )
    }
}

pub(in crate::draft_piece) fn progress_receipt_closure_is_exact(
    storage: &SyndicStorage,
    store: &HomeStore,
    current: &DraftPieceBuildProgressReceiptV1,
    previous: Option<&DraftPieceBuildProgressReceiptV1>,
    limit: crate::SyndicPointReadLimit,
) -> Result<bool, SyndicReadError> {
    let mut evidence = Evidence::default();
    if current.previous() != previous.map(|previous| previous.reference()) {
        return Ok(false);
    }
    if let Some(previous) = previous {
        if !evidence.effects(storage, store, previous, limit)?
            || !evidence.transition(storage, store, previous, current, limit)?
        {
            return Ok(false);
        }
    }
    evidence.effects(storage, store, current, limit)
}
