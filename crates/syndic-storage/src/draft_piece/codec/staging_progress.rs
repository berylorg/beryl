use crate::codec::CodecError;
use crate::codec::parts::{Decoder, Encoder};

use super::super::staging::draft_mutation_staging_receipt_is_locally_exact;
use super::super::*;
use super::staging_head::{
    dec_staging_frontier, dec_staging_identity, dec_staging_lifecycle, dec_staging_progress_key,
    dec_staging_receipt_reference, enc_staging_frontier, enc_staging_identity,
    enc_staging_lifecycle, enc_staging_progress_key, enc_staging_receipt_reference,
};
use super::staging_page::{dec_staging_page_key, enc_staging_page_key};
use super::{
    dec_digest, dec_progress_reference, dec_root_key, dec_settlement_key, enc_digest,
    enc_progress_reference, enc_root_key, enc_settlement_key,
};

fn enc_staging_terminal_evidence(e: &mut Encoder, value: DraftMutationStagingTerminalEvidenceV1) {
    match value {
        DraftMutationStagingTerminalEvidenceV1::Rejected {
            reason,
            anchor,
            digest,
            candidate_generation,
            root,
            history,
            session_revision,
        } => {
            e.u8(0);
            e.u8(reason as u8);
            enc_staging_terminal_anchor(e, anchor);
            enc_digest(e, digest);
            e.u64(candidate_generation);
            enc_root_reference(e, root);
            enc_history_reference(e, history);
            e.u64(session_revision);
        }
        DraftMutationStagingTerminalEvidenceV1::Conflict {
            expected_generation,
            expected_root,
            expected_history,
            observed_generation,
            observed_root,
            observed_history,
            session_revision,
        } => {
            e.u8(1);
            e.u64(expected_generation);
            enc_root_reference(e, expected_root);
            enc_history_reference(e, expected_history);
            e.u64(observed_generation);
            enc_root_reference(e, observed_root);
            enc_history_reference(e, observed_history);
            e.u64(session_revision);
        }
        DraftMutationStagingTerminalEvidenceV1::Cancelled {
            request_id,
            source_lifecycle,
            writer_admitted,
            candidate_generation,
            root,
            history,
            session_revision,
        } => {
            e.u8(2);
            e.fixed16(request_id.as_bytes());
            enc_staging_lifecycle(e, source_lifecycle);
            e.u8(u8::from(writer_admitted));
            e.u64(candidate_generation);
            enc_root_reference(e, root);
            enc_history_reference(e, history);
            e.u64(session_revision);
        }
        DraftMutationStagingTerminalEvidenceV1::Error {
            error,
            candidate_generation,
            root,
            history,
            session_revision,
        } => {
            e.u8(3);
            enc_staging_error_evidence(e, error);
            e.u64(candidate_generation);
            enc_root_reference(e, root);
            enc_history_reference(e, history);
            e.u64(session_revision);
        }
    }
}

fn enc_staging_terminal_anchor(e: &mut Encoder, value: DraftMutationStagingTerminalAnchorV1) {
    match value {
        DraftMutationStagingTerminalAnchorV1::Begin(identity) => {
            e.u8(0);
            enc_staging_identity(e, identity);
        }
        DraftMutationStagingTerminalAnchorV1::Page(key) => {
            e.u8(1);
            enc_staging_page_key(e, key);
        }
        DraftMutationStagingTerminalAnchorV1::Finish(identity) => {
            e.u8(2);
            enc_staging_identity(e, identity);
        }
    }
}

fn dec_staging_terminal_anchor(
    d: &mut Decoder<'_>,
) -> Result<DraftMutationStagingTerminalAnchorV1, CodecError> {
    match d.u8()? {
        0 => Ok(DraftMutationStagingTerminalAnchorV1::Begin(
            dec_staging_identity(d)?,
        )),
        1 => Ok(DraftMutationStagingTerminalAnchorV1::Page(
            dec_staging_page_key(d)?,
        )),
        2 => Ok(DraftMutationStagingTerminalAnchorV1::Finish(
            dec_staging_identity(d)?,
        )),
        tag => Err(CodecError::InvalidTag {
            kind: "draft mutation terminal anchor",
            tag,
        }),
    }
}

fn enc_staging_compared_byte(e: &mut Encoder, value: DraftMutationStagingComparedByteV1) {
    match value {
        DraftMutationStagingComparedByteV1::Byte(byte) => {
            e.u8(0);
            e.u8(byte);
        }
        DraftMutationStagingComparedByteV1::End => e.u8(1),
    }
}

fn dec_staging_compared_byte(
    d: &mut Decoder<'_>,
) -> Result<DraftMutationStagingComparedByteV1, CodecError> {
    match d.u8()? {
        0 => Ok(DraftMutationStagingComparedByteV1::Byte(d.u8()?)),
        1 => Ok(DraftMutationStagingComparedByteV1::End),
        tag => Err(CodecError::InvalidTag {
            kind: "draft mutation compared byte",
            tag,
        }),
    }
}

fn enc_staging_occupied_key(e: &mut Encoder, value: DraftMutationStagingOccupiedKeyV1) {
    match value {
        DraftMutationStagingOccupiedKeyV1::Head(identity) => {
            e.u8(0);
            enc_staging_identity(e, identity);
        }
        DraftMutationStagingOccupiedKeyV1::Page(key) => {
            e.u8(1);
            enc_staging_page_key(e, key);
        }
        DraftMutationStagingOccupiedKeyV1::Progress(key) => {
            e.u8(2);
            enc_staging_progress_key(e, key);
        }
        DraftMutationStagingOccupiedKeyV1::Build(key) => {
            e.u8(3);
            enc_settlement_key(e, key);
        }
        DraftMutationStagingOccupiedKeyV1::Settlement(key) => {
            e.u8(4);
            enc_settlement_key(e, key);
        }
        DraftMutationStagingOccupiedKeyV1::CandidateRoot(key) => {
            e.u8(5);
            enc_root_key(e, key);
        }
    }
}

fn dec_staging_occupied_key(
    d: &mut Decoder<'_>,
) -> Result<DraftMutationStagingOccupiedKeyV1, CodecError> {
    match d.u8()? {
        0 => Ok(DraftMutationStagingOccupiedKeyV1::Head(
            dec_staging_identity(d)?,
        )),
        1 => Ok(DraftMutationStagingOccupiedKeyV1::Page(
            dec_staging_page_key(d)?,
        )),
        2 => Ok(DraftMutationStagingOccupiedKeyV1::Progress(
            dec_staging_progress_key(d)?,
        )),
        3 => Ok(DraftMutationStagingOccupiedKeyV1::Build(
            dec_settlement_key(d)?,
        )),
        4 => Ok(DraftMutationStagingOccupiedKeyV1::Settlement(
            dec_settlement_key(d)?,
        )),
        5 => Ok(DraftMutationStagingOccupiedKeyV1::CandidateRoot(
            dec_root_key(d)?,
        )),
        tag => Err(CodecError::InvalidTag {
            kind: "draft mutation occupied key",
            tag,
        }),
    }
}

fn enc_staging_error_evidence(e: &mut Encoder, value: DraftMutationStagingErrorEvidenceV1) {
    match value {
        DraftMutationStagingErrorEvidenceV1::Operational { reason, anchor } => {
            e.u8(0);
            e.u8(reason as u8);
            enc_staging_terminal_anchor(e, anchor);
        }
        DraftMutationStagingErrorEvidenceV1::OccupiedIdentity {
            key,
            stored_digest,
            requested_digest,
            first_difference,
            stored,
            requested,
        } => {
            e.u8(1);
            enc_staging_occupied_key(e, key);
            enc_digest(e, stored_digest);
            enc_digest(e, requested_digest);
            e.u64(first_difference);
            enc_staging_compared_byte(e, stored);
            enc_staging_compared_byte(e, requested);
        }
    }
}

fn dec_staging_error_evidence(
    d: &mut Decoder<'_>,
) -> Result<DraftMutationStagingErrorEvidenceV1, CodecError> {
    match d.u8()? {
        0 => {
            let reason = match d.u8()? {
                0 => DraftMutationStagingErrorReasonV1::Operational,
                1 => DraftMutationStagingErrorReasonV1::Overflow,
                2 => DraftMutationStagingErrorReasonV1::Corruption,
                tag => {
                    return Err(CodecError::InvalidTag {
                        kind: "draft mutation operational error",
                        tag,
                    });
                }
            };
            Ok(DraftMutationStagingErrorEvidenceV1::Operational {
                reason,
                anchor: dec_staging_terminal_anchor(d)?,
            })
        }
        1 => Ok(DraftMutationStagingErrorEvidenceV1::OccupiedIdentity {
            key: dec_staging_occupied_key(d)?,
            stored_digest: dec_digest(d)?,
            requested_digest: dec_digest(d)?,
            first_difference: d.u64()?,
            stored: dec_staging_compared_byte(d)?,
            requested: dec_staging_compared_byte(d)?,
        }),
        tag => Err(CodecError::InvalidTag {
            kind: "draft mutation error evidence",
            tag,
        }),
    }
}

fn dec_staging_terminal_evidence(
    d: &mut Decoder<'_>,
) -> Result<DraftMutationStagingTerminalEvidenceV1, CodecError> {
    match d.u8()? {
        0 => {
            let reason = match d.u8()? {
                0 => DraftMutationStagingRejectedReasonV1::InvalidEnvelope,
                1 => DraftMutationStagingRejectedReasonV1::InvalidPage,
                2 => DraftMutationStagingRejectedReasonV1::InvalidFinish,
                3 => DraftMutationStagingRejectedReasonV1::EmptyProposal,
                tag => {
                    return Err(CodecError::InvalidTag {
                        kind: "draft mutation rejection",
                        tag,
                    });
                }
            };
            Ok(DraftMutationStagingTerminalEvidenceV1::Rejected {
                reason,
                anchor: dec_staging_terminal_anchor(d)?,
                digest: dec_digest(d)?,
                candidate_generation: d.u64()?,
                root: dec_root_reference(d)?,
                history: dec_history_reference(d)?,
                session_revision: d.u64()?,
            })
        }
        1 => Ok(DraftMutationStagingTerminalEvidenceV1::Conflict {
            expected_generation: d.u64()?,
            expected_root: dec_root_reference(d)?,
            expected_history: dec_history_reference(d)?,
            observed_generation: d.u64()?,
            observed_root: dec_root_reference(d)?,
            observed_history: dec_history_reference(d)?,
            session_revision: d.u64()?,
        }),
        2 => {
            let request_id = DraftMutationOperationIdV1::from_bytes(d.fixed16()?);
            let source_lifecycle = dec_staging_lifecycle(d)?;
            let writer_admitted = match d.u8()? {
                0 => false,
                1 => true,
                tag => {
                    return Err(CodecError::InvalidTag {
                        kind: "draft mutation writer admission",
                        tag,
                    });
                }
            };
            Ok(DraftMutationStagingTerminalEvidenceV1::Cancelled {
                request_id,
                source_lifecycle,
                writer_admitted,
                candidate_generation: d.u64()?,
                root: dec_root_reference(d)?,
                history: dec_history_reference(d)?,
                session_revision: d.u64()?,
            })
        }
        3 => Ok(DraftMutationStagingTerminalEvidenceV1::Error {
            error: dec_staging_error_evidence(d)?,
            candidate_generation: d.u64()?,
            root: dec_root_reference(d)?,
            history: dec_history_reference(d)?,
            session_revision: d.u64()?,
        }),
        tag => Err(CodecError::InvalidTag {
            kind: "draft mutation terminal evidence",
            tag,
        }),
    }
}

pub(super) fn encode_staging_progress(
    value: &DraftMutationStagingProgressReceiptV1,
) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_staging_progress_key(&mut e, value.key());
    crate::codec::parts::enc_opt(&mut e, value.prior(), enc_staging_receipt_reference);
    e.u8(match value.command() {
        DraftMutationStagingCommandKindV1::Begin => 0,
        DraftMutationStagingCommandKindV1::SourcePage => 1,
        DraftMutationStagingCommandKindV1::ProposalPage => 2,
        DraftMutationStagingCommandKindV1::Finish => 3,
        DraftMutationStagingCommandKindV1::Transfer => 4,
        DraftMutationStagingCommandKindV1::Terminal => 5,
    });
    crate::codec::parts::enc_opt(&mut e, value.page(), |e, (key, digest)| {
        enc_staging_page_key(e, key);
        enc_digest(e, digest)
    });
    crate::codec::parts::enc_opt(&mut e, value.finish_digest(), enc_digest);
    enc_staging_frontier(&mut e, value.before_source());
    enc_staging_frontier(&mut e, value.after_source());
    enc_staging_frontier(&mut e, value.before_proposal());
    enc_staging_frontier(&mut e, value.after_proposal());
    crate::codec::parts::enc_opt(&mut e, value.before_head_digest(), enc_digest);
    enc_digest(&mut e, value.after_head_digest());
    crate::codec::parts::enc_opt(&mut e, value.before_lifecycle(), enc_staging_lifecycle);
    enc_staging_lifecycle(&mut e, value.after_lifecycle());
    e.u8(value.custody_before() as u8);
    e.u8(value.custody_after() as u8);
    crate::codec::parts::enc_opt(&mut e, value.build_endpoint(), enc_progress_reference);
    crate::codec::parts::enc_opt(
        &mut e,
        value.terminal_evidence(),
        enc_staging_terminal_evidence,
    );
    enc_digest(&mut e, value.digest());
    Ok(e.finish())
}

pub(crate) fn canonical_staging_progress_bytes(
    value: &DraftMutationStagingProgressReceiptV1,
) -> Result<Vec<u8>, CodecError> {
    encode_staging_progress(value)
}

pub(super) fn decode_staging_progress(
    bytes: &[u8],
) -> Result<DraftMutationStagingProgressReceiptV1, CodecError> {
    let mut d = Decoder::new(bytes);
    let key = dec_staging_progress_key(&mut d)?;
    let prior = crate::codec::parts::dec_opt(
        &mut d,
        "draft mutation prior receipt",
        dec_staging_receipt_reference,
    )?;
    let command = match d.u8()? {
        0 => DraftMutationStagingCommandKindV1::Begin,
        1 => DraftMutationStagingCommandKindV1::SourcePage,
        2 => DraftMutationStagingCommandKindV1::ProposalPage,
        3 => DraftMutationStagingCommandKindV1::Finish,
        4 => DraftMutationStagingCommandKindV1::Transfer,
        5 => DraftMutationStagingCommandKindV1::Terminal,
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "draft mutation staging command",
                tag,
            });
        }
    };
    let page = crate::codec::parts::dec_opt(&mut d, "draft mutation page effect", |d| {
        Ok((dec_staging_page_key(d)?, dec_digest(d)?))
    })?;
    let finish_digest =
        crate::codec::parts::dec_opt(&mut d, "draft mutation finish digest", dec_digest)?;
    let before_source = dec_staging_frontier(&mut d)?;
    let after_source = dec_staging_frontier(&mut d)?;
    let before_proposal = dec_staging_frontier(&mut d)?;
    let after_proposal = dec_staging_frontier(&mut d)?;
    let before_head_digest =
        crate::codec::parts::dec_opt(&mut d, "draft mutation prior head digest", dec_digest)?;
    let after_head_digest = dec_digest(&mut d)?;
    let before_lifecycle = crate::codec::parts::dec_opt(
        &mut d,
        "draft mutation prior lifecycle",
        dec_staging_lifecycle,
    )?;
    let after_lifecycle = dec_staging_lifecycle(&mut d)?;
    let custody_before = match d.u8()? {
        0 => DraftMutationStagingCustodyTagV1::None,
        1 => DraftMutationStagingCustodyTagV1::Staging,
        2 => DraftMutationStagingCustodyTagV1::Building,
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "draft mutation custody",
                tag,
            });
        }
    };
    let custody_after = match d.u8()? {
        0 => DraftMutationStagingCustodyTagV1::None,
        1 => DraftMutationStagingCustodyTagV1::Staging,
        2 => DraftMutationStagingCustodyTagV1::Building,
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "draft mutation custody",
                tag,
            });
        }
    };
    let build_endpoint = crate::codec::parts::dec_opt(
        &mut d,
        "draft mutation build endpoint",
        dec_progress_reference,
    )?;
    let terminal_evidence = crate::codec::parts::dec_opt(
        &mut d,
        "draft mutation terminal evidence",
        dec_staging_terminal_evidence,
    )?;
    let digest = dec_digest(&mut d)?;
    d.finish()?;
    let value = DraftMutationStagingProgressReceiptV1::from_parts(
        key,
        prior,
        command,
        page,
        finish_digest,
        before_source,
        after_source,
        before_proposal,
        after_proposal,
        before_head_digest,
        after_head_digest,
        before_lifecycle,
        after_lifecycle,
        custody_before,
        custody_after,
        build_endpoint,
        terminal_evidence,
        digest,
    );
    if !draft_mutation_staging_receipt_is_locally_exact(&value) {
        return Err(CodecError::InvalidLength(
            "draft mutation staging progress receipt",
        ));
    }
    Ok(value)
}
