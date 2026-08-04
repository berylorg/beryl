use super::*;
use sha2::{Digest, Sha256};

const SETTLEMENT_RECEIPT_COMMITMENT_V1: &[u8] =
    b"beryl.syndic.compaction-settlement-receipt.commitment.v1\0";

pub(crate) struct CompactionSettlementReceiptsFamily;
pub(crate) type CompactionSettlementReceiptsCodec = ExactCodec<CompactionSettlementReceiptsFamily>;

impl Family for CompactionSettlementReceiptsFamily {
    type Key = CompactionOperationId;
    type Value = CompactionSettlementReceiptRecord;
    const NAME: &'static str = "compaction-settlement-receipts";
    const RECORD_VERSION: beryl_home_store::RecordVersion = beryl_home_store::RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 32;
    const MAX_VALUE_BYTES: usize = SMALL_MAX;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, CodecError> {
        let mut e = Encoder::new();
        enc_id(&mut e, *key);
        Ok(e.finish())
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, CodecError> {
        let mut d = Decoder::new(encoded);
        let key = dec_id(&mut d)?;
        d.finish()?;
        Ok(key)
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, CodecError> {
        let mut e = Encoder::new();
        enc_id(&mut e, value.operation_id());
        e.u64(value.source_operation_revision().get());
        e.u64(value.successor_operation_revision().get());
        enc_input_gate_record(&mut e, value.source_gate());
        enc_input_gate_record(&mut e, value.successor_gate());
        enc_settlement(&mut e, value.settlement());
        enc_opt(&mut e, value.continuation(), |e, continuation| {
            enc_parent(e, continuation.parent());
            enc_selected_path(e, continuation.selected_path());
            enc_binding_rev(e, continuation.binding_revision());
            enc_content_ref(e, continuation.content());
        });
        Ok(e.finish())
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, CodecError> {
        let mut d = Decoder::new(encoded);
        let operation_id = dec_id(&mut d)?;
        let source_operation_revision =
            compaction_revision(&mut d, "compaction settlement source operation revision")?;
        let successor_operation_revision =
            compaction_revision(&mut d, "compaction settlement successor operation revision")?;
        let source_gate = dec_input_gate_record(&mut d)?;
        let successor_gate = dec_input_gate_record(&mut d)?;
        let settlement = dec_settlement(&mut d)?;
        let continuation = dec_opt(&mut d, "compaction continuation receipt", |d| {
            Ok(CompactionContinuationReceipt::new(
                dec_parent(d)?,
                dec_selected_path(d)?,
                dec_binding_rev(d)?,
                dec_content_ref(d)?,
            ))
        })?;
        let value = CompactionSettlementReceiptRecord::new(
            operation_id,
            source_operation_revision,
            successor_operation_revision,
            source_gate,
            successor_gate,
            settlement,
            continuation,
        )
        .map_err(|source| invalid("compaction settlement receipt", source))?;
        d.finish()?;
        Ok(value)
    }
}

pub(crate) fn compaction_settlement_receipt_commitment(
    value: &CompactionSettlementReceiptRecord,
) -> Result<CompactionSettlementReceiptCommitment, CodecError> {
    let encoded = CompactionSettlementReceiptsFamily::encode_value(value)?;
    let mut digest = Sha256::new();
    digest.update(SETTLEMENT_RECEIPT_COMMITMENT_V1);
    digest.update((encoded.len() as u64).to_be_bytes());
    digest.update(encoded);
    Ok(CompactionSettlementReceiptCommitment::from_bytes(
        digest.finalize().into(),
    ))
}
