use super::*;

pub(super) fn discover(
    reader: &ParentRead<'_>,
    expected: &GeneratedDiscussionInputLookup,
) -> Result<GeneratedDiscussionInputDiscovery, SyndicReadError> {
    use GeneratedDiscussionInputDiscovery::{Absent, Collision, Exact};
    let content = prepare_resolution_content(&expected.resolution).map_err(|_| {
        SyndicReadError::Invariant("generated input lookup resolution exceeds its bounds")
    })?;
    let input_id = SyndicAcceptedInputId::from_bytes(*expected.job_id.as_bytes());
    if expected.job_id.as_bytes() != expected.intent_id.as_bytes()
        || expected.parent_thread_id == expected.child_thread_id
        || expected.parent_turn_id == expected.resolving_turn_id
        || expected.parent_turn_id.as_bytes() == input_id.as_bytes()
        || expected.canonical_item_id.as_bytes() == input_id.as_bytes()
        || expected.canonical_item_id.as_bytes() == expected.parent_turn_id.as_bytes()
    {
        return Ok(Collision);
    }
    let input = reader.read::<AcceptedInputsFamily>(&input_id)?;
    let turn = reader.read::<TurnsFamily>(&expected.parent_turn_id)?;
    let item = reader.read::<CanonicalItemsFamily>(&expected.canonical_item_id)?;
    let Some(input) = input else {
        return Ok(if turn.is_none() && item.is_none() {
            Absent
        } else {
            Collision
        });
    };
    let (Some(turn), Some(item)) = (turn, item) else {
        return Ok(Collision);
    };
    let AcceptedInputSource::DiscussionHandoff(receipt) = input.source() else {
        return Ok(Collision);
    };
    let digest: [u8; 32] = Sha256::digest(expected.resolution.as_bytes()).into();
    if input.id() != input_id
        || input.thread_id() != expected.parent_thread_id
        || input.asset_reference_set().is_some()
        || receipt.child_thread_id != expected.child_thread_id
        || receipt.intent_id != expected.intent_id
        || receipt.job_id != expected.job_id
        || receipt.context_owner != expected.context_owner
        || receipt.context_digest != expected.context_digest
        || receipt.resolving_turn_id != expected.resolving_turn_id
        || receipt.parent_turn_id != expected.parent_turn_id
        || receipt.canonical_item_id != expected.canonical_item_id
        || receipt.resolution_digest != digest
        || turn.id() != expected.parent_turn_id
        || turn.kind() != TurnKind::BerylDiscussionHandoff
        || turn.origin_thread_id() != input.thread_id()
        || turn.submitted_at() != input.admitted_at()
        || item.id() != expected.canonical_item_id
        || item.turn_id() != turn.id()
        || item.ordinal() != TurnItemOrdinal::FIRST
        || item.provider_kind() != ProviderItemKind::UserMessage
        || item.presentation()
            != &(CanonicalItemPresentation::DiscussionHandoff {
                content: input.content(),
                accepted_input_id: input_id,
            })
    {
        return Ok(Collision);
    }
    let index = reader.read::<TurnItemsFamily>(&TurnItemKey {
        owner: turn.id(),
        ordinal: TurnItemOrdinal::FIRST,
    })?;
    let order = reader.read::<AcceptedOrderFamily>(&ThreadAcceptedKey {
        owner: input.thread_id(),
        ordinal: input.ordinal(),
    })?;
    let parent = reader.read::<ThreadsFamily>(&input.thread_id())?;
    let gate = reader.read::<InputGatesFamily>(&input.thread_id())?;
    if index.is_none_or(|index| {
        index.turn_id() != turn.id()
            || index.ordinal() != TurnItemOrdinal::FIRST
            || index.item_id() != item.id()
            || index.item_revision() != item.revision()
    }) || order
        != Some(AcceptedOrderIndexRecord::from_source(
            input.thread_id(),
            input.ordinal(),
            input_id,
            AcceptedOrderSource::DiscussionHandoff,
        ))
        || parent.is_none_or(|parent| {
            parent.id() != input.thread_id() || parent.revision() <= receipt.parent_thread_revision
        })
        || gate.is_none_or(|gate| {
            gate.thread_id() != input.thread_id()
                || gate.revision() <= receipt.parent_gate_revision
                || gate.accepted_high_water() < input.ordinal().get()
        })
        || reader
            .read::<AcceptedRouteLeavesFamily>(&input_id)?
            .is_some()
    {
        return Ok(Collision);
    }
    let child = reader.read::<ThreadsFamily>(&receipt.child_thread_id)?;
    let context =
        reader.read::<ContextEnvelopesFamily>(&ContextOwnerKey::from(receipt.context_owner))?;
    let resolving = reader.read::<TurnsFamily>(&receipt.resolving_turn_id)?;
    let link = reader.read::<ThreadParentFamily>(&ThreadPairKey {
        first: input.thread_id(),
        second: receipt.child_thread_id,
    })?;
    let Some(child) = child else {
        return Ok(Collision);
    };
    if child.id() != receipt.child_thread_id
        || child.parent_thread_id() != Some(input.thread_id())
        || child.context_owner_id() != Some(receipt.context_owner)
        || context.is_none_or(|context| {
            context.owner() != receipt.context_owner
                || context.envelope().descriptor().digest() != receipt.context_digest
                || context.envelope().descriptor().source().thread_id() != input.thread_id()
        })
        || resolving.is_none_or(|turn| {
            turn.id() != receipt.resolving_turn_id
                || turn.origin_thread_id() != receipt.child_thread_id
        })
        || link
            != Some(ThreadParentIndexRecord::new(
                input.thread_id(),
                child.id(),
                child.revision(),
                receipt.context_owner,
            ))
    {
        return Ok(Collision);
    }
    let shape = crate::mutation::admission_helpers::turn_shape(reader, turn.id(), turn.parent());
    match shape {
        Ok((depth, digest, skip))
            if depth == turn.depth()
                && digest == turn.chain_digest()
                && skip == turn.ancestor_skip() => {}
        Err(SyndicMutationError::Read(error)) => return Err(error.into()),
        _ => return Ok(Collision),
    }
    if let Some(parent_id) = turn.parent().turn() {
        if reader.read::<TurnChildrenFamily>(&TurnPairKey {
            parent: parent_id,
            child: turn.id(),
        })? != Some(TurnChildIndexRecord::new(
            parent_id,
            turn.id(),
            turn.depth(),
            turn.chain_digest(),
        )) {
            return Ok(Collision);
        }
    }
    let mut content_changes = Vec::new();
    match content::prepare_content(reader, content, &mut content_changes) {
        Ok(reference) if reference == input.content() => {}
        Err(SyndicMutationError::Read(error)) => return Err(error.into()),
        _ => return Ok(Collision),
    }
    for change in &content_changes {
        if !change.matches(reader)?.1 {
            return Ok(Collision);
        }
    }
    Ok(Exact)
}
