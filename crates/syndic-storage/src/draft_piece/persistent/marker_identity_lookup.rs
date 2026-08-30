use beryl_home_store::{DomainReader, HomeStore, PointReadLimit, ReadError};
use beryl_model::SyndicDraftMarkerId;

use crate::{SyndicStorage, draft_piece::*};

use super::{
    IndexRef, MarkerOrderRef, index_child_for_record, validate_index_record,
    validate_marker_order_record,
};

#[derive(Debug)]
pub(crate) enum SnapshotMarkerLookupErrorV1 {
    Read(ReadError),
    Rejected,
}

impl From<ReadError> for SnapshotMarkerLookupErrorV1 {
    fn from(value: ReadError) -> Self {
        Self::Read(value)
    }
}

trait MarkerIdentityLookupReader {
    type Error;

    fn root(
        &mut self,
        key: DraftPieceRootKeyV1,
    ) -> Result<Option<DraftPieceRootRecordV1>, Self::Error>;

    fn sequence_node(
        &mut self,
        key: DraftPieceRecordKeyV1,
    ) -> Result<Option<DraftPieceNodeRecordV1>, Self::Error>;

    fn index_record(
        &mut self,
        key: DraftMarkerIdentityRecordKeyV1,
    ) -> Result<Option<DraftMarkerIdentityRecordV1>, Self::Error>;

    fn marker_order_record(
        &mut self,
        key: DraftMarkerOrderRecordKeyV1,
    ) -> Result<Option<DraftMarkerOrderRecordV1>, Self::Error>;

    fn rejected(&self) -> Self::Error;
}

struct HomeMarkerIdentityLookupReader<'a> {
    storage: &'a SyndicStorage,
    store: &'a HomeStore,
}

impl MarkerIdentityLookupReader for HomeMarkerIdentityLookupReader<'_> {
    type Error = DraftPiecePrepareErrorV1;

    fn root(
        &mut self,
        key: DraftPieceRootKeyV1,
    ) -> Result<Option<DraftPieceRootRecordV1>, Self::Error> {
        Ok(self
            .storage
            .point::<DraftPieceRootsFamily>(self.store, key, point_limit())?)
    }

    fn sequence_node(
        &mut self,
        key: DraftPieceRecordKeyV1,
    ) -> Result<Option<DraftPieceNodeRecordV1>, Self::Error> {
        Ok(self
            .storage
            .point::<DraftPieceNodesFamily>(self.store, key, point_limit())?)
    }

    fn index_record(
        &mut self,
        key: DraftMarkerIdentityRecordKeyV1,
    ) -> Result<Option<DraftMarkerIdentityRecordV1>, Self::Error> {
        Ok(self
            .storage
            .point::<DraftMarkerIdentityIndexFamily>(self.store, key, point_limit())?)
    }

    fn marker_order_record(
        &mut self,
        key: DraftMarkerOrderRecordKeyV1,
    ) -> Result<Option<DraftMarkerOrderRecordV1>, Self::Error> {
        Ok(self.storage.point::<DraftMarkerOrderCommitmentsFamily>(
            self.store,
            key,
            point_limit(),
        )?)
    }

    fn rejected(&self) -> Self::Error {
        DraftPiecePrepareErrorV1::InvalidRoot
    }
}

struct SnapshotMarkerIdentityLookupReader<'a> {
    reader: &'a DomainReader<'a, crate::domain::SyndicDomain>,
}

impl MarkerIdentityLookupReader for SnapshotMarkerIdentityLookupReader<'_> {
    type Error = SnapshotMarkerLookupErrorV1;

    fn root(
        &mut self,
        key: DraftPieceRootKeyV1,
    ) -> Result<Option<DraftPieceRootRecordV1>, Self::Error> {
        self.reader
            .point::<DraftPieceRootsCodec>(&key, snapshot_marker_point_limit())
            .map_err(SnapshotMarkerLookupErrorV1::Read)
    }

    fn sequence_node(
        &mut self,
        key: DraftPieceRecordKeyV1,
    ) -> Result<Option<DraftPieceNodeRecordV1>, Self::Error> {
        self.reader
            .point::<DraftPieceNodesCodec>(&key, snapshot_marker_point_limit())
            .map_err(SnapshotMarkerLookupErrorV1::Read)
    }

    fn index_record(
        &mut self,
        key: DraftMarkerIdentityRecordKeyV1,
    ) -> Result<Option<DraftMarkerIdentityRecordV1>, Self::Error> {
        self.reader
            .point::<DraftMarkerIdentityIndexCodec>(&key, snapshot_marker_point_limit())
            .map_err(SnapshotMarkerLookupErrorV1::Read)
    }

    fn marker_order_record(
        &mut self,
        key: DraftMarkerOrderRecordKeyV1,
    ) -> Result<Option<DraftMarkerOrderRecordV1>, Self::Error> {
        self.reader
            .point::<DraftMarkerOrderCommitmentsCodec>(&key, snapshot_marker_point_limit())
            .map_err(SnapshotMarkerLookupErrorV1::Read)
    }

    fn rejected(&self) -> Self::Error {
        SnapshotMarkerLookupErrorV1::Rejected
    }
}

pub(crate) fn marker_identity_lookup(
    storage: &SyndicStorage,
    store: &HomeStore,
    root: DraftPieceRootReferenceV1,
    marker_id: SyndicDraftMarkerId,
) -> Result<Option<DraftMarkerIdentityOccurrenceV1>, DraftPiecePrepareErrorV1> {
    marker_identity_lookup_with(
        &mut HomeMarkerIdentityLookupReader { storage, store },
        root,
        marker_id,
    )
}

pub(crate) fn marker_identity_lookup_on_snapshot(
    reader: &DomainReader<'_, crate::domain::SyndicDomain>,
    root: DraftPieceRootReferenceV1,
    marker_id: SyndicDraftMarkerId,
) -> Result<Option<DraftMarkerIdentityOccurrenceV1>, SnapshotMarkerLookupErrorV1> {
    marker_identity_lookup_with(
        &mut SnapshotMarkerIdentityLookupReader { reader },
        root,
        marker_id,
    )
}

fn snapshot_marker_point_limit() -> PointReadLimit {
    PointReadLimit::new(point_limit().max_bytes()).expect("draft-piece point limit is nonzero")
}

fn marker_identity_lookup_with<R: MarkerIdentityLookupReader>(
    reader: &mut R,
    root: DraftPieceRootReferenceV1,
    marker_id: SyndicDraftMarkerId,
) -> Result<Option<DraftMarkerIdentityOccurrenceV1>, R::Error> {
    let stored = reader.root(root.key())?.ok_or_else(|| reader.rejected())?;
    if stored.reference() != root || !marker_lookup_root_header_is_exact(root) {
        return Err(reader.rejected());
    }
    let Some(sequence_root_id) = root.root_node() else {
        return if marker_lookup_empty_root_is_exact(root) {
            Ok(None)
        } else {
            Err(reader.rejected())
        };
    };
    let sequence_key = DraftPieceRecordKeyV1::new(root.key().draft_id(), sequence_root_id);
    let sequence = reader
        .sequence_node(sequence_key)?
        .ok_or_else(|| reader.rejected())?;
    if sequence.key() != sequence_key
        || validate_sequence_root_node(sequence, root.summary()).is_err()
    {
        return Err(reader.rejected());
    }
    marker_lookup_marker_order_is_exact(reader, root)?;
    let Some(mut current) = marker_lookup_index_root(reader, root)? else {
        return Ok(None);
    };
    loop {
        let kind = if current.height == 0 {
            DraftMarkerIdentityRecordKindV1::Leaf
        } else {
            DraftMarkerIdentityRecordKindV1::Internal
        };
        let key =
            DraftMarkerIdentityRecordKeyV1::new(root.key().draft_id(), kind, current.link.id());
        let record = reader.index_record(key)?.ok_or_else(|| reader.rejected())?;
        if record.key() != key
            || validate_index_record(record.clone(), current, current.selected_root).is_err()
        {
            return Err(reader.rejected());
        }
        if current.height == 0 {
            let occurrence = record.occurrence().ok_or_else(|| reader.rejected())?;
            return Ok((occurrence.marker_id() == marker_id).then_some(occurrence));
        }
        let selected = record.children().and_then(|children| {
            children
                .iter()
                .copied()
                .find(|child| child.first() <= marker_id && marker_id <= child.last())
        });
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

fn marker_lookup_root_header_is_exact(root: DraftPieceRootReferenceV1) -> bool {
    root.summary().marker_count() == root.marker_index_summary().record_count()
        && root.summary().marker_count() == root.marker_commitment().marker_count()
        && root.combined_digest()
            == combined_root_digest(
                root.summary(),
                root.marker_index_summary(),
                root.marker_commitment(),
            )
        && root.summary().text_summary().is_canonical()
        && match root.key().build_identity() {
            DraftPieceRootBuildIdentityV1::DirectCanonicalEmpty { operation_id } => {
                operation_id == canonical_empty_draft_root_operation_id_v1(root.key().draft_id())
                    && root.root_node().is_none()
            }
            DraftPieceRootBuildIdentityV1::EditorCandidate { .. } => true,
        }
}

fn marker_lookup_empty_root_is_exact(root: DraftPieceRootReferenceV1) -> bool {
    root.summary().logical_utf8_bytes() == 0
        && root.summary().newline_count() == 0
        && root.summary().logical_line_count() == 0
        && root.summary().piece_count() == 0
        && root.summary().marker_count() == 0
        && root.summary().height() == 0
        && root.summary().root_digest() == canonical_empty_root_digest_v1()
        && root.summary().marker_digest() == canonical_empty_marker_digest_v1()
        && root.marker_index_root().is_none()
        && root.marker_index_summary().record_count() == 0
        && root.marker_index_summary().height() == 0
        && root.marker_index_summary().root_digest()
            == canonical_empty_marker_identity_index_digest_v1()
        && root.marker_order_root().is_none()
        && root.marker_order_height() == 0
        && root.marker_commitment() == canonical_empty_draft_marker_commitment_v1()
}

fn marker_lookup_index_root<R: MarkerIdentityLookupReader>(
    reader: &mut R,
    root: DraftPieceRootReferenceV1,
) -> Result<Option<IndexRef>, R::Error> {
    let Some(id) = root.marker_index_root() else {
        return if root.marker_index_summary().record_count() == 0
            && root.marker_index_summary().height() == 0
            && root.marker_index_summary().root_digest()
                == canonical_empty_marker_identity_index_digest_v1()
        {
            Ok(None)
        } else {
            Err(reader.rejected())
        };
    };
    let kind = if root.marker_index_summary().height() == 0 {
        DraftMarkerIdentityRecordKindV1::Leaf
    } else {
        DraftMarkerIdentityRecordKindV1::Internal
    };
    let key = DraftMarkerIdentityRecordKeyV1::new(root.key().draft_id(), kind, id);
    let record = reader.index_record(key)?.ok_or_else(|| reader.rejected())?;
    if record.key() != key
        || validate_index_root_record(record.clone(), root.marker_index_summary()).is_err()
    {
        return Err(reader.rejected());
    }
    let link = index_child_for_record(&record).map_err(|_| reader.rejected())?;
    Ok(Some(IndexRef {
        link,
        height: record.height(),
        selected_root: true,
    }))
}

fn marker_lookup_marker_order_is_exact<R: MarkerIdentityLookupReader>(
    reader: &mut R,
    root: DraftPieceRootReferenceV1,
) -> Result<(), R::Error> {
    let commitment = root.marker_commitment();
    if commitment.marker_count() == 0 {
        return if root.marker_order_root().is_none()
            && root.marker_order_height() == 0
            && commitment == canonical_empty_draft_marker_commitment_v1()
        {
            Ok(())
        } else {
            Err(reader.rejected())
        };
    }
    let Some(id) = root.marker_order_root() else {
        return Err(reader.rejected());
    };
    if root.marker_order_height() == 0 {
        return Err(reader.rejected());
    }
    let link = DraftMarkerOrderChildV1::new(
        id,
        DraftPieceDigestV1::from_bytes(commitment.tree_root_digest()),
        commitment.marker_count(),
        commitment.maximum_image_label(),
    )
    .ok_or_else(|| reader.rejected())?;
    let key = DraftMarkerOrderRecordKeyV1::new(
        root.key().draft_id(),
        DraftMarkerOrderRecordKindV1::Internal,
        id,
    );
    let record = reader
        .marker_order_record(key)?
        .ok_or_else(|| reader.rejected())?;
    if record.key() != key
        || validate_marker_order_record(
            &record,
            MarkerOrderRef {
                link,
                height: root.marker_order_height(),
                selected_root: true,
            },
            true,
        )
        .is_err()
    {
        return Err(reader.rejected());
    }
    Ok(())
}
