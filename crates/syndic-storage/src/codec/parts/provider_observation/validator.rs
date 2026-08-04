fn enc_validator(encoder: &mut Encoder, state: &ProviderObservationValidatorState) {
    super::enc_opt(encoder, state.active_text, enc_context);
    encoder.u8(state.utf8.remaining);
    encoder.u32(state.utf8.codepoint);
    encoder.u32(state.utf8.minimum);
    super::enc_opt(encoder, state.active_identity, |encoder, identity| {
        encoder.u64(identity.bytes);
        encoder.u8(u8::from(identity.saw_scalar));
        encoder.u8(u8::from(identity.first_whitespace));
        encoder.u8(u8::from(identity.last_whitespace));
    });
    encoder.u32(u32::try_from(state.frames.len()).expect("validator frame depth is bounded"));
    for frame in &state.frames {
        match frame {
            ProviderObservationFrame::List {
                context,
                kind,
                next,
            } => {
                encoder.u8(0);
                enc_context(encoder, *context);
                encoder.u8(*kind as u8);
                encoder.u64(*next);
            }
            ProviderObservationFrame::Object {
                context,
                schema,
                seen,
                variant,
            } => {
                encoder.u8(1);
                enc_context(encoder, *context);
                encoder.u8(*schema as u8);
                encoder.u64(seen[0]);
                encoder.u64(seen[1]);
                super::enc_opt(encoder, *variant, |encoder, value| {
                    encoder.u8(value.tag());
                });
            }
            ProviderObservationFrame::Structured {
                context,
                container,
                next,
                depth,
            } => {
                encoder.u8(2);
                enc_context(encoder, *context);
                encoder.u8(*container as u8);
                encoder.u64(*next);
                encoder.u8(*depth);
            }
            ProviderObservationFrame::AgentStates { context, next } => {
                encoder.u8(3);
                enc_context(encoder, *context);
                encoder.u64(*next);
            }
            ProviderObservationFrame::Element {
                context,
                index,
                kind,
                started,
                complete,
            } => {
                encoder.u8(4);
                enc_context(encoder, *context);
                encoder.u64(*index);
                match kind {
                    ProviderObservationElementKind::Typed(kind) => {
                        encoder.u8(0);
                        encoder.u8(*kind as u8);
                    }
                    ProviderObservationElementKind::Structured { root, depth } => {
                        encoder.u8(1);
                        encoder.u8(root.tag());
                        encoder.u8(*depth);
                    }
                }
                encoder.u8(u8::from(*started));
                encoder.u8(u8::from(*complete));
            }
            ProviderObservationFrame::StructuredEntry {
                root,
                depth,
                entry,
                key_started,
                key_complete,
                value_started,
                value_complete,
            } => {
                encoder.u8(5);
                encoder.u8(root.tag());
                encoder.u8(*depth);
                encoder.u64(*entry);
                encoder.u8(u8::from(*key_started));
                encoder.u8(u8::from(*key_complete));
                encoder.u8(u8::from(*value_started));
                encoder.u8(u8::from(*value_complete));
            }
            ProviderObservationFrame::AgentStateEntry {
                entry,
                key_started,
                key_complete,
                seen,
            } => {
                encoder.u8(6);
                encoder.u64(*entry);
                encoder.u8(u8::from(*key_started));
                encoder.u8(u8::from(*key_complete));
                encoder.u64(seen[0]);
                encoder.u64(seen[1]);
            }
        }
    }
    encoder.u8(match state.history_support {
        ProviderFrameHistorySupportV1::Supported => 0,
        ProviderFrameHistorySupportV1::Unsupported(
            crate::UnsupportedHistoryReason::UnsupportedRequiredPayload,
        ) => 1,
        ProviderFrameHistorySupportV1::Unsupported(_) => {
            unreachable!("provider observation staging has one exact unsupported reason")
        }
    });
    encoder.u64(state.token_count);
    encoder.u64(state.text_bytes);
    encoder.u64(state.seen_fields[0]);
    encoder.u64(state.seen_fields[1]);
    super::enc_opt(encoder, state.item_status, |encoder, value| {
        encoder.u8(value.tag());
    });
}

fn dec_validator(
    decoder: &mut Decoder<'_>,
) -> Result<ProviderObservationValidatorState, CodecError> {
    let active_text = super::dec_opt(decoder, "provider active text", dec_context)?;
    let utf8 = Utf8ValidatorState {
        remaining: decoder.u8()?,
        codepoint: decoder.u32()?,
        minimum: decoder.u32()?,
    };
    if utf8.remaining > 3 || (active_text.is_none() && utf8.remaining != 0) {
        return Err(CodecError::InvalidLength("provider UTF-8 state"));
    }
    let active_identity = super::dec_opt(decoder, "provider identity state", |decoder| {
        Ok(ProviderIdentityValidatorState {
            bytes: decoder.u64()?,
            saw_scalar: dec_bool(decoder, "provider identity saw scalar")?,
            first_whitespace: dec_bool(decoder, "provider identity first whitespace")?,
            last_whitespace: dec_bool(decoder, "provider identity last whitespace")?,
        })
    })?;
    if active_identity.is_some() && active_text.is_none() {
        return Err(CodecError::InvalidLength("provider identity state"));
    }
    if active_identity.is_some_and(|identity| {
        identity.bytes > ProviderIdentityValidatorState::MAX_BYTES
            || (!identity.saw_scalar && (identity.first_whitespace || identity.last_whitespace))
    }) {
        return Err(CodecError::InvalidLength("provider identity state"));
    }
    let count = usize::try_from(decoder.u32()?)
        .map_err(|_| CodecError::InvalidLength("provider validator frames"))?;
    if count > PROVIDER_OBSERVATION_MAX_FRAME_DEPTH {
        return Err(CodecError::InvalidLength("provider validator frames"));
    }
    let mut frames = Vec::with_capacity(count);
    for _ in 0..count {
        frames.push(match decoder.u8()? {
            0 => ProviderObservationFrame::List {
                context: dec_context(decoder)?,
                kind: dec_list_kind(decoder)?,
                next: decoder.u64()?,
            },
            1 => ProviderObservationFrame::Object {
                context: dec_context(decoder)?,
                schema: dec_object_schema(decoder)?,
                seen: [decoder.u64()?, decoder.u64()?],
                variant: super::dec_opt(decoder, "provider object variant", |decoder| {
                    let tag = decoder.u8()?;
                    ProviderEnumValue::from_tag(tag).ok_or(CodecError::InvalidTag {
                        kind: "provider enum",
                        tag,
                    })
                })?,
            },
            2 => ProviderObservationFrame::Structured {
                context: dec_context(decoder)?,
                container: dec_container(decoder)?,
                next: decoder.u64()?,
                depth: decoder.u8()?,
            },
            3 => ProviderObservationFrame::AgentStates {
                context: dec_context(decoder)?,
                next: decoder.u64()?,
            },
            4 => ProviderObservationFrame::Element {
                context: dec_context(decoder)?,
                index: decoder.u64()?,
                kind: match decoder.u8()? {
                    0 => ProviderObservationElementKind::Typed(dec_list_kind(decoder)?),
                    1 => ProviderObservationElementKind::Structured {
                        root: dec_field(decoder)?,
                        depth: decoder.u8()?,
                    },
                    tag => {
                        return Err(CodecError::InvalidTag {
                            kind: "provider element kind",
                            tag,
                        });
                    }
                },
                started: dec_bool(decoder, "provider element started")?,
                complete: dec_bool(decoder, "provider element complete")?,
            },
            5 => ProviderObservationFrame::StructuredEntry {
                root: dec_field(decoder)?,
                depth: decoder.u8()?,
                entry: decoder.u64()?,
                key_started: dec_bool(decoder, "provider structured key started")?,
                key_complete: dec_bool(decoder, "provider structured key complete")?,
                value_started: dec_bool(decoder, "provider structured value started")?,
                value_complete: dec_bool(decoder, "provider structured value complete")?,
            },
            6 => ProviderObservationFrame::AgentStateEntry {
                entry: decoder.u64()?,
                key_started: dec_bool(decoder, "provider agent-state key started")?,
                key_complete: dec_bool(decoder, "provider agent-state key complete")?,
                seen: [decoder.u64()?, decoder.u64()?],
            },
            tag => {
                return Err(CodecError::InvalidTag {
                    kind: "provider validator frame",
                    tag,
                });
            }
        });
    }
    let history_support = match decoder.u8()? {
        0 => ProviderFrameHistorySupportV1::Supported,
        1 => ProviderFrameHistorySupportV1::Unsupported(
            crate::UnsupportedHistoryReason::UnsupportedRequiredPayload,
        ),
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "provider observation history support",
                tag,
            });
        }
    };
    let token_count = decoder.u64()?;
    let text_bytes = decoder.u64()?;
    let seen_fields = [decoder.u64()?, decoder.u64()?];
    let item_status = super::dec_opt(decoder, "provider item status", |decoder| {
        let tag = decoder.u8()?;
        ProviderEnumValue::from_tag(tag).ok_or(CodecError::InvalidTag {
            kind: "provider enum",
            tag,
        })
    })?;
    Ok(ProviderObservationValidatorState {
        active_text,
        active_identity,
        utf8,
        frames,
        token_count,
        text_bytes,
        seen_fields,
        item_status,
        history_support,
    })
}
