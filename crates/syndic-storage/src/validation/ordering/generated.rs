use super::*;
use crate::*;
use beryl_model::SyndicAcceptedInputId;
use sha2::{Digest, Sha256};

pub(super) fn validate(
    reader: &DomainReader<'_, SyndicDomain>,
    key: SyndicAcceptedInputId,
    input: &AcceptedInputRecord,
) -> Result<(), SyndicValidationError> {
    let AcceptedInputSource::DiscussionHandoff(receipt) = input.source() else {
        return invariant("generated input has composer provenance");
    };
    if key != input.id()
        || input.asset_reference_set().is_some()
        || input.content().summary().image_marker_count() != 0
    {
        return invariant("generated input identity or marker authority disagrees");
    }
    let parent = require::<ThreadsFamily>(
        reader,
        &input.thread_id(),
        "generated input parent is missing",
    )?;
    let gate = require::<InputGatesFamily>(
        reader,
        &input.thread_id(),
        "generated input parent gate is missing",
    )?;
    let order = require::<AcceptedOrderFamily>(
        reader,
        &ThreadAcceptedKey {
            owner: input.thread_id(),
            ordinal: input.ordinal(),
        },
        "generated input order is missing",
    )?;
    if parent.id() != input.thread_id()
        || receipt.parent_thread_revision >= parent.revision()
        || receipt.parent_gate_revision >= gate.revision()
        || order
            != AcceptedOrderIndexRecord::from_source(
                input.thread_id(),
                input.ordinal(),
                input.id(),
                AcceptedOrderSource::DiscussionHandoff,
            )
        || point::<AcceptedRouteLeavesFamily>(reader, &input.id())?.is_some()
    {
        return invariant("generated input parent revisions, order or route authority disagree");
    }
    let child = require::<ThreadsFamily>(
        reader,
        &receipt.child_thread_id,
        "generated input child is missing",
    )?;
    let context = require::<ContextEnvelopesFamily>(
        reader,
        &ContextOwnerKey::from(receipt.context_owner),
        "generated input context is missing",
    )?;
    let link = require::<ThreadParentFamily>(
        reader,
        &ThreadPairKey {
            first: input.thread_id(),
            second: receipt.child_thread_id,
        },
        "generated input child-parent link is missing",
    )?;
    let resolving = require::<TurnsFamily>(
        reader,
        &receipt.resolving_turn_id,
        "generated input resolving turn is missing",
    )?;
    if child.parent_thread_id() != Some(input.thread_id())
        || child.context_owner_id() != Some(receipt.context_owner)
        || context.owner() != receipt.context_owner
        || context.envelope().descriptor().digest() != receipt.context_digest
        || context.envelope().descriptor().source().thread_id() != input.thread_id()
        || link.parent_thread_id() != input.thread_id()
        || link.child_thread_id() != child.id()
        || link.context_owner_id() != receipt.context_owner
        || link.child_revision() != child.revision()
        || resolving.id() != receipt.resolving_turn_id
        || resolving.origin_thread_id() != child.id()
    {
        return invariant("generated input immutable child context disagrees");
    }
    let turn = require::<TurnsFamily>(
        reader,
        &receipt.parent_turn_id,
        "generated input parent turn is missing",
    )?;
    let item = require::<CanonicalItemsFamily>(
        reader,
        &receipt.canonical_item_id,
        "generated input canonical item is missing",
    )?;
    let index = require::<TurnItemsFamily>(
        reader,
        &TurnItemKey {
            owner: turn.id(),
            ordinal: TurnItemOrdinal::FIRST,
        },
        "generated input first item index is missing",
    )?;
    if turn.kind() != TurnKind::BerylDiscussionHandoff
        || turn.origin_thread_id() != input.thread_id()
        || turn.submitted_at() != input.admitted_at()
        || item.id() != receipt.canonical_item_id
        || item.turn_id() != turn.id()
        || item.ordinal() != TurnItemOrdinal::FIRST
        || item.provider_kind() != ProviderItemKind::UserMessage
        || item.presentation()
            != &(CanonicalItemPresentation::DiscussionHandoff {
                content: input.content(),
                accepted_input_id: input.id(),
            })
        || index.item_id() != item.id()
        || index.turn_id() != turn.id()
        || index.ordinal() != TurnItemOrdinal::FIRST
    {
        return invariant("generated input turn, item and receipt disagree");
    }
    validate_text(reader, input.content(), receipt.resolution_digest)
}

fn validate_text(
    reader: &DomainReader<'_, SyndicDomain>,
    content: ContentReference,
    digest: [u8; 32],
) -> Result<(), SyndicValidationError> {
    const PREFIX: &[u8] = b"Discussion resolution:\n\n";
    let length = content.summary().logical_utf8_bytes();
    if content.encoding() != ContentEncoding::ComposerV1
        || length <= PREFIX.len() as u64
        || length > 262_168
    {
        return invariant("generated input text bound or encoding disagrees");
    }
    let source = ProjectionTextSource::Composer(content);
    if super::super::read_projection_text_range(reader, source, 0, PREFIX.len() as u64)? != PREFIX {
        return invariant("generated input prefix disagrees");
    }
    let mut hash = Sha256::new();
    let mut scalars = 0usize;
    let mut start = PREFIX.len() as u64;
    while start < length {
        let end = (start + 65_536).min(length);
        let bytes = super::super::read_projection_text_range(reader, source, start, end)?;
        // Physical content validation has already checked UTF-8, including split chunks.
        scalars += bytes.iter().filter(|byte| **byte & 0xc0 != 0x80).count();
        hash.update(&bytes);
        start = end;
    }
    if scalars > 65_536 || <[u8; 32]>::from(hash.finalize()) != digest {
        return invariant("generated resolution scalar bound or digest disagrees");
    }
    Ok(())
}

pub(in crate::validation) fn validate_item(
    reader: &DomainReader<'_, SyndicDomain>,
    item: &CanonicalItemRecord,
) -> Result<(), SyndicValidationError> {
    let CanonicalItemPresentation::DiscussionHandoff {
        accepted_input_id,
        content,
    } = item.presentation()
    else {
        return Ok(());
    };
    let input = require::<AcceptedInputsFamily>(
        reader,
        accepted_input_id,
        "generated item accepted input is missing",
    )?;
    let AcceptedInputSource::DiscussionHandoff(receipt) = input.source() else {
        return invariant("generated item references composer input");
    };
    if input.content() != *content
        || receipt.canonical_item_id != item.id()
        || receipt.parent_turn_id != item.turn_id()
    {
        return invariant("generated item receipt identity disagrees");
    }
    Ok(())
}

pub(in crate::validation) fn validate_turn(
    reader: &DomainReader<'_, SyndicDomain>,
    turn: &TurnRecord,
) -> Result<(), SyndicValidationError> {
    if turn.kind() != TurnKind::BerylDiscussionHandoff {
        return Ok(());
    }
    let index = require::<TurnItemsFamily>(
        reader,
        &TurnItemKey {
            owner: turn.id(),
            ordinal: TurnItemOrdinal::FIRST,
        },
        "generated turn first item is missing",
    )?;
    let item = require::<CanonicalItemsFamily>(
        reader,
        &index.item_id(),
        "generated turn item is missing",
    )?;
    if item.turn_id() != turn.id()
        || !matches!(
            item.presentation(),
            CanonicalItemPresentation::DiscussionHandoff { .. }
        )
    {
        return invariant("generated turn has no generated first input");
    }
    validate_item(reader, &item)
}
