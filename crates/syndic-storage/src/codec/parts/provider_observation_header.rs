use beryl_model::ProviderObservationId;

use crate::{
    ProviderDeltaKind, ProviderObservationBegin, ProviderObservationItemKind,
    ProviderObservationItemLifecycle,
};

use super::{CodecError, Decoder, Encoder};

pub(crate) fn enc_provider_observation_id(encoder: &mut Encoder, identity: ProviderObservationId) {
    encoder.fixed16(identity.as_bytes());
}

pub(crate) fn dec_provider_observation_id(
    decoder: &mut Decoder<'_>,
) -> Result<ProviderObservationId, CodecError> {
    Ok(ProviderObservationId::from_bytes(decoder.fixed16()?))
}

pub(crate) fn enc_provider_observation_begin(
    encoder: &mut Encoder,
    begin: ProviderObservationBegin,
) {
    match begin {
        ProviderObservationBegin::Item { lifecycle, kind } => {
            encoder.u8(0);
            encoder.u8(lifecycle as u8);
            encoder.u8(kind as u8);
        }
        ProviderObservationBegin::Delta { kind } => {
            encoder.u8(1);
            encoder.u8(kind as u8);
        }
    }
}

pub(crate) fn dec_provider_observation_begin(
    decoder: &mut Decoder<'_>,
) -> Result<ProviderObservationBegin, CodecError> {
    match decoder.u8()? {
        0 => {
            let lifecycle_tag = decoder.u8()?;
            let item_tag = decoder.u8()?;
            Ok(ProviderObservationBegin::Item {
                lifecycle: ProviderObservationItemLifecycle::from_tag(lifecycle_tag).ok_or(
                    CodecError::InvalidTag {
                        kind: "provider-observation lifecycle",
                        tag: lifecycle_tag,
                    },
                )?,
                kind: ProviderObservationItemKind::from_tag(item_tag).ok_or(
                    CodecError::InvalidTag {
                        kind: "provider-observation item kind",
                        tag: item_tag,
                    },
                )?,
            })
        }
        1 => {
            let tag = decoder.u8()?;
            Ok(ProviderObservationBegin::Delta {
                kind: ProviderDeltaKind::from_tag(tag).ok_or(CodecError::InvalidTag {
                    kind: "provider-observation delta kind",
                    tag,
                })?,
            })
        }
        tag => Err(CodecError::InvalidTag {
            kind: "provider-observation begin",
            tag,
        }),
    }
}
