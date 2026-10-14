use super::marker_advance::PendingRootAcquisition;
use super::*;
use crate::draft_piece::deferred_prefix::{is_deferred, is_prefix, is_removal_at, prefix_ordinal};

pub(in crate::draft_piece) trait DeferredFragmentAcquisition {
    fn deferred_fragment(
        &mut self,
        key: DraftPieceBuildFragmentKeyV1,
    ) -> Result<DraftPieceBuildFragmentV1, DraftPiecePrepareErrorV1>;
}

impl<T: PendingRootAcquisition> DeferredFragmentAcquisition for T {
    fn deferred_fragment(
        &mut self,
        key: DraftPieceBuildFragmentKeyV1,
    ) -> Result<DraftPieceBuildFragmentV1, DraftPiecePrepareErrorV1> {
        self.acquire::<DraftPieceBuildFragmentsFamily>(key)
    }
}

fn canonical_fragment(
    acquisition: &mut impl DeferredFragmentAcquisition,
    key: DraftPieceBuildFragmentKeyV1,
) -> Result<DraftPieceBuildFragmentV1, DraftPiecePrepareErrorV1> {
    let value = acquisition.deferred_fragment(key)?;
    if value.key() != key
        || validate_fragment(value.replacement()).is_err()
        || value.chain_digest()
            != draft_piece_fragment_chain_link_v1(
                value.preceding_chain(),
                key.ordinal(),
                value.replacement(),
            )
    {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    Ok(value)
}

fn fragment(
    acquisition: &mut impl DeferredFragmentAcquisition,
    key: DraftPieceBuildFragmentKeyV1,
) -> Result<DraftPieceBuildFragmentV1, DraftPiecePrepareErrorV1> {
    let value = canonical_fragment(acquisition, key)?;
    if key.ordinal() == 1 {
        if value.preceding_chain() != canonical_empty_draft_piece_fragment_chain_v1() {
            return Err(DraftPiecePrepareErrorV1::InvalidRoot);
        }
    } else {
        let prior_key = DraftPieceBuildFragmentKeyV1::new(
            key.draft_id(),
            key.session_id(),
            key.operation_id(),
            key.ordinal() - 1,
        );
        let prior = acquisition.deferred_fragment(prior_key)?;
        if prior.key() != prior_key
            || prior.chain_digest() != value.preceding_chain()
            || validate_fragment(prior.replacement()).is_err()
        {
            return Err(DraftPiecePrepareErrorV1::InvalidRoot);
        }
    }
    Ok(value)
}

pub(in crate::draft_piece) fn validate_transition(
    acquisition: &mut impl DeferredFragmentAcquisition,
    previous: &DraftPieceBuildProgressReceiptV1,
    current: &DraftPieceBuildProgressReceiptV1,
) -> Result<(), DraftPiecePrepareErrorV1> {
    let selected = [previous, current]
        .into_iter()
        .find(|receipt| is_deferred(receipt.frontier(), receipt.marker_effect_continuation()));
    let Some(selected) = selected else {
        return Ok(());
    };
    let key = selected.key();
    let ordinal =
        prefix_ordinal(selected.frontier()).ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
    let prefix = fragment(
        acquisition,
        DraftPieceBuildFragmentKeyV1::new(
            key.draft_id(),
            key.session_id(),
            key.operation_id(),
            ordinal,
        ),
    )?;
    if !is_prefix(&prefix) {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    let anchor = prefix.replacement().end().utf8_offset();
    for receipt in [previous, current] {
        if !is_deferred(receipt.frontier(), receipt.marker_effect_continuation()) {
            continue;
        }
        let scan = receipt.marker_effect_continuation().scan();
        let removal_key = DraftPieceBuildFragmentKeyV1::new(
            key.draft_id(),
            key.session_id(),
            key.operation_id(),
            scan.next_fragment_ordinal(),
        );
        let removal = fragment(acquisition, removal_key)?;
        if !is_removal_at(&removal, anchor)
            || receipt
                .marker_effect_continuation()
                .active()
                .is_some_and(|active| {
                    active.fragment_key() != removal.key()
                        || active.fragment_digest()
                            != canonical_fragment_endpoint(&removal).digest()
                        || removal.replacement().marker_effect() != Some(active.effect())
                })
        {
            return Err(DraftPiecePrepareErrorV1::InvalidRoot);
        }
        if let Some(endpoint) = scan.scanned_endpoint() {
            let scanned = canonical_fragment(acquisition, endpoint.key())?;
            if canonical_fragment_endpoint(&scanned) != endpoint
                || (endpoint.key().ordinal() != ordinal && !is_removal_at(&scanned, anchor))
            {
                return Err(DraftPiecePrepareErrorV1::InvalidRoot);
            }
        }
    }
    let before = previous.marker_effect_continuation();
    let after = current.marker_effect_continuation();
    let entering =
        !is_deferred(previous.frontier(), before) && is_deferred(current.frontier(), after);
    let deciding = before
        .active()
        .is_some_and(|active| active.phase() == DraftPieceActiveMarkerPhaseV1::Removing)
        && previous.mapping().is_some_and(|mapping| {
            mapping.mapping_stage == DraftPieceMappingStageV1::MarkerMapBoundary
        })
        && current
            .mapping()
            .is_some_and(|mapping| mapping.mapping_stage == DraftPieceMappingStageV1::Idle)
        && current.lifecycle() == DraftPieceBuildLifecycleV1::Open;
    if entering || deciding {
        let next = if entering {
            ordinal.checked_add(1)
        } else {
            before.scan().next_fragment_ordinal().checked_add(1)
        }
        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
        let end = current
            .fragment_endpoint()
            .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?
            .key()
            .ordinal();
        let eligible = if next > end {
            false
        } else {
            let lookahead_key = DraftPieceBuildFragmentKeyV1::new(
                key.draft_id(),
                key.session_id(),
                key.operation_id(),
                next,
            );
            is_removal_at(&fragment(acquisition, lookahead_key)?, anchor)
        };
        let continues = entering || after.active().is_none();
        if eligible != continues {
            return Err(DraftPiecePrepareErrorV1::InvalidRoot);
        }
    }
    Ok(())
}
