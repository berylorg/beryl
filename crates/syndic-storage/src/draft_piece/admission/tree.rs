use std::num::NonZeroU64;

use beryl_model::{AssetId, ImageLabelOrdinal, SyndicDraftMarkerId};

use super::{
    DRAFT_MARKER_ADMISSION_COMMAND_MAX_ENCODED_BYTES, DRAFT_MARKER_ADMISSION_MAX_ASSOCIATIONS,
    DRAFT_MARKER_ADMISSION_PAGE_MAX_ASSOCIATIONS, DRAFT_MARKER_ADMISSION_TREE_FANOUT,
    DRAFT_MARKER_ADMISSION_TREE_MAX_HEIGHT, DraftMarkerAdmissionAssignmentContinuationV1,
    DraftMarkerAdmissionCapacityV1, DraftMarkerAdmissionChildV1,
    DraftMarkerAdmissionCleanupCursorV1, DraftMarkerAdmissionCommandIdV1,
    DraftMarkerAdmissionDigestV1, DraftMarkerAdmissionEnvelopeV1, DraftMarkerAdmissionEvidenceV1,
    DraftMarkerAdmissionHeadPartsV1, DraftMarkerAdmissionHeadV1, DraftMarkerAdmissionLifecycleV1,
    DraftMarkerAdmissionLimitsV1, DraftMarkerAdmissionNodeKeyV1, DraftMarkerAdmissionNodeKindV1,
    DraftMarkerAdmissionNodePayloadV1, DraftMarkerAdmissionNodeV1, DraftMarkerAdmissionOwnerV1,
    DraftMarkerAdmissionPageIdentityV1, DraftMarkerAdmissionReceiptTransitionV1,
    DraftMarkerAdmissionReplayReceiptPartsV1, DraftMarkerAdmissionReplayReceiptV1,
    DraftMarkerAdmissionRetainedChargeV1, DraftMarkerAdmissionRootV1,
    DraftMarkerAdmissionSchemaErrorV1, DraftMarkerAdmissionSourceKeyV1,
    DraftMarkerAdmissionTargetDispositionV1, DraftMarkerAdmissionTreeV1,
    DraftMarkerLabelAllocationRangeV1,
};

impl DraftMarkerAdmissionCapacityV1 {
    pub fn new(
        revision: NonZeroU64,
        charge: DraftMarkerAdmissionRetainedChargeV1,
    ) -> Result<Self, DraftMarkerAdmissionSchemaErrorV1> {
        if !charge.fits(DraftMarkerAdmissionLimitsV1::PRODUCTION) {
            return Err(DraftMarkerAdmissionSchemaErrorV1::CapacityExceeded);
        }
        let digest = super::codec::capacity_digest(
            revision,
            charge,
            DraftMarkerAdmissionLimitsV1::PRODUCTION,
        );
        Ok(Self::from_parts(
            revision,
            charge,
            DraftMarkerAdmissionLimitsV1::PRODUCTION,
            digest,
        ))
    }

    pub(crate) fn validate(&self) -> Result<(), DraftMarkerAdmissionSchemaErrorV1> {
        if self.limits() != DraftMarkerAdmissionLimitsV1::PRODUCTION
            || !self.charge().fits(self.limits())
        {
            return Err(DraftMarkerAdmissionSchemaErrorV1::CapacityExceeded);
        }
        if self.digest()
            != super::codec::capacity_digest(self.revision(), self.charge(), self.limits())
        {
            return Err(DraftMarkerAdmissionSchemaErrorV1::DigestMismatch);
        }
        Ok(())
    }
}

pub fn canonical_empty_draft_marker_admission_root_v1(
    tree: DraftMarkerAdmissionTreeV1,
) -> DraftMarkerAdmissionRootV1 {
    DraftMarkerAdmissionRootV1::from_parts(tree, None, 0, super::codec::empty_root_digest(tree), 0)
}

impl DraftMarkerAdmissionRootV1 {
    pub fn new(
        tree: DraftMarkerAdmissionTreeV1,
        node: DraftMarkerAdmissionNodeKeyV1,
        height: u8,
        digest: DraftMarkerAdmissionDigestV1,
        count: u64,
    ) -> Result<Self, DraftMarkerAdmissionSchemaErrorV1> {
        if height == 0
            || height > DRAFT_MARKER_ADMISSION_TREE_MAX_HEIGHT
            || count == 0
            || count > DRAFT_MARKER_ADMISSION_MAX_ASSOCIATIONS
            || (height == 1 && node.kind() != DraftMarkerAdmissionNodeKindV1::Leaf)
            || (height > 1 && node.kind() != DraftMarkerAdmissionNodeKindV1::Internal)
        {
            return Err(DraftMarkerAdmissionSchemaErrorV1::InvalidRoot);
        }
        Ok(Self::from_parts(tree, Some(node), height, digest, count))
    }

    pub(crate) fn validate_shape(self) -> Result<(), DraftMarkerAdmissionSchemaErrorV1> {
        match self.node() {
            None if self.height() == 0
                && self.count() == 0
                && self.digest() == super::codec::empty_root_digest(self.tree()) =>
            {
                Ok(())
            }
            Some(node)
                if self.height() > 0
                    && self.height() <= DRAFT_MARKER_ADMISSION_TREE_MAX_HEIGHT
                    && self.count() > 0
                    && self.count() <= DRAFT_MARKER_ADMISSION_MAX_ASSOCIATIONS
                    && ((self.height() == 1
                        && node.kind() == DraftMarkerAdmissionNodeKindV1::Leaf)
                        || (self.height() > 1
                            && node.kind() == DraftMarkerAdmissionNodeKindV1::Internal)) =>
            {
                Ok(())
            }
            _ => Err(DraftMarkerAdmissionSchemaErrorV1::InvalidRoot),
        }
    }
}

impl DraftMarkerAdmissionNodeV1 {
    pub fn source_leaf(
        key: DraftMarkerAdmissionNodeKeyV1,
        source_key: DraftMarkerAdmissionSourceKeyV1,
        evidence: DraftMarkerAdmissionEvidenceV1,
        asset_id: AssetId,
    ) -> Result<Self, DraftMarkerAdmissionSchemaErrorV1> {
        if key.kind() != DraftMarkerAdmissionNodeKindV1::Leaf {
            return Err(DraftMarkerAdmissionSchemaErrorV1::InvalidTree);
        }
        let payload = DraftMarkerAdmissionNodePayloadV1::SourceLeaf {
            source_key,
            evidence,
            asset_id,
        };
        Self::finish(key, DraftMarkerAdmissionTreeV1::SourceOrder, payload)
    }

    pub fn target_leaf(
        key: DraftMarkerAdmissionNodeKeyV1,
        target_marker_id: SyndicDraftMarkerId,
        page: DraftMarkerAdmissionPageIdentityV1,
        evidence: DraftMarkerAdmissionEvidenceV1,
        source_label: ImageLabelOrdinal,
        asset_id: AssetId,
        disposition: DraftMarkerAdmissionTargetDispositionV1,
    ) -> Result<Self, DraftMarkerAdmissionSchemaErrorV1> {
        if key.kind() != DraftMarkerAdmissionNodeKindV1::Leaf {
            return Err(DraftMarkerAdmissionSchemaErrorV1::InvalidTree);
        }
        let payload = DraftMarkerAdmissionNodePayloadV1::TargetLeaf {
            target_marker_id,
            page,
            evidence,
            source_label,
            asset_id,
            disposition,
        };
        Self::finish(key, DraftMarkerAdmissionTreeV1::TargetId, payload)
    }

    pub fn internal(
        key: DraftMarkerAdmissionNodeKeyV1,
        tree: DraftMarkerAdmissionTreeV1,
        height: u8,
        children: impl Into<Box<[DraftMarkerAdmissionChildV1]>>,
    ) -> Result<Self, DraftMarkerAdmissionSchemaErrorV1> {
        let children = children.into();
        if height < 2 || height > DRAFT_MARKER_ADMISSION_TREE_MAX_HEIGHT {
            return Err(DraftMarkerAdmissionSchemaErrorV1::TreeHeight);
        }
        if key.kind() != DraftMarkerAdmissionNodeKindV1::Internal
            || children.is_empty()
            || children.len() > DRAFT_MARKER_ADMISSION_TREE_FANOUT
        {
            return Err(DraftMarkerAdmissionSchemaErrorV1::NodeFanout);
        }
        validate_children(key.owner(), tree, &children)?;
        let payload = DraftMarkerAdmissionNodePayloadV1::Internal { height, children };
        Self::finish(key, tree, payload)
    }

    fn finish(
        key: DraftMarkerAdmissionNodeKeyV1,
        tree: DraftMarkerAdmissionTreeV1,
        payload: DraftMarkerAdmissionNodePayloadV1,
    ) -> Result<Self, DraftMarkerAdmissionSchemaErrorV1> {
        let digest = super::codec::node_digest(key, tree, &payload)?;
        let node = Self::from_parts(key, tree, payload, digest);
        node.validate()?;
        Ok(node)
    }

    pub fn height(&self) -> u8 {
        match self.payload() {
            DraftMarkerAdmissionNodePayloadV1::Internal { height, .. } => *height,
            DraftMarkerAdmissionNodePayloadV1::SourceLeaf { .. }
            | DraftMarkerAdmissionNodePayloadV1::TargetLeaf { .. } => 1,
        }
    }

    pub fn count(&self) -> Result<u64, DraftMarkerAdmissionSchemaErrorV1> {
        match self.payload() {
            DraftMarkerAdmissionNodePayloadV1::Internal { children, .. } => children
                .iter()
                .try_fold(0_u64, |sum, child| sum.checked_add(child.count()))
                .ok_or(DraftMarkerAdmissionSchemaErrorV1::ArithmeticOverflow),
            DraftMarkerAdmissionNodePayloadV1::SourceLeaf { .. }
            | DraftMarkerAdmissionNodePayloadV1::TargetLeaf { .. } => Ok(1),
        }
    }

    pub fn envelope(
        &self,
    ) -> Result<DraftMarkerAdmissionEnvelopeV1, DraftMarkerAdmissionSchemaErrorV1> {
        match self.payload() {
            DraftMarkerAdmissionNodePayloadV1::Internal { children, .. } => {
                let first = children
                    .first()
                    .ok_or(DraftMarkerAdmissionSchemaErrorV1::NodeFanout)?
                    .envelope();
                let last = children
                    .last()
                    .ok_or(DraftMarkerAdmissionSchemaErrorV1::NodeFanout)?
                    .envelope();
                merge_envelopes(first, last)
            }
            DraftMarkerAdmissionNodePayloadV1::SourceLeaf { source_key, .. } => {
                Ok(DraftMarkerAdmissionEnvelopeV1::SourceOrder {
                    first: *source_key,
                    last: *source_key,
                })
            }
            DraftMarkerAdmissionNodePayloadV1::TargetLeaf {
                target_marker_id, ..
            } => Ok(DraftMarkerAdmissionEnvelopeV1::TargetId {
                first: *target_marker_id,
                last: *target_marker_id,
            }),
        }
    }

    pub(crate) fn validate(&self) -> Result<(), DraftMarkerAdmissionSchemaErrorV1> {
        match self.payload() {
            DraftMarkerAdmissionNodePayloadV1::Internal { height, children } => {
                if *height < 2 || *height > DRAFT_MARKER_ADMISSION_TREE_MAX_HEIGHT {
                    return Err(DraftMarkerAdmissionSchemaErrorV1::TreeHeight);
                }
                if self.key().kind() != DraftMarkerAdmissionNodeKindV1::Internal
                    || children.is_empty()
                    || children.len() > DRAFT_MARKER_ADMISSION_TREE_FANOUT
                {
                    return Err(DraftMarkerAdmissionSchemaErrorV1::NodeFanout);
                }
                validate_children(self.key().owner(), self.tree(), children)?;
                if self.count()? > DRAFT_MARKER_ADMISSION_MAX_ASSOCIATIONS {
                    return Err(DraftMarkerAdmissionSchemaErrorV1::InvalidCount);
                }
            }
            DraftMarkerAdmissionNodePayloadV1::SourceLeaf { .. }
                if self.key().kind() == DraftMarkerAdmissionNodeKindV1::Leaf
                    && self.tree() == DraftMarkerAdmissionTreeV1::SourceOrder => {}
            DraftMarkerAdmissionNodePayloadV1::TargetLeaf { .. }
                if self.key().kind() == DraftMarkerAdmissionNodeKindV1::Leaf
                    && self.tree() == DraftMarkerAdmissionTreeV1::TargetId => {}
            _ => return Err(DraftMarkerAdmissionSchemaErrorV1::InvalidTree),
        }
        if self.digest() != super::codec::node_digest(self.key(), self.tree(), self.payload())? {
            return Err(DraftMarkerAdmissionSchemaErrorV1::DigestMismatch);
        }
        Ok(())
    }
}

impl DraftMarkerAdmissionHeadV1 {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        owner: DraftMarkerAdmissionOwnerV1,
        revision: NonZeroU64,
        home_generation: NonZeroU64,
        lifecycle: DraftMarkerAdmissionLifecycleV1,
        request_commitment: DraftMarkerAdmissionDigestV1,
        custody_commitment: DraftMarkerAdmissionDigestV1,
        next_page_ordinal: NonZeroU64,
        ingestion_association_cursor: u64,
        evidence_eof: bool,
        selected_receipt: Option<DraftMarkerAdmissionCommandIdV1>,
        source_root: DraftMarkerAdmissionRootV1,
        target_root: DraftMarkerAdmissionRootV1,
        occurrence_commitment: DraftMarkerAdmissionDigestV1,
        unassigned_count: u64,
        assignment_continuation: Option<DraftMarkerAdmissionAssignmentContinuationV1>,
        remaining_builder_count: u64,
        charge: DraftMarkerAdmissionRetainedChargeV1,
        cleanup_cursor: Option<DraftMarkerAdmissionCleanupCursorV1>,
    ) -> Result<Self, DraftMarkerAdmissionSchemaErrorV1> {
        let limits = DraftMarkerAdmissionLimitsV1::PRODUCTION;
        let provisional = DraftMarkerAdmissionHeadPartsV1 {
            owner,
            revision,
            home_generation,
            lifecycle,
            request_commitment,
            custody_commitment,
            next_page_ordinal,
            ingestion_association_cursor,
            evidence_eof,
            selected_receipt,
            source_root,
            target_root,
            occurrence_commitment,
            unassigned_count,
            assignment_continuation,
            remaining_builder_count,
            charge,
            limits,
            cleanup_cursor,
            digest: DraftMarkerAdmissionDigestV1::from_bytes([0; 32]),
        };
        let digest = super::codec::head_digest(&provisional)?;
        let head = Self::from_parts(DraftMarkerAdmissionHeadPartsV1 {
            digest,
            ..provisional
        });
        head.validate()?;
        Ok(head)
    }

    pub(crate) fn validate(&self) -> Result<(), DraftMarkerAdmissionSchemaErrorV1> {
        self.source_root().validate_shape()?;
        self.target_root().validate_shape()?;
        if self.source_root().tree() != DraftMarkerAdmissionTreeV1::SourceOrder
            || self.target_root().tree() != DraftMarkerAdmissionTreeV1::TargetId
            || self.limits() != DraftMarkerAdmissionLimitsV1::PRODUCTION
            || self.charge().heads() != 1
            || !self.charge().fits(self.limits())
            || self.charge().associations() < self.target_root().count()
            || self.unassigned_count() > self.target_root().count()
            || self.remaining_builder_count() > self.target_root().count()
            || self.ingestion_association_cursor() > DRAFT_MARKER_ADMISSION_PAGE_MAX_ASSOCIATIONS
        {
            return Err(DraftMarkerAdmissionSchemaErrorV1::InvalidHead);
        }
        if self.cleanup_cursor().is_some_and(|cursor| {
            cursor
                .after()
                .is_some_and(|key| key.owner() != self.owner())
        }) {
            return Err(DraftMarkerAdmissionSchemaErrorV1::InvalidHead);
        }
        let active_receipt = matches!(
            self.lifecycle(),
            DraftMarkerAdmissionLifecycleV1::Ingesting
                | DraftMarkerAdmissionLifecycleV1::Assigning
                | DraftMarkerAdmissionLifecycleV1::Ready
        );
        if active_receipt != self.selected_receipt().is_some() {
            return Err(DraftMarkerAdmissionSchemaErrorV1::InvalidHead);
        }
        match self.lifecycle() {
            DraftMarkerAdmissionLifecycleV1::Ingesting
                if self.evidence_eof()
                    || self.assignment_continuation().is_some()
                    || self.source_root().count() != self.target_root().count()
                    || self.unassigned_count() != self.target_root().count()
                    || self.remaining_builder_count() != 0 =>
            {
                return Err(DraftMarkerAdmissionSchemaErrorV1::InvalidHead);
            }
            DraftMarkerAdmissionLifecycleV1::Assigning => {
                let continuation = self
                    .assignment_continuation()
                    .ok_or(DraftMarkerAdmissionSchemaErrorV1::InvalidHead)?;
                let occurrence_count = self.target_root().count();
                let unassigned_count = self.source_root().count();
                let processed_count = occurrence_count
                    .checked_sub(unassigned_count)
                    .ok_or(DraftMarkerAdmissionSchemaErrorV1::InvalidHead)?;
                let reservation_count = continuation
                    .allocation_range()
                    .map(DraftMarkerLabelAllocationRangeV1::count);
                let allocated_count = match (
                    continuation.allocation_range(),
                    continuation.next_allocation(),
                    continuation.prior_source(),
                ) {
                    (Some(range), Some(next), prior) => next
                        .get()
                        .checked_sub(range.first().get())
                        .and_then(|count| count.checked_add(u64::from(prior.is_some()))),
                    (None, None, _) => Some(0),
                    _ => None,
                }
                .ok_or(DraftMarkerAdmissionSchemaErrorV1::InvalidHead)?;
                if !self.evidence_eof()
                    || self.ingestion_association_cursor() != 0
                    || (occurrence_count != 0 && unassigned_count == 0)
                    || self.unassigned_count() != unassigned_count
                    || self.remaining_builder_count() != 0
                    || reservation_count.is_some_and(|count| count != occurrence_count)
                    || allocated_count > processed_count
                    || (processed_count == 0
                        && (continuation.prior_source().is_some() || allocated_count != 0))
                    || (processed_count != 0 && continuation.prior_source().is_none())
                {
                    return Err(DraftMarkerAdmissionSchemaErrorV1::InvalidHead);
                }
            }
            DraftMarkerAdmissionLifecycleV1::Ready
            | DraftMarkerAdmissionLifecycleV1::Staging
            | DraftMarkerAdmissionLifecycleV1::Building
                if !self.evidence_eof()
                    || self.ingestion_association_cursor() != 0
                    || self.assignment_continuation().is_some()
                    || self.source_root().count() != 0
                    || self.unassigned_count() != 0
                    || self.remaining_builder_count() != self.target_root().count()
                    || self.charge().associations() != self.target_root().count()
                    || self.cleanup_cursor().is_some() =>
            {
                return Err(DraftMarkerAdmissionSchemaErrorV1::InvalidHead);
            }
            DraftMarkerAdmissionLifecycleV1::TerminalCleanup
                if self.cleanup_cursor().is_none()
                    || self.ingestion_association_cursor() != 0
                    || self.assignment_continuation().is_some() =>
            {
                return Err(DraftMarkerAdmissionSchemaErrorV1::InvalidHead);
            }
            DraftMarkerAdmissionLifecycleV1::Settled
                if self.source_root().count() != 0
                    || self.target_root().count() != 0
                    || self.charge().associations() != 0
                    || self.unassigned_count() != 0
                    || self.remaining_builder_count() != 0
                    || self.ingestion_association_cursor() != 0
                    || self.assignment_continuation().is_some()
                    || self.cleanup_cursor().is_some() =>
            {
                return Err(DraftMarkerAdmissionSchemaErrorV1::InvalidHead);
            }
            _ => {}
        }
        let parts = DraftMarkerAdmissionHeadPartsV1 {
            owner: self.owner(),
            revision: self.revision(),
            home_generation: self.home_generation(),
            lifecycle: self.lifecycle(),
            request_commitment: self.request_commitment(),
            custody_commitment: self.custody_commitment(),
            next_page_ordinal: self.next_page_ordinal(),
            ingestion_association_cursor: self.ingestion_association_cursor(),
            evidence_eof: self.evidence_eof(),
            selected_receipt: self.selected_receipt(),
            source_root: self.source_root(),
            target_root: self.target_root(),
            occurrence_commitment: self.occurrence_commitment(),
            unassigned_count: self.unassigned_count(),
            assignment_continuation: self.assignment_continuation(),
            remaining_builder_count: self.remaining_builder_count(),
            charge: self.charge(),
            limits: self.limits(),
            cleanup_cursor: self.cleanup_cursor(),
            digest: self.digest(),
        };
        if self.digest() != super::codec::head_digest(&parts)? {
            return Err(DraftMarkerAdmissionSchemaErrorV1::DigestMismatch);
        }
        Ok(())
    }
}

impl DraftMarkerAdmissionReplayReceiptV1 {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        owner: DraftMarkerAdmissionOwnerV1,
        command_id: DraftMarkerAdmissionCommandIdV1,
        page_ordinal: NonZeroU64,
        request_commitment: DraftMarkerAdmissionDigestV1,
        source_head_bytes: impl Into<Box<[u8]>>,
        target_head_bytes: impl Into<Box<[u8]>>,
        source_before: DraftMarkerAdmissionRootV1,
        source_after: DraftMarkerAdmissionRootV1,
        target_before: DraftMarkerAdmissionRootV1,
        target_after: DraftMarkerAdmissionRootV1,
        retained_predecessor_nodes: impl Into<Box<[DraftMarkerAdmissionChildV1]>>,
        transition: DraftMarkerAdmissionReceiptTransitionV1,
    ) -> Result<Self, DraftMarkerAdmissionSchemaErrorV1> {
        let mut parts = DraftMarkerAdmissionReplayReceiptPartsV1 {
            owner,
            command_id,
            page_ordinal,
            request_commitment,
            source_head_bytes: source_head_bytes.into(),
            target_head_bytes: target_head_bytes.into(),
            source_before,
            source_after,
            target_before,
            target_after,
            retained_predecessor_nodes: retained_predecessor_nodes.into(),
            transition,
            digest: DraftMarkerAdmissionDigestV1::from_bytes([0; 32]),
        };
        parts.digest = super::codec::receipt_digest(&parts)?;
        let value = Self::from_parts(parts);
        value.validate()?;
        Ok(value)
    }

    pub(crate) fn validate(&self) -> Result<(), DraftMarkerAdmissionSchemaErrorV1> {
        if self.source_head_bytes().is_empty()
            || self.target_head_bytes().is_empty()
            || self.retained_predecessor_nodes().len()
                > usize::from(DRAFT_MARKER_ADMISSION_TREE_MAX_HEIGHT) * 2
        {
            return Err(DraftMarkerAdmissionSchemaErrorV1::InvalidCount);
        }
        for (index, retained) in self.retained_predecessor_nodes().iter().enumerate() {
            if self.retained_predecessor_nodes()[..index]
                .iter()
                .any(|prior| prior.key() == retained.key())
            {
                return Err(DraftMarkerAdmissionSchemaErrorV1::InvalidCount);
            }
        }
        for root in [
            self.source_before(),
            self.source_after(),
            self.target_before(),
            self.target_after(),
        ] {
            root.validate_shape()?;
        }
        if self.source_before().tree() != DraftMarkerAdmissionTreeV1::SourceOrder
            || self.source_after().tree() != DraftMarkerAdmissionTreeV1::SourceOrder
            || self.target_before().tree() != DraftMarkerAdmissionTreeV1::TargetId
            || self.target_after().tree() != DraftMarkerAdmissionTreeV1::TargetId
            || self
                .retained_predecessor_nodes()
                .iter()
                .any(|node| node.key().owner() != self.owner())
        {
            return Err(DraftMarkerAdmissionSchemaErrorV1::InvalidTree);
        }
        let parts = DraftMarkerAdmissionReplayReceiptPartsV1 {
            owner: self.owner(),
            command_id: self.command_id(),
            page_ordinal: self.page_ordinal(),
            request_commitment: self.request_commitment(),
            source_head_bytes: self.source_head_bytes().into(),
            target_head_bytes: self.target_head_bytes().into(),
            source_before: self.source_before(),
            source_after: self.source_after(),
            target_before: self.target_before(),
            target_after: self.target_after(),
            retained_predecessor_nodes: self.retained_predecessor_nodes().into(),
            transition: self.transition(),
            digest: self.digest(),
        };
        if self.digest() != super::codec::receipt_digest(&parts)? {
            return Err(DraftMarkerAdmissionSchemaErrorV1::DigestMismatch);
        }
        Ok(())
    }
}

pub fn checked_draft_marker_admission_command_charge_v1(
    charges: impl IntoIterator<Item = u64>,
) -> Result<u64, DraftMarkerAdmissionSchemaErrorV1> {
    let total = charges
        .into_iter()
        .try_fold(0_u64, |sum, charge| sum.checked_add(charge))
        .ok_or(DraftMarkerAdmissionSchemaErrorV1::ArithmeticOverflow)?;
    if total > DRAFT_MARKER_ADMISSION_COMMAND_MAX_ENCODED_BYTES {
        return Err(DraftMarkerAdmissionSchemaErrorV1::CommandTooLarge);
    }
    Ok(total)
}

pub fn draft_marker_admission_head_encoded_charge_v1(
    head: &DraftMarkerAdmissionHeadV1,
) -> Result<u64, DraftMarkerAdmissionSchemaErrorV1> {
    super::codec::encoded_head_record_charge(&head.owner(), head)
}

pub fn draft_marker_admission_node_encoded_charge_v1(
    node: &DraftMarkerAdmissionNodeV1,
) -> Result<u64, DraftMarkerAdmissionSchemaErrorV1> {
    super::codec::encoded_node_record_charge(&node.key(), node)
}

pub fn draft_marker_admission_receipt_encoded_charge_v1(
    receipt: &DraftMarkerAdmissionReplayReceiptV1,
) -> Result<u64, DraftMarkerAdmissionSchemaErrorV1> {
    super::codec::encoded_receipt_record_charge(
        &super::DraftMarkerAdmissionReceiptKeyV1::new(receipt.owner(), receipt.command_id()),
        receipt,
    )
}

pub fn checked_draft_marker_admission_capacity_successor_v1(
    current: DraftMarkerAdmissionRetainedChargeV1,
    prior: DraftMarkerAdmissionRetainedChargeV1,
    successor: DraftMarkerAdmissionRetainedChargeV1,
) -> Result<DraftMarkerAdmissionRetainedChargeV1, DraftMarkerAdmissionSchemaErrorV1> {
    let next = current
        .checked_sub(prior)
        .and_then(|value| value.checked_add(successor))
        .ok_or(DraftMarkerAdmissionSchemaErrorV1::ArithmeticOverflow)?;
    if !next.fits(DraftMarkerAdmissionLimitsV1::PRODUCTION) {
        return Err(DraftMarkerAdmissionSchemaErrorV1::CapacityExceeded);
    }
    Ok(next)
}

fn validate_children(
    owner: DraftMarkerAdmissionOwnerV1,
    tree: DraftMarkerAdmissionTreeV1,
    children: &[DraftMarkerAdmissionChildV1],
) -> Result<(), DraftMarkerAdmissionSchemaErrorV1> {
    let mut previous = None;
    for child in children {
        if child.count() == 0 || child.count() > DRAFT_MARKER_ADMISSION_MAX_ASSOCIATIONS {
            return Err(DraftMarkerAdmissionSchemaErrorV1::InvalidCount);
        }
        if child.key().owner() != owner
            || envelope_tree(child.envelope()) != tree
            || !envelope_is_valid(child.envelope())
        {
            return Err(DraftMarkerAdmissionSchemaErrorV1::InvalidEnvelope);
        }
        if let Some(previous) = previous {
            if !envelopes_disjoint(previous, child.envelope()) {
                return Err(DraftMarkerAdmissionSchemaErrorV1::InvalidEnvelope);
            }
        }
        previous = Some(child.envelope());
    }
    Ok(())
}

fn envelope_is_valid(envelope: DraftMarkerAdmissionEnvelopeV1) -> bool {
    match envelope {
        DraftMarkerAdmissionEnvelopeV1::SourceOrder { first, last } => {
            first == last || source_key_less(first, last)
        }
        DraftMarkerAdmissionEnvelopeV1::TargetId { first, last } => first <= last,
    }
}

fn envelope_tree(envelope: DraftMarkerAdmissionEnvelopeV1) -> DraftMarkerAdmissionTreeV1 {
    match envelope {
        DraftMarkerAdmissionEnvelopeV1::SourceOrder { .. } => {
            DraftMarkerAdmissionTreeV1::SourceOrder
        }
        DraftMarkerAdmissionEnvelopeV1::TargetId { .. } => DraftMarkerAdmissionTreeV1::TargetId,
    }
}

pub(crate) fn source_key_less(
    a: DraftMarkerAdmissionSourceKeyV1,
    b: DraftMarkerAdmissionSourceKeyV1,
) -> bool {
    (a.source_label().get(), a.target_marker_id()) < (b.source_label().get(), b.target_marker_id())
}

fn envelopes_disjoint(
    left: DraftMarkerAdmissionEnvelopeV1,
    right: DraftMarkerAdmissionEnvelopeV1,
) -> bool {
    match (left, right) {
        (
            DraftMarkerAdmissionEnvelopeV1::SourceOrder {
                first: left_first,
                last: left_last,
            },
            DraftMarkerAdmissionEnvelopeV1::SourceOrder {
                first: right_first,
                last: right_last,
            },
        ) => {
            (source_key_less(left_first, left_last) || left_first == left_last)
                && (source_key_less(right_first, right_last) || right_first == right_last)
                && source_key_less(left_last, right_first)
        }
        (
            DraftMarkerAdmissionEnvelopeV1::TargetId {
                first: left_first,
                last: left_last,
            },
            DraftMarkerAdmissionEnvelopeV1::TargetId {
                first: right_first,
                last: right_last,
            },
        ) => left_first <= left_last && right_first <= right_last && left_last < right_first,
        _ => false,
    }
}

fn merge_envelopes(
    first: DraftMarkerAdmissionEnvelopeV1,
    last: DraftMarkerAdmissionEnvelopeV1,
) -> Result<DraftMarkerAdmissionEnvelopeV1, DraftMarkerAdmissionSchemaErrorV1> {
    match (first, last) {
        (
            DraftMarkerAdmissionEnvelopeV1::SourceOrder { first, .. },
            DraftMarkerAdmissionEnvelopeV1::SourceOrder { last, .. },
        ) => Ok(DraftMarkerAdmissionEnvelopeV1::SourceOrder { first, last }),
        (
            DraftMarkerAdmissionEnvelopeV1::TargetId { first, .. },
            DraftMarkerAdmissionEnvelopeV1::TargetId { last, .. },
        ) => Ok(DraftMarkerAdmissionEnvelopeV1::TargetId { first, last }),
        _ => Err(DraftMarkerAdmissionSchemaErrorV1::InvalidEnvelope),
    }
}
