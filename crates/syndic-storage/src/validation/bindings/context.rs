use super::*;
use crate::context_projection::{ContextEstablishment, frame};
use beryl_model::{BerylHomeId, DiscussionContextOwnerId, SyndicThreadId};
mod replay;

pub(super) fn validate_context_history(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    scan::<BindingsFamily>(reader, |_, binding| {
        if let Some(usable) = super::binding_usable(binding) {
            validate_context(
                reader,
                binding.thread_id(),
                usable.execution(),
                usable.tool_profile(),
                usable.represented_prefix(),
                usable.lineage(),
            )?;
        } else if let BindingState::Stale(stale) = binding.state()
            && let Some(lineage @ CasLineageProof::ContextAware(_)) = stale.observed_lineage()
        {
            let tool = stale
                .observed_tool_profile()
                .ok_or(SyndicValidationError::Invariant(
                    "context stale proof omitted tool profile",
                ))?;
            validate_context(
                reader,
                binding.thread_id(),
                stale.execution(),
                tool,
                stale
                    .observed_prefix()
                    .ok_or(SyndicValidationError::Invariant(
                        "context stale prefix missing",
                    ))?,
                lineage,
            )?;
        }
        Ok(())
    })
}

fn context_frame(
    reader: &DomainReader<'_, SyndicDomain>,
    home: BerylHomeId,
    thread_id: SyndicThreadId,
) -> Result<Option<frame::ContextFrameSource>, SyndicValidationError> {
    let thread = require::<ThreadsFamily>(reader, &thread_id, "context owner thread missing")?;
    let Some(owner) = thread.context_owner_id() else {
        return Ok(None);
    };
    let DiscussionContextOwnerId::SubmittedTurn(owner_turn) = owner else {
        return invariant("usable context proof has a draft owner");
    };
    let record = require::<ContextEnvelopesFamily>(
        reader,
        &ContextOwnerKey::from(owner),
        "context proof envelope missing",
    )?;
    let original = require::<TurnsFamily>(reader, &owner_turn, "context original owner missing")?;
    let source = record.envelope().descriptor().source();
    if original.origin_thread_id() != thread_id
        || original.parent() != crate::ConversationParent::Turn(source.turn_id())
        || thread.parent_thread_id() != Some(source.thread_id())
    {
        return invariant("context proof original owner/boundary disagrees");
    }
    frame::describe_for_home(home, thread_id, &record)
        .map(Some)
        .map_err(|_| SyndicValidationError::Invariant("context entry derivation failed"))
}

fn derive_summary(
    reader: &DomainReader<'_, SyndicDomain>,
    home: BerylHomeId,
    target: SyndicThreadId,
    prefix: CasRepresentedPrefixProof,
    include_target: bool,
) -> Result<crate::ContextSummary, SyndicValidationError> {
    let Some(tail_id) = prefix.tail() else {
        return Ok(crate::ContextSummary::empty());
    };
    let tail = require::<TurnsFamily>(reader, &tail_id, "context prefix missing")?;
    let mut summary = crate::ContextSummary::empty();
    let mut previous = None;
    for depth in 1..=tail.depth().get() {
        let turn = crate::selected_path::lift_to_depth(
            tail.clone(),
            crate::TurnDepth::new(depth).unwrap(),
            |id| require::<TurnsFamily>(reader, &id, "context prefix ancestor missing"),
            SyndicValidationError::Invariant,
        )?;
        if previous != Some(turn.origin_thread_id()) {
            if let Some(frame) = context_frame(reader, home, turn.origin_thread_id())? {
                let envelope = require::<ContextEnvelopesFamily>(
                    reader,
                    &ContextOwnerKey::from(frame.owner),
                    "context entry envelope missing",
                )?;
                if turn.parent()
                    != crate::ConversationParent::Turn(
                        envelope.envelope().descriptor().source_turn_id(),
                    )
                {
                    return invariant("context insertion boundary differs from selected crossing");
                }
                summary = summary
                    .append(frame.entry, frame.utf8_bytes)
                    .map_err(|_| SyndicValidationError::Invariant("context summary overflow"))?;
            }
            previous = Some(turn.origin_thread_id());
        }
    }
    if include_target
        && tail.origin_thread_id() != target
        && let Some(frame) = context_frame(reader, home, target)?
    {
        let envelope = require::<ContextEnvelopesFamily>(
            reader,
            &ContextOwnerKey::from(frame.owner),
            "target context missing",
        )?;
        if envelope.envelope().descriptor().source_turn_id() != tail.id() {
            return invariant("target context boundary outside represented prefix");
        }
        summary = summary
            .append(frame.entry, frame.utf8_bytes)
            .map_err(|_| SyndicValidationError::Invariant("context summary overflow"))?;
    }
    Ok(summary)
}

fn validate_context(
    reader: &DomainReader<'_, SyndicDomain>,
    thread: SyndicThreadId,
    execution: &beryl_model::ExecutionBinding,
    tool: beryl_model::CasConversationToolProfile,
    represented: CasRepresentedPrefixProof,
    lineage: CasLineageProof,
) -> Result<(), SyndicValidationError> {
    let CasLineageProof::ContextAware(proof) = lineage else {
        let target =
            require::<ThreadsFamily>(reader, &thread, "context classification target missing")?;
        if target.context_owner_id().is_some() {
            return invariant("legacy usable lineage omits target context");
        }
        if let Some(tail) = represented.tail() {
            let tail = require::<TurnsFamily>(reader, &tail, "legacy context prefix missing")?;
            for depth in 1..=tail.depth().get() {
                let turn = crate::selected_path::lift_to_depth(
                    tail.clone(),
                    crate::TurnDepth::new(depth).unwrap(),
                    |id| require::<TurnsFamily>(reader, &id, "legacy context ancestor missing"),
                    SyndicValidationError::Invariant,
                )?;
                let origin = require::<ThreadsFamily>(
                    reader,
                    &turn.origin_thread_id(),
                    "legacy context origin missing",
                )?;
                if origin.context_owner_id().is_some() {
                    return invariant("legacy usable lineage omits inherited context");
                }
            }
        }
        return Ok(());
    };
    if proof.home_id() != reader.home_id() {
        return invariant("context proof belongs to another Home");
    }
    let summary = derive_summary(
        reader,
        proof.home_id(),
        thread,
        proof.established_prefix(),
        true,
    )?;
    if summary != proof.summary()
        || crate::context_projection::read::projection_digest(
            proof.home_id(),
            thread,
            proof.established_prefix(),
            summary,
            execution,
            tool,
        ) != proof.projection_digest()
    {
        return invariant("context proof logical prefix/digest disagrees");
    }
    let (source, cut) = match proof.establishment {
        ContextEstablishment::NativeSeed {
            source,
            cut,
            entry,
            injection,
        } => {
            let own = context_frame(reader, proof.home_id(), thread)?
                .ok_or(SyndicValidationError::Invariant("seed context missing"))?;
            if own.entry != entry
                || own.utf8_bytes != injection.utf8_bytes().get()
                || own.sequence_digest != injection.sequence_digest()
                || cut.summary.append(entry, own.utf8_bytes).ok() != Some(summary)
            {
                return invariant("context seed entry/summary disagrees");
            }
            (source, cut)
        }
        ContextEstablishment::Inherited { source, cut, .. } => (source, cut),
        ContextEstablishment::Replay { injection } => {
            return replay::validate_replay(reader, thread, proof, injection);
        }
    };
    let source_record = require::<BindingsFamily>(
        reader,
        &BindingKey {
            thread: source.thread,
            revision: source.revision,
        },
        "context source binding missing",
    )?;
    let source_usable = super::binding_usable(&source_record).ok_or(
        SyndicValidationError::Invariant("context source binding was not usable"),
    )?;
    if source_usable.execution() != execution || source_usable.tool_profile() != tool {
        return invariant("context source execution/tool profile disagrees");
    }
    if source_usable
        .lineage()
        .recovered_injection_generation()
        .is_some_and(|generation| generation.process() != proof.loaded_generation().process())
    {
        return invariant("native context source belongs to another CAS process");
    }
    let before = derive_summary(reader, proof.home_id(), source.thread, cut.prefix, false)?;
    let full = if let CasLineageProof::ContextAware(source_proof) = source_usable.lineage() {
        if source_proof.home_id() != proof.home_id() {
            return invariant("native context source Home identity differs");
        }
        cut.prefix.tail() == source_usable.represented_prefix().tail()
            && cut.prefix.digest() == source_usable.represented_prefix().digest()
            && cut.summary == source_proof.summary()
    } else {
        false
    };
    if cut.summary != before && !full {
        return invariant("native context cut inclusion disagrees");
    }
    let source_floor = source_usable
        .lineage()
        .recovered_completed_at()
        .unwrap_or(crate::SyndicTimestamp::from_unix_millis(0));
    match proof.establishment {
        ContextEstablishment::NativeSeed { .. } if proof.ordering_floor() < source_floor => {
            invariant("context seed ordering floor regressed")
        }
        ContextEstablishment::Inherited { .. } if proof.ordering_floor() != source_floor => {
            invariant("inherited context ordering floor changed")
        }
        _ => Ok(()),
    }
}
