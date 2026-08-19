use super::*;

#[derive(Clone, Copy)]
pub(super) struct UnchangedMarkerTransform {
    predecessor_start: u64,
    predecessor_end: u64,
    successor_end: u64,
}

impl UnchangedMarkerTransform {
    pub(super) fn new(
        proposal: MutationProposal,
        fragments: &[MutationFragment],
    ) -> Result<Self, ComposerHostError> {
        let predecessor_start = proposal.replacement().start().byte_offset.get();
        let predecessor_end = proposal.replacement().end().byte_offset.get();
        if predecessor_start > predecessor_end {
            return Err(ComposerHostError::MutationMalformed);
        }
        let inserted = fragments.iter().try_fold(0_u64, |inserted, fragment| {
            let MutationFragmentPayload::Utf8 { text, .. } = fragment.payload() else {
                return Ok(inserted);
            };
            inserted
                .checked_add(
                    u64::try_from(text.len()).map_err(|_| ComposerHostError::MutationMalformed)?,
                )
                .ok_or(ComposerHostError::MutationMalformed)
        })?;
        let successor_end = predecessor_start
            .checked_add(inserted)
            .ok_or(ComposerHostError::MutationMalformed)?;
        Ok(Self {
            predecessor_start,
            predecessor_end,
            successor_end,
        })
    }

    pub(super) fn predecessor_anchor_candidates(
        self,
        successor_anchor: u64,
    ) -> Result<[Option<u64>; 2], ComposerHostError> {
        if successor_anchor < self.predecessor_start {
            return Ok([Some(successor_anchor), None]);
        }
        if successor_anchor > self.successor_end {
            let predecessor_anchor = self
                .predecessor_end
                .checked_add(successor_anchor - self.successor_end)
                .ok_or(ComposerHostError::MutationMalformed)?;
            return Ok([Some(predecessor_anchor), None]);
        }
        Ok([
            Some(self.predecessor_start),
            (self.predecessor_end != self.predecessor_start).then_some(self.predecessor_end),
        ])
    }

    pub(super) fn successor_anchor(
        self,
        proposal: MutationProposal,
        predecessor_anchor: u64,
        marker: gpui_text_input::InlineObjectNeighbor,
    ) -> Result<u64, ComposerHostError> {
        if predecessor_anchor < self.predecessor_start {
            return Ok(predecessor_anchor);
        }
        if predecessor_anchor > self.predecessor_end {
            return self
                .successor_end
                .checked_add(predecessor_anchor - self.predecessor_end)
                .ok_or(ComposerHostError::MutationMalformed);
        }
        if self.predecessor_start == self.predecessor_end {
            return match compare_marker_to_position(marker, proposal.replacement().start())? {
                Ordering::Less => Ok(self.predecessor_start),
                Ordering::Greater => Ok(self.successor_end),
                Ordering::Equal => Err(ComposerHostError::MutationMalformed),
            };
        }
        if predecessor_anchor == self.predecessor_start
            && compare_marker_to_position(marker, proposal.replacement().start())? == Ordering::Less
        {
            return Ok(self.predecessor_start);
        }
        if predecessor_anchor == self.predecessor_end
            && compare_marker_to_position(marker, proposal.replacement().end())?
                == Ordering::Greater
        {
            return Ok(self.successor_end);
        }
        Err(ComposerHostError::MutationMalformed)
    }
}

fn compare_marker_to_position(
    marker: gpui_text_input::InlineObjectNeighbor,
    position: SourcePosition,
) -> Result<Ordering, ComposerHostError> {
    let marker_key = (marker.order(), marker.id());
    let ordering = match position.gap {
        InlineObjectGap::NoObjects => return Err(ComposerHostError::MutationMalformed),
        InlineObjectGap::Before(following) => marker_key.cmp(&(following.order(), following.id())),
        InlineObjectGap::After(preceding) => marker_key.cmp(&(preceding.order(), preceding.id())),
        InlineObjectGap::Between {
            preceding,
            following,
        } => {
            if marker_key <= (preceding.order(), preceding.id()) {
                Ordering::Less
            } else if marker_key >= (following.order(), following.id()) {
                Ordering::Greater
            } else {
                return Err(ComposerHostError::MutationMalformed);
            }
        }
    };
    Ok(match position.gap {
        InlineObjectGap::Before(_) if ordering == Ordering::Equal => Ordering::Greater,
        InlineObjectGap::After(_) if ordering == Ordering::Equal => Ordering::Less,
        _ => ordering,
    })
}
