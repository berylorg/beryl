use super::*;
use crate::{
    ConsumedRepairRequest, RepairCaptureGap, RepairCaptureGapReason, RepairRequestAttemptNonce,
    RepairRequestDisposition, RepairRequiredTarget, RepairSourceEventDigest,
    RepairSourceEventWitness,
};

pub(crate) fn enc_repair_target(e: &mut Encoder, target: &RepairRequiredTarget) {
    enc_turn(e, target.turn_id());
    enc_cas_turn_source(e, target.source());
    let gap = target.gap();
    enc_witness(e, gap.terminal());
    enc_turn_end_status(e, gap.status());
    e.u8(match gap.reason() {
        RepairCaptureGapReason::TerminalCaptureIncomplete => 0,
        RepairCaptureGapReason::ForcedAbortOrderingUnproven => 1,
        RepairCaptureGapReason::ProviderObservationIssue => 2,
    });
    enc_opt(e, gap.issue(), enc_witness);
    match target.request() {
        RepairRequestDisposition::Available => e.u8(0),
        RepairRequestDisposition::Consumed(request) => {
            e.u8(1);
            e.fixed16(request.attempt_nonce().as_bytes());
            enc_input_gate_rev(e, request.source_gate_revision());
            enc_input_gate_rev(e, request.successor_gate_revision());
        }
    }
}

pub(crate) fn dec_repair_target(d: &mut Decoder<'_>) -> Result<RepairRequiredTarget, CodecError> {
    let turn = dec_turn(d)?;
    let source = dec_cas_turn_source(d)?;
    let terminal = dec_witness(d)?;
    let status = dec_turn_end_status(d)?;
    let reason = match d.u8()? {
        0 => RepairCaptureGapReason::TerminalCaptureIncomplete,
        1 => RepairCaptureGapReason::ForcedAbortOrderingUnproven,
        2 => RepairCaptureGapReason::ProviderObservationIssue,
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "repair gap reason",
                tag,
            });
        }
    };
    let issue = dec_opt(d, "repair issue witness", dec_witness)?;
    let gap = RepairCaptureGap::new(terminal, status, reason, issue)
        .map_err(|source| invalid("repair capture gap", source))?;
    let request = match d.u8()? {
        0 => RepairRequestDisposition::Available,
        1 => RepairRequestDisposition::Consumed(
            ConsumedRepairRequest::new(
                RepairRequestAttemptNonce::from_bytes(d.fixed16()?),
                dec_input_gate_rev(d)?,
                dec_input_gate_rev(d)?,
            )
            .map_err(|source| invalid("consumed repair request", source))?,
        ),
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "repair request disposition",
                tag,
            });
        }
    };
    Ok(RepairRequiredTarget::new(turn, source, gap, request))
}

fn enc_witness(e: &mut Encoder, witness: RepairSourceEventWitness) {
    enc_source_seq(e, witness.sequence());
    e.fixed32(witness.digest().as_bytes());
}

fn dec_witness(d: &mut Decoder<'_>) -> Result<RepairSourceEventWitness, CodecError> {
    Ok(RepairSourceEventWitness::new(
        dec_source_seq(d)?,
        RepairSourceEventDigest::from_bytes(d.fixed32()?),
    ))
}
