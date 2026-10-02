use super::*;

struct CommittedParts {
    predecessor_session: Box<DraftEditorCandidateSessionV1>,
    adopted_session: Box<DraftEditorCandidateSessionV1>,
    adopted_root: Box<DraftPieceRootRecordV1>,
    predecessor_history: Box<DraftEditHistoryFrontierV1>,
    transition: Box<DraftEditHistoryTransitionV1>,
    adopted_history: Box<DraftEditHistoryFrontierV1>,
}

#[inline(never)]
pub(super) fn decode_settlement_closure(
    d: &mut Decoder<'_>,
) -> Result<Box<DraftPieceSettlementClosureV1>, CodecError> {
    match d.u8()? {
        0 => {
            let parts = decode_committed_parts(d)?;
            Ok(assemble_committed(parts))
        }
        1 => decode_noncommit(d),
        tag => Err(CodecError::InvalidTag {
            kind: "draft-piece settlement closure",
            tag,
        }),
    }
}

#[inline(never)]
fn decode_committed_parts(d: &mut Decoder<'_>) -> Result<CommittedParts, CodecError> {
    Ok(CommittedParts {
        predecessor_session: read_session(d)?,
        adopted_session: read_session(d)?,
        adopted_root: read_root(d)?,
        predecessor_history: read_frontier(d)?,
        transition: read_transition(d)?,
        adopted_history: read_frontier(d)?,
    })
}

#[inline(never)]
fn read_session(d: &mut Decoder<'_>) -> Result<Box<DraftEditorCandidateSessionV1>, CodecError> {
    dec_session_head(d).map(Box::new)
}

#[inline(never)]
fn read_root(d: &mut Decoder<'_>) -> Result<Box<DraftPieceRootRecordV1>, CodecError> {
    dec_root_reference(d).map(|reference| Box::new(DraftPieceRootRecordV1::new(reference)))
}

#[inline(never)]
fn read_frontier(d: &mut Decoder<'_>) -> Result<Box<DraftEditHistoryFrontierV1>, CodecError> {
    dec_history_frontier(d).map(Box::new)
}

#[inline(never)]
fn read_transition(d: &mut Decoder<'_>) -> Result<Box<DraftEditHistoryTransitionV1>, CodecError> {
    dec_history_transition(d).map(Box::new)
}

#[inline(never)]
fn assemble_committed(parts: CommittedParts) -> Box<DraftPieceSettlementClosureV1> {
    Box::new(DraftPieceSettlementClosureV1::Committed(
        DraftPieceCommittedAdoptionV1::new(
            *parts.predecessor_session,
            *parts.adopted_session,
            *parts.adopted_root,
            *parts.predecessor_history,
            *parts.transition,
            *parts.adopted_history,
        ),
    ))
}

#[inline(never)]
fn decode_noncommit(d: &mut Decoder<'_>) -> Result<Box<DraftPieceSettlementClosureV1>, CodecError> {
    let observed_session = read_session(d)?;
    let observed_history = read_frontier(d)?;
    let proposed_successor = match d.u8()? {
        0 => None,
        1 => Some(dec_root_reference(d)?),
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "draft-piece proposed successor option",
                tag,
            });
        }
    };
    let occupied_identity = match d.u8()? {
        0 => None,
        1 => Some(dec_occupied_identity_proof(d)?),
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "draft-piece occupied identity proof option",
                tag,
            });
        }
    };
    assemble_noncommit(
        observed_session,
        observed_history,
        proposed_successor,
        occupied_identity,
    )
}

#[inline(never)]
fn assemble_noncommit(
    observed_session: Box<DraftEditorCandidateSessionV1>,
    observed_history: Box<DraftEditHistoryFrontierV1>,
    proposed_successor: Option<DraftPieceRootReferenceV1>,
    occupied_identity: Option<OccupiedIdentityNoncommitProofV1>,
) -> Result<Box<DraftPieceSettlementClosureV1>, CodecError> {
    let noncommit = match (proposed_successor, occupied_identity) {
        (Some(successor), Some(proof)) => DraftPieceNoncommitClosureV1::with_occupied_identity(
            *observed_session,
            *observed_history,
            successor,
            proof,
        ),
        (successor, None) => {
            DraftPieceNoncommitClosureV1::new(*observed_session, *observed_history, successor)
        }
        (None, Some(_)) => {
            return Err(CodecError::InvalidLength(
                "draft-piece occupied identity successor",
            ));
        }
    };
    Ok(Box::new(DraftPieceSettlementClosureV1::Noncommit(
        noncommit,
    )))
}
