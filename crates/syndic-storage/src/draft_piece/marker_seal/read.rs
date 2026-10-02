use super::*;

pub(super) fn close_exhausted_cursor(
    storage: &SyndicStorage,
    store: ReadAccess<'_>,
    record: &mut DraftMarkerSealRecordV1,
) -> Result<(), DraftMarkerSealErrorV1> {
    let DraftMarkerSealCursorV1::Positioned(frames) = &record.cursor else {
        return Ok(());
    };
    for frame in frames {
        let node = load_marker_record(
            storage,
            store,
            record.key.root_key.draft_id(),
            DraftMarkerOrderRecordKindV1::Internal,
            frame.record_id,
        )?;
        validate_internal(&node, frame.digest, frame.height)?;
        let children = node.children().ok_or(DraftMarkerSealErrorV1::Corruption)?;
        if usize::from(frame.next_child_index) < children.len() {
            return Ok(());
        }
        if usize::from(frame.next_child_index) > children.len() {
            return Err(DraftMarkerSealErrorV1::Corruption);
        }
    }
    let terminal_frames = frames.clone();
    record.cursor = DraftMarkerSealCursorV1::Eof(terminal_frames);
    Ok(())
}

pub(super) fn release_for(record: &DraftMarkerSealRecordV1) -> DraftMarkerSealCustodyReleaseV1 {
    DraftMarkerSealCustodyReleaseV1 {
        key: record.key,
        completed_marker_count: record.completed_marker_count,
    }
}

pub(super) fn validate_source(
    storage: &SyndicStorage,
    store: ReadAccess<'_>,
    source: DraftPieceRootReferenceV1,
) -> Result<(), DraftMarkerSealErrorV1> {
    let Some(root) = storage.point_with_access::<DraftPieceRootsFamily>(
        store,
        source.key(),
        storage_point_limit::<DraftPieceRootsFamily>(),
    )?
    else {
        return Err(DraftMarkerSealErrorV1::MissingSource);
    };
    if root.reference() != source {
        return Err(DraftMarkerSealErrorV1::IdentityCollision);
    }
    let commitment = source.marker_commitment();
    if (commitment.marker_count() == 0) != source.marker_order_root().is_none()
        || commitment.marker_count() != source.summary().marker_count()
    {
        return Err(DraftMarkerSealErrorV1::Corruption);
    }
    Ok(())
}

pub(super) fn validate_key_source(
    storage: &SyndicStorage,
    store: ReadAccess<'_>,
    key: DraftMarkerSealKeyV1,
) -> Result<DraftPieceRootReferenceV1, DraftMarkerSealErrorV1> {
    let Some(root) = storage.point_with_access::<DraftPieceRootsFamily>(
        store,
        key.root_key,
        storage_point_limit::<DraftPieceRootsFamily>(),
    )?
    else {
        return Err(DraftMarkerSealErrorV1::MissingSource);
    };
    let source = root.reference();
    if source.combined_digest() != key.combined_digest
        || source.marker_order_root() != key.marker_order_root
        || source.marker_commitment() != key.commitment
    {
        return Err(DraftMarkerSealErrorV1::IdentityCollision);
    }
    validate_source(storage, store, source)?;
    Ok(source)
}

pub(super) fn source_marker_order_height(
    storage: &SyndicStorage,
    store: ReadAccess<'_>,
    key: DraftMarkerSealKeyV1,
) -> Result<u8, DraftMarkerSealErrorV1> {
    let source = validate_key_source(storage, store, key)?;
    let height = source.marker_order_height();
    if height == 0 || height > DRAFT_PIECE_MAX_HEIGHT {
        return Err(DraftMarkerSealErrorV1::Corruption);
    }
    Ok(height)
}

pub(super) fn next_marker(
    storage: &SyndicStorage,
    store: ReadAccess<'_>,
    record: &mut DraftMarkerSealRecordV1,
) -> Result<Option<DraftMarkerSealFrontierV1>, DraftMarkerSealErrorV1> {
    match &record.cursor {
        DraftMarkerSealCursorV1::Eof(_) => return Ok(None),
        DraftMarkerSealCursorV1::BeforeRoot => {
            let Some(root_id) = record.key.marker_order_root else {
                record.cursor = DraftMarkerSealCursorV1::Eof(Vec::new());
                return Ok(None);
            };
            let expected_height = source_marker_order_height(storage, store, record.key)?;
            let (frontier, frames) = descend_to_leaf(
                storage,
                store,
                record.key.root_key.draft_id(),
                root_id,
                DraftPieceDigestV1::from_bytes(record.key.commitment.tree_root_digest()),
                expected_height,
                Vec::new(),
            )?;
            record.cursor = if frames.is_empty() {
                DraftMarkerSealCursorV1::Eof(Vec::new())
            } else {
                DraftMarkerSealCursorV1::Positioned(frames)
            };
            return Ok(Some(frontier));
        }
        DraftMarkerSealCursorV1::Positioned(_) => {}
    }

    let DraftMarkerSealCursorV1::Positioned(mut frames) = record.cursor.clone() else {
        unreachable!()
    };
    let terminal_frames = frames.clone();
    while let Some(frame) = frames.last_mut() {
        let node = load_marker_record(
            storage,
            store,
            record.key.root_key.draft_id(),
            DraftMarkerOrderRecordKindV1::Internal,
            frame.record_id,
        )?;
        validate_internal(&node, frame.digest, frame.height)?;
        let children = node.children().ok_or(DraftMarkerSealErrorV1::Corruption)?;
        let index = usize::from(frame.next_child_index);
        if index < children.len() {
            frame.next_child_index = frame
                .next_child_index
                .checked_add(1)
                .ok_or(DraftMarkerSealErrorV1::Corruption)?;
            let child = children[index];
            let expected_height = frame
                .height
                .checked_sub(1)
                .ok_or(DraftMarkerSealErrorV1::Corruption)?;
            let (frontier, next_frames) = descend_to_leaf(
                storage,
                store,
                record.key.root_key.draft_id(),
                child.id(),
                child.digest(),
                expected_height,
                frames,
            )?;
            record.cursor = if next_frames.is_empty() {
                DraftMarkerSealCursorV1::Eof(Vec::new())
            } else {
                DraftMarkerSealCursorV1::Positioned(next_frames)
            };
            return Ok(Some(frontier));
        }
        frames.pop();
    }
    record.cursor = DraftMarkerSealCursorV1::Eof(terminal_frames);
    Ok(None)
}

pub(super) fn descend_to_leaf(
    storage: &SyndicStorage,
    store: ReadAccess<'_>,
    draft_id: SyndicDraftId,
    mut record_id: DraftPieceRecordIdV1,
    mut expected_digest: DraftPieceDigestV1,
    mut expected_height: u8,
    mut frames: Vec<DraftMarkerSealCursorFrameV1>,
) -> Result<(DraftMarkerSealFrontierV1, Vec<DraftMarkerSealCursorFrameV1>), DraftMarkerSealErrorV1>
{
    loop {
        if frames.len() >= usize::from(DRAFT_PIECE_MAX_HEIGHT) {
            return Err(DraftMarkerSealErrorV1::Corruption);
        }
        let kind = if expected_height == 0 {
            DraftMarkerOrderRecordKindV1::Leaf
        } else {
            DraftMarkerOrderRecordKindV1::Internal
        };
        let record = load_marker_record(storage, store, draft_id, kind, record_id)?;
        if expected_height == 0 {
            validate_leaf(&record, expected_digest)?;
            let (marker_id, label, asset_id) =
                record.marker().ok_or(DraftMarkerSealErrorV1::Corruption)?;
            return Ok((
                DraftMarkerSealFrontierV1 {
                    record_id,
                    digest: expected_digest,
                    marker_id,
                    label,
                    asset_id,
                },
                frames,
            ));
        }
        validate_internal(&record, expected_digest, expected_height)?;
        let children = record
            .children()
            .ok_or(DraftMarkerSealErrorV1::Corruption)?;
        let child = *children.first().ok_or(DraftMarkerSealErrorV1::Corruption)?;
        frames.push(DraftMarkerSealCursorFrameV1 {
            record_id,
            digest: expected_digest,
            height: expected_height,
            next_child_index: 1,
        });
        record_id = child.id();
        expected_digest = child.digest();
        expected_height = expected_height
            .checked_sub(1)
            .ok_or(DraftMarkerSealErrorV1::Corruption)?;
    }
}

pub(super) fn load_marker_record(
    storage: &SyndicStorage,
    store: ReadAccess<'_>,
    draft_id: SyndicDraftId,
    kind: DraftMarkerOrderRecordKindV1,
    record_id: DraftPieceRecordIdV1,
) -> Result<DraftMarkerOrderRecordV1, DraftMarkerSealErrorV1> {
    storage
        .point_with_access::<DraftMarkerOrderCommitmentsFamily>(
            store,
            DraftMarkerOrderRecordKeyV1::new(draft_id, kind, record_id),
            storage_point_limit::<DraftMarkerOrderCommitmentsFamily>(),
        )?
        .ok_or(DraftMarkerSealErrorV1::Corruption)
}

pub(super) fn validate_internal(
    record: &DraftMarkerOrderRecordV1,
    expected_digest: DraftPieceDigestV1,
    expected_height: u8,
) -> Result<(), DraftMarkerSealErrorV1> {
    let children = record
        .children()
        .ok_or(DraftMarkerSealErrorV1::Corruption)?;
    if expected_height == 0
        || record.height() != expected_height
        || record.digest() != expected_digest
        || children.is_empty()
        || children.len() > DRAFT_PIECE_MAX_CHILDREN
        || marker_order_node_digest(expected_height, children) != expected_digest
    {
        return Err(DraftMarkerSealErrorV1::Corruption);
    }
    let mut count = 0u64;
    let mut maximum = None;
    for child in children {
        count = count
            .checked_add(child.marker_count())
            .ok_or(DraftMarkerSealErrorV1::Corruption)?;
        let child_maximum = child
            .maximum_image_label()
            .ok_or(DraftMarkerSealErrorV1::Corruption)?;
        maximum = Some(maximum.map_or(child_maximum, |value: ImageLabelOrdinal| {
            value.max(child_maximum)
        }));
    }
    if count == 0 || maximum.is_none() {
        return Err(DraftMarkerSealErrorV1::Corruption);
    }
    Ok(())
}

pub(super) fn validate_leaf(
    record: &DraftMarkerOrderRecordV1,
    expected_digest: DraftPieceDigestV1,
) -> Result<(), DraftMarkerSealErrorV1> {
    let (marker_id, label, asset_id) = record.marker().ok_or(DraftMarkerSealErrorV1::Corruption)?;
    if record.height() != 0
        || record.digest() != expected_digest
        || marker_order_leaf_digest(marker_id, label, asset_id) != expected_digest
    {
        return Err(DraftMarkerSealErrorV1::Corruption);
    }
    Ok(())
}

pub(super) fn validate_cursor_closure(
    storage: &SyndicStorage,
    store: ReadAccess<'_>,
    record: &DraftMarkerSealRecordV1,
) -> Result<(), DraftMarkerSealErrorV1> {
    validate_record(record)?;
    match &record.cursor {
        DraftMarkerSealCursorV1::BeforeRoot => {
            if record.frontier.is_some() || record.completed_marker_count != 0 {
                return Err(DraftMarkerSealErrorV1::Corruption);
            }
        }
        DraftMarkerSealCursorV1::Eof(frames) if record.completed_marker_count == 0 => {
            if !frames.is_empty()
                || record.key.marker_order_root.is_some()
                || record.frontier.is_some()
            {
                return Err(DraftMarkerSealErrorV1::Corruption);
            }
        }
        DraftMarkerSealCursorV1::Eof(frames) => {
            validate_cursor_path(storage, store, record, frames, true)?;
        }
        DraftMarkerSealCursorV1::Positioned(frames) => {
            validate_cursor_path(storage, store, record, frames, false)?;
        }
    }
    Ok(())
}

pub(super) fn validate_cursor_path(
    storage: &SyndicStorage,
    store: ReadAccess<'_>,
    record: &DraftMarkerSealRecordV1,
    frames: &[DraftMarkerSealCursorFrameV1],
    require_eof: bool,
) -> Result<(), DraftMarkerSealErrorV1> {
    if frames.is_empty() {
        return Err(DraftMarkerSealErrorV1::Corruption);
    }
    if frames.len() > usize::from(DRAFT_PIECE_MAX_HEIGHT) {
        return Err(DraftMarkerSealErrorV1::Corruption);
    }
    let mut selected_child = None;
    for (index, frame) in frames.iter().enumerate() {
        let node = load_marker_record(
            storage,
            store,
            record.key.root_key.draft_id(),
            DraftMarkerOrderRecordKindV1::Internal,
            frame.record_id,
        )?;
        validate_internal(&node, frame.digest, frame.height)?;
        if index == 0
            && (Some(frame.record_id) != record.key.marker_order_root
                || frame.digest
                    != DraftPieceDigestV1::from_bytes(record.key.commitment.tree_root_digest()))
        {
            return Err(DraftMarkerSealErrorV1::Corruption);
        }
        let children = node.children().ok_or(DraftMarkerSealErrorV1::Corruption)?;
        if require_eof && usize::from(frame.next_child_index) != children.len() {
            return Err(DraftMarkerSealErrorV1::Corruption);
        }
        let selected_index = usize::from(frame.next_child_index)
            .checked_sub(1)
            .ok_or(DraftMarkerSealErrorV1::Corruption)?;
        let child = *children
            .get(selected_index)
            .ok_or(DraftMarkerSealErrorV1::Corruption)?;
        if let Some((id, digest, height)) = selected_child
            && (id != frame.record_id || digest != frame.digest || height != frame.height)
        {
            return Err(DraftMarkerSealErrorV1::Corruption);
        }
        selected_child = Some((
            child.id(),
            child.digest(),
            frame
                .height
                .checked_sub(1)
                .ok_or(DraftMarkerSealErrorV1::Corruption)?,
        ));
    }
    let frontier = record.frontier.ok_or(DraftMarkerSealErrorV1::Corruption)?;
    if selected_child != Some((frontier.record_id, frontier.digest, 0)) {
        return Err(DraftMarkerSealErrorV1::Corruption);
    }
    validate_frontier_leaf(storage, store, record)
}

pub(super) fn validate_frontier_leaf(
    storage: &SyndicStorage,
    store: ReadAccess<'_>,
    record: &DraftMarkerSealRecordV1,
) -> Result<(), DraftMarkerSealErrorV1> {
    let frontier = record.frontier.ok_or(DraftMarkerSealErrorV1::Corruption)?;
    let leaf = load_marker_record(
        storage,
        store,
        record.key.root_key.draft_id(),
        DraftMarkerOrderRecordKindV1::Leaf,
        frontier.record_id,
    )?;
    validate_leaf(&leaf, frontier.digest)?;
    if leaf.marker() != Some((frontier.marker_id, frontier.label, frontier.asset_id)) {
        return Err(DraftMarkerSealErrorV1::Corruption);
    }
    Ok(())
}
