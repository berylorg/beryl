use super::*;
use crate::context_projection::ContextEstablishment;

fn enc_summary(e: &mut Encoder, value: crate::ContextSummary) {
    e.u64(value.count);
    e.u64(value.utf8_bytes);
    e.fixed32(&value.digest);
}
fn dec_summary(d: &mut Decoder<'_>) -> Result<crate::ContextSummary, CodecError> {
    crate::ContextSummary {
        count: d.u64()?,
        utf8_bytes: d.u64()?,
        digest: d.fixed32()?,
    }
    .checked()
    .map_err(|err| invalid("context summary", err))
}
fn enc_source(e: &mut Encoder, value: crate::ContextSourceAnchor) {
    enc_thread(e, value.thread);
    enc_binding_rev(e, value.revision);
}
fn dec_source(d: &mut Decoder<'_>) -> Result<crate::ContextSourceAnchor, CodecError> {
    Ok(crate::ContextSourceAnchor {
        thread: dec_thread(d)?,
        revision: dec_binding_rev(d)?,
    })
}
fn enc_cut(e: &mut Encoder, value: crate::ContextNativeCut) {
    enc_represented_prefix(e, value.prefix);
    enc_summary(e, value.summary);
}
fn dec_cut(d: &mut Decoder<'_>) -> Result<crate::ContextNativeCut, CodecError> {
    Ok(crate::ContextNativeCut {
        prefix: dec_represented_prefix(d)?,
        summary: dec_summary(d)?,
    })
}
fn enc_observation(e: &mut Encoder, value: crate::ContextInjectionObservation) {
    e.u64(u64::from(value.items.get()));
    e.u64(value.bytes.get());
    e.fixed32(value.digest.as_bytes());
    enc_timestamp(e, value.completed_at);
}
fn dec_observation(d: &mut Decoder<'_>) -> Result<crate::ContextInjectionObservation, CodecError> {
    let items = crate::RecoveryItemCount::new(d.u64()?)
        .map_err(|err| invalid("context injection items", err))?;
    let bytes = crate::RecoveryUtf8ByteCount::new(d.u64()?)
        .map_err(|err| invalid("context injection bytes", err))?;
    crate::ContextInjectionObservation::new(
        items,
        bytes,
        RecoveryItemSequenceDigest::from_bytes(d.fixed32()?),
        dec_timestamp(d)?,
    )
    .map_err(|err| invalid("context injection", err))
}
pub(crate) fn enc_context_lineage(e: &mut Encoder, value: crate::ContextLineageProof) {
    e.u8(1);
    e.u8(match value.establishment {
        ContextEstablishment::NativeSeed { .. } => 0,
        ContextEstablishment::Inherited { .. } => 1,
        ContextEstablishment::Replay { .. } => 2,
    });
    e.fixed16(value.home.as_bytes());
    enc_represented_prefix(e, value.prefix);
    enc_summary(e, value.summary);
    e.fixed32(&value.projection_digest);
    enc_timestamp(e, value.ordering_floor);
    enc_loaded_generation(e, value.generation);
    match value.establishment {
        ContextEstablishment::NativeSeed {
            source,
            cut,
            entry,
            injection,
        } => {
            enc_source(e, source);
            enc_cut(e, cut);
            e.fixed32(&entry);
            enc_observation(e, injection);
        }
        ContextEstablishment::Inherited {
            mechanism,
            source,
            cut,
        } => {
            e.u8(match mechanism {
                crate::NativeCasLineage::Fresh => 0,
                crate::NativeCasLineage::Continuation => 1,
                crate::NativeCasLineage::Resume => 2,
                crate::NativeCasLineage::Fork => 3,
            });
            enc_source(e, source);
            enc_cut(e, cut);
        }
        ContextEstablishment::Replay { injection } => {
            e.u8(2);
            enc_observation(e, injection);
        }
    }
}
pub(crate) fn dec_context_lineage(
    d: &mut Decoder<'_>,
) -> Result<crate::ContextLineageProof, CodecError> {
    let version = d.u8()?;
    if version != 1 {
        return Err(CodecError::InvalidTag {
            kind: "context lineage version",
            tag: version,
        });
    }
    let kind = d.u8()?;
    let home = BerylHomeId::from_bytes(d.fixed16()?);
    let prefix = dec_represented_prefix(d)?;
    let summary = dec_summary(d)?;
    let projection_digest = d.fixed32()?;
    let ordering_floor = dec_timestamp(d)?;
    let generation = dec_loaded_generation(d)?;
    let establishment = match kind {
        0 => ContextEstablishment::NativeSeed {
            source: dec_source(d)?,
            cut: dec_cut(d)?,
            entry: d.fixed32()?,
            injection: dec_observation(d)?,
        },
        1 => {
            let mechanism = match d.u8()? {
                1 => crate::NativeCasLineage::Continuation,
                2 => crate::NativeCasLineage::Resume,
                3 => crate::NativeCasLineage::Fork,
                tag => {
                    return Err(CodecError::InvalidTag {
                        kind: "inherited context mechanism",
                        tag,
                    });
                }
            };
            ContextEstablishment::Inherited {
                mechanism,
                source: dec_source(d)?,
                cut: dec_cut(d)?,
            }
        }
        2 => {
            let version = d.u8()?;
            if version != 2 {
                return Err(CodecError::InvalidTag {
                    kind: "context replay version",
                    tag: version,
                });
            }
            ContextEstablishment::Replay {
                injection: dec_observation(d)?,
            }
        }
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "context establishment",
                tag,
            });
        }
    };
    crate::ContextLineageProof {
        home,
        prefix,
        summary,
        projection_digest,
        ordering_floor,
        generation,
        establishment,
    }
    .checked()
    .map_err(|err| invalid("context lineage", err))
}
