use crate::{
    ProviderContainer, ProviderEnumValue, ProviderField, ProviderFiniteF64,
    ProviderFrameHistorySupportV1, ProviderObservationBuildLifecycle,
    ProviderObservationBuildRecord, ProviderObservationChunkPayload,
    ProviderObservationChunkRecord, ProviderObservationControl, ProviderObservationDigest,
    ProviderScalar, ProviderStructuredPosition, ProviderValueContext,
    provider_observation::{
        ListKind, ObjectSchema, PROVIDER_OBSERVATION_MAX_FRAME_DEPTH,
        ProviderIdentityValidatorState, ProviderObservationElementKind, ProviderObservationFrame,
        ProviderObservationValidatorState, Utf8ValidatorState,
    },
};

use super::{
    CodecError, Decoder, Encoder, dec_bool, dec_provider_observation_begin,
    dec_provider_observation_id, enc_provider_observation_begin, enc_provider_observation_id,
    invalid,
};

fn enc_context(encoder: &mut Encoder, context: ProviderValueContext) {
    match context {
        ProviderValueContext::Field(field) => {
            encoder.u8(0);
            encoder.u8(field.tag());
        }
        ProviderValueContext::Structured {
            root,
            depth,
            position,
        } => {
            encoder.u8(1);
            encoder.u8(root.tag());
            encoder.u8(depth);
            match position {
                ProviderStructuredPosition::ListElement { index } => {
                    encoder.u8(0);
                    encoder.u64(index);
                }
                ProviderStructuredPosition::ObjectKey { entry } => {
                    encoder.u8(1);
                    encoder.u64(entry);
                }
                ProviderStructuredPosition::ObjectValue { entry } => {
                    encoder.u8(2);
                    encoder.u64(entry);
                }
            }
        }
    }
}

fn dec_context(decoder: &mut Decoder<'_>) -> Result<ProviderValueContext, CodecError> {
    match decoder.u8()? {
        0 => Ok(ProviderValueContext::Field(dec_field(decoder)?)),
        1 => {
            let root = dec_field(decoder)?;
            let depth = decoder.u8()?;
            let position = match decoder.u8()? {
                0 => ProviderStructuredPosition::ListElement {
                    index: decoder.u64()?,
                },
                1 => ProviderStructuredPosition::ObjectKey {
                    entry: decoder.u64()?,
                },
                2 => ProviderStructuredPosition::ObjectValue {
                    entry: decoder.u64()?,
                },
                tag => {
                    return Err(CodecError::InvalidTag {
                        kind: "provider structured position",
                        tag,
                    });
                }
            };
            Ok(ProviderValueContext::Structured {
                root,
                depth,
                position,
            })
        }
        tag => Err(CodecError::InvalidTag {
            kind: "provider value context",
            tag,
        }),
    }
}

fn dec_field(decoder: &mut Decoder<'_>) -> Result<ProviderField, CodecError> {
    let tag = decoder.u8()?;
    ProviderField::from_tag(tag).ok_or(CodecError::InvalidTag {
        kind: "provider field",
        tag,
    })
}

fn enc_scalar(encoder: &mut Encoder, scalar: ProviderScalar) {
    match scalar {
        ProviderScalar::Null => encoder.u8(0),
        ProviderScalar::Boolean(value) => {
            encoder.u8(1);
            encoder.u8(u8::from(value));
        }
        ProviderScalar::Signed(value) => {
            encoder.u8(2);
            encoder.u64(value as u64);
        }
        ProviderScalar::Unsigned(value) => {
            encoder.u8(3);
            encoder.u64(value);
        }
        ProviderScalar::FiniteFloat(value) => {
            encoder.u8(4);
            encoder.u64(value.bits());
        }
    }
}

fn dec_scalar(decoder: &mut Decoder<'_>) -> Result<ProviderScalar, CodecError> {
    match decoder.u8()? {
        0 => Ok(ProviderScalar::Null),
        1 => match decoder.u8()? {
            0 => Ok(ProviderScalar::Boolean(false)),
            1 => Ok(ProviderScalar::Boolean(true)),
            tag => Err(CodecError::InvalidTag {
                kind: "provider boolean",
                tag,
            }),
        },
        2 => Ok(ProviderScalar::Signed(decoder.u64()? as i64)),
        3 => Ok(ProviderScalar::Unsigned(decoder.u64()?)),
        4 => {
            let bits = decoder.u64()?;
            ProviderFiniteF64::from_bits(bits)
                .map(ProviderScalar::FiniteFloat)
                .ok_or(CodecError::InvalidValue {
                    kind: "provider finite float",
                    source: std::io::Error::other("non-finite value").into(),
                })
        }
        tag => Err(CodecError::InvalidTag {
            kind: "provider scalar",
            tag,
        }),
    }
}

fn enc_control(encoder: &mut Encoder, control: ProviderObservationControl) {
    match control {
        ProviderObservationControl::BeginField(context) => {
            encoder.u8(0);
            enc_context(encoder, context);
        }
        ProviderObservationControl::EndField(context) => {
            encoder.u8(1);
            enc_context(encoder, context);
        }
        ProviderObservationControl::BeginContainer { context, container } => {
            encoder.u8(2);
            enc_context(encoder, context);
            encoder.u8(container as u8);
        }
        ProviderObservationControl::EndContainer { context, container } => {
            encoder.u8(3);
            enc_context(encoder, context);
            encoder.u8(container as u8);
        }
        ProviderObservationControl::BeginElement { context, index } => {
            encoder.u8(4);
            enc_context(encoder, context);
            encoder.u64(index);
        }
        ProviderObservationControl::EndElement { context, index } => {
            encoder.u8(5);
            enc_context(encoder, context);
            encoder.u64(index);
        }
        ProviderObservationControl::BeginObjectEntry { root, depth, entry } => {
            encoder.u8(6);
            encoder.u8(root.tag());
            encoder.u8(depth);
            encoder.u64(entry);
        }
        ProviderObservationControl::EndObjectEntry { root, depth, entry } => {
            encoder.u8(7);
            encoder.u8(root.tag());
            encoder.u8(depth);
            encoder.u64(entry);
        }
        ProviderObservationControl::Enum { context, value } => {
            encoder.u8(8);
            enc_context(encoder, context);
            encoder.u8(value.tag());
        }
        ProviderObservationControl::Scalar { context, value } => {
            encoder.u8(9);
            enc_context(encoder, context);
            enc_scalar(encoder, value);
        }
    }
}

fn dec_control(decoder: &mut Decoder<'_>) -> Result<ProviderObservationControl, CodecError> {
    Ok(match decoder.u8()? {
        0 => ProviderObservationControl::BeginField(dec_context(decoder)?),
        1 => ProviderObservationControl::EndField(dec_context(decoder)?),
        2 => ProviderObservationControl::BeginContainer {
            context: dec_context(decoder)?,
            container: dec_container(decoder)?,
        },
        3 => ProviderObservationControl::EndContainer {
            context: dec_context(decoder)?,
            container: dec_container(decoder)?,
        },
        4 => ProviderObservationControl::BeginElement {
            context: dec_context(decoder)?,
            index: decoder.u64()?,
        },
        5 => ProviderObservationControl::EndElement {
            context: dec_context(decoder)?,
            index: decoder.u64()?,
        },
        6 => ProviderObservationControl::BeginObjectEntry {
            root: dec_field(decoder)?,
            depth: decoder.u8()?,
            entry: decoder.u64()?,
        },
        7 => ProviderObservationControl::EndObjectEntry {
            root: dec_field(decoder)?,
            depth: decoder.u8()?,
            entry: decoder.u64()?,
        },
        8 => {
            let context = dec_context(decoder)?;
            let tag = decoder.u8()?;
            ProviderObservationControl::Enum {
                context,
                value: ProviderEnumValue::from_tag(tag).ok_or(CodecError::InvalidTag {
                    kind: "provider enum",
                    tag,
                })?,
            }
        }
        9 => ProviderObservationControl::Scalar {
            context: dec_context(decoder)?,
            value: dec_scalar(decoder)?,
        },
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "provider observation control",
                tag,
            });
        }
    })
}

fn dec_container(decoder: &mut Decoder<'_>) -> Result<ProviderContainer, CodecError> {
    match decoder.u8()? {
        0 => Ok(ProviderContainer::List),
        1 => Ok(ProviderContainer::Object),
        tag => Err(CodecError::InvalidTag {
            kind: "provider container",
            tag,
        }),
    }
}

fn dec_list_kind(decoder: &mut Decoder<'_>) -> Result<ListKind, CodecError> {
    Ok(match decoder.u8()? {
        0 => ListKind::HookFragments,
        1 => ListKind::MemoryCitationEntries,
        2 => ListKind::MemoryCitationThreadIds,
        3 => ListKind::ReasoningSummaries,
        4 => ListKind::CommandActions,
        5 => ListKind::FileChanges,
        6 => ListKind::McpResultContents,
        7 => ListKind::DynamicContentItems,
        8 => ListKind::CollabReceiverThreadIds,
        9 => ListKind::WebSearchActionQueries,
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "provider list kind",
                tag,
            });
        }
    })
}

fn dec_object_schema(decoder: &mut Decoder<'_>) -> Result<ObjectSchema, CodecError> {
    Ok(match decoder.u8()? {
        0 => ObjectSchema::HookFragment,
        1 => ObjectSchema::MemoryCitation,
        2 => ObjectSchema::MemoryCitationEntry,
        3 => ObjectSchema::CommandAction,
        4 => ObjectSchema::FileChange,
        5 => ObjectSchema::FileChangeKind,
        6 => ObjectSchema::McpAppContext,
        7 => ObjectSchema::McpResult,
        8 => ObjectSchema::McpError,
        9 => ObjectSchema::DynamicContent,
        10 => ObjectSchema::CollabAgentState,
        11 => ObjectSchema::WebSearchAction,
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "provider object schema",
                tag,
            });
        }
    })
}

include!("provider_observation/validator.rs");

pub(crate) fn enc_provider_observation_build_record(
    encoder: &mut Encoder,
    value: &ProviderObservationBuildRecord,
) {
    enc_provider_observation_id(encoder, value.identity());
    enc_provider_observation_begin(encoder, value.begin());
    encoder.u64(value.revision());
    encoder.u64(value.chunk_count());
    encoder.u64(value.canonical_bytes());
    encoder.fixed32(value.digest().as_bytes());
    enc_validator(encoder, value.validator());
    encoder.u8(match value.lifecycle() {
        ProviderObservationBuildLifecycle::Building => 0,
        ProviderObservationBuildLifecycle::Sealed => 1,
    });
}

pub(crate) fn dec_provider_observation_build_record(
    decoder: &mut Decoder<'_>,
) -> Result<ProviderObservationBuildRecord, CodecError> {
    let identity = dec_provider_observation_id(decoder)?;
    let begin = dec_provider_observation_begin(decoder)?;
    let revision = decoder.u64()?;
    let chunk_count = decoder.u64()?;
    let canonical_bytes = decoder.u64()?;
    let digest = ProviderObservationDigest::from_bytes(decoder.fixed32()?);
    let validator = dec_validator(decoder)?;
    let lifecycle = match decoder.u8()? {
        0 => ProviderObservationBuildLifecycle::Building,
        1 => ProviderObservationBuildLifecycle::Sealed,
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "provider-observation build lifecycle",
                tag,
            });
        }
    };
    ProviderObservationBuildRecord::from_stored_parts(
        identity,
        begin,
        revision,
        chunk_count,
        canonical_bytes,
        digest,
        validator,
        lifecycle,
    )
    .map_err(|source| invalid("provider-observation build", source))
}

pub(crate) fn enc_provider_observation_chunk_record(
    encoder: &mut Encoder,
    value: &ProviderObservationChunkRecord,
) {
    enc_provider_observation_id(encoder, value.identity());
    encoder.u64(value.ordinal());
    match value.payload() {
        ProviderObservationChunkPayload::Control(control) => {
            encoder.u8(0);
            enc_control(encoder, *control);
        }
        ProviderObservationChunkPayload::Fragment { context, bytes } => {
            encoder.u8(1);
            enc_context(encoder, *context);
            encoder.bytes(bytes);
        }
    }
}

pub(crate) fn dec_provider_observation_chunk_record(
    decoder: &mut Decoder<'_>,
) -> Result<ProviderObservationChunkRecord, CodecError> {
    let identity = dec_provider_observation_id(decoder)?;
    let ordinal = decoder.u64()?;
    match decoder.u8()? {
        0 => ProviderObservationChunkRecord::control(identity, ordinal, dec_control(decoder)?),
        1 => {
            let context = dec_context(decoder)?;
            let bytes = decoder.bytes("provider-observation fragment")?;
            ProviderObservationChunkRecord::fragment(identity, ordinal, context, bytes)
        }
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "provider-observation chunk payload",
                tag,
            });
        }
    }
    .map_err(|source| invalid("provider-observation chunk", source))
}
