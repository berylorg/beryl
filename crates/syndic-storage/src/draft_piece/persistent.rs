use std::collections::BTreeMap;

use beryl_home_store::HomeStore;
use beryl_model::{AssetId, ImageLabelOrdinal, SyndicDraftId, SyndicDraftMarkerId};
use sha2::{Digest, Sha256};

use crate::{SyndicStorage, draft_piece::*};

mod marker_identity_lookup;

pub(crate) use marker_identity_lookup::{
    SnapshotMarkerLookupErrorV1, marker_identity_lookup, marker_identity_lookup_on_snapshot,
};

const INDEX_LEAF: &[u8] = b"syndic/draft-marker-identity-index-leaf/v2";
const INDEX_NODE: &[u8] = b"syndic/draft-marker-identity-index-node/v1";
const INDEX_ROOT: &[u8] = b"syndic/draft-marker-identity-index-root/v1";

#[derive(Clone, Copy)]
struct SequenceRef {
    link: DraftPieceChildV1,
    height: u8,
    selected_root: bool,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct Boundary {
    rank: u64,
    inner: usize,
}

#[derive(Clone, Copy)]
struct LocatedLeaf {
    rank: u64,
    anchor: u64,
    link: DraftPieceChildV1,
}

#[derive(Clone, Copy)]
pub(super) struct IndexRef {
    pub(super) link: DraftMarkerIdentityChildV1,
    pub(super) height: u8,
    pub(super) selected_root: bool,
}

#[derive(Clone, Copy)]
pub(crate) struct MarkerOrderRef {
    pub(super) link: DraftMarkerOrderChildV1,
    pub(super) height: u8,
    pub(super) selected_root: bool,
}

struct BuildContext<'a> {
    storage: &'a SyndicStorage,
    store: &'a HomeStore,
    draft_id: SyndicDraftId,
    session_id: Option<DraftEditorCandidateSessionIdV1>,
    operation_id: DraftPieceOperationIdV1,
    ordinal: u64,
    sequence_nodes: BTreeMap<DraftPieceRecordIdV1, DraftPieceNodeRecordV1>,
    sequence_leaves: BTreeMap<DraftPieceRecordIdV1, DraftPieceLeafRecordV1>,
    index_records: BTreeMap<DraftMarkerIdentityRecordKeyV1, DraftMarkerIdentityRecordV1>,
    marker_order_records: BTreeMap<DraftMarkerOrderRecordKeyV1, DraftMarkerOrderRecordV1>,
    loaded_sequence_nodes: BTreeMap<DraftPieceRecordIdV1, DraftPieceNodeRecordV1>,
    loaded_sequence_leaves: BTreeMap<DraftPieceRecordIdV1, DraftPieceLeafRecordV1>,
    loaded_index_records: BTreeMap<DraftMarkerIdentityRecordKeyV1, DraftMarkerIdentityRecordV1>,
    loaded_marker_order_records: BTreeMap<DraftMarkerOrderRecordKeyV1, DraftMarkerOrderRecordV1>,
    records_read: u64,
}

impl<'a> BuildContext<'a> {
    fn new(
        storage: &'a SyndicStorage,
        store: &'a HomeStore,
        draft_id: SyndicDraftId,
        session_id: Option<DraftEditorCandidateSessionIdV1>,
        operation_id: DraftPieceOperationIdV1,
    ) -> Self {
        Self::with_ordinal(storage, store, draft_id, session_id, operation_id, 1)
    }

    fn with_ordinal(
        storage: &'a SyndicStorage,
        store: &'a HomeStore,
        draft_id: SyndicDraftId,
        session_id: Option<DraftEditorCandidateSessionIdV1>,
        operation_id: DraftPieceOperationIdV1,
        ordinal: u64,
    ) -> Self {
        Self {
            storage,
            store,
            draft_id,
            session_id,
            operation_id,
            ordinal,
            sequence_nodes: BTreeMap::new(),
            sequence_leaves: BTreeMap::new(),
            index_records: BTreeMap::new(),
            marker_order_records: BTreeMap::new(),
            loaded_sequence_nodes: BTreeMap::new(),
            loaded_sequence_leaves: BTreeMap::new(),
            loaded_index_records: BTreeMap::new(),
            loaded_marker_order_records: BTreeMap::new(),
            records_read: 0,
        }
    }

    fn next_id(
        &mut self,
        digest: DraftPieceDigestV1,
    ) -> Result<DraftPieceRecordIdV1, DraftPiecePrepareErrorV1> {
        let session_id = self
            .session_id
            .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
        let id = record_id(
            self.draft_id,
            session_id,
            self.operation_id,
            self.ordinal,
            digest,
        );
        self.ordinal = self
            .ordinal
            .checked_add(1)
            .ok_or(DraftPiecePrepareErrorV1::Rejected(
                DraftPieceRejectedReasonV1::AggregateOverflow,
            ))?;
        Ok(id)
    }

    fn load_sequence_node(
        &mut self,
        expected: DraftPieceChildV1,
        height: u8,
        selected_root: bool,
    ) -> Result<DraftPieceNodeRecordV1, DraftPiecePrepareErrorV1> {
        let id = expected.id();
        if let Some(node) = self.sequence_nodes.get(&id) {
            return validate_sequence_node(node.clone(), expected, height, true);
        }
        if let Some(node) = self.loaded_sequence_nodes.get(&id) {
            return validate_sequence_node(node.clone(), expected, height, selected_root);
        }
        let key = DraftPieceRecordKeyV1::new(self.draft_id, id);
        let node = self
            .storage
            .point::<DraftPieceNodesFamily>(self.store, key, point_limit())?
            .ok_or(DraftPiecePrepareErrorV1::Absent)?;
        self.records_read =
            self.records_read
                .checked_add(1)
                .ok_or(DraftPiecePrepareErrorV1::Rejected(
                    DraftPieceRejectedReasonV1::AggregateOverflow,
                ))?;
        let node = validate_sequence_node(node, expected, height, selected_root)?;
        self.loaded_sequence_nodes.insert(id, node.clone());
        Ok(node)
    }

    fn load_sequence_root_node(
        &mut self,
        id: DraftPieceRecordIdV1,
        summary: DraftPieceSummaryV1,
    ) -> Result<DraftPieceNodeRecordV1, DraftPiecePrepareErrorV1> {
        if let Some(node) = self.sequence_nodes.get(&id) {
            return validate_sequence_root_node(node.clone(), summary);
        }
        if let Some(node) = self.loaded_sequence_nodes.get(&id) {
            return validate_sequence_root_node(node.clone(), summary);
        }
        let key = DraftPieceRecordKeyV1::new(self.draft_id, id);
        let node = self
            .storage
            .point::<DraftPieceNodesFamily>(self.store, key, point_limit())?
            .ok_or(DraftPiecePrepareErrorV1::Absent)?;
        self.records_read =
            self.records_read
                .checked_add(1)
                .ok_or(DraftPiecePrepareErrorV1::Rejected(
                    DraftPieceRejectedReasonV1::AggregateOverflow,
                ))?;
        if node.key() != key {
            return Err(DraftPiecePrepareErrorV1::InvalidRoot);
        }
        let node = validate_sequence_root_node(node, summary)?;
        self.loaded_sequence_nodes.insert(id, node.clone());
        Ok(node)
    }

    fn load_sequence_leaf(
        &mut self,
        expected: DraftPieceChildV1,
    ) -> Result<DraftPieceLeafRecordV1, DraftPiecePrepareErrorV1> {
        let id = expected.id();
        if let Some(leaf) = self.sequence_leaves.get(&id) {
            return validate_sequence_leaf(leaf.clone(), expected);
        }
        if let Some(leaf) = self.loaded_sequence_leaves.get(&id) {
            return validate_sequence_leaf(leaf.clone(), expected);
        }
        let key = DraftPieceRecordKeyV1::new(self.draft_id, id);
        let leaf = self
            .storage
            .point::<DraftPieceLeavesFamily>(self.store, key, point_limit())?
            .ok_or(DraftPiecePrepareErrorV1::Absent)?;
        self.records_read =
            self.records_read
                .checked_add(1)
                .ok_or(DraftPiecePrepareErrorV1::Rejected(
                    DraftPieceRejectedReasonV1::AggregateOverflow,
                ))?;
        let leaf = validate_sequence_leaf(leaf, expected)?;
        self.loaded_sequence_leaves.insert(id, leaf.clone());
        Ok(leaf)
    }

    fn new_sequence_leaf(
        &mut self,
        value: DraftPieceLeafValueV1,
    ) -> Result<SequenceRef, DraftPiecePrepareErrorV1> {
        let text_summary = match &value {
            DraftPieceLeafValueV1::Text(text) => DraftPieceTextSummaryV1::from_utf8(text),
            DraftPieceLeafValueV1::Marker(_) => DraftPieceTextSummaryV1::empty(),
        };
        let digest = leaf_digest(&value, text_summary);
        let id = self.next_id(digest)?;
        let record = DraftPieceLeafRecordV1::new(
            DraftPieceRecordKeyV1::new(self.draft_id, id),
            value,
            text_summary,
            digest,
        );
        let link = child_for_leaf(&record);
        self.sequence_leaves.insert(id, record);
        Ok(SequenceRef {
            link,
            height: 0,
            selected_root: true,
        })
    }

    fn new_sequence_node(
        &mut self,
        height: u8,
        children: Vec<DraftPieceChildV1>,
    ) -> Result<SequenceRef, DraftPiecePrepareErrorV1> {
        if height == 0
            || height > DRAFT_PIECE_MAX_HEIGHT
            || children.is_empty()
            || children.len() > DRAFT_PIECE_MAX_CHILDREN
        {
            return Err(DraftPiecePrepareErrorV1::Rejected(
                DraftPieceRejectedReasonV1::TreeLimit,
            ));
        }
        let digest = node_digest(height, &children);
        let id = self.next_id(digest)?;
        let record = DraftPieceNodeRecordV1::new(
            DraftPieceRecordKeyV1::new(self.draft_id, id),
            height,
            children,
            digest,
        );
        let link = child_for_node(&record).map_err(DraftPiecePrepareErrorV1::Rejected)?;
        self.sequence_nodes.insert(id, record);
        Ok(SequenceRef {
            link,
            height,
            selected_root: true,
        })
    }

    fn load_index_record(
        &mut self,
        expected: IndexRef,
        selected_root: bool,
    ) -> Result<DraftMarkerIdentityRecordV1, DraftPiecePrepareErrorV1> {
        let kind = if expected.height == 0 {
            DraftMarkerIdentityRecordKindV1::Leaf
        } else {
            DraftMarkerIdentityRecordKindV1::Internal
        };
        let key = DraftMarkerIdentityRecordKeyV1::new(self.draft_id, kind, expected.link.id());
        if let Some(record) = self.index_records.get(&key) {
            return validate_index_record(record.clone(), expected, true);
        }
        if let Some(record) = self.loaded_index_records.get(&key) {
            return validate_index_record(record.clone(), expected, selected_root);
        }
        let record = self
            .storage
            .point::<DraftMarkerIdentityIndexFamily>(self.store, key, point_limit())?
            .ok_or(DraftPiecePrepareErrorV1::Absent)?;
        self.records_read =
            self.records_read
                .checked_add(1)
                .ok_or(DraftPiecePrepareErrorV1::Rejected(
                    DraftPieceRejectedReasonV1::AggregateOverflow,
                ))?;
        let record = validate_index_record(record, expected, selected_root)?;
        self.loaded_index_records.insert(key, record.clone());
        Ok(record)
    }

    fn load_index_root_record(
        &mut self,
        id: DraftPieceRecordIdV1,
        summary: DraftMarkerIdentityIndexSummaryV1,
    ) -> Result<DraftMarkerIdentityRecordV1, DraftPiecePrepareErrorV1> {
        let kind = if summary.height() == 0 {
            DraftMarkerIdentityRecordKindV1::Leaf
        } else {
            DraftMarkerIdentityRecordKindV1::Internal
        };
        let key = DraftMarkerIdentityRecordKeyV1::new(self.draft_id, kind, id);
        if let Some(record) = self.index_records.get(&key) {
            return validate_index_root_record(record.clone(), summary);
        }
        if let Some(record) = self.loaded_index_records.get(&key) {
            return validate_index_root_record(record.clone(), summary);
        }
        let record = self
            .storage
            .point::<DraftMarkerIdentityIndexFamily>(self.store, key, point_limit())?
            .ok_or(DraftPiecePrepareErrorV1::Absent)?;
        self.records_read =
            self.records_read
                .checked_add(1)
                .ok_or(DraftPiecePrepareErrorV1::Rejected(
                    DraftPieceRejectedReasonV1::AggregateOverflow,
                ))?;
        if record.key() != key {
            return Err(DraftPiecePrepareErrorV1::InvalidRoot);
        }
        let record = validate_index_root_record(record, summary)?;
        self.loaded_index_records.insert(key, record.clone());
        Ok(record)
    }

    fn new_index_leaf(
        &mut self,
        occurrence: DraftMarkerIdentityOccurrenceV1,
    ) -> Result<IndexRef, DraftPiecePrepareErrorV1> {
        let digest = index_leaf_digest(occurrence);
        let id = self.next_id(digest)?;
        let key = DraftMarkerIdentityRecordKeyV1::new(
            self.draft_id,
            DraftMarkerIdentityRecordKindV1::Leaf,
            id,
        );
        let record = DraftMarkerIdentityRecordV1::Leaf {
            key,
            occurrence,
            digest,
        };
        self.index_records.insert(key, record);
        Ok(IndexRef {
            link: DraftMarkerIdentityChildV1::new(
                id,
                digest,
                1,
                occurrence.marker_id(),
                occurrence.marker_id(),
            ),
            height: 0,
            selected_root: true,
        })
    }

    fn new_index_node(
        &mut self,
        height: u8,
        children: Vec<DraftMarkerIdentityChildV1>,
    ) -> Result<IndexRef, DraftPiecePrepareErrorV1> {
        if height == 0
            || height > DRAFT_PIECE_MAX_HEIGHT
            || children.is_empty()
            || children.len() > DRAFT_PIECE_MAX_CHILDREN
        {
            return Err(DraftPiecePrepareErrorV1::Rejected(
                DraftPieceRejectedReasonV1::TreeLimit,
            ));
        }
        validate_index_children(&children)?;
        let digest = index_node_digest(height, &children);
        let id = self.next_id(digest)?;
        let key = DraftMarkerIdentityRecordKeyV1::new(
            self.draft_id,
            DraftMarkerIdentityRecordKindV1::Internal,
            id,
        );
        let record = DraftMarkerIdentityRecordV1::Internal {
            key,
            height,
            children,
            digest,
        };
        let link = index_child_for_record(&record)?;
        self.index_records.insert(key, record);
        Ok(IndexRef {
            link,
            height,
            selected_root: true,
        })
    }

    fn load_marker_order_record(
        &mut self,
        expected: MarkerOrderRef,
        selected_root: bool,
    ) -> Result<DraftMarkerOrderRecordV1, DraftPiecePrepareErrorV1> {
        let kind = if expected.height == 0 {
            DraftMarkerOrderRecordKindV1::Leaf
        } else {
            DraftMarkerOrderRecordKindV1::Internal
        };
        let key = DraftMarkerOrderRecordKeyV1::new(self.draft_id, kind, expected.link.id());
        let record = if let Some(record) = self.marker_order_records.get(&key) {
            record.clone()
        } else if let Some(record) = self.loaded_marker_order_records.get(&key) {
            record.clone()
        } else {
            let record = self
                .storage
                .point::<DraftMarkerOrderCommitmentsFamily>(self.store, key, point_limit())?
                .ok_or(DraftPiecePrepareErrorV1::Absent)?;
            self.records_read =
                self.records_read
                    .checked_add(1)
                    .ok_or(DraftPiecePrepareErrorV1::Rejected(
                        DraftPieceRejectedReasonV1::AggregateOverflow,
                    ))?;
            self.loaded_marker_order_records.insert(key, record.clone());
            record
        };
        validate_marker_order_record(&record, expected, selected_root)?;
        Ok(record)
    }

    fn new_marker_order_leaf(
        &mut self,
        marker_id: SyndicDraftMarkerId,
        label: ImageLabelOrdinal,
        asset_id: AssetId,
    ) -> Result<MarkerOrderRef, DraftPiecePrepareErrorV1> {
        let digest = marker_order_leaf_digest(marker_id, label, asset_id);
        let id = self.next_id(digest)?;
        let key =
            DraftMarkerOrderRecordKeyV1::new(self.draft_id, DraftMarkerOrderRecordKindV1::Leaf, id);
        self.marker_order_records.insert(
            key,
            DraftMarkerOrderRecordV1::Leaf {
                key,
                marker_id,
                label,
                asset_id,
                digest,
            },
        );
        Ok(MarkerOrderRef {
            link: DraftMarkerOrderChildV1::new(id, digest, 1, Some(label))
                .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?,
            height: 0,
            selected_root: true,
        })
    }

    fn new_marker_order_node(
        &mut self,
        height: u8,
        children: Vec<DraftMarkerOrderChildV1>,
    ) -> Result<MarkerOrderRef, DraftPiecePrepareErrorV1> {
        if height == 0
            || height > DRAFT_PIECE_MAX_HEIGHT
            || children.is_empty()
            || children.len() > DRAFT_PIECE_MAX_CHILDREN
        {
            return Err(DraftPiecePrepareErrorV1::Rejected(
                DraftPieceRejectedReasonV1::TreeLimit,
            ));
        }
        let marker_count = children.iter().try_fold(0_u64, |count, child| {
            count
                .checked_add(child.marker_count())
                .ok_or(DraftPiecePrepareErrorV1::Rejected(
                    DraftPieceRejectedReasonV1::AggregateOverflow,
                ))
        })?;
        let maximum = children
            .iter()
            .filter_map(|child| child.maximum_image_label())
            .max()
            .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
        let digest = marker_order_node_digest(height, &children);
        let id = self.next_id(digest)?;
        let key = DraftMarkerOrderRecordKeyV1::new(
            self.draft_id,
            DraftMarkerOrderRecordKindV1::Internal,
            id,
        );
        self.marker_order_records.insert(
            key,
            DraftMarkerOrderRecordV1::Internal {
                key,
                height,
                children,
                digest,
            },
        );
        Ok(MarkerOrderRef {
            link: DraftMarkerOrderChildV1::new(id, digest, marker_count, Some(maximum))
                .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?,
            height,
            selected_root: true,
        })
    }
}

fn validate_sequence_node(
    node: DraftPieceNodeRecordV1,
    expected: DraftPieceChildV1,
    height: u8,
    selected_root: bool,
) -> Result<DraftPieceNodeRecordV1, DraftPiecePrepareErrorV1> {
    let minimum_children = if selected_root { 1 } else { 2 };
    if node.key().id() != expected.id()
        || node.height() != height
        || height == 0
        || node.children().len() < minimum_children
        || node.children().len() > DRAFT_PIECE_MAX_CHILDREN
        || node.digest() != node_digest(height, node.children())
        || child_for_node(&node).map_err(|_| DraftPiecePrepareErrorV1::InvalidRoot)? != expected
    {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    Ok(node)
}

pub(crate) fn validate_sequence_root_node(
    node: DraftPieceNodeRecordV1,
    summary: DraftPieceSummaryV1,
) -> Result<DraftPieceNodeRecordV1, DraftPiecePrepareErrorV1> {
    if node.height() != summary.height()
        || node.height() == 0
        || node.children().is_empty()
        || node.children().len() > DRAFT_PIECE_MAX_CHILDREN
        || node.digest() != node_digest(node.height(), node.children())
    {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    let link = child_for_node(&node).map_err(|_| DraftPiecePrepareErrorV1::InvalidRoot)?;
    let provisional = DraftPieceSummaryV1::new(
        link.logical_utf8_bytes(),
        link.newline_count(),
        link.logical_line_count(),
        link.piece_count(),
        link.marker_count(),
        link.marker_digest(),
        node.height(),
        DraftPieceDigestV1::from_bytes([0; 32]),
    );
    if summary.logical_utf8_bytes() != link.logical_utf8_bytes()
        || summary.newline_count() != link.newline_count()
        || summary.logical_line_count() != link.logical_line_count()
        || summary.piece_count() != link.piece_count()
        || summary.marker_count() != link.marker_count()
        || summary.marker_digest() != link.marker_digest()
        || summary.root_digest() != root_digest(provisional, link.digest())
    {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    Ok(node)
}

fn validate_sequence_leaf(
    leaf: DraftPieceLeafRecordV1,
    expected: DraftPieceChildV1,
) -> Result<DraftPieceLeafRecordV1, DraftPiecePrepareErrorV1> {
    if leaf.key().id() != expected.id()
        || !leaf.text_summary().is_canonical()
        || match leaf.value() {
            DraftPieceLeafValueV1::Text(text) => {
                leaf.text_summary() != DraftPieceTextSummaryV1::from_utf8(text)
            }
            DraftPieceLeafValueV1::Marker(_) => {
                leaf.text_summary() != DraftPieceTextSummaryV1::empty()
            }
        }
        || leaf.digest() != leaf_digest(leaf.value(), leaf.text_summary())
        || child_for_leaf(&leaf) != expected
    {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    Ok(leaf)
}

pub(super) fn validate_index_record(
    record: DraftMarkerIdentityRecordV1,
    expected: IndexRef,
    selected_root: bool,
) -> Result<DraftMarkerIdentityRecordV1, DraftPiecePrepareErrorV1> {
    if record.key().id() != expected.link.id()
        || record.digest() != index_record_digest(&record)
        || record.height() != expected.height
        || index_child_for_record(&record).map_err(|_| DraftPiecePrepareErrorV1::InvalidRoot)?
            != expected.link
    {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    match &record {
        DraftMarkerIdentityRecordV1::Leaf { .. } if expected.height == 0 => {}
        DraftMarkerIdentityRecordV1::Internal { children, .. } if expected.height != 0 => {
            let minimum_children = if selected_root { 1 } else { 2 };
            if children.len() < minimum_children || children.len() > DRAFT_PIECE_MAX_CHILDREN {
                return Err(DraftPiecePrepareErrorV1::InvalidRoot);
            }
        }
        _ => return Err(DraftPiecePrepareErrorV1::InvalidRoot),
    }
    Ok(record)
}

pub(crate) fn validate_index_root_record(
    record: DraftMarkerIdentityRecordV1,
    summary: DraftMarkerIdentityIndexSummaryV1,
) -> Result<DraftMarkerIdentityRecordV1, DraftPiecePrepareErrorV1> {
    if record.height() != summary.height()
        || record.digest() != index_record_digest(&record)
        || matches!(record, DraftMarkerIdentityRecordV1::Leaf { .. })
    {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    let DraftMarkerIdentityRecordV1::Internal { children, .. } = &record else {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    };
    if children.is_empty() || children.len() > DRAFT_PIECE_MAX_CHILDREN {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    let link =
        index_child_for_record(&record).map_err(|_| DraftPiecePrepareErrorV1::InvalidRoot)?;
    if link.record_count() != summary.record_count()
        || summary.root_digest()
            != index_root_digest(link.record_count(), summary.height(), link.digest())
    {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    Ok(record)
}

pub(super) fn validate_marker_order_record(
    record: &DraftMarkerOrderRecordV1,
    expected: MarkerOrderRef,
    selected_root: bool,
) -> Result<(), DraftPiecePrepareErrorV1> {
    if record.key().id() != expected.link.id()
        || record.digest() != expected.link.digest()
        || record.height() != expected.height
    {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    match record {
        DraftMarkerOrderRecordV1::Leaf {
            marker_id,
            label,
            asset_id,
            digest,
            ..
        } => {
            if expected.height != 0
                || expected.link.marker_count() != 1
                || expected.link.maximum_image_label() != Some(*label)
                || *digest != marker_order_leaf_digest(*marker_id, *label, *asset_id)
            {
                return Err(DraftPiecePrepareErrorV1::InvalidRoot);
            }
        }
        DraftMarkerOrderRecordV1::Internal {
            height,
            children,
            digest,
            ..
        } => {
            if *height == 0
                || *height > DRAFT_PIECE_MAX_HEIGHT
                || children.is_empty()
                || children.len() > DRAFT_PIECE_MAX_CHILDREN
                || (!selected_root && children.len() < 2)
                || *digest != marker_order_node_digest(*height, children)
            {
                return Err(DraftPiecePrepareErrorV1::InvalidRoot);
            }
            let count = children
                .iter()
                .try_fold(0_u64, |count, child| {
                    count.checked_add(child.marker_count())
                })
                .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
            let maximum = children
                .iter()
                .filter_map(|child| child.maximum_image_label())
                .max();
            if count != expected.link.marker_count()
                || maximum != expected.link.maximum_image_label()
            {
                return Err(DraftPiecePrepareErrorV1::InvalidRoot);
            }
        }
    }
    Ok(())
}

fn digest_parts(domain: &[u8], parts: &[&[u8]]) -> DraftPieceDigestV1 {
    let mut digest = Sha256::new();
    digest.update((domain.len() as u64).to_be_bytes());
    digest.update(domain);
    for part in parts {
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part);
    }
    DraftPieceDigestV1::from_bytes(digest.finalize().into())
}

fn index_leaf_digest(value: DraftMarkerIdentityOccurrenceV1) -> DraftPieceDigestV1 {
    digest_parts(
        INDEX_LEAF,
        &[
            value.marker_id().as_bytes(),
            &value.label().get().to_be_bytes(),
            &[value.asset_id().version() as u8],
            &value.asset_id().digest(),
            &value.asset_id().length().get().to_be_bytes(),
            &value.order_key().to_be_bytes(),
            value.sequence_leaf_id().as_bytes(),
            value.sequence_leaf_digest().as_bytes(),
        ],
    )
}

pub(crate) fn index_node_digest(
    height: u8,
    children: &[DraftMarkerIdentityChildV1],
) -> DraftPieceDigestV1 {
    let mut bytes = Vec::with_capacity(children.len() * 104 + 9);
    bytes.push(height);
    bytes.extend_from_slice(&(children.len() as u64).to_be_bytes());
    for child in children {
        bytes.extend_from_slice(child.id().as_bytes());
        bytes.extend_from_slice(child.digest().as_bytes());
        bytes.extend_from_slice(&child.record_count().to_be_bytes());
        bytes.extend_from_slice(child.first().as_bytes());
        bytes.extend_from_slice(child.last().as_bytes());
    }
    digest_parts(INDEX_NODE, &[&bytes])
}

fn index_root_digest(
    count: u64,
    height: u8,
    node_digest: DraftPieceDigestV1,
) -> DraftPieceDigestV1 {
    digest_parts(
        INDEX_ROOT,
        &[&count.to_be_bytes(), &[height], node_digest.as_bytes()],
    )
}

fn index_record_digest(record: &DraftMarkerIdentityRecordV1) -> DraftPieceDigestV1 {
    match record {
        DraftMarkerIdentityRecordV1::Internal {
            height, children, ..
        } => index_node_digest(*height, children),
        DraftMarkerIdentityRecordV1::Leaf { occurrence, .. } => index_leaf_digest(*occurrence),
    }
}

fn validate_index_children(
    children: &[DraftMarkerIdentityChildV1],
) -> Result<(), DraftPiecePrepareErrorV1> {
    let mut previous = None;
    for child in children {
        if child.record_count() == 0 || child.first() > child.last() {
            return Err(DraftPiecePrepareErrorV1::InvalidRoot);
        }
        if previous.is_some_and(|last| last >= child.first()) {
            return Err(DraftPiecePrepareErrorV1::InvalidRoot);
        }
        previous = Some(child.last());
    }
    Ok(())
}

pub(super) fn index_child_for_record(
    record: &DraftMarkerIdentityRecordV1,
) -> Result<DraftMarkerIdentityChildV1, DraftPiecePrepareErrorV1> {
    match record {
        DraftMarkerIdentityRecordV1::Leaf {
            key,
            occurrence,
            digest,
        } => Ok(DraftMarkerIdentityChildV1::new(
            key.id(),
            *digest,
            1,
            occurrence.marker_id(),
            occurrence.marker_id(),
        )),
        DraftMarkerIdentityRecordV1::Internal {
            key,
            children,
            digest,
            ..
        } => {
            validate_index_children(children)?;
            let count = children.iter().try_fold(0_u64, |total, child| {
                total
                    .checked_add(child.record_count())
                    .ok_or(DraftPiecePrepareErrorV1::Rejected(
                        DraftPieceRejectedReasonV1::AggregateOverflow,
                    ))
            })?;
            Ok(DraftMarkerIdentityChildV1::new(
                key.id(),
                *digest,
                count,
                children
                    .first()
                    .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?
                    .first(),
                children
                    .last()
                    .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?
                    .last(),
            ))
        }
    }
}

fn load_root(
    context: &mut BuildContext<'_>,
    root: DraftPieceRootReferenceV1,
) -> Result<Option<SequenceRef>, DraftPiecePrepareErrorV1> {
    let stored = context
        .storage
        .point::<DraftPieceRootsFamily>(context.store, root.key(), point_limit())?
        .ok_or(DraftPiecePrepareErrorV1::Absent)?;
    context.records_read =
        context
            .records_read
            .checked_add(1)
            .ok_or(DraftPiecePrepareErrorV1::Rejected(
                DraftPieceRejectedReasonV1::AggregateOverflow,
            ))?;
    if stored.reference() != root
        || root.summary().marker_count() != root.marker_index_summary().record_count()
        || root.summary().marker_count() != root.marker_commitment().marker_count()
        || root.combined_digest()
            != combined_root_digest(
                root.summary(),
                root.marker_index_summary(),
                root.marker_commitment(),
            )
        || !root.summary().text_summary().is_canonical()
    {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    match root.key().build_identity() {
        DraftPieceRootBuildIdentityV1::DirectCanonicalEmpty { operation_id }
            if operation_id
                == canonical_empty_draft_root_operation_id_v1(root.key().draft_id())
                && root.root_node().is_none() => {}
        DraftPieceRootBuildIdentityV1::EditorCandidate { .. } => {}
        _ => return Err(DraftPiecePrepareErrorV1::InvalidRoot),
    }
    match root.root_node() {
        None => {
            if root.summary().logical_utf8_bytes() != 0
                || root.summary().newline_count() != 0
                || root.summary().logical_line_count() != 0
                || root.summary().piece_count() != 0
                || root.summary().marker_count() != 0
                || root.summary().height() != 0
                || root.summary().root_digest() != canonical_empty_root_digest_v1()
                || root.summary().marker_digest() != canonical_empty_marker_digest_v1()
                || root.marker_index_root().is_some()
                || root.marker_index_summary().record_count() != 0
                || root.marker_index_summary().height() != 0
                || root.marker_index_summary().root_digest()
                    != canonical_empty_marker_identity_index_digest_v1()
                || root.marker_order_root().is_some()
                || root.marker_order_height() != 0
                || root.marker_commitment() != canonical_empty_draft_marker_commitment_v1()
            {
                return Err(DraftPiecePrepareErrorV1::InvalidRoot);
            }
            Ok(None)
        }
        Some(id) => {
            let node = context.load_sequence_root_node(id, root.summary())?;
            let link = child_for_node(&node).map_err(|_| DraftPiecePrepareErrorV1::InvalidRoot)?;
            validate_index_root(context, root)?;
            marker_order_root(
                context,
                root.marker_order_root(),
                root.marker_order_height(),
                root.marker_commitment(),
            )?;
            Ok(Some(SequenceRef {
                link,
                height: root.summary().height(),
                selected_root: true,
            }))
        }
    }
}

fn validate_index_root(
    context: &mut BuildContext<'_>,
    root: DraftPieceRootReferenceV1,
) -> Result<Option<IndexRef>, DraftPiecePrepareErrorV1> {
    match root.marker_index_root() {
        None => {
            if root.marker_index_summary().record_count() != 0
                || root.marker_index_summary().height() != 0
                || root.marker_index_summary().root_digest()
                    != canonical_empty_marker_identity_index_digest_v1()
            {
                return Err(DraftPiecePrepareErrorV1::InvalidRoot);
            }
            Ok(None)
        }
        Some(id) => {
            let record = context.load_index_root_record(id, root.marker_index_summary())?;
            let link = index_child_for_record(&record)?;
            Ok(Some(IndexRef {
                link,
                height: record.height(),
                selected_root: true,
            }))
        }
    }
}

fn marker_rank_before_piece(
    context: &mut BuildContext<'_>,
    tree: Option<SequenceRef>,
    rank: u64,
) -> Result<u64, DraftPiecePrepareErrorV1> {
    let Some(mut current) = tree else {
        return if rank == 0 {
            Ok(0)
        } else {
            Err(DraftPiecePrepareErrorV1::InvalidRoot)
        };
    };
    if rank > current.link.piece_count() {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    if rank == current.link.piece_count() {
        return Ok(current.link.marker_count());
    }
    let mut remaining = rank;
    let mut marker_rank = 0_u64;
    while current.height != 0 {
        let node =
            context.load_sequence_node(current.link, current.height, current.selected_root)?;
        let mut selected = None;
        for child in node.children() {
            if remaining < child.piece_count() {
                selected = Some(*child);
                break;
            }
            remaining -= child.piece_count();
            marker_rank = marker_rank.checked_add(child.marker_count()).ok_or(
                DraftPiecePrepareErrorV1::Rejected(DraftPieceRejectedReasonV1::AggregateOverflow),
            )?;
        }
        current = SequenceRef {
            link: selected.ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?,
            height: current.height - 1,
            selected_root: false,
        };
    }
    Ok(marker_rank)
}

fn require_marker_free_range(
    context: &mut BuildContext<'_>,
    tree: Option<SequenceRef>,
    start: Boundary,
    end: Boundary,
) -> Result<(), DraftPiecePrepareErrorV1> {
    if start > end {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    if start == end {
        return Ok(());
    }
    let start_marker_rank = marker_rank_before_piece(context, tree, start.rank)?;
    let end_marker_rank = marker_rank_before_piece(context, tree, end.rank)?;
    if start_marker_rank != end_marker_rank {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    Ok(())
}

fn locate_search_key(
    context: &mut BuildContext<'_>,
    tree: SequenceRef,
    target: DraftCompositeSearchKeyV1,
) -> Result<Option<LocatedLeaf>, DraftPiecePrepareErrorV1> {
    let mut current = tree;
    let mut anchor = 0_u64;
    let mut rank = 0_u64;
    while current.height != 0 {
        let node =
            context.load_sequence_node(current.link, current.height, current.selected_root)?;
        let mut child_anchor = anchor;
        let mut child_rank = rank;
        let mut selected = None;
        for child in node.children().iter().copied() {
            let first = checked_offset_key(child.first(), child_anchor)
                .map_err(DraftPiecePrepareErrorV1::Rejected)?;
            let last = checked_offset_key(child.last(), child_anchor)
                .map_err(DraftPiecePrepareErrorV1::Rejected)?;
            let accepts = match target {
                DraftCompositeSearchKeyV1::Marker { .. } => {
                    child.marker_count() != 0 && first <= target && target <= last
                }
                DraftCompositeSearchKeyV1::BeforeMarkers(target_anchor) => {
                    last > target && last != DraftCompositeSearchKeyV1::AfterMarkers(target_anchor)
                }
                DraftCompositeSearchKeyV1::AfterMarkers(_) => last > target,
            };
            if accepts {
                selected = Some((child, child_anchor, child_rank));
                break;
            }
            child_anchor = child_anchor.checked_add(child.logical_utf8_bytes()).ok_or(
                DraftPiecePrepareErrorV1::Rejected(DraftPieceRejectedReasonV1::AggregateOverflow),
            )?;
            child_rank = child_rank.checked_add(child.piece_count()).ok_or(
                DraftPiecePrepareErrorV1::Rejected(DraftPieceRejectedReasonV1::AggregateOverflow),
            )?;
        }
        let Some((child, selected_anchor, selected_rank)) = selected else {
            return Ok(None);
        };
        current = SequenceRef {
            link: child,
            height: current.height - 1,
            selected_root: false,
        };
        anchor = selected_anchor;
        rank = selected_rank;
    }
    Ok(Some(LocatedLeaf {
        rank,
        anchor,
        link: current.link,
    }))
}

fn locate_marker_insertion_target(
    context: &mut BuildContext<'_>,
    tree: SequenceRef,
    target: DraftCompositeSearchKeyV1,
) -> Result<Option<LocatedLeaf>, DraftPiecePrepareErrorV1> {
    let target_anchor = target.anchor();
    let mut current = tree;
    let mut anchor = 0_u64;
    let mut rank = 0_u64;
    while current.height != 0 {
        let node =
            context.load_sequence_node(current.link, current.height, current.selected_root)?;
        let mut child_anchor = anchor;
        let mut child_rank = rank;
        let mut text_candidate = None;
        let mut marker_candidate = None;
        for child in node.children().iter().copied() {
            let last = checked_offset_key(child.last(), child_anchor)
                .map_err(DraftPiecePrepareErrorV1::Rejected)?;
            if child.marker_count() != 0 && target <= last {
                marker_candidate = Some((child, child_anchor, child_rank));
                break;
            }
            let child_end = child_anchor.checked_add(child.logical_utf8_bytes()).ok_or(
                DraftPiecePrepareErrorV1::Rejected(DraftPieceRejectedReasonV1::AggregateOverflow),
            )?;
            if child.logical_utf8_bytes() != 0
                && child_anchor <= target_anchor
                && target_anchor <= child_end
                && (text_candidate.is_none() || child_anchor == target_anchor)
            {
                text_candidate = Some((child, child_anchor, child_rank));
            }
            child_anchor = child_end;
            child_rank = child_rank.checked_add(child.piece_count()).ok_or(
                DraftPiecePrepareErrorV1::Rejected(DraftPieceRejectedReasonV1::AggregateOverflow),
            )?;
        }
        let Some((child, selected_anchor, selected_rank)) = marker_candidate.or(text_candidate)
        else {
            return Ok(None);
        };
        current = SequenceRef {
            link: child,
            height: current.height - 1,
            selected_root: false,
        };
        anchor = selected_anchor;
        rank = selected_rank;
    }
    Ok(Some(LocatedLeaf {
        rank,
        anchor,
        link: current.link,
    }))
}

fn resolve_position(
    context: &mut BuildContext<'_>,
    tree: Option<SequenceRef>,
    position: DraftCompositePositionV1,
) -> Result<Boundary, DraftPiecePrepareErrorV1> {
    let Some(tree) = tree else {
        return if position.utf8_offset() == 0
            && position.gap() == DraftCompositeGapWitnessV1::Unambiguous
        {
            Ok(Boundary { rank: 0, inner: 0 })
        } else {
            Err(DraftPiecePrepareErrorV1::Rejected(
                DraftPieceRejectedReasonV1::InvalidGapWitness,
            ))
        };
    };
    let offset = position.utf8_offset();
    if offset > tree.link.logical_utf8_bytes() {
        return Err(DraftPiecePrepareErrorV1::Rejected(
            DraftPieceRejectedReasonV1::InvalidUtf8Boundary,
        ));
    }
    let before = locate_search_key(
        context,
        tree,
        DraftCompositeSearchKeyV1::BeforeMarkers(offset),
    )?;
    match position.gap() {
        DraftCompositeGapWitnessV1::Unambiguous => match before {
            None if offset == tree.link.logical_utf8_bytes() => Ok(Boundary {
                rank: tree.link.piece_count(),
                inner: 0,
            }),
            Some(located) => {
                let leaf = context.load_sequence_leaf(located.link)?;
                let DraftPieceLeafValueV1::Text(text) = leaf.value() else {
                    return Err(DraftPiecePrepareErrorV1::Rejected(
                        DraftPieceRejectedReasonV1::InvalidGapWitness,
                    ));
                };
                let inner = usize::try_from(
                    offset
                        .checked_sub(located.anchor)
                        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?,
                )
                .map_err(|_| {
                    DraftPiecePrepareErrorV1::Rejected(
                        DraftPieceRejectedReasonV1::InvalidUtf8Boundary,
                    )
                })?;
                if inner > text.len() || !text.is_char_boundary(inner) {
                    return Err(DraftPiecePrepareErrorV1::Rejected(
                        DraftPieceRejectedReasonV1::InvalidUtf8Boundary,
                    ));
                }
                if inner == text.len() {
                    return Ok(Boundary {
                        rank: located.rank.checked_add(1).ok_or(
                            DraftPiecePrepareErrorV1::Rejected(
                                DraftPieceRejectedReasonV1::AggregateOverflow,
                            ),
                        )?,
                        inner: 0,
                    });
                }
                Ok(Boundary {
                    rank: located.rank,
                    inner,
                })
            }
            None => Err(DraftPiecePrepareErrorV1::InvalidRoot),
        },
        DraftCompositeGapWitnessV1::BeforeAll => {
            let located = before.ok_or(DraftPiecePrepareErrorV1::Rejected(
                DraftPieceRejectedReasonV1::InvalidGapWitness,
            ))?;
            let leaf = context.load_sequence_leaf(located.link)?;
            match leaf.value() {
                DraftPieceLeafValueV1::Marker(_) if located.anchor == offset => Ok(Boundary {
                    rank: located.rank,
                    inner: 0,
                }),
                _ => Err(DraftPiecePrepareErrorV1::Rejected(
                    DraftPieceRejectedReasonV1::InvalidGapWitness,
                )),
            }
        }
        DraftCompositeGapWitnessV1::AfterAll => {
            let after = locate_search_key(
                context,
                tree,
                DraftCompositeSearchKeyV1::AfterMarkers(offset),
            )?;
            let rank = after.map_or(tree.link.piece_count(), |located| located.rank);
            if before.is_some_and(|located| located.anchor == offset) && rank > before.unwrap().rank
            {
                Ok(Boundary { rank, inner: 0 })
            } else {
                Err(DraftPiecePrepareErrorV1::Rejected(
                    DraftPieceRejectedReasonV1::InvalidGapWitness,
                ))
            }
        }
        DraftCompositeGapWitnessV1::Between {
            left_order_key,
            left_marker_id,
            right_order_key,
            right_marker_id,
        } => {
            let left = locate_search_key(
                context,
                tree,
                DraftCompositeSearchKeyV1::Marker {
                    anchor: offset,
                    order_key: left_order_key,
                    marker_id: left_marker_id,
                },
            )?;
            let right = locate_search_key(
                context,
                tree,
                DraftCompositeSearchKeyV1::Marker {
                    anchor: offset,
                    order_key: right_order_key,
                    marker_id: right_marker_id,
                },
            )?;
            let (Some(left), Some(right)) = (left, right) else {
                return Err(DraftPiecePrepareErrorV1::Rejected(
                    DraftPieceRejectedReasonV1::InvalidGapWitness,
                ));
            };
            let left_leaf = context.load_sequence_leaf(left.link)?;
            let right_leaf = context.load_sequence_leaf(right.link)?;
            let left_exact = matches!(
                left_leaf.value(),
                DraftPieceLeafValueV1::Marker(marker)
                    if left.anchor == offset
                        && marker.order_key() == left_order_key
                        && marker.marker_id() == left_marker_id
            );
            let right_exact = matches!(
                right_leaf.value(),
                DraftPieceLeafValueV1::Marker(marker)
                    if right.anchor == offset
                        && marker.order_key() == right_order_key
                        && marker.marker_id() == right_marker_id
            );
            if left_exact && right_exact && right.rank == left.rank + 1 {
                Ok(Boundary {
                    rank: right.rank,
                    inner: 0,
                })
            } else {
                Err(DraftPiecePrepareErrorV1::Rejected(
                    DraftPieceRejectedReasonV1::InvalidGapWitness,
                ))
            }
        }
    }
}

fn make_sequence_tree(
    context: &mut BuildContext<'_>,
    height: u8,
    children: Vec<DraftPieceChildV1>,
) -> Result<Option<SequenceRef>, DraftPiecePrepareErrorV1> {
    match children.as_slice() {
        [] => Ok(None),
        [child] => Ok(Some(SequenceRef {
            link: *child,
            height: height
                .checked_sub(1)
                .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?,
            selected_root: true,
        })),
        _ => context.new_sequence_node(height, children).map(Some),
    }
}

fn split_sequence(
    context: &mut BuildContext<'_>,
    tree: SequenceRef,
    boundary: Boundary,
) -> Result<(Option<SequenceRef>, Option<SequenceRef>), DraftPiecePrepareErrorV1> {
    if boundary.inner == 0 && boundary.rank == 0 {
        return Ok((None, Some(tree)));
    }
    if boundary.inner == 0 && boundary.rank == tree.link.piece_count() {
        return Ok((Some(tree), None));
    }
    if tree.height == 0 {
        if boundary.rank != 0 || boundary.inner == 0 {
            return Err(DraftPiecePrepareErrorV1::InvalidRoot);
        }
        let leaf = context.load_sequence_leaf(tree.link)?;
        let DraftPieceLeafValueV1::Text(text) = leaf.value() else {
            return Err(DraftPiecePrepareErrorV1::InvalidRoot);
        };
        if boundary.inner >= text.len() || !text.is_char_boundary(boundary.inner) {
            return Err(DraftPiecePrepareErrorV1::Rejected(
                DraftPieceRejectedReasonV1::InvalidUtf8Boundary,
            ));
        }
        let left = context.new_sequence_leaf(DraftPieceLeafValueV1::Text(
            text[..boundary.inner].to_owned(),
        ))?;
        let right = context.new_sequence_leaf(DraftPieceLeafValueV1::Text(
            text[boundary.inner..].to_owned(),
        ))?;
        return Ok((Some(left), Some(right)));
    }
    let node = context.load_sequence_node(tree.link, tree.height, tree.selected_root)?;
    let mut consumed = 0_u64;
    for (index, child) in node.children().iter().copied().enumerate() {
        let next =
            consumed
                .checked_add(child.piece_count())
                .ok_or(DraftPiecePrepareErrorV1::Rejected(
                    DraftPieceRejectedReasonV1::AggregateOverflow,
                ))?;
        if boundary.rank < next || (boundary.rank == consumed && boundary.inner != 0) {
            let child_boundary = Boundary {
                rank: boundary.rank - consumed,
                inner: boundary.inner,
            };
            let (left_child, right_child) = split_sequence(
                context,
                SequenceRef {
                    link: child,
                    height: tree.height - 1,
                    selected_root: false,
                },
                child_boundary,
            )?;
            let mut left = node.children()[..index].to_vec();
            if let Some(child) = left_child {
                left.push(child.link);
            }
            let mut right = Vec::new();
            if let Some(child) = right_child {
                right.push(child.link);
            }
            right.extend_from_slice(&node.children()[index + 1..]);
            return Ok((
                make_sequence_tree(context, tree.height, left)?,
                make_sequence_tree(context, tree.height, right)?,
            ));
        }
        if boundary.rank == next && boundary.inner == 0 {
            return Ok((
                make_sequence_tree(context, tree.height, node.children()[..=index].to_vec())?,
                make_sequence_tree(context, tree.height, node.children()[index + 1..].to_vec())?,
            ));
        }
        consumed = next;
    }
    Err(DraftPiecePrepareErrorV1::InvalidRoot)
}

fn pack_sequence_children(
    context: &mut BuildContext<'_>,
    height: u8,
    children: Vec<DraftPieceChildV1>,
) -> Result<SequenceRef, DraftPiecePrepareErrorV1> {
    if let [child] = children.as_slice() {
        return Ok(SequenceRef {
            link: *child,
            height: height
                .checked_sub(1)
                .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?,
            selected_root: true,
        });
    }
    if children.len() <= DRAFT_PIECE_MAX_CHILDREN {
        return context.new_sequence_node(height, children);
    }
    let split = children.len() / 2;
    let left = context.new_sequence_node(height, children[..split].to_vec())?;
    let right = context.new_sequence_node(height, children[split..].to_vec())?;
    context.new_sequence_node(height + 1, vec![left.link, right.link])
}

fn pack_inserted_sequence_children(
    context: &mut BuildContext<'_>,
    height: u8,
    children: Vec<DraftPieceChildV1>,
) -> Result<Vec<SequenceRef>, DraftPiecePrepareErrorV1> {
    if children.len() <= DRAFT_PIECE_MAX_CHILDREN {
        return Ok(vec![context.new_sequence_node(height, children)?]);
    }
    let split = children.len() / 2;
    Ok(vec![
        context.new_sequence_node(height, children[..split].to_vec())?,
        context.new_sequence_node(height, children[split..].to_vec())?,
    ])
}

fn insert_sequence_leaf_recursive(
    context: &mut BuildContext<'_>,
    tree: SequenceRef,
    boundary: Boundary,
    inserted: SequenceRef,
) -> Result<Vec<SequenceRef>, DraftPiecePrepareErrorV1> {
    if tree.height == 0 {
        if boundary.inner == 0 && boundary.rank == 0 {
            return Ok(vec![inserted, tree]);
        }
        if boundary.inner == 0 && boundary.rank == 1 {
            return Ok(vec![tree, inserted]);
        }
        if boundary.rank != 0 || boundary.inner == 0 {
            return Err(DraftPiecePrepareErrorV1::InvalidRoot);
        }
        let leaf = context.load_sequence_leaf(tree.link)?;
        let DraftPieceLeafValueV1::Text(text) = leaf.value() else {
            return Err(DraftPiecePrepareErrorV1::InvalidRoot);
        };
        if boundary.inner >= text.len() || !text.is_char_boundary(boundary.inner) {
            return Err(DraftPiecePrepareErrorV1::Rejected(
                DraftPieceRejectedReasonV1::InvalidUtf8Boundary,
            ));
        }
        let left = context.new_sequence_leaf(DraftPieceLeafValueV1::Text(
            text[..boundary.inner].to_owned(),
        ))?;
        let right = context.new_sequence_leaf(DraftPieceLeafValueV1::Text(
            text[boundary.inner..].to_owned(),
        ))?;
        return Ok(vec![left, inserted, right]);
    }

    let node = context.load_sequence_node(tree.link, tree.height, tree.selected_root)?;
    let mut consumed = 0_u64;
    for (index, child) in node.children().iter().copied().enumerate() {
        let next =
            consumed
                .checked_add(child.piece_count())
                .ok_or(DraftPiecePrepareErrorV1::Rejected(
                    DraftPieceRejectedReasonV1::AggregateOverflow,
                ))?;
        let selects_child =
            (boundary.inner == 0 && boundary.rank >= consumed && boundary.rank <= next)
                || (boundary.inner != 0 && boundary.rank >= consumed && boundary.rank < next);
        if selects_child {
            let replacements = insert_sequence_leaf_recursive(
                context,
                SequenceRef {
                    link: child,
                    height: tree.height - 1,
                    selected_root: false,
                },
                Boundary {
                    rank: boundary.rank - consumed,
                    inner: boundary.inner,
                },
                inserted,
            )?;
            let mut children = Vec::with_capacity(node.children().len() + replacements.len() - 1);
            children.extend_from_slice(&node.children()[..index]);
            children.extend(replacements.into_iter().map(|replacement| replacement.link));
            children.extend_from_slice(&node.children()[index + 1..]);
            return pack_inserted_sequence_children(context, tree.height, children);
        }
        consumed = next;
    }
    Err(DraftPiecePrepareErrorV1::InvalidRoot)
}

fn insert_sequence_leaf(
    context: &mut BuildContext<'_>,
    tree: Option<SequenceRef>,
    boundary: Boundary,
    inserted: SequenceRef,
) -> Result<SequenceRef, DraftPiecePrepareErrorV1> {
    let Some(tree) = tree else {
        if boundary.rank != 0 || boundary.inner != 0 {
            return Err(DraftPiecePrepareErrorV1::InvalidRoot);
        }
        return Ok(inserted);
    };
    let roots = insert_sequence_leaf_recursive(context, tree, boundary, inserted)?;
    match roots.as_slice() {
        [root] => Ok(*root),
        [left, right] if left.height == right.height => {
            context.new_sequence_node(left.height + 1, vec![left.link, right.link])
        }
        _ => Err(DraftPiecePrepareErrorV1::InvalidRoot),
    }
}

fn join_sequence(
    context: &mut BuildContext<'_>,
    left: Option<SequenceRef>,
    right: Option<SequenceRef>,
) -> Result<Option<SequenceRef>, DraftPiecePrepareErrorV1> {
    let (Some(left), Some(right)) = (left, right) else {
        return Ok(left.or(right));
    };
    if let (
        DraftCompositeSearchKeyV1::Marker {
            anchor: left_anchor,
            order_key: left_order,
            marker_id: left_id,
        },
        DraftCompositeSearchKeyV1::Marker {
            anchor: right_anchor,
            order_key: right_order,
            marker_id: right_id,
        },
    ) = (
        left.link.last(),
        checked_offset_key(right.link.first(), left.link.logical_utf8_bytes())
            .map_err(DraftPiecePrepareErrorV1::Rejected)?,
    ) {
        if left_anchor == right_anchor && left_order == right_order {
            return Err(DraftPiecePrepareErrorV1::Rejected(
                DraftPieceRejectedReasonV1::DuplicateMarkerOrder,
            ));
        }
        if left_anchor == right_anchor && (left_order, left_id) >= (right_order, right_id) {
            return Err(DraftPiecePrepareErrorV1::Rejected(
                DraftPieceRejectedReasonV1::OutOfOrder,
            ));
        }
    }
    if left.height == right.height {
        if left.height == 0 {
            return context
                .new_sequence_node(1, vec![left.link, right.link])
                .map(Some);
        }
        let left_node = context.load_sequence_node(left.link, left.height, left.selected_root)?;
        let right_node =
            context.load_sequence_node(right.link, right.height, right.selected_root)?;
        let mut children = left_node.children().to_vec();
        children.extend_from_slice(right_node.children());
        return pack_sequence_children(context, left.height, children).map(Some);
    }
    if left.height > right.height {
        let node = context.load_sequence_node(left.link, left.height, left.selected_root)?;
        let mut children = node.children().to_vec();
        let last = children
            .pop()
            .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
        let joined = join_sequence(
            context,
            Some(SequenceRef {
                link: last,
                height: left.height - 1,
                selected_root: false,
            }),
            Some(right),
        )?
        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
        if joined.height == left.height - 1 {
            children.push(joined.link);
        } else if joined.height == left.height {
            let joined_node =
                context.load_sequence_node(joined.link, joined.height, joined.selected_root)?;
            children.extend_from_slice(joined_node.children());
        } else {
            return Err(DraftPiecePrepareErrorV1::InvalidRoot);
        }
        return pack_sequence_children(context, left.height, children).map(Some);
    }
    let node = context.load_sequence_node(right.link, right.height, right.selected_root)?;
    let mut children = node.children().to_vec();
    let first = children.remove(0);
    let joined = join_sequence(
        context,
        Some(left),
        Some(SequenceRef {
            link: first,
            height: right.height - 1,
            selected_root: false,
        }),
    )?
    .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
    if joined.height == right.height - 1 {
        children.insert(0, joined.link);
    } else if joined.height == right.height {
        let joined_node =
            context.load_sequence_node(joined.link, joined.height, joined.selected_root)?;
        let mut combined = joined_node.children().to_vec();
        combined.extend(children);
        children = combined;
    } else {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    pack_sequence_children(context, right.height, children).map(Some)
}

fn index_lookup(
    context: &mut BuildContext<'_>,
    root: Option<IndexRef>,
    marker_id: SyndicDraftMarkerId,
) -> Result<Option<DraftMarkerIdentityOccurrenceV1>, DraftPiecePrepareErrorV1> {
    let Some(mut current) = root else {
        return Ok(None);
    };
    loop {
        let record = context.load_index_record(current, current.selected_root)?;
        if current.height == 0 {
            let occurrence = record
                .occurrence()
                .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
            return Ok((occurrence.marker_id() == marker_id).then_some(occurrence));
        }
        let children = record
            .children()
            .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
        let selected = children
            .iter()
            .copied()
            .find(|child| child.first() <= marker_id && marker_id <= child.last());
        let Some(selected) = selected else {
            return Ok(None);
        };
        current = IndexRef {
            link: selected,
            height: current.height - 1,
            selected_root: false,
        };
    }
}

fn index_insert_recursive(
    context: &mut BuildContext<'_>,
    tree: IndexRef,
    occurrence: DraftMarkerIdentityOccurrenceV1,
) -> Result<Vec<IndexRef>, DraftPiecePrepareErrorV1> {
    if tree.height == 0 {
        let existing = context
            .load_index_record(tree, tree.selected_root)?
            .occurrence()
            .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
        if existing.marker_id() == occurrence.marker_id() {
            return Err(DraftPiecePrepareErrorV1::Rejected(
                DraftPieceRejectedReasonV1::DuplicateMarkerIdentity,
            ));
        }
        let new = context.new_index_leaf(occurrence)?;
        return Ok(if occurrence.marker_id() < existing.marker_id() {
            vec![new, tree]
        } else {
            vec![tree, new]
        });
    }
    let record = context.load_index_record(tree, tree.selected_root)?;
    let mut children = record
        .children()
        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?
        .to_vec();
    let index = children
        .iter()
        .position(|child| occurrence.marker_id() <= child.last())
        .unwrap_or(children.len() - 1);
    let child = children.remove(index);
    let replacements = index_insert_recursive(
        context,
        IndexRef {
            link: child,
            height: tree.height - 1,
            selected_root: false,
        },
        occurrence,
    )?;
    for replacement in replacements.into_iter().rev() {
        children.insert(index, replacement.link);
    }
    if children.len() <= DRAFT_PIECE_MAX_CHILDREN {
        return Ok(vec![context.new_index_node(tree.height, children)?]);
    }
    let split = children.len() / 2;
    Ok(vec![
        context.new_index_node(tree.height, children[..split].to_vec())?,
        context.new_index_node(tree.height, children[split..].to_vec())?,
    ])
}

fn index_insert(
    context: &mut BuildContext<'_>,
    root: Option<IndexRef>,
    occurrence: DraftMarkerIdentityOccurrenceV1,
) -> Result<Option<IndexRef>, DraftPiecePrepareErrorV1> {
    let Some(root) = root else {
        let leaf = context.new_index_leaf(occurrence)?;
        return context.new_index_node(1, vec![leaf.link]).map(Some);
    };
    if index_lookup(context, Some(root), occurrence.marker_id())?.is_some() {
        return Err(DraftPiecePrepareErrorV1::Rejected(
            DraftPieceRejectedReasonV1::DuplicateMarkerIdentity,
        ));
    }
    let parts = index_insert_recursive(context, root, occurrence)?;
    if parts.len() == 1 {
        Ok(parts.into_iter().next())
    } else {
        context
            .new_index_node(
                root.height + 1,
                parts.into_iter().map(|part| part.link).collect(),
            )
            .map(Some)
    }
}

fn index_delete_recursive(
    context: &mut BuildContext<'_>,
    tree: IndexRef,
    marker_id: SyndicDraftMarkerId,
) -> Result<Option<IndexRef>, DraftPiecePrepareErrorV1> {
    if tree.height == 0 {
        let occurrence = context
            .load_index_record(tree, tree.selected_root)?
            .occurrence()
            .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
        return if occurrence.marker_id() == marker_id {
            Ok(None)
        } else {
            Err(DraftPiecePrepareErrorV1::InvalidRoot)
        };
    }
    let record = context.load_index_record(tree, tree.selected_root)?;
    let mut children = record
        .children()
        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?
        .to_vec();
    let index = children
        .iter()
        .position(|child| child.first() <= marker_id && marker_id <= child.last())
        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
    let child = children.remove(index);
    if let Some(replacement) = index_delete_recursive(
        context,
        IndexRef {
            link: child,
            height: tree.height - 1,
            selected_root: false,
        },
        marker_id,
    )? {
        children.insert(index, replacement.link);
        if replacement.height != 0 {
            let replacement_record =
                context.load_index_record(replacement, replacement.selected_root)?;
            let replacement_children = replacement_record
                .children()
                .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
            if replacement_children.len() == 1 && children.len() > 1 {
                let (left_index, right_index) = if index == 0 {
                    (0, 1)
                } else {
                    (index - 1, index)
                };
                let left = children[left_index];
                let right = children[right_index];
                let left_record = context.load_index_record(
                    IndexRef {
                        link: left,
                        height: replacement.height,
                        selected_root: left == replacement.link,
                    },
                    left == replacement.link,
                )?;
                let right_record = context.load_index_record(
                    IndexRef {
                        link: right,
                        height: replacement.height,
                        selected_root: right == replacement.link,
                    },
                    right == replacement.link,
                )?;
                let mut combined = left_record
                    .children()
                    .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?
                    .to_vec();
                combined.extend_from_slice(
                    right_record
                        .children()
                        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?,
                );
                let split = if combined.len() <= DRAFT_PIECE_MAX_CHILDREN {
                    vec![context.new_index_node(replacement.height, combined)?]
                } else {
                    let middle = combined.len() / 2;
                    vec![
                        context.new_index_node(replacement.height, combined[..middle].to_vec())?,
                        context.new_index_node(replacement.height, combined[middle..].to_vec())?,
                    ]
                };
                children.splice(
                    left_index..=right_index,
                    split.into_iter().map(|part| part.link),
                );
            }
        }
    }
    if children.is_empty() {
        Ok(None)
    } else {
        context.new_index_node(tree.height, children).map(Some)
    }
}

fn compress_index_root(
    context: &mut BuildContext<'_>,
    mut root: Option<IndexRef>,
) -> Result<Option<IndexRef>, DraftPiecePrepareErrorV1> {
    loop {
        let Some(current) = root else {
            return Ok(None);
        };
        if current.height <= 1 {
            return Ok(root);
        }
        let record = context.load_index_record(current, current.selected_root)?;
        let children = record
            .children()
            .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
        if children.len() != 1 {
            return Ok(root);
        }
        root = Some(IndexRef {
            link: children[0],
            height: current.height - 1,
            selected_root: true,
        });
    }
}

fn index_delete(
    context: &mut BuildContext<'_>,
    root: Option<IndexRef>,
    occurrence: DraftMarkerIdentityOccurrenceV1,
) -> Result<Option<IndexRef>, DraftPiecePrepareErrorV1> {
    if index_lookup(context, root, occurrence.marker_id())? != Some(occurrence) {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    let successor = index_delete_recursive(
        context,
        root.ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?,
        occurrence.marker_id(),
    )?;
    compress_index_root(context, successor)
}

fn marker_order_root(
    context: &mut BuildContext<'_>,
    root_id: Option<DraftPieceRecordIdV1>,
    height: u8,
    commitment: DraftMarkerCommitmentV1,
) -> Result<Option<MarkerOrderRef>, DraftPiecePrepareErrorV1> {
    if commitment.marker_count() == 0 {
        if root_id.is_some()
            || height != 0
            || commitment != canonical_empty_draft_marker_commitment_v1()
        {
            return Err(DraftPiecePrepareErrorV1::InvalidRoot);
        }
        return Ok(None);
    }
    let root = MarkerOrderRef {
        link: DraftMarkerOrderChildV1::new(
            root_id.ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?,
            DraftPieceDigestV1::from_bytes(commitment.tree_root_digest()),
            commitment.marker_count(),
            commitment.maximum_image_label(),
        )
        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?,
        height,
        selected_root: true,
    };
    if root.height == 0 {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    context.load_marker_order_record(root, true)?;
    Ok(Some(root))
}

fn marker_order_insert_recursive(
    context: &mut BuildContext<'_>,
    tree: MarkerOrderRef,
    rank: u64,
    marker_id: SyndicDraftMarkerId,
    label: ImageLabelOrdinal,
    asset_id: AssetId,
) -> Result<Vec<MarkerOrderRef>, DraftPiecePrepareErrorV1> {
    if rank > tree.link.marker_count() {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    if tree.height == 0 {
        let record = context.load_marker_order_record(tree, tree.selected_root)?;
        let (existing_id, existing_label, existing_asset_id) = record
            .marker()
            .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
        let new = context.new_marker_order_leaf(marker_id, label, asset_id)?;
        return match rank {
            0 => Ok(vec![new, tree]),
            1 => Ok(vec![tree, new]),
            _ => {
                let _ = (existing_id, existing_label, existing_asset_id);
                Err(DraftPiecePrepareErrorV1::InvalidRoot)
            }
        };
    }
    let record = context.load_marker_order_record(tree, tree.selected_root)?;
    let mut children = record
        .children()
        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?
        .to_vec();
    let mut remaining = rank;
    let mut index = children.len() - 1;
    for (candidate, child) in children.iter().enumerate() {
        if remaining <= child.marker_count() {
            index = candidate;
            break;
        }
        remaining -= child.marker_count();
    }
    let child = children.remove(index);
    let replacements = marker_order_insert_recursive(
        context,
        MarkerOrderRef {
            link: child,
            height: tree.height - 1,
            selected_root: false,
        },
        remaining,
        marker_id,
        label,
        asset_id,
    )?;
    children.splice(index..index, replacements.into_iter().map(|part| part.link));
    if children.len() <= DRAFT_PIECE_MAX_CHILDREN {
        Ok(vec![context.new_marker_order_node(tree.height, children)?])
    } else {
        let split = children.len() / 2;
        Ok(vec![
            context.new_marker_order_node(tree.height, children[..split].to_vec())?,
            context.new_marker_order_node(tree.height, children[split..].to_vec())?,
        ])
    }
}

fn marker_order_insert(
    context: &mut BuildContext<'_>,
    root: Option<MarkerOrderRef>,
    rank: u64,
    marker_id: SyndicDraftMarkerId,
    label: ImageLabelOrdinal,
    asset_id: AssetId,
) -> Result<Option<MarkerOrderRef>, DraftPiecePrepareErrorV1> {
    let Some(root) = root else {
        if rank != 0 {
            return Err(DraftPiecePrepareErrorV1::InvalidRoot);
        }
        let leaf = context.new_marker_order_leaf(marker_id, label, asset_id)?;
        return context.new_marker_order_node(1, vec![leaf.link]).map(Some);
    };
    let parts = marker_order_insert_recursive(context, root, rank, marker_id, label, asset_id)?;
    if parts.len() == 1 {
        Ok(parts.into_iter().next())
    } else {
        context
            .new_marker_order_node(
                root.height + 1,
                parts.into_iter().map(|part| part.link).collect(),
            )
            .map(Some)
    }
}

fn marker_order_delete_recursive(
    context: &mut BuildContext<'_>,
    tree: MarkerOrderRef,
    rank: u64,
    expected: (SyndicDraftMarkerId, ImageLabelOrdinal, AssetId),
) -> Result<Option<MarkerOrderRef>, DraftPiecePrepareErrorV1> {
    if rank >= tree.link.marker_count() {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    if tree.height == 0 {
        let record = context.load_marker_order_record(tree, tree.selected_root)?;
        return if rank == 0 && record.marker() == Some(expected) {
            Ok(None)
        } else {
            Err(DraftPiecePrepareErrorV1::InvalidRoot)
        };
    }
    let record = context.load_marker_order_record(tree, tree.selected_root)?;
    let mut children = record
        .children()
        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?
        .to_vec();
    let mut remaining = rank;
    let index = children
        .iter()
        .position(|child| {
            if remaining < child.marker_count() {
                true
            } else {
                remaining -= child.marker_count();
                false
            }
        })
        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
    let child = children.remove(index);
    if let Some(replacement) = marker_order_delete_recursive(
        context,
        MarkerOrderRef {
            link: child,
            height: tree.height - 1,
            selected_root: false,
        },
        remaining,
        expected,
    )? {
        children.insert(index, replacement.link);
        if replacement.height != 0 {
            let replacement_record =
                context.load_marker_order_record(replacement, replacement.selected_root)?;
            let replacement_children = replacement_record
                .children()
                .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
            if replacement_children.len() == 1 && children.len() > 1 {
                let (left_index, right_index) = if index == 0 {
                    (0, 1)
                } else {
                    (index - 1, index)
                };
                let left = children[left_index];
                let right = children[right_index];
                let left_is_replacement = left == replacement.link;
                let right_is_replacement = right == replacement.link;
                let left_record = context.load_marker_order_record(
                    MarkerOrderRef {
                        link: left,
                        height: replacement.height,
                        selected_root: left_is_replacement,
                    },
                    left_is_replacement,
                )?;
                let right_record = context.load_marker_order_record(
                    MarkerOrderRef {
                        link: right,
                        height: replacement.height,
                        selected_root: right_is_replacement,
                    },
                    right_is_replacement,
                )?;
                let mut combined = left_record
                    .children()
                    .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?
                    .to_vec();
                combined.extend_from_slice(
                    right_record
                        .children()
                        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?,
                );
                let rebalanced = if combined.len() <= DRAFT_PIECE_MAX_CHILDREN {
                    vec![context.new_marker_order_node(replacement.height, combined)?]
                } else {
                    let middle = combined.len() / 2;
                    vec![
                        context.new_marker_order_node(
                            replacement.height,
                            combined[..middle].to_vec(),
                        )?,
                        context.new_marker_order_node(
                            replacement.height,
                            combined[middle..].to_vec(),
                        )?,
                    ]
                };
                children.splice(
                    left_index..=right_index,
                    rebalanced.into_iter().map(|part| part.link),
                );
            }
        }
    }
    if children.is_empty() {
        Ok(None)
    } else {
        context
            .new_marker_order_node(tree.height, children)
            .map(Some)
    }
}

fn compress_marker_order_root(
    context: &mut BuildContext<'_>,
    mut root: Option<MarkerOrderRef>,
) -> Result<Option<MarkerOrderRef>, DraftPiecePrepareErrorV1> {
    while let Some(current) = root {
        if current.height <= 1 {
            break;
        }
        let record = context.load_marker_order_record(current, true)?;
        let children = record
            .children()
            .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
        if children.len() != 1 {
            break;
        }
        root = Some(MarkerOrderRef {
            link: children[0],
            height: current.height - 1,
            selected_root: true,
        });
    }
    Ok(root)
}

fn marker_order_delete(
    context: &mut BuildContext<'_>,
    root: Option<MarkerOrderRef>,
    rank: u64,
    expected: (SyndicDraftMarkerId, ImageLabelOrdinal, AssetId),
) -> Result<Option<MarkerOrderRef>, DraftPiecePrepareErrorV1> {
    let successor = marker_order_delete_recursive(
        context,
        root.ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?,
        rank,
        expected,
    )?;
    compress_marker_order_root(context, successor)
}

fn compress_sequence_root(
    context: &mut BuildContext<'_>,
    mut root: Option<SequenceRef>,
) -> Result<Option<SequenceRef>, DraftPiecePrepareErrorV1> {
    loop {
        let Some(current) = root else {
            return Ok(None);
        };
        if current.height <= 1 {
            return Ok(root);
        }
        let node =
            context.load_sequence_node(current.link, current.height, current.selected_root)?;
        if node.children().len() != 1 {
            return Ok(root);
        }
        root = Some(SequenceRef {
            link: node.children()[0],
            height: current.height - 1,
            selected_root: true,
        });
    }
}

fn finalize_build_root(
    context: &mut BuildContext<'_>,
    operation_id: DraftPieceOperationIdV1,
    sequence: Option<SequenceRef>,
    index: Option<IndexRef>,
    marker_order: Option<MarkerOrderRef>,
) -> Result<DraftPieceRootRecordV1, DraftPiecePrepareErrorV1> {
    let sequence = compress_sequence_root(context, sequence)?;
    let index = compress_index_root(context, index)?;
    let marker_order = compress_marker_order_root(context, marker_order)?;
    let session_id = context
        .session_id
        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
    let root_key =
        DraftPieceRootKeyV1::editor_candidate(context.draft_id, session_id, operation_id);
    if sequence.is_none() {
        if index.is_some() || marker_order.is_some() {
            return Err(DraftPiecePrepareErrorV1::InvalidRoot);
        }
        let sequence_summary = DraftPieceSummaryV1::new(
            0,
            0,
            0,
            0,
            0,
            canonical_empty_marker_digest_v1(),
            0,
            canonical_empty_root_digest_v1(),
        );
        let index_summary = DraftMarkerIdentityIndexSummaryV1::new(
            0,
            0,
            canonical_empty_marker_identity_index_digest_v1(),
        );
        let marker_commitment = canonical_empty_draft_marker_commitment_v1();
        let combined = combined_root_digest(sequence_summary, index_summary, marker_commitment);
        let reference = DraftPieceRootReferenceV1::new_authenticated(
            root_key,
            None,
            sequence_summary,
            None,
            index_summary,
            None,
            0,
            marker_commitment,
            combined,
        );
        return Ok(DraftPieceRootRecordV1::new(reference));
    }
    let mut sequence = sequence.expect("checked");
    if sequence.height == 0 {
        sequence = context.new_sequence_node(1, vec![sequence.link])?;
    }
    let provisional = DraftPieceSummaryV1::new(
        sequence.link.logical_utf8_bytes(),
        sequence.link.newline_count(),
        sequence.link.logical_line_count(),
        sequence.link.piece_count(),
        sequence.link.marker_count(),
        sequence.link.marker_digest(),
        sequence.height,
        DraftPieceDigestV1::from_bytes([0; 32]),
    );
    let sequence_summary = DraftPieceSummaryV1::new(
        provisional.logical_utf8_bytes(),
        provisional.newline_count(),
        provisional.logical_line_count(),
        provisional.piece_count(),
        provisional.marker_count(),
        provisional.marker_digest(),
        provisional.height(),
        root_digest(provisional, sequence.link.digest()),
    );
    let (index_root, index_summary) = match index {
        Some(index) => (
            Some(index.link.id()),
            DraftMarkerIdentityIndexSummaryV1::new(
                index.link.record_count(),
                index.height,
                index_root_digest(index.link.record_count(), index.height, index.link.digest()),
            ),
        ),
        None => (
            None,
            DraftMarkerIdentityIndexSummaryV1::new(
                0,
                0,
                canonical_empty_marker_identity_index_digest_v1(),
            ),
        ),
    };
    let (marker_order_root, marker_order_height, marker_commitment) = match marker_order {
        Some(root) => (
            Some(root.link.id()),
            root.height,
            DraftMarkerCommitmentV1::new(
                *root.link.digest().as_bytes(),
                root.link.marker_count(),
                root.link.maximum_image_label(),
            )
            .map_err(|_| DraftPiecePrepareErrorV1::InvalidRoot)?,
        ),
        None => (None, 0, canonical_empty_draft_marker_commitment_v1()),
    };
    if sequence_summary.marker_count() != index_summary.record_count()
        || sequence_summary.marker_count() != marker_commitment.marker_count()
    {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    let combined = combined_root_digest(sequence_summary, index_summary, marker_commitment);
    let reference = DraftPieceRootReferenceV1::new_authenticated(
        root_key,
        Some(sequence.link.id()),
        sequence_summary,
        index_root,
        index_summary,
        marker_order_root,
        marker_order_height,
        marker_commitment,
        combined,
    );
    Ok(DraftPieceRootRecordV1::new(reference))
}

#[derive(Clone)]
pub(crate) struct DraftPieceTreeQuantumV1 {
    pub(crate) roots: DraftPieceBuildRootsV1,
    pub(crate) base_frontier: DraftPieceBuildBoundaryV1,
    pub(crate) successor_frontier: DraftPieceBuildBoundaryV1,
    pub(crate) next_record_ordinal: u64,
    pub(crate) frontier: DraftPieceBuildFrontierV1,
    pub(crate) successor: Option<DraftPieceRootRecordV1>,
    pub(crate) build_digest: Option<DraftPieceDigestV1>,
    pub(crate) leaves: Vec<DraftPieceLeafRecordV1>,
    pub(crate) nodes: Vec<DraftPieceNodeRecordV1>,
    pub(crate) index_records: Vec<DraftMarkerIdentityRecordV1>,
    pub(crate) marker_order_records: Vec<DraftMarkerOrderRecordV1>,
    pub(crate) records_read: u64,
}

fn checked_boundary(
    value: DraftPieceBuildBoundaryV1,
) -> Result<Boundary, DraftPiecePrepareErrorV1> {
    Ok(Boundary {
        rank: value.rank(),
        inner: usize::try_from(value.inner()).map_err(|_| {
            DraftPiecePrepareErrorV1::Rejected(DraftPieceRejectedReasonV1::AggregateOverflow)
        })?,
    })
}

fn durable_boundary(value: Boundary) -> DraftPieceBuildBoundaryV1 {
    DraftPieceBuildBoundaryV1::new(value.rank, value.inner as u64)
}

fn load_working_roots(
    context: &mut BuildContext<'_>,
    roots: DraftPieceBuildRootsV1,
) -> Result<
    (
        Option<SequenceRef>,
        Option<IndexRef>,
        Option<MarkerOrderRef>,
    ),
    DraftPiecePrepareErrorV1,
> {
    let sequence = match roots.sequence_root() {
        Some(id) => {
            let node = context.load_sequence_root_node(id, roots.sequence_summary())?;
            Some(SequenceRef {
                link: child_for_node(&node).map_err(|_| DraftPiecePrepareErrorV1::InvalidRoot)?,
                height: roots.sequence_summary().height(),
                selected_root: true,
            })
        }
        None => {
            let summary = roots.sequence_summary();
            if summary.logical_utf8_bytes() != 0
                || summary.piece_count() != 0
                || summary.marker_count() != 0
                || summary.height() != 0
                || summary.root_digest() != canonical_empty_root_digest_v1()
                || summary.marker_digest() != canonical_empty_marker_digest_v1()
            {
                return Err(DraftPiecePrepareErrorV1::InvalidRoot);
            }
            None
        }
    };
    let index = match roots.marker_index_root() {
        Some(id) => {
            let record = context.load_index_root_record(id, roots.marker_index_summary())?;
            Some(IndexRef {
                link: index_child_for_record(&record)?,
                height: roots.marker_index_summary().height(),
                selected_root: true,
            })
        }
        None => {
            let summary = roots.marker_index_summary();
            if summary.record_count() != 0
                || summary.height() != 0
                || summary.root_digest() != canonical_empty_marker_identity_index_digest_v1()
            {
                return Err(DraftPiecePrepareErrorV1::InvalidRoot);
            }
            None
        }
    };
    let marker_order = marker_order_root(
        context,
        roots.marker_order_root(),
        roots.marker_order_height(),
        roots.marker_commitment(),
    )?;
    if roots.sequence_summary().marker_count() != roots.marker_commitment().marker_count()
        || roots.marker_index_summary().record_count() != roots.marker_commitment().marker_count()
    {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    Ok((sequence, index, marker_order))
}

fn build_roots(
    context: &mut BuildContext<'_>,
    sequence: Option<SequenceRef>,
    index: Option<IndexRef>,
    marker_order: Option<MarkerOrderRef>,
) -> Result<DraftPieceBuildRootsV1, DraftPiecePrepareErrorV1> {
    let sequence = compress_sequence_root(context, sequence)?;
    let sequence = match sequence {
        Some(sequence) if sequence.height == 0 => {
            Some(context.new_sequence_node(1, vec![sequence.link])?)
        }
        value => value,
    };
    let index = compress_index_root(context, index)?;
    let marker_order = compress_marker_order_root(context, marker_order)?;
    let (sequence_root, sequence_summary) = match sequence {
        Some(sequence) => {
            let provisional = DraftPieceSummaryV1::new(
                sequence.link.logical_utf8_bytes(),
                sequence.link.newline_count(),
                sequence.link.logical_line_count(),
                sequence.link.piece_count(),
                sequence.link.marker_count(),
                sequence.link.marker_digest(),
                sequence.height,
                DraftPieceDigestV1::from_bytes([0; 32]),
            );
            (
                Some(sequence.link.id()),
                DraftPieceSummaryV1::new(
                    provisional.logical_utf8_bytes(),
                    provisional.newline_count(),
                    provisional.logical_line_count(),
                    provisional.piece_count(),
                    provisional.marker_count(),
                    provisional.marker_digest(),
                    provisional.height(),
                    root_digest(provisional, sequence.link.digest()),
                ),
            )
        }
        None => (
            None,
            DraftPieceSummaryV1::new(
                0,
                0,
                0,
                0,
                0,
                canonical_empty_marker_digest_v1(),
                0,
                canonical_empty_root_digest_v1(),
            ),
        ),
    };
    let (marker_index_root, marker_index_summary) = match index {
        Some(index) => (
            Some(index.link.id()),
            DraftMarkerIdentityIndexSummaryV1::new(
                index.link.record_count(),
                index.height,
                index_root_digest(index.link.record_count(), index.height, index.link.digest()),
            ),
        ),
        None => (
            None,
            DraftMarkerIdentityIndexSummaryV1::new(
                0,
                0,
                canonical_empty_marker_identity_index_digest_v1(),
            ),
        ),
    };
    let (marker_order_root, marker_order_height, marker_commitment) = match marker_order {
        Some(root) => (
            Some(root.link.id()),
            root.height,
            DraftMarkerCommitmentV1::new(
                *root.link.digest().as_bytes(),
                root.link.marker_count(),
                root.link.maximum_image_label(),
            )
            .map_err(|_| DraftPiecePrepareErrorV1::InvalidRoot)?,
        ),
        None => (None, 0, canonical_empty_draft_marker_commitment_v1()),
    };
    if sequence_summary.marker_count() != marker_index_summary.record_count()
        || sequence_summary.marker_count() != marker_commitment.marker_count()
    {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    Ok(DraftPieceBuildRootsV1::new(
        sequence_root,
        sequence_summary,
        marker_index_root,
        marker_index_summary,
        marker_order_root,
        marker_order_height,
        marker_commitment,
    ))
}

fn mapped_boundary(
    base_frontier: DraftPieceBuildBoundaryV1,
    successor_frontier: DraftPieceBuildBoundaryV1,
    target: Boundary,
) -> Result<Boundary, DraftPiecePrepareErrorV1> {
    let base = checked_boundary(base_frontier)?;
    let successor = checked_boundary(successor_frontier)?;
    if target < base {
        return Err(DraftPiecePrepareErrorV1::Rejected(
            DraftPieceRejectedReasonV1::OutOfOrder,
        ));
    }
    if target.rank == base.rank {
        return Ok(Boundary {
            rank: successor.rank,
            inner: successor
                .inner
                .checked_add(target.inner - base.inner)
                .ok_or(DraftPiecePrepareErrorV1::Rejected(
                    DraftPieceRejectedReasonV1::AggregateOverflow,
                ))?,
        });
    }
    Ok(Boundary {
        rank: successor.rank.checked_add(target.rank - base.rank).ok_or(
            DraftPiecePrepareErrorV1::Rejected(DraftPieceRejectedReasonV1::AggregateOverflow),
        )?,
        inner: target.inner,
    })
}

fn boundary_after_marker_insertion(
    frontier: Boundary,
    insertion: Boundary,
) -> Result<Boundary, DraftPiecePrepareErrorV1> {
    if insertion > frontier {
        return Err(DraftPiecePrepareErrorV1::Rejected(
            DraftPieceRejectedReasonV1::OutOfOrder,
        ));
    }
    if insertion.rank < frontier.rank {
        let added = 1_u64 + u64::from(insertion.inner != 0);
        return Ok(Boundary {
            rank: frontier
                .rank
                .checked_add(added)
                .ok_or(DraftPiecePrepareErrorV1::Rejected(
                    DraftPieceRejectedReasonV1::AggregateOverflow,
                ))?,
            inner: frontier.inner,
        });
    }
    if insertion.inner == 0 {
        return Ok(Boundary {
            rank: frontier
                .rank
                .checked_add(1)
                .ok_or(DraftPiecePrepareErrorV1::Rejected(
                    DraftPieceRejectedReasonV1::AggregateOverflow,
                ))?,
            inner: frontier.inner,
        });
    }
    if insertion.inner == frontier.inner {
        return Ok(Boundary {
            rank: frontier
                .rank
                .checked_add(2)
                .ok_or(DraftPiecePrepareErrorV1::Rejected(
                    DraftPieceRejectedReasonV1::AggregateOverflow,
                ))?,
            inner: 0,
        });
    }
    Ok(Boundary {
        rank: frontier
            .rank
            .checked_add(2)
            .ok_or(DraftPiecePrepareErrorV1::Rejected(
                DraftPieceRejectedReasonV1::AggregateOverflow,
            ))?,
        inner: frontier.inner - insertion.inner,
    })
}

fn boundary_after_marker_removal(frontier: Boundary, removal_rank: Option<u64>) -> Boundary {
    if removal_rank.is_some_and(|rank| rank < frontier.rank) {
        Boundary {
            rank: frontier.rank - 1,
            inner: frontier.inner,
        }
    } else {
        frontier
    }
}

fn derived_marker_insertion_boundary(
    context: &mut BuildContext<'_>,
    sequence: Option<SequenceRef>,
    insertion: DraftPieceMarkerInsertionV1,
) -> Result<Boundary, DraftPiecePrepareErrorV1> {
    let Some(sequence) = sequence else {
        return if insertion.anchor() == 0 {
            Ok(Boundary { rank: 0, inner: 0 })
        } else {
            Err(DraftPiecePrepareErrorV1::Rejected(
                DraftPieceRejectedReasonV1::OutOfOrder,
            ))
        };
    };
    if insertion.anchor() > sequence.link.logical_utf8_bytes() {
        return Err(DraftPiecePrepareErrorV1::Rejected(
            DraftPieceRejectedReasonV1::OutOfOrder,
        ));
    }
    let first_at_anchor = locate_search_key(
        context,
        sequence,
        DraftCompositeSearchKeyV1::BeforeMarkers(insertion.anchor()),
    )?;
    let anchor_has_markers = match first_at_anchor {
        Some(located) if located.anchor == insertion.anchor() => matches!(
            context.load_sequence_leaf(located.link)?.value(),
            DraftPieceLeafValueV1::Marker(_)
        ),
        _ => false,
    };
    if !anchor_has_markers {
        return resolve_position(
            context,
            Some(sequence),
            DraftCompositePositionV1::new(
                insertion.anchor(),
                DraftCompositeGapWitnessV1::Unambiguous,
            ),
        );
    }
    let marker = insertion.marker();
    let target = DraftCompositeSearchKeyV1::Marker {
        anchor: insertion.anchor(),
        order_key: marker.order_key(),
        marker_id: beryl_model::SyndicDraftMarkerId::from_bytes([0; 16]),
    };
    if let Some(located) = locate_marker_insertion_target(context, sequence, target)? {
        let leaf = context.load_sequence_leaf(located.link)?;
        match leaf.value() {
            DraftPieceLeafValueV1::Marker(existing)
                if existing.order_key() == marker.order_key() =>
            {
                return Err(DraftPiecePrepareErrorV1::Rejected(
                    DraftPieceRejectedReasonV1::DuplicateMarkerOrder,
                ));
            }
            DraftPieceLeafValueV1::Marker(existing)
                if located.anchor == insertion.anchor()
                    && existing.order_key() > marker.order_key() =>
            {
                return Ok(Boundary {
                    rank: located.rank,
                    inner: 0,
                });
            }
            _ => {}
        }
    }
    let after = locate_search_key(
        context,
        sequence,
        DraftCompositeSearchKeyV1::AfterMarkers(insertion.anchor()),
    )?;
    Ok(Boundary {
        rank: after.map_or(sequence.link.piece_count(), |located| located.rank),
        inner: 0,
    })
}

fn validate_marker_effect_charge(
    effect: DraftPieceMarkerEffectV1,
    leaf: &DraftPieceLeafRecordV1,
) -> Result<(), DraftPiecePrepareErrorV1> {
    let charges = match effect {
        DraftPieceMarkerEffectV1::Insert(insertion)
        | DraftPieceMarkerEffectV1::Move { insertion, .. }
        | DraftPieceMarkerEffectV1::SameIdReplacement { insertion, .. } => insertion.charges(),
        DraftPieceMarkerEffectV1::Remove { charges, .. } => charges,
    };
    let encoded_bytes = super::codec::canonical_draft_piece_leaf_encoded_bytes(leaf)
        .map_err(|_| DraftPiecePrepareErrorV1::InvalidRoot)?;
    if charges != DraftPieceMarkerEffectChargesV1::new(0, 1, encoded_bytes) {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    Ok(())
}

fn finish_quantum(
    context: BuildContext<'_>,
    roots: DraftPieceBuildRootsV1,
    base_frontier: DraftPieceBuildBoundaryV1,
    successor_frontier: DraftPieceBuildBoundaryV1,
    frontier: DraftPieceBuildFrontierV1,
    successor: Option<DraftPieceRootRecordV1>,
    build_digest: Option<DraftPieceDigestV1>,
) -> DraftPieceTreeQuantumV1 {
    DraftPieceTreeQuantumV1 {
        roots,
        base_frontier,
        successor_frontier,
        next_record_ordinal: context.ordinal,
        frontier,
        successor,
        build_digest,
        leaves: context.sequence_leaves.into_values().collect(),
        nodes: context.sequence_nodes.into_values().collect(),
        index_records: context.index_records.into_values().collect(),
        marker_order_records: context.marker_order_records.into_values().collect(),
        records_read: context.records_read,
    }
}

pub(crate) fn load_authenticated_build_fragment(
    storage: &SyndicStorage,
    store: &HomeStore,
    build: &DraftPieceBuildRecordV1,
    ordinal: u64,
) -> Result<DraftPieceBuildFragmentV1, DraftPiecePrepareErrorV1> {
    if ordinal == 0 || ordinal > build.fragment_count() {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    let key = DraftPieceBuildFragmentKeyV1::new(
        build.draft_id(),
        build.session_id(),
        build.operation_id(),
        ordinal,
    );
    let fragment = storage
        .point::<DraftPieceBuildFragmentsFamily>(store, key, point_limit())?
        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
    if fragment.key() != key
        || validate_fragment(fragment.replacement()).is_err()
        || fragment.chain_digest()
            != draft_piece_fragment_chain_link_v1(
                fragment.preceding_chain(),
                ordinal,
                fragment.replacement(),
            )
    {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    if ordinal == 1 {
        if fragment.preceding_chain() != canonical_empty_draft_piece_fragment_chain_v1() {
            return Err(DraftPiecePrepareErrorV1::InvalidRoot);
        }
    } else {
        let predecessor_key = DraftPieceBuildFragmentKeyV1::new(
            build.draft_id(),
            build.session_id(),
            build.operation_id(),
            ordinal - 1,
        );
        let predecessor = storage
            .point::<DraftPieceBuildFragmentsFamily>(store, predecessor_key, point_limit())?
            .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
        if predecessor.key() != predecessor_key
            || validate_fragment(predecessor.replacement()).is_err()
            || predecessor.chain_digest() != fragment.preceding_chain()
        {
            return Err(DraftPiecePrepareErrorV1::InvalidRoot);
        }
    }
    if ordinal == build.fragment_count() && fragment.chain_digest() != build.fragment_chain() {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    Ok(fragment)
}

fn exact_replacement_boundaries(
    storage: &SyndicStorage,
    store: &HomeStore,
    build: &DraftPieceBuildRecordV1,
    replacement: &DraftPieceReplacementV1,
) -> Result<
    (
        Boundary,
        Boundary,
        DraftPieceBuildBoundaryV1,
        DraftPieceBuildBoundaryV1,
        DraftPieceBuildBoundaryV1,
    ),
    DraftPiecePrepareErrorV1,
> {
    let mut base_context = BuildContext::with_ordinal(
        storage,
        store,
        build.draft_id(),
        build.predecessor_root().key().session_id(),
        build.predecessor_root().key().operation_id(),
        build.next_record_ordinal(),
    );
    let base_sequence = load_root(&mut base_context, build.predecessor_root())?;
    let start = resolve_position(&mut base_context, base_sequence, replacement.start())?;
    let end = resolve_position(&mut base_context, base_sequence, replacement.end())?;
    let base_end = replacement_base_end(&mut base_context, base_sequence, replacement, end)?;
    let removal_rank = active_marker_removal_rank(storage, store, build, replacement)?;
    let successor_start = durable_boundary(boundary_after_marker_removal(
        mapped_boundary(build.base_frontier(), build.successor_frontier(), start)?,
        removal_rank,
    ));
    let successor_end = durable_boundary(boundary_after_marker_removal(
        mapped_boundary(build.base_frontier(), build.successor_frontier(), end)?,
        removal_rank,
    ));
    Ok((
        start,
        end,
        durable_boundary(base_end),
        successor_start,
        successor_end,
    ))
}

fn active_marker_removal_rank(
    storage: &SyndicStorage,
    store: &HomeStore,
    build: &DraftPieceBuildRecordV1,
    replacement: &DraftPieceReplacementV1,
) -> Result<Option<u64>, DraftPiecePrepareErrorV1> {
    let effect = replacement.marker_effect();
    let removal = match effect {
        Some(DraftPieceMarkerEffectV1::Remove { removal, .. })
        | Some(DraftPieceMarkerEffectV1::Move { removal, .. })
        | Some(DraftPieceMarkerEffectV1::SameIdReplacement { removal, .. }) => removal,
        _ => return Ok(None),
    };
    let active = build
        .marker_effect_continuation()
        .active()
        .filter(|active| Some(active.effect()) == effect)
        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
    let current_anchor = removal
        .position()
        .utf8_offset()
        .checked_sub(active.source_frontier())
        .and_then(|offset| active.successor_frontier().checked_add(offset))
        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
    let occurrence = removal.occurrence();
    let marker = DraftPieceMarkerV1::new(
        occurrence.marker_id(),
        occurrence.order_key(),
        occurrence.label(),
        occurrence.asset_id(),
    );
    let mut context = BuildContext::with_ordinal(
        storage,
        store,
        build.draft_id(),
        Some(build.session_id()),
        build.operation_id(),
        build.next_record_ordinal(),
    );
    let (sequence, _, _) = load_working_roots(&mut context, active.source_roots())?;
    let (located, leaf) = marker_location_by_witness(
        &mut context,
        sequence.ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?,
        &DraftPieceMarkerAtV1::new(current_anchor, marker),
    )?
    .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
    if leaf.key().id() != occurrence.sequence_leaf_id()
        || leaf.digest() != occurrence.sequence_leaf_digest()
    {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    Ok(Some(located.rank))
}

fn replacement_base_end(
    context: &mut BuildContext<'_>,
    base_sequence: Option<SequenceRef>,
    replacement: &DraftPieceReplacementV1,
    end: Boundary,
) -> Result<Boundary, DraftPiecePrepareErrorV1> {
    let removal = match replacement.marker_effect() {
        Some(DraftPieceMarkerEffectV1::Remove { removal, .. })
        | Some(DraftPieceMarkerEffectV1::SameIdReplacement { removal, .. }) => removal,
        _ => return Ok(end),
    };
    let sequence = base_sequence.ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
    let occurrence = removal.occurrence();
    let marker = DraftPieceMarkerV1::new(
        occurrence.marker_id(),
        occurrence.order_key(),
        occurrence.label(),
        occurrence.asset_id(),
    );
    let witness = DraftPieceMarkerAtV1::new(removal.position().utf8_offset(), marker);
    let removal_boundary = resolve_position(context, Some(sequence), removal.position())?;
    let (located, leaf) = marker_location_by_witness(context, sequence, &witness)?
        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
    if removal_boundary != end
        || removal_boundary
            != (Boundary {
                rank: located.rank,
                inner: 0,
            })
        || leaf.key().id() != occurrence.sequence_leaf_id()
        || leaf.digest() != occurrence.sequence_leaf_digest()
    {
        return Err(DraftPiecePrepareErrorV1::Rejected(
            DraftPieceRejectedReasonV1::Overlap,
        ));
    }
    Ok(Boundary {
        rank: located
            .rank
            .checked_add(1)
            .ok_or(DraftPiecePrepareErrorV1::Rejected(
                DraftPieceRejectedReasonV1::AggregateOverflow,
            ))?,
        inner: 0,
    })
}

fn begin_marker_effect(
    storage: &SyndicStorage,
    store: &HomeStore,
    build: &DraftPieceBuildRecordV1,
    fragment: &DraftPieceBuildFragmentV1,
    context: &mut BuildContext<'_>,
    roots: DraftPieceBuildRootsV1,
) -> Result<(DraftPieceBuildRootsV1, Option<u64>), DraftPiecePrepareErrorV1> {
    let Some(effect) = fragment.replacement().marker_effect() else {
        return Ok((roots, None));
    };
    if build.writer_admission().is_none()
        && !unadmitted_marker_builder_is_authorized_for_test(DraftPieceSettlementKeyV1::new(
            build.draft_id(),
            build.session_id(),
            build.operation_id(),
        ))
    {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    let removal = match effect {
        DraftPieceMarkerEffectV1::Remove { removal, .. }
        | DraftPieceMarkerEffectV1::Move { removal, .. }
        | DraftPieceMarkerEffectV1::SameIdReplacement { removal, .. } => Some(removal),
        DraftPieceMarkerEffectV1::Insert(_) => None,
    };
    let (mut sequence, mut index, mut marker_order) = load_working_roots(context, roots)?;
    let mut removal_rank = None;
    if let Some(removal) = removal {
        let occurrence = removal.occurrence();
        let marker = DraftPieceMarkerV1::new(
            occurrence.marker_id(),
            occurrence.order_key(),
            occurrence.label(),
            occurrence.asset_id(),
        );
        let mut base_context = BuildContext::with_ordinal(
            storage,
            store,
            build.draft_id(),
            build.predecessor_root().key().session_id(),
            build.predecessor_root().key().operation_id(),
            context.ordinal,
        );
        let base_sequence = load_root(&mut base_context, build.predecessor_root())?
            .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
        resolve_position(&mut base_context, Some(base_sequence), removal.position())?;
        let base_index = validate_index_root(&mut base_context, build.predecessor_root())?;
        let base_witness = DraftPieceMarkerAtV1::new(removal.position().utf8_offset(), marker);
        let (_, leaf) =
            marker_location_by_witness(&mut base_context, base_sequence, &base_witness)?
                .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
        validate_marker_effect_charge(effect, &leaf)?;
        if index_lookup(&mut base_context, base_index, occurrence.marker_id())? != Some(occurrence)
            || leaf.key().id() != occurrence.sequence_leaf_id()
            || leaf.digest() != occurrence.sequence_leaf_digest()
        {
            return Err(DraftPiecePrepareErrorV1::Rejected(
                DraftPieceRejectedReasonV1::Overlap,
            ));
        }
        if index_lookup(context, index, occurrence.marker_id())? != Some(occurrence) {
            return Err(DraftPiecePrepareErrorV1::Rejected(
                DraftPieceRejectedReasonV1::DuplicateMarkerIdentity,
            ));
        }
        let continuation = build.marker_effect_continuation();
        let current_anchor = removal
            .position()
            .utf8_offset()
            .checked_sub(continuation.source_logical_frontier())
            .and_then(|offset| {
                continuation
                    .successor_logical_frontier()
                    .checked_add(offset)
            })
            .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
        let current_witness = DraftPieceMarkerAtV1::new(current_anchor, marker);
        let current_sequence = sequence.ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
        let (located, _) = marker_location_by_witness(context, current_sequence, &current_witness)?
            .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
        if located.link.id() != occurrence.sequence_leaf_id()
            || located.link.digest() != occurrence.sequence_leaf_digest()
        {
            return Err(DraftPiecePrepareErrorV1::Rejected(
                DraftPieceRejectedReasonV1::Overlap,
            ));
        }
        removal_rank = Some(located.rank);
        let marker_rank = marker_rank_before_piece(context, Some(current_sequence), located.rank)?;
        let (prefix, tail) = split_sequence(
            context,
            current_sequence,
            Boundary {
                rank: located.rank,
                inner: 0,
            },
        )?;
        let tail = tail.ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
        let (_, suffix) = split_sequence(context, tail, Boundary { rank: 1, inner: 0 })?;
        sequence = join_sequence(context, prefix, suffix)?;
        index = index_delete(context, index, occurrence)?;
        marker_order = marker_order_delete(
            context,
            marker_order,
            marker_rank,
            (
                occurrence.marker_id(),
                occurrence.label(),
                occurrence.asset_id(),
            ),
        )?;
    } else if let DraftPieceMarkerEffectV1::Insert(insertion) = effect {
        if index_lookup(context, index, insertion.marker().marker_id())?.is_some() {
            return Err(DraftPiecePrepareErrorV1::Rejected(
                DraftPieceRejectedReasonV1::DuplicateMarkerIdentity,
            ));
        }
    }
    Ok((
        build_roots(context, sequence, index, marker_order)?,
        removal_rank,
    ))
}

fn validate_build_phase_cursor(
    storage: &SyndicStorage,
    store: &HomeStore,
    build: &DraftPieceBuildRecordV1,
    fragment: &DraftPieceBuildFragmentV1,
) -> Result<(), DraftPiecePrepareErrorV1> {
    let replacement = fragment.replacement();
    match build.frontier() {
        DraftPieceBuildFrontierV1::Planning { .. } => {}
        DraftPieceBuildFrontierV1::Removing {
            next_rank,
            end_rank,
            removed_markers,
            base_end,
            successor_start,
            successor_end,
            ..
        } => {
            if replacement.is_continuation() {
                return Err(DraftPiecePrepareErrorV1::InvalidRoot);
            }
            let (start, end, exact_base_end, exact_start, exact_end) =
                exact_replacement_boundaries(storage, store, build, replacement)?;
            if start > end
                || base_end != exact_base_end
                || successor_start != exact_start
                || successor_end != exact_end
                || end_rank != end.rank
                || removed_markers != 0
                || next_rank < start.rank
                || next_rank > end_rank
            {
                return Err(DraftPiecePrepareErrorV1::InvalidRoot);
            }
        }
        DraftPieceBuildFrontierV1::Applying {
            base_end,
            successor_start,
            successor_end,
            ..
        } => {
            if replacement.is_continuation() {
                return Err(DraftPiecePrepareErrorV1::InvalidRoot);
            }
            let (start, end, exact_base_end, exact_start, exact_end) =
                exact_replacement_boundaries(storage, store, build, replacement)?;
            if start > end
                || base_end != exact_base_end
                || successor_start != exact_start
                || successor_end != exact_end
            {
                return Err(DraftPiecePrepareErrorV1::InvalidRoot);
            }
        }
        DraftPieceBuildFrontierV1::Inserting {
            next_piece,
            next_byte,
            base_end,
            successor_end,
            ..
        } => {
            let pieces = replacement.inserted();
            if next_piece > pieces.len() as u64
                || (next_piece == pieces.len() as u64 && next_byte != 0)
                || successor_end.rank()
                    > build
                        .marker_effect_continuation()
                        .active()
                        .map_or(build.working_roots(), |active| active.working_roots())
                        .sequence_summary()
                        .piece_count()
            {
                return Err(DraftPiecePrepareErrorV1::InvalidRoot);
            }
            if next_piece < pieces.len() as u64 {
                match &pieces[next_piece as usize] {
                    DraftPieceV1::Text(text) => {
                        let byte = usize::try_from(next_byte)
                            .map_err(|_| DraftPiecePrepareErrorV1::InvalidRoot)?;
                        if byte >= text.len() || !text.is_char_boundary(byte) {
                            return Err(DraftPiecePrepareErrorV1::InvalidRoot);
                        }
                    }
                    DraftPieceV1::Marker(_) if next_byte != 0 => {
                        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
                    }
                    DraftPieceV1::Marker(_) => {}
                }
            }
            if replacement.is_continuation() {
                if fragment.key().ordinal() == 1 || base_end != build.base_frontier() {
                    return Err(DraftPiecePrepareErrorV1::InvalidRoot);
                }
                let previous = load_authenticated_build_fragment(
                    storage,
                    store,
                    build,
                    fragment.key().ordinal() - 1,
                )?;
                if previous.replacement().start() != replacement.start()
                    || previous.replacement().end() != replacement.end()
                {
                    return Err(DraftPiecePrepareErrorV1::InvalidRoot);
                }
            } else {
                let mut base_context = BuildContext::with_ordinal(
                    storage,
                    store,
                    build.draft_id(),
                    build.predecessor_root().key().session_id(),
                    build.predecessor_root().key().operation_id(),
                    build.next_record_ordinal(),
                );
                let base_sequence = load_root(&mut base_context, build.predecessor_root())?;
                let end = resolve_position(&mut base_context, base_sequence, replacement.end())?;
                let exact_base_end =
                    replacement_base_end(&mut base_context, base_sequence, replacement, end)?;
                if base_end != durable_boundary(exact_base_end) {
                    return Err(DraftPiecePrepareErrorV1::InvalidRoot);
                }
            }
        }
        _ => return Err(DraftPiecePrepareErrorV1::InvalidRoot),
    }
    Ok(())
}

pub(crate) fn advance_persistent_tree_build(
    storage: &SyndicStorage,
    store: &HomeStore,
    build: &DraftPieceBuildRecordV1,
    fragment: Option<&DraftPieceBuildFragmentV1>,
) -> Result<DraftPieceTreeQuantumV1, DraftPiecePrepareErrorV1> {
    if let Some(fragment) = fragment {
        validate_build_phase_cursor(storage, store, build, fragment)?;
    }
    let mut context = BuildContext::with_ordinal(
        storage,
        store,
        build.draft_id(),
        Some(build.session_id()),
        build.operation_id(),
        build.next_record_ordinal(),
    );
    let continuation = build.marker_effect_continuation();
    let mut roots = continuation
        .active()
        .map_or(build.working_roots(), |active| active.working_roots());
    let mut base_frontier = build.base_frontier();
    let mut successor_frontier = build.successor_frontier();
    let mut successor = None;
    let mut build_digest = None;
    let frontier = match build.frontier() {
        DraftPieceBuildFrontierV1::Planning { fragment_ordinal } => {
            let fragment = fragment.ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
            if fragment.key()
                != DraftPieceBuildFragmentKeyV1::new(
                    build.draft_id(),
                    build.session_id(),
                    build.operation_id(),
                    fragment_ordinal,
                )
            {
                return Err(DraftPiecePrepareErrorV1::InvalidRoot);
            }
            if fragment.replacement().is_continuation() {
                if fragment_ordinal == 1 || fragment.replacement().inserted().is_empty() {
                    return Err(DraftPiecePrepareErrorV1::Rejected(
                        DraftPieceRejectedReasonV1::OutOfOrder,
                    ));
                }
                let previous =
                    load_authenticated_build_fragment(storage, store, build, fragment_ordinal - 1)?;
                if previous.replacement().start() != fragment.replacement().start()
                    || previous.replacement().end() != fragment.replacement().end()
                {
                    return Err(DraftPiecePrepareErrorV1::Rejected(
                        DraftPieceRejectedReasonV1::OutOfOrder,
                    ));
                }
                return Ok(finish_quantum(
                    context,
                    roots,
                    base_frontier,
                    successor_frontier,
                    DraftPieceBuildFrontierV1::Inserting {
                        fragment_ordinal,
                        next_piece: 0,
                        next_byte: 0,
                        base_end: base_frontier,
                        successor_end: successor_frontier,
                    },
                    None,
                    None,
                ));
            }
            let (effect_roots, removal_rank) =
                begin_marker_effect(storage, store, build, fragment, &mut context, roots)?;
            roots = effect_roots;
            let base_sequence = load_root(&mut context, build.predecessor_root())?;
            let start =
                resolve_position(&mut context, base_sequence, fragment.replacement().start())?;
            let end = resolve_position(&mut context, base_sequence, fragment.replacement().end())?;
            let effective_end =
                replacement_base_end(&mut context, base_sequence, fragment.replacement(), end)?;
            if start > end {
                return Err(DraftPiecePrepareErrorV1::Rejected(
                    DraftPieceRejectedReasonV1::OutOfOrder,
                ));
            }
            let previous = checked_boundary(base_frontier)?;
            if start < previous {
                return Err(DraftPiecePrepareErrorV1::Rejected(
                    DraftPieceRejectedReasonV1::Overlap,
                ));
            }
            if fragment_ordinal != 1 && start == end && start == previous {
                let previous_fragment =
                    load_authenticated_build_fragment(storage, store, build, fragment_ordinal - 1)?;
                let previous_start = resolve_position(
                    &mut context,
                    base_sequence,
                    previous_fragment.replacement().start(),
                )?;
                let previous_end = resolve_position(
                    &mut context,
                    base_sequence,
                    previous_fragment.replacement().end(),
                )?;
                if previous_start == previous_end
                    && !(fragment.replacement().marker_effect().is_some()
                        && previous_fragment.replacement().marker_effect().is_some())
                {
                    return Err(DraftPiecePrepareErrorV1::Rejected(
                        DraftPieceRejectedReasonV1::DuplicateEmptyRange,
                    ));
                }
            }
            let mapped_start = boundary_after_marker_removal(
                mapped_boundary(base_frontier, successor_frontier, start)?,
                removal_rank,
            );
            let mapped_end = boundary_after_marker_removal(
                mapped_boundary(base_frontier, successor_frontier, end)?,
                removal_rank,
            );
            let (working_sequence, _, _) = load_working_roots(&mut context, roots)?;
            require_marker_free_range(&mut context, working_sequence, mapped_start, mapped_end)?;
            DraftPieceBuildFrontierV1::Removing {
                fragment_ordinal,
                next_rank: start.rank,
                end_rank: end.rank,
                removed_markers: 0,
                base_end: durable_boundary(effective_end),
                successor_start: durable_boundary(mapped_start),
                successor_end: durable_boundary(mapped_end),
            }
        }
        DraftPieceBuildFrontierV1::Removing {
            fragment_ordinal,
            next_rank,
            end_rank,
            removed_markers,
            base_end,
            successor_start,
            successor_end,
        } => {
            if next_rank < end_rank {
                DraftPieceBuildFrontierV1::Removing {
                    fragment_ordinal,
                    next_rank: next_rank + 1,
                    end_rank,
                    removed_markers,
                    base_end,
                    successor_start,
                    successor_end,
                }
            } else {
                DraftPieceBuildFrontierV1::Applying {
                    fragment_ordinal,
                    base_end,
                    successor_start,
                    successor_end,
                }
            }
        }
        DraftPieceBuildFrontierV1::Applying {
            fragment_ordinal,
            base_end,
            successor_start,
            successor_end,
        } => {
            let (sequence, index, marker_order) = load_working_roots(&mut context, roots)?;
            let start = checked_boundary(successor_start)?;
            let end = checked_boundary(successor_end)?;
            require_marker_free_range(&mut context, sequence, start, end)?;
            let (sequence, insertion) = if start == end {
                (sequence, start)
            } else {
                let (prefix, tail) = match sequence {
                    Some(tree) => split_sequence(&mut context, tree, start)?,
                    None => return Err(DraftPiecePrepareErrorV1::InvalidRoot),
                };
                let relative_end = Boundary {
                    rank: end.rank - start.rank,
                    inner: if end.rank == start.rank {
                        end.inner - start.inner
                    } else {
                        end.inner
                    },
                };
                let (_, suffix) = match tail {
                    Some(tail) => split_sequence(&mut context, tail, relative_end)?,
                    None if relative_end.rank == 0 && relative_end.inner == 0 => (None, None),
                    None => return Err(DraftPiecePrepareErrorV1::InvalidRoot),
                };
                let insertion = Boundary {
                    rank: prefix.map_or(0, |tree| tree.link.piece_count()),
                    inner: 0,
                };
                (join_sequence(&mut context, prefix, suffix)?, insertion)
            };
            roots = build_roots(&mut context, sequence, index, marker_order)?;
            DraftPieceBuildFrontierV1::Inserting {
                fragment_ordinal,
                next_piece: 0,
                next_byte: 0,
                base_end,
                successor_end: durable_boundary(insertion),
            }
        }
        DraftPieceBuildFrontierV1::Inserting {
            fragment_ordinal,
            next_piece,
            next_byte,
            base_end,
            successor_end,
        } => {
            let fragment = fragment.ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
            let pieces = fragment.replacement().inserted();
            if next_piece < pieces.len() as u64 {
                let (mut sequence, mut index, mut marker_order) =
                    load_working_roots(&mut context, roots)?;
                let boundary = checked_boundary(successor_end)?;
                let prefix_count = boundary
                    .rank
                    .checked_add(u64::from(boundary.inner != 0))
                    .ok_or(DraftPiecePrepareErrorV1::Rejected(
                        DraftPieceRejectedReasonV1::AggregateOverflow,
                    ))?;
                let mut next_end = Boundary {
                    rank: prefix_count,
                    inner: 0,
                };
                let mut next_text_byte = 0;
                match &pieces[next_piece as usize] {
                    DraftPieceV1::Text(text) => {
                        let start = usize::try_from(next_byte).map_err(|_| {
                            DraftPiecePrepareErrorV1::Rejected(
                                DraftPieceRejectedReasonV1::AggregateOverflow,
                            )
                        })?;
                        if start >= text.len() || !text.is_char_boundary(start) {
                            return Err(DraftPiecePrepareErrorV1::InvalidRoot);
                        }
                        let mut end = (start + DRAFT_PIECE_TEXT_LEAF_MAX_BYTES).min(text.len());
                        while !text.is_char_boundary(end) {
                            end -= 1;
                        }
                        let leaf = context.new_sequence_leaf(DraftPieceLeafValueV1::Text(
                            text[start..end].to_owned(),
                        ))?;
                        sequence = Some(insert_sequence_leaf(
                            &mut context,
                            sequence,
                            boundary,
                            leaf,
                        )?);
                        next_end.rank = next_end.rank.checked_add(1).ok_or(
                            DraftPiecePrepareErrorV1::Rejected(
                                DraftPieceRejectedReasonV1::AggregateOverflow,
                            ),
                        )?;
                        next_text_byte = end as u64;
                    }
                    DraftPieceV1::Marker(marker) => {
                        if next_byte != 0 {
                            return Err(DraftPiecePrepareErrorV1::InvalidRoot);
                        }
                        let insertion = match fragment
                            .replacement()
                            .marker_effect()
                            .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?
                        {
                            DraftPieceMarkerEffectV1::Insert(insertion)
                            | DraftPieceMarkerEffectV1::Move { insertion, .. }
                            | DraftPieceMarkerEffectV1::SameIdReplacement { insertion, .. } => {
                                insertion
                            }
                            DraftPieceMarkerEffectV1::Remove { .. } => {
                                return Err(DraftPiecePrepareErrorV1::InvalidRoot);
                            }
                        };
                        if insertion.marker() != *marker
                            || index_lookup(&mut context, index, marker.marker_id())?.is_some()
                        {
                            return Err(DraftPiecePrepareErrorV1::Rejected(
                                DraftPieceRejectedReasonV1::DuplicateMarkerIdentity,
                            ));
                        }
                        let boundary =
                            derived_marker_insertion_boundary(&mut context, sequence, insertion)?;
                        let marker_rank =
                            marker_rank_before_piece(&mut context, sequence, boundary.rank)?;
                        next_end = boundary_after_marker_insertion(
                            checked_boundary(successor_end)?,
                            boundary,
                        )?;
                        let leaf =
                            context.new_sequence_leaf(DraftPieceLeafValueV1::Marker(*marker))?;
                        let leaf_record = context
                            .sequence_leaves
                            .get(&leaf.link.id())
                            .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
                        validate_marker_effect_charge(
                            fragment
                                .replacement()
                                .marker_effect()
                                .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?,
                            leaf_record,
                        )?;
                        let occurrence = DraftMarkerIdentityOccurrenceV1::new(
                            marker.marker_id(),
                            marker.label(),
                            marker.asset_id(),
                            marker.order_key(),
                            leaf.link.id(),
                            leaf.link.digest(),
                        );
                        sequence = Some(insert_sequence_leaf(
                            &mut context,
                            sequence,
                            boundary,
                            leaf,
                        )?);
                        index = index_insert(&mut context, index, occurrence)?;
                        marker_order = marker_order_insert(
                            &mut context,
                            marker_order,
                            marker_rank,
                            marker.marker_id(),
                            marker.label(),
                            marker.asset_id(),
                        )?;
                    }
                }
                roots = build_roots(&mut context, sequence, index, marker_order)?;
                let next_end = durable_boundary(next_end);
                if matches!(&pieces[next_piece as usize], DraftPieceV1::Text(text) if next_text_byte < text.len() as u64)
                {
                    DraftPieceBuildFrontierV1::Inserting {
                        fragment_ordinal,
                        next_piece,
                        next_byte: next_text_byte,
                        base_end,
                        successor_end: next_end,
                    }
                } else if next_piece + 1 < pieces.len() as u64 {
                    DraftPieceBuildFrontierV1::Inserting {
                        fragment_ordinal,
                        next_piece: next_piece + 1,
                        next_byte: 0,
                        base_end,
                        successor_end: next_end,
                    }
                } else {
                    base_frontier = base_end;
                    successor_frontier = next_end;
                    if fragment_ordinal < build.fragment_count() {
                        DraftPieceBuildFrontierV1::Planning {
                            fragment_ordinal: fragment_ordinal + 1,
                        }
                    } else {
                        DraftPieceBuildFrontierV1::CrossValidating
                    }
                }
            } else {
                base_frontier = base_end;
                successor_frontier = successor_end;
                if fragment_ordinal < build.fragment_count() {
                    DraftPieceBuildFrontierV1::Planning {
                        fragment_ordinal: fragment_ordinal + 1,
                    }
                } else {
                    DraftPieceBuildFrontierV1::CrossValidating
                }
            }
        }
        DraftPieceBuildFrontierV1::CrossValidating => {
            let (sequence, index, marker_order) = load_working_roots(&mut context, roots)?;
            let sequence_count = sequence.map_or(0, |tree| tree.link.marker_count());
            let index_count = index.map_or(0, |tree| tree.link.record_count());
            let commitment_count = marker_order.map_or(0, |tree| tree.link.marker_count());
            if sequence_count != index_count || sequence_count != commitment_count {
                return Err(DraftPiecePrepareErrorV1::Rejected(
                    DraftPieceRejectedReasonV1::DuplicateMarkerIdentity,
                ));
            }
            resolve_position(&mut context, sequence, build.caret())?;
            resolve_position(&mut context, sequence, build.selection())?;
            let root = finalize_build_root(
                &mut context,
                build.operation_id(),
                sequence,
                index,
                marker_order,
            )?;
            roots = DraftPieceBuildRootsV1::from_root(root.reference());
            build_digest = Some(digest_parts(
                b"syndic/draft-piece-build/v3",
                &[
                    build.proposal_digest().as_bytes(),
                    root.reference().combined_digest().as_bytes(),
                ],
            ));
            successor = Some(root);
            DraftPieceBuildFrontierV1::Complete
        }
        _ => return Err(DraftPiecePrepareErrorV1::InvalidRoot),
    };
    Ok(finish_quantum(
        context,
        roots,
        base_frontier,
        successor_frontier,
        frontier,
        successor,
        build_digest,
    ))
}

fn read_context<'a>(
    storage: &'a SyndicStorage,
    store: &'a HomeStore,
    root: DraftPieceRootReferenceV1,
) -> Result<(BuildContext<'a>, Option<SequenceRef>), DraftPiecePrepareErrorV1> {
    let mut context = BuildContext::new(
        storage,
        store,
        root.key().draft_id(),
        root.key().session_id(),
        root.key().operation_id(),
    );
    let sequence = load_root(&mut context, root)?;
    Ok((context, sequence))
}

#[derive(Clone)]
struct CursorFrame {
    tree: SequenceRef,
    node: DraftPieceNodeRecordV1,
    index: usize,
    anchor: u64,
    rank: u64,
}

#[derive(Clone)]
struct SequenceCursor {
    frames: Vec<CursorFrame>,
    leaf: LocatedLeaf,
}

pub(crate) struct DraftPieceMaterializationPageV1 {
    pieces: Vec<DraftPieceLeafValueV1>,
    records_read: u64,
    payload_bytes: usize,
}

impl DraftPieceMaterializationPageV1 {
    pub(crate) fn pieces(&self) -> &[DraftPieceLeafValueV1] {
        &self.pieces
    }

    pub(crate) const fn records_read(&self) -> u64 {
        self.records_read
    }

    pub(crate) const fn payload_bytes(&self) -> usize {
        self.payload_bytes
    }
}

fn piece_cursor(
    context: &mut BuildContext<'_>,
    tree: SequenceRef,
    target_rank: u64,
) -> Result<SequenceCursor, DraftPiecePrepareErrorV1> {
    if target_rank >= tree.link.piece_count() {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    let mut current = tree;
    let mut remaining = target_rank;
    let mut anchor = 0_u64;
    let mut rank = 0_u64;
    let mut frames = Vec::new();
    while current.height != 0 {
        let node =
            context.load_sequence_node(current.link, current.height, current.selected_root)?;
        let mut selected = None;
        for (index, child) in node.children().iter().copied().enumerate() {
            if remaining < child.piece_count() {
                selected = Some((index, child));
                break;
            }
            remaining -= child.piece_count();
            anchor = anchor.checked_add(child.logical_utf8_bytes()).ok_or(
                DraftPiecePrepareErrorV1::Rejected(DraftPieceRejectedReasonV1::AggregateOverflow),
            )?;
            rank =
                rank.checked_add(child.piece_count())
                    .ok_or(DraftPiecePrepareErrorV1::Rejected(
                        DraftPieceRejectedReasonV1::AggregateOverflow,
                    ))?;
        }
        let (index, child) = selected.ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
        frames.push(CursorFrame {
            tree: current,
            node,
            index,
            anchor,
            rank,
        });
        current = SequenceRef {
            link: child,
            height: current.height - 1,
            selected_root: false,
        };
    }
    Ok(SequenceCursor {
        frames,
        leaf: LocatedLeaf {
            rank: target_rank,
            anchor,
            link: current.link,
        },
    })
}

fn descend_matching(
    context: &mut BuildContext<'_>,
    mut tree: SequenceRef,
    mut anchor: u64,
    mut rank: u64,
    frames: &mut Vec<CursorFrame>,
    matches: fn(DraftPieceChildV1) -> bool,
) -> Result<LocatedLeaf, DraftPiecePrepareErrorV1> {
    while tree.height != 0 {
        let node = context.load_sequence_node(tree.link, tree.height, tree.selected_root)?;
        let node_anchor = anchor;
        let node_rank = rank;
        let index = node
            .children()
            .iter()
            .position(|child| matches(*child))
            .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
        for child in &node.children()[..index] {
            anchor = anchor.checked_add(child.logical_utf8_bytes()).ok_or(
                DraftPiecePrepareErrorV1::Rejected(DraftPieceRejectedReasonV1::AggregateOverflow),
            )?;
            rank =
                rank.checked_add(child.piece_count())
                    .ok_or(DraftPiecePrepareErrorV1::Rejected(
                        DraftPieceRejectedReasonV1::AggregateOverflow,
                    ))?;
        }
        let child = node.children()[index];
        frames.push(CursorFrame {
            tree,
            node,
            index,
            anchor: node_anchor,
            rank: node_rank,
        });
        tree = SequenceRef {
            link: child,
            height: tree.height - 1,
            selected_root: false,
        };
    }
    Ok(LocatedLeaf {
        rank,
        anchor,
        link: tree.link,
    })
}

fn advance_matching(
    context: &mut BuildContext<'_>,
    cursor: &mut SequenceCursor,
    matches: fn(DraftPieceChildV1) -> bool,
) -> Result<bool, DraftPiecePrepareErrorV1> {
    while let Some(mut frame) = cursor.frames.pop() {
        let mut anchor = frame.anchor;
        let mut rank = frame.rank;
        for child in &frame.node.children()[..=frame.index] {
            anchor = anchor.checked_add(child.logical_utf8_bytes()).ok_or(
                DraftPiecePrepareErrorV1::Rejected(DraftPieceRejectedReasonV1::AggregateOverflow),
            )?;
            rank =
                rank.checked_add(child.piece_count())
                    .ok_or(DraftPiecePrepareErrorV1::Rejected(
                        DraftPieceRejectedReasonV1::AggregateOverflow,
                    ))?;
        }
        let Some(index) = frame.node.children()[frame.index + 1..]
            .iter()
            .position(|child| matches(*child))
            .map(|offset| frame.index + 1 + offset)
        else {
            continue;
        };
        for child in &frame.node.children()[frame.index + 1..index] {
            anchor = anchor.checked_add(child.logical_utf8_bytes()).ok_or(
                DraftPiecePrepareErrorV1::Rejected(DraftPieceRejectedReasonV1::AggregateOverflow),
            )?;
            rank =
                rank.checked_add(child.piece_count())
                    .ok_or(DraftPiecePrepareErrorV1::Rejected(
                        DraftPieceRejectedReasonV1::AggregateOverflow,
                    ))?;
        }
        frame.index = index;
        cursor.frames.push(frame);
        let child = cursor.frames.last().expect("pushed").node.children()[index];
        cursor.leaf = descend_matching(
            context,
            SequenceRef {
                link: child,
                height: cursor.frames.last().expect("pushed").tree.height - 1,
                selected_root: false,
            },
            anchor,
            rank,
            &mut cursor.frames,
            matches,
        )?;
        return Ok(true);
    }
    Ok(false)
}

fn descend_matching_reverse(
    context: &mut BuildContext<'_>,
    mut tree: SequenceRef,
    mut anchor: u64,
    mut rank: u64,
    frames: &mut Vec<CursorFrame>,
    matches: fn(DraftPieceChildV1) -> bool,
) -> Result<LocatedLeaf, DraftPiecePrepareErrorV1> {
    while tree.height != 0 {
        let node = context.load_sequence_node(tree.link, tree.height, tree.selected_root)?;
        let node_anchor = anchor;
        let node_rank = rank;
        let index = node
            .children()
            .iter()
            .rposition(|child| matches(*child))
            .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
        for child in &node.children()[..index] {
            anchor = anchor.checked_add(child.logical_utf8_bytes()).ok_or(
                DraftPiecePrepareErrorV1::Rejected(DraftPieceRejectedReasonV1::AggregateOverflow),
            )?;
            rank =
                rank.checked_add(child.piece_count())
                    .ok_or(DraftPiecePrepareErrorV1::Rejected(
                        DraftPieceRejectedReasonV1::AggregateOverflow,
                    ))?;
        }
        let child = node.children()[index];
        frames.push(CursorFrame {
            tree,
            node,
            index,
            anchor: node_anchor,
            rank: node_rank,
        });
        tree = SequenceRef {
            link: child,
            height: tree.height - 1,
            selected_root: false,
        };
    }
    Ok(LocatedLeaf {
        rank,
        anchor,
        link: tree.link,
    })
}

fn retreat_matching(
    context: &mut BuildContext<'_>,
    cursor: &mut SequenceCursor,
    matches: fn(DraftPieceChildV1) -> bool,
) -> Result<bool, DraftPiecePrepareErrorV1> {
    while let Some(mut frame) = cursor.frames.pop() {
        let Some(index) = frame.node.children()[..frame.index]
            .iter()
            .rposition(|child| matches(*child))
        else {
            continue;
        };
        let mut anchor = frame.anchor;
        let mut rank = frame.rank;
        for child in &frame.node.children()[..index] {
            anchor = anchor.checked_add(child.logical_utf8_bytes()).ok_or(
                DraftPiecePrepareErrorV1::Rejected(DraftPieceRejectedReasonV1::AggregateOverflow),
            )?;
            rank =
                rank.checked_add(child.piece_count())
                    .ok_or(DraftPiecePrepareErrorV1::Rejected(
                        DraftPieceRejectedReasonV1::AggregateOverflow,
                    ))?;
        }
        frame.index = index;
        let child = frame.node.children()[index];
        let height = frame.tree.height - 1;
        cursor.frames.push(frame);
        cursor.leaf = descend_matching_reverse(
            context,
            SequenceRef {
                link: child,
                height,
                selected_root: false,
            },
            anchor,
            rank,
            &mut cursor.frames,
            matches,
        )?;
        return Ok(true);
    }
    Ok(false)
}

fn last_matching_cursor(
    context: &mut BuildContext<'_>,
    tree: SequenceRef,
    matches: fn(DraftPieceChildV1) -> bool,
) -> Result<Option<SequenceCursor>, DraftPiecePrepareErrorV1> {
    if !matches(tree.link) {
        return Ok(None);
    }
    let mut frames = Vec::new();
    let leaf = descend_matching_reverse(context, tree, 0, 0, &mut frames, matches)?;
    Ok(Some(SequenceCursor { frames, leaf }))
}

fn text_cursor(
    context: &mut BuildContext<'_>,
    tree: SequenceRef,
    offset: u64,
) -> Result<Option<(SequenceCursor, usize)>, DraftPiecePrepareErrorV1> {
    if offset == tree.link.logical_utf8_bytes() {
        return Ok(None);
    }
    let mut current = tree;
    let mut remaining = offset;
    let mut anchor = 0_u64;
    let mut rank = 0_u64;
    let mut frames = Vec::new();
    while current.height != 0 {
        let node =
            context.load_sequence_node(current.link, current.height, current.selected_root)?;
        let mut selected = None;
        for (index, child) in node.children().iter().copied().enumerate() {
            if child.logical_utf8_bytes() != 0 && remaining < child.logical_utf8_bytes() {
                selected = Some((index, child));
                break;
            }
            remaining = remaining.checked_sub(child.logical_utf8_bytes()).ok_or(
                DraftPiecePrepareErrorV1::Rejected(DraftPieceRejectedReasonV1::AggregateOverflow),
            )?;
            anchor = anchor.checked_add(child.logical_utf8_bytes()).ok_or(
                DraftPiecePrepareErrorV1::Rejected(DraftPieceRejectedReasonV1::AggregateOverflow),
            )?;
            rank =
                rank.checked_add(child.piece_count())
                    .ok_or(DraftPiecePrepareErrorV1::Rejected(
                        DraftPieceRejectedReasonV1::AggregateOverflow,
                    ))?;
        }
        let (index, child) = selected.ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
        frames.push(CursorFrame {
            tree: current,
            node,
            index,
            anchor,
            rank,
        });
        current = SequenceRef {
            link: child,
            height: current.height - 1,
            selected_root: false,
        };
    }
    let leaf = LocatedLeaf {
        rank,
        anchor,
        link: current.link,
    };
    Ok(Some((
        SequenceCursor { frames, leaf },
        usize::try_from(remaining).map_err(|_| {
            DraftPiecePrepareErrorV1::Rejected(DraftPieceRejectedReasonV1::AggregateOverflow)
        })?,
    )))
}

fn marker_cursor(
    context: &mut BuildContext<'_>,
    tree: SequenceRef,
    target: DraftCompositeSearchKeyV1,
) -> Result<Option<SequenceCursor>, DraftPiecePrepareErrorV1> {
    let mut current = tree;
    let mut anchor = 0_u64;
    let mut rank = 0_u64;
    let mut frames = Vec::new();
    while current.height != 0 {
        let node =
            context.load_sequence_node(current.link, current.height, current.selected_root)?;
        let mut prefix_anchor = anchor;
        let mut prefix_rank = rank;
        let mut selected = None;
        for (index, child) in node.children().iter().copied().enumerate() {
            if child.marker_count() != 0
                && checked_offset_key(child.last(), prefix_anchor)
                    .map_err(DraftPiecePrepareErrorV1::Rejected)?
                    >= target
            {
                selected = Some((index, child, prefix_anchor, prefix_rank));
                break;
            }
            prefix_anchor = prefix_anchor
                .checked_add(child.logical_utf8_bytes())
                .ok_or(DraftPiecePrepareErrorV1::Rejected(
                    DraftPieceRejectedReasonV1::AggregateOverflow,
                ))?;
            prefix_rank = prefix_rank.checked_add(child.piece_count()).ok_or(
                DraftPiecePrepareErrorV1::Rejected(DraftPieceRejectedReasonV1::AggregateOverflow),
            )?;
        }
        let Some((index, child, child_anchor, child_rank)) = selected else {
            return Ok(None);
        };
        frames.push(CursorFrame {
            tree: current,
            node,
            index,
            anchor,
            rank,
        });
        anchor = child_anchor;
        rank = child_rank;
        current = SequenceRef {
            link: child,
            height: current.height - 1,
            selected_root: false,
        };
    }
    Ok(Some(SequenceCursor {
        frames,
        leaf: LocatedLeaf {
            rank,
            anchor,
            link: current.link,
        },
    }))
}

fn read_forward_text_demand(
    storage: &SyndicStorage,
    store: &HomeStore,
    root: DraftPieceRootReferenceV1,
    start: u64,
    max_bytes: usize,
) -> Result<DraftPieceTextDemandResultV1, DraftPiecePrepareErrorV1> {
    let (mut context, sequence) = read_context(storage, store, root)?;
    if start > root.summary().logical_utf8_bytes() {
        return Err(DraftPiecePrepareErrorV1::Rejected(
            DraftPieceRejectedReasonV1::InvalidUtf8Boundary,
        ));
    }
    let Some(sequence) = sequence else {
        return Ok(DraftPieceTextDemandResultV1::new(
            root,
            DraftPieceTextDemandV1::Forward(start),
            start,
            start,
            Vec::new(),
            DraftPieceTextEdgeFactV1::DocumentStart,
            DraftPieceTextEdgeFactV1::DocumentEnd,
            context.records_read,
        ));
    };
    let mut bytes = Vec::new();
    let Some((mut cursor, mut from)) = text_cursor(&mut context, sequence, start)? else {
        return Ok(DraftPieceTextDemandResultV1::new(
            root,
            DraftPieceTextDemandV1::Forward(start),
            start,
            start,
            bytes,
            if start == 0 {
                DraftPieceTextEdgeFactV1::DocumentStart
            } else {
                DraftPieceTextEdgeFactV1::Continuation(start)
            },
            DraftPieceTextEdgeFactV1::DocumentEnd,
            context.records_read,
        ));
    };
    let mut visited = 0_usize;
    loop {
        let leaf = context.load_sequence_leaf(cursor.leaf.link)?;
        let DraftPieceLeafValueV1::Text(text) = leaf.value() else {
            return Err(DraftPiecePrepareErrorV1::InvalidRoot);
        };
        if !text.is_char_boundary(from) {
            return Err(DraftPiecePrepareErrorV1::Rejected(
                DraftPieceRejectedReasonV1::InvalidUtf8Boundary,
            ));
        }
        let mut take = (max_bytes - bytes.len()).min(text.len() - from);
        while take != 0 && !text.is_char_boundary(from + take) {
            take -= 1;
        }
        bytes.extend_from_slice(&text.as_bytes()[from..from + take]);
        visited += 1;
        if take < text.len() - from
            || bytes.len() == max_bytes
            || visited == DRAFT_PIECE_PAGE_MAX_RECORDS
            || !advance_matching(&mut context, &mut cursor, |child| {
                child.logical_utf8_bytes() != 0
            })?
        {
            break;
        }
        from = 0;
    }
    let consumed = bytes.len() as u64;
    let end = start + consumed;
    Ok(DraftPieceTextDemandResultV1::new(
        root,
        DraftPieceTextDemandV1::Forward(start),
        start,
        end,
        bytes,
        if start == 0 {
            DraftPieceTextEdgeFactV1::DocumentStart
        } else {
            DraftPieceTextEdgeFactV1::Continuation(start)
        },
        if end == root.summary().logical_utf8_bytes() {
            DraftPieceTextEdgeFactV1::DocumentEnd
        } else {
            DraftPieceTextEdgeFactV1::Continuation(end)
        },
        context.records_read,
    ))
}

pub(crate) fn read_text_demand(
    storage: &SyndicStorage,
    store: &HomeStore,
    root: DraftPieceRootReferenceV1,
    demand: DraftPieceTextDemandV1,
    max_bytes: usize,
) -> Result<DraftPieceTextDemandResultV1, DraftPiecePrepareErrorV1> {
    let extent = root.summary().logical_utf8_bytes();
    let coordinate = match demand {
        DraftPieceTextDemandV1::Forward(value)
        | DraftPieceTextDemandV1::Backward(value)
        | DraftPieceTextDemandV1::Validate(value) => value,
    };
    if coordinate > extent {
        return Err(DraftPiecePrepareErrorV1::Rejected(
            DraftPieceRejectedReasonV1::InvalidUtf8Boundary,
        ));
    }
    if let DraftPieceTextDemandV1::Forward(start) = demand {
        return read_forward_text_demand(storage, store, root, start, max_bytes);
    }
    let (mut context, sequence) = read_context(storage, store, root)?;
    let Some(sequence) = sequence else {
        return Ok(DraftPieceTextDemandResultV1::new(
            root,
            demand,
            0,
            0,
            Vec::new(),
            DraftPieceTextEdgeFactV1::DocumentStart,
            DraftPieceTextEdgeFactV1::DocumentEnd,
            context.records_read,
        ));
    };
    match demand {
        DraftPieceTextDemandV1::Backward(end) => {
            if end == 0 {
                return Ok(DraftPieceTextDemandResultV1::new(
                    root,
                    demand,
                    0,
                    0,
                    Vec::new(),
                    DraftPieceTextEdgeFactV1::DocumentStart,
                    if extent == 0 {
                        DraftPieceTextEdgeFactV1::DocumentEnd
                    } else {
                        DraftPieceTextEdgeFactV1::Continuation(0)
                    },
                    context.records_read,
                ));
            }
            let (mut cursor, from) = text_cursor(&mut context, sequence, end - 1)?
                .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
            let mut leaf_end = from + 1;
            let first = context.load_sequence_leaf(cursor.leaf.link)?;
            let DraftPieceLeafValueV1::Text(first_text) = first.value() else {
                return Err(DraftPiecePrepareErrorV1::InvalidRoot);
            };
            if !first_text.is_char_boundary(leaf_end) {
                return Err(DraftPiecePrepareErrorV1::Rejected(
                    DraftPieceRejectedReasonV1::InvalidUtf8Boundary,
                ));
            }
            let mut chunks = Vec::new();
            let mut retained = 0_usize;
            let mut visited = 0_usize;
            loop {
                let leaf = context.load_sequence_leaf(cursor.leaf.link)?;
                let DraftPieceLeafValueV1::Text(text) = leaf.value() else {
                    return Err(DraftPiecePrepareErrorV1::InvalidRoot);
                };
                let available = max_bytes - retained;
                let mut start = leaf_end.saturating_sub(available);
                while start < leaf_end && !text.is_char_boundary(start) {
                    start += 1;
                }
                if start == leaf_end && retained != 0 {
                    break;
                }
                chunks.push(text.as_bytes()[start..leaf_end].to_vec());
                retained += leaf_end - start;
                visited += 1;
                if start != 0
                    || retained == max_bytes
                    || visited == DRAFT_PIECE_PAGE_MAX_RECORDS
                    || !retreat_matching(&mut context, &mut cursor, |child| {
                        child.logical_utf8_bytes() != 0
                    })?
                {
                    break;
                }
                let leaf = context.load_sequence_leaf(cursor.leaf.link)?;
                let DraftPieceLeafValueV1::Text(text) = leaf.value() else {
                    return Err(DraftPiecePrepareErrorV1::InvalidRoot);
                };
                leaf_end = text.len();
            }
            chunks.reverse();
            let mut bytes = Vec::with_capacity(retained);
            for chunk in chunks {
                bytes.extend_from_slice(&chunk);
            }
            let start = end
                .checked_sub(bytes.len() as u64)
                .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
            Ok(DraftPieceTextDemandResultV1::new(
                root,
                demand,
                start,
                end,
                bytes,
                if start == 0 {
                    DraftPieceTextEdgeFactV1::DocumentStart
                } else {
                    DraftPieceTextEdgeFactV1::Continuation(start)
                },
                if end == extent {
                    DraftPieceTextEdgeFactV1::DocumentEnd
                } else {
                    DraftPieceTextEdgeFactV1::Continuation(end)
                },
                context.records_read,
            ))
        }
        DraftPieceTextDemandV1::Validate(candidate) => {
            let probe = if candidate == extent {
                candidate.saturating_sub(1)
            } else {
                candidate
            };
            let Some((cursor, from)) = text_cursor(&mut context, sequence, probe)? else {
                return Err(DraftPiecePrepareErrorV1::InvalidRoot);
            };
            let leaf = context.load_sequence_leaf(cursor.leaf.link)?;
            let DraftPieceLeafValueV1::Text(text) = leaf.value() else {
                return Err(DraftPiecePrepareErrorV1::InvalidRoot);
            };
            let mut local_start = from;
            while local_start != 0 && !text.is_char_boundary(local_start) {
                local_start -= 1;
            }
            let mut local_end = if candidate == extent {
                from + 1
            } else {
                from.max(local_start) + 1
            };
            while local_end < text.len() && !text.is_char_boundary(local_end) {
                local_end += 1;
            }
            if local_end > text.len() || local_end - local_start > max_bytes {
                return Err(DraftPiecePrepareErrorV1::InvalidRoot);
            }
            let start = cursor
                .leaf
                .anchor
                .checked_add(local_start as u64)
                .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
            let end = cursor
                .leaf
                .anchor
                .checked_add(local_end as u64)
                .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
            Ok(DraftPieceTextDemandResultV1::new(
                root,
                demand,
                start,
                end,
                text.as_bytes()[local_start..local_end].to_vec(),
                if start == 0 {
                    DraftPieceTextEdgeFactV1::DocumentStart
                } else {
                    DraftPieceTextEdgeFactV1::Continuation(start)
                },
                if end == extent {
                    DraftPieceTextEdgeFactV1::DocumentEnd
                } else {
                    DraftPieceTextEdgeFactV1::Continuation(end)
                },
                context.records_read,
            ))
        }
        DraftPieceTextDemandV1::Forward(_) => unreachable!(),
    }
}

pub(crate) fn read_materialization_page(
    storage: &SyndicStorage,
    store: &HomeStore,
    root: DraftPieceRootReferenceV1,
    start_rank: u64,
    max_records: usize,
    max_bytes: usize,
) -> Result<DraftPieceMaterializationPageV1, DraftPiecePrepareErrorV1> {
    let (mut context, sequence) = read_context(storage, store, root)?;
    if start_rank > root.summary().piece_count() || max_records == 0 || max_bytes == 0 {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    }
    let Some(sequence) = sequence else {
        return Ok(DraftPieceMaterializationPageV1 {
            pieces: Vec::new(),
            records_read: context.records_read,
            payload_bytes: 0,
        });
    };
    if start_rank == root.summary().piece_count() {
        return Ok(DraftPieceMaterializationPageV1 {
            pieces: Vec::new(),
            records_read: context.records_read,
            payload_bytes: 0,
        });
    }
    let mut cursor = piece_cursor(&mut context, sequence, start_rank)?;
    let mut pieces = Vec::new();
    let mut payload_bytes = 0_usize;
    loop {
        let leaf = context.load_sequence_leaf(cursor.leaf.link)?;
        let bytes = match leaf.value() {
            DraftPieceLeafValueV1::Text(text) => text.len(),
            DraftPieceLeafValueV1::Marker(_) => 25,
        };
        if !pieces.is_empty()
            && payload_bytes
                .checked_add(bytes)
                .is_none_or(|total| total > max_bytes)
        {
            break;
        }
        payload_bytes =
            payload_bytes
                .checked_add(bytes)
                .ok_or(DraftPiecePrepareErrorV1::Rejected(
                    DraftPieceRejectedReasonV1::AggregateOverflow,
                ))?;
        pieces.push(leaf.value().clone());
        if pieces.len() == max_records || !advance_matching(&mut context, &mut cursor, |_| true)? {
            break;
        }
    }
    Ok(DraftPieceMaterializationPageV1 {
        pieces,
        records_read: context.records_read,
        payload_bytes,
    })
}

fn marker_at_cursor(
    context: &mut BuildContext<'_>,
    cursor: &SequenceCursor,
) -> Result<(DraftCompositeSearchKeyV1, DraftPieceMarkerAtV1), DraftPiecePrepareErrorV1> {
    let leaf = context.load_sequence_leaf(cursor.leaf.link)?;
    let DraftPieceLeafValueV1::Marker(marker) = leaf.value() else {
        return Err(DraftPiecePrepareErrorV1::InvalidRoot);
    };
    Ok((
        DraftCompositeSearchKeyV1::Marker {
            anchor: cursor.leaf.anchor,
            order_key: marker.order_key(),
            marker_id: marker.marker_id(),
        },
        DraftPieceMarkerAtV1::new(cursor.leaf.anchor, *marker),
    ))
}

fn marker_in_scope(anchor: u64, scope: DraftPieceMarkerScopeV1) -> bool {
    match scope {
        DraftPieceMarkerScopeV1::Range { start, end } => start <= anchor && anchor < end,
        DraftPieceMarkerScopeV1::InclusiveRange { start, end } => start <= anchor && anchor <= end,
        DraftPieceMarkerScopeV1::ExactAnchor(expected) => anchor == expected,
    }
}

fn marker_retained_bytes(
    count: usize,
    preceding: DraftPieceMarkerEdgeFactV1,
    following: DraftPieceMarkerEdgeFactV1,
    continuation: Option<DraftCompositeSearchKeyV1>,
) -> usize {
    fn edge(value: DraftPieceMarkerEdgeFactV1) -> usize {
        match value {
            DraftPieceMarkerEdgeFactV1::RangeStart | DraftPieceMarkerEdgeFactV1::RangeEnd => 1,
            DraftPieceMarkerEdgeFactV1::Marker(_) => 34,
        }
    }
    count * 40 + edge(preceding) + edge(following) + 1 + 1 + continuation.map_or(0, |_| 33)
}

fn cursor_edge(
    context: &mut BuildContext<'_>,
    cursor: &SequenceCursor,
    scope: DraftPieceMarkerScopeV1,
    preceding: bool,
) -> Result<DraftPieceMarkerEdgeFactV1, DraftPiecePrepareErrorV1> {
    let mut adjacent = cursor.clone();
    let exists = if preceding {
        retreat_matching(context, &mut adjacent, |child| child.marker_count() != 0)?
    } else {
        advance_matching(context, &mut adjacent, |child| child.marker_count() != 0)?
    };
    if exists {
        let (key, marker) = marker_at_cursor(context, &adjacent)?;
        if marker_in_scope(marker.anchor(), scope) {
            return Ok(DraftPieceMarkerEdgeFactV1::Marker(key));
        }
    }
    Ok(if preceding {
        DraftPieceMarkerEdgeFactV1::RangeStart
    } else {
        DraftPieceMarkerEdgeFactV1::RangeEnd
    })
}

pub(crate) fn read_marker_demand(
    storage: &SyndicStorage,
    store: &HomeStore,
    root: DraftPieceRootReferenceV1,
    demand: &DraftPieceMarkerDemandV1,
) -> Result<DraftPieceMarkerDemandResultV1, DraftPiecePrepareErrorV1> {
    let (start, end, inclusive_end) = demand.scope().bounds();
    if start > root.summary().logical_utf8_bytes()
        || end > root.summary().logical_utf8_bytes()
        || start > end
    {
        return Err(DraftPiecePrepareErrorV1::Rejected(
            DraftPieceRejectedReasonV1::InvalidGapWitness,
        ));
    }
    let (mut context, sequence) = read_context(storage, store, root)?;
    let Some(sequence) = sequence else {
        let retained = marker_retained_bytes(
            0,
            DraftPieceMarkerEdgeFactV1::RangeStart,
            DraftPieceMarkerEdgeFactV1::RangeEnd,
            None,
        );
        if retained > demand.retained_byte_ceiling() {
            return Err(DraftPiecePrepareErrorV1::Rejected(
                DraftPieceRejectedReasonV1::TreeLimit,
            ));
        }
        return Ok(DraftPieceMarkerDemandResultV1::new(
            root,
            demand,
            Vec::new(),
            DraftPieceMarkerEdgeFactV1::RangeStart,
            DraftPieceMarkerEdgeFactV1::RangeEnd,
            true,
            None,
            retained,
            context.records_read,
        ));
    };
    let mut cursor = match demand.direction() {
        DraftPieceMarkerDirectionV1::Forward => {
            let target = demand
                .cursor()
                .unwrap_or(DraftCompositeSearchKeyV1::BeforeMarkers(start));
            let mut cursor = marker_cursor(&mut context, sequence, target)?;
            if let Some(expected) = demand.cursor() {
                let Some(current) = cursor.as_mut() else {
                    return Err(DraftPiecePrepareErrorV1::Rejected(
                        DraftPieceRejectedReasonV1::InvalidGapWitness,
                    ));
                };
                if marker_at_cursor(&mut context, current)?.0 != expected {
                    return Err(DraftPiecePrepareErrorV1::Rejected(
                        DraftPieceRejectedReasonV1::InvalidGapWitness,
                    ));
                }
                if !marker_in_scope(
                    marker_at_cursor(&mut context, current)?.1.anchor(),
                    demand.scope(),
                ) {
                    return Err(DraftPiecePrepareErrorV1::Rejected(
                        DraftPieceRejectedReasonV1::InvalidGapWitness,
                    ));
                }
                if !advance_matching(&mut context, current, |child| child.marker_count() != 0)? {
                    cursor = None;
                }
            }
            cursor
        }
        DraftPieceMarkerDirectionV1::Backward => {
            if let Some(expected) = demand.cursor() {
                let mut cursor = marker_cursor(&mut context, sequence, expected)?.ok_or(
                    DraftPiecePrepareErrorV1::Rejected(
                        DraftPieceRejectedReasonV1::InvalidGapWitness,
                    ),
                )?;
                if marker_at_cursor(&mut context, &cursor)?.0 != expected {
                    return Err(DraftPiecePrepareErrorV1::Rejected(
                        DraftPieceRejectedReasonV1::InvalidGapWitness,
                    ));
                }
                if !marker_in_scope(
                    marker_at_cursor(&mut context, &cursor)?.1.anchor(),
                    demand.scope(),
                ) {
                    return Err(DraftPiecePrepareErrorV1::Rejected(
                        DraftPieceRejectedReasonV1::InvalidGapWitness,
                    ));
                }
                if retreat_matching(&mut context, &mut cursor, |child| child.marker_count() != 0)? {
                    Some(cursor)
                } else {
                    None
                }
            } else {
                let target = if inclusive_end {
                    DraftCompositeSearchKeyV1::AfterMarkers(end)
                } else {
                    DraftCompositeSearchKeyV1::BeforeMarkers(end)
                };
                match marker_cursor(&mut context, sequence, target)? {
                    Some(mut cursor) => {
                        if retreat_matching(&mut context, &mut cursor, |child| {
                            child.marker_count() != 0
                        })? {
                            Some(cursor)
                        } else {
                            None
                        }
                    }
                    None => last_matching_cursor(&mut context, sequence, |child| {
                        child.marker_count() != 0
                    })?,
                }
            }
        }
    };
    if let Some(current) = cursor.as_ref() {
        let (_, marker) = marker_at_cursor(&mut context, current)?;
        if !marker_in_scope(marker.anchor(), demand.scope()) {
            cursor = None;
        }
    }
    let mut retained: Vec<(SequenceCursor, DraftPieceMarkerAtV1)> = Vec::new();
    while let Some(current) = cursor.as_mut() {
        let (_, marker) = marker_at_cursor(&mut context, current)?;
        if !marker_in_scope(marker.anchor(), demand.scope()) {
            break;
        }
        retained.push((current.clone(), marker));
        if retained.len() == demand.object_ceiling() {
            break;
        }
        let advanced = match demand.direction() {
            DraftPieceMarkerDirectionV1::Forward => {
                advance_matching(&mut context, current, |child| child.marker_count() != 0)?
            }
            DraftPieceMarkerDirectionV1::Backward => {
                retreat_matching(&mut context, current, |child| child.marker_count() != 0)?
            }
        };
        if !advanced {
            break;
        }
    }
    let had_available = !retained.is_empty();
    loop {
        let (first_cursor, last_cursor) = match demand.direction() {
            DraftPieceMarkerDirectionV1::Forward => (
                retained.first().map(|v| &v.0),
                retained.last().map(|v| &v.0),
            ),
            DraftPieceMarkerDirectionV1::Backward => (
                retained.last().map(|v| &v.0),
                retained.first().map(|v| &v.0),
            ),
        };
        let preceding = match first_cursor {
            Some(cursor) => cursor_edge(&mut context, cursor, demand.scope(), true)?,
            None => match (demand.direction(), demand.cursor()) {
                (DraftPieceMarkerDirectionV1::Forward, Some(cursor)) => {
                    DraftPieceMarkerEdgeFactV1::Marker(cursor)
                }
                _ => DraftPieceMarkerEdgeFactV1::RangeStart,
            },
        };
        let following = match last_cursor {
            Some(cursor) => cursor_edge(&mut context, cursor, demand.scope(), false)?,
            None => match (demand.direction(), demand.cursor()) {
                (DraftPieceMarkerDirectionV1::Backward, Some(cursor)) => {
                    DraftPieceMarkerEdgeFactV1::Marker(cursor)
                }
                _ => DraftPieceMarkerEdgeFactV1::RangeEnd,
            },
        };
        let continuation = match demand.direction() {
            DraftPieceMarkerDirectionV1::Forward => match following {
                DraftPieceMarkerEdgeFactV1::Marker(_) => retained.last().map(|v| marker_key(v.1)),
                _ => None,
            },
            DraftPieceMarkerDirectionV1::Backward => match preceding {
                DraftPieceMarkerEdgeFactV1::Marker(_) => retained.last().map(|v| marker_key(v.1)),
                _ => None,
            },
        };
        let bytes = marker_retained_bytes(retained.len(), preceding, following, continuation);
        if bytes <= demand.retained_byte_ceiling() {
            if had_available && retained.is_empty() {
                return Err(DraftPiecePrepareErrorV1::Rejected(
                    DraftPieceRejectedReasonV1::TreeLimit,
                ));
            }
            let mut markers: Vec<_> = retained.into_iter().map(|(_, marker)| marker).collect();
            if demand.direction() == DraftPieceMarkerDirectionV1::Backward {
                markers.reverse();
            }
            return Ok(DraftPieceMarkerDemandResultV1::new(
                root,
                demand,
                markers,
                preceding,
                following,
                continuation.is_none(),
                continuation,
                bytes,
                context.records_read,
            ));
        }
        if retained.pop().is_none() {
            return Err(DraftPiecePrepareErrorV1::Rejected(
                DraftPieceRejectedReasonV1::TreeLimit,
            ));
        }
    }
}

fn marker_key(marker: DraftPieceMarkerAtV1) -> DraftCompositeSearchKeyV1 {
    DraftCompositeSearchKeyV1::Marker {
        anchor: marker.anchor(),
        order_key: marker.marker().order_key(),
        marker_id: marker.marker().marker_id(),
    }
}

pub(crate) fn prove_marker_edge(
    storage: &SyndicStorage,
    store: &HomeStore,
    root: DraftPieceRootReferenceV1,
    request: DraftPieceMarkerEdgeProofRequestV1,
) -> Result<Option<DraftPieceMarkerEdgeProofV1>, DraftPiecePrepareErrorV1> {
    let anchor = match request {
        DraftPieceMarkerEdgeProofRequestV1::Absence { anchor } => anchor,
        DraftPieceMarkerEdgeProofRequestV1::First { marker }
        | DraftPieceMarkerEdgeProofRequestV1::Last { marker } => marker.anchor(),
        DraftPieceMarkerEdgeProofRequestV1::Adjacent { left, right } => {
            if left.anchor() != right.anchor() {
                return Ok(None);
            }
            left.anchor()
        }
    };
    if anchor > root.summary().logical_utf8_bytes() {
        return Err(DraftPiecePrepareErrorV1::Rejected(
            DraftPieceRejectedReasonV1::InvalidGapWitness,
        ));
    }
    let (mut context, sequence) = read_context(storage, store, root)?;
    let Some(sequence) = sequence else {
        return Ok(
            matches!(request, DraftPieceMarkerEdgeProofRequestV1::Absence { .. })
                .then_some(DraftPieceMarkerEdgeProofV1::Absence { anchor }),
        );
    };
    match request {
        DraftPieceMarkerEdgeProofRequestV1::Absence { anchor } => {
            let cursor = marker_cursor(
                &mut context,
                sequence,
                DraftCompositeSearchKeyV1::BeforeMarkers(anchor),
            )?;
            let absent = match cursor {
                Some(cursor) => marker_at_cursor(&mut context, &cursor)?.1.anchor() != anchor,
                None => true,
            };
            Ok(absent.then_some(DraftPieceMarkerEdgeProofV1::Absence { anchor }))
        }
        DraftPieceMarkerEdgeProofRequestV1::First { marker } => {
            let Some(mut cursor) = marker_cursor(&mut context, sequence, marker_key(marker))?
            else {
                return Ok(None);
            };
            if marker_at_cursor(&mut context, &cursor)?.1 != marker {
                return Ok(None);
            }
            let first =
                !retreat_matching(&mut context, &mut cursor, |child| child.marker_count() != 0)?
                    || marker_at_cursor(&mut context, &cursor)?.1.anchor() != marker.anchor();
            Ok(first.then_some(DraftPieceMarkerEdgeProofV1::First { marker }))
        }
        DraftPieceMarkerEdgeProofRequestV1::Last { marker } => {
            let Some(mut cursor) = marker_cursor(&mut context, sequence, marker_key(marker))?
            else {
                return Ok(None);
            };
            if marker_at_cursor(&mut context, &cursor)?.1 != marker {
                return Ok(None);
            }
            let last =
                !advance_matching(&mut context, &mut cursor, |child| child.marker_count() != 0)?
                    || marker_at_cursor(&mut context, &cursor)?.1.anchor() != marker.anchor();
            Ok(last.then_some(DraftPieceMarkerEdgeProofV1::Last { marker }))
        }
        DraftPieceMarkerEdgeProofRequestV1::Adjacent { left, right } => {
            let Some(mut cursor) = marker_cursor(&mut context, sequence, marker_key(left))? else {
                return Ok(None);
            };
            if marker_at_cursor(&mut context, &cursor)?.1 != left
                || !advance_matching(&mut context, &mut cursor, |child| child.marker_count() != 0)?
                || marker_at_cursor(&mut context, &cursor)?.1 != right
            {
                return Ok(None);
            }
            Ok(Some(DraftPieceMarkerEdgeProofV1::Adjacent { left, right }))
        }
    }
}

pub(crate) fn validate_position(
    storage: &SyndicStorage,
    store: &HomeStore,
    root: DraftPieceRootReferenceV1,
    position: DraftCompositePositionV1,
) -> Result<(), DraftPiecePrepareErrorV1> {
    let (mut context, sequence) = read_context(storage, store, root)?;
    resolve_position(&mut context, sequence, position).map(|_| ())
}

#[cfg(feature = "test-faults")]
pub(crate) fn validate_position_record_count(
    storage: &SyndicStorage,
    store: &HomeStore,
    root: DraftPieceRootReferenceV1,
    position: DraftCompositePositionV1,
) -> Result<u64, DraftPiecePrepareErrorV1> {
    let (mut context, sequence) = read_context(storage, store, root)?;
    resolve_position(&mut context, sequence, position)?;
    Ok(context.records_read)
}

fn marker_location_by_witness(
    context: &mut BuildContext<'_>,
    mut tree: SequenceRef,
    witness: &DraftPieceMarkerAtV1,
) -> Result<Option<(LocatedLeaf, DraftPieceLeafRecordV1)>, DraftPiecePrepareErrorV1> {
    let target = DraftCompositeSearchKeyV1::Marker {
        anchor: witness.anchor(),
        order_key: witness.marker().order_key(),
        marker_id: witness.marker().marker_id(),
    };
    let mut base = 0_u64;
    let mut rank = 0_u64;
    while tree.height != 0 {
        let node = context.load_sequence_node(tree.link, tree.height, tree.selected_root)?;
        let mut prefix = base;
        let mut prefix_rank = rank;
        let mut selected = None;
        for child in node.children() {
            let first = checked_offset_key(child.first(), prefix)
                .map_err(DraftPiecePrepareErrorV1::Rejected)?;
            let last = checked_offset_key(child.last(), prefix)
                .map_err(DraftPiecePrepareErrorV1::Rejected)?;
            if child.marker_count() != 0 && first <= target && target <= last {
                selected = Some((*child, prefix_rank));
                base = prefix;
                break;
            }
            prefix = prefix.checked_add(child.logical_utf8_bytes()).ok_or(
                DraftPiecePrepareErrorV1::Rejected(DraftPieceRejectedReasonV1::AggregateOverflow),
            )?;
            prefix_rank = prefix_rank.checked_add(child.piece_count()).ok_or(
                DraftPiecePrepareErrorV1::Rejected(DraftPieceRejectedReasonV1::AggregateOverflow),
            )?;
        }
        let Some((link, selected_rank)) = selected else {
            return Ok(None);
        };
        rank = selected_rank;
        tree = SequenceRef {
            link,
            height: tree.height - 1,
            selected_root: false,
        };
    }
    let leaf = context.load_sequence_leaf(tree.link)?;
    Ok(
        (leaf.value() == &DraftPieceLeafValueV1::Marker(witness.marker())).then_some((
            LocatedLeaf {
                rank,
                anchor: base,
                link: tree.link,
            },
            leaf,
        )),
    )
}

fn marker_leaf_by_witness(
    context: &mut BuildContext<'_>,
    tree: SequenceRef,
    witness: &DraftPieceMarkerAtV1,
) -> Result<Option<DraftPieceLeafRecordV1>, DraftPiecePrepareErrorV1> {
    Ok(marker_location_by_witness(context, tree, witness)?.map(|(_, leaf)| leaf))
}

pub(crate) fn validate_marker_location(
    storage: &SyndicStorage,
    store: &HomeStore,
    root: DraftPieceRootReferenceV1,
    witness: DraftPieceMarkerAtV1,
) -> Result<bool, DraftPiecePrepareErrorV1> {
    let (mut context, sequence) = read_context(storage, store, root)?;
    let index = validate_index_root(&mut context, root)?;
    let Some(occurrence) = index_lookup(&mut context, index, witness.marker().marker_id())? else {
        return Ok(false);
    };
    if occurrence.order_key() != witness.marker().order_key()
        || occurrence.label() != witness.marker().label()
        || occurrence.asset_id() != witness.marker().asset_id()
    {
        return Ok(false);
    }
    let Some(sequence) = sequence else {
        return Ok(false);
    };
    let Some(leaf) = marker_leaf_by_witness(&mut context, sequence, &witness)? else {
        return Ok(false);
    };
    Ok(leaf.key().id() == occurrence.sequence_leaf_id()
        && leaf.digest() == occurrence.sequence_leaf_digest())
}
