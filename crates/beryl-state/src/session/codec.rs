use std::io;

use beryl_home_store::{RecordCodec, RecordVersion};
use beryl_model::{ClaimRevision, RootId, RuntimeId, SessionRevision, SyndicThreadId, WindowId};

use crate::{
    encoding::{CodecError, Decoder, Encoder},
    RecordRevision,
};

use super::{
    RememberedTarget, SessionDomain, SessionExitIntent, SessionHeader, SessionWindowRecord,
    SessionWindowReference, ThreadClaimRecord, ThreadClaimState, WindowClaimSelection,
    CLAIM_V1_BYTES, MAX_RESTORABLE_WINDOWS, SESSION_HEADER_V1_BYTES, SESSION_WINDOW_V1_BYTES,
};

mod placement;

use placement::{decode_placement, encode_placement};

pub(super) struct SessionHeaderCodec;
pub(super) struct SessionWindowCodec;
pub(super) struct ClaimByWindowCodec;
pub(super) struct ClaimByThreadCodec;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct HeaderKey;

pub(super) const HEADER_KEY: HeaderKey = HeaderKey;

impl RecordCodec<SessionDomain> for SessionHeaderCodec {
    type Key = HeaderKey;
    type Value = SessionHeader;
    type Error = CodecError;

    const FAMILY: &'static str = "active-header";
    const VERSION: RecordVersion = RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 1;
    const MAX_VALUE_BYTES: usize = SESSION_HEADER_V1_BYTES;

    fn encode_key(_key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
        Ok(vec![0])
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, Self::Error> {
        if encoded == [0] {
            Ok(HeaderKey)
        } else {
            Err(CodecError::InvalidLength {
                kind: "session header key",
            })
        }
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, Self::Error> {
        if value.windows.len() > MAX_RESTORABLE_WINDOWS
            || value
                .windows
                .windows(2)
                .any(|pair| pair[0].window_id >= pair[1].window_id)
        {
            return Err(invariant(
                "session header window references are not canonical",
            ));
        }
        let mut encoder = Encoder::new();
        encoder.u64(value.revision.get());
        encoder.u8(match value.exit_intent {
            SessionExitIntent::Running => 0,
            SessionExitIntent::OrderlyExit => 1,
        });
        encode_target(&mut encoder, value.fallback);
        encoder.u16(value.windows.len() as u16);
        for reference in &value.windows {
            encoder.fixed(reference.window_id.as_bytes());
            encoder.u64(reference.record_revision.get());
        }
        for _ in value.windows.len()..MAX_RESTORABLE_WINDOWS {
            encoder.fixed(&[0; 16]);
            encoder.u64(0);
        }
        let encoded = encoder.finish();
        debug_assert_eq!(encoded.len(), SESSION_HEADER_V1_BYTES);
        Ok(encoded)
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, Self::Error> {
        let mut decoder = Decoder::new(encoded);
        let revision = session_revision(&mut decoder)?;
        let exit_intent = match decoder.u8()? {
            0 => SessionExitIntent::Running,
            1 => SessionExitIntent::OrderlyExit,
            tag => {
                return Err(CodecError::InvalidTag {
                    kind: "session exit intent",
                    tag,
                });
            }
        };
        let fallback = decode_target(&mut decoder)?;
        let count = usize::from(decoder.u16()?);
        if count > MAX_RESTORABLE_WINDOWS {
            return Err(invariant("session header window count exceeds 256"));
        }
        let mut windows = Vec::with_capacity(count);
        for index in 0..MAX_RESTORABLE_WINDOWS {
            let identity = decoder.fixed()?;
            let raw_revision = decoder.u64()?;
            if index < count {
                let record_revision = RecordRevision::new(raw_revision)
                    .map_err(|source| invalid("window record revision", source))?;
                windows.push(SessionWindowReference::new(
                    WindowId::from_bytes(identity),
                    record_revision,
                ));
            } else if identity != [0; 16] || raw_revision != 0 {
                return Err(invariant(
                    "session header has nonzero unused window capacity",
                ));
            }
        }
        decoder.finish()?;
        if windows
            .windows(2)
            .any(|pair| pair[0].window_id >= pair[1].window_id)
        {
            return Err(invariant(
                "session header window references are not sorted and unique",
            ));
        }
        Ok(SessionHeader {
            revision,
            exit_intent,
            fallback,
            windows,
        })
    }
}

impl RecordCodec<SessionDomain> for SessionWindowCodec {
    type Key = WindowId;
    type Value = SessionWindowRecord;
    type Error = CodecError;

    const FAMILY: &'static str = "windows";
    const VERSION: RecordVersion = RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 16;
    const MAX_VALUE_BYTES: usize = SESSION_WINDOW_V1_BYTES;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
        Ok(key.as_bytes().to_vec())
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, Self::Error> {
        identity(encoded, "window identity").map(WindowId::from_bytes)
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, Self::Error> {
        let mut encoder = Encoder::new();
        encoder.fixed(value.window_id.as_bytes());
        encode_target(&mut encoder, value.remembered_target);
        encode_selection(&mut encoder, value.selected_thread);
        encode_placement(&mut encoder, &value.placement);
        encoder.u64(value.revision.get());
        let encoded = encoder.finish();
        debug_assert_eq!(encoded.len(), SESSION_WINDOW_V1_BYTES);
        Ok(encoded)
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, Self::Error> {
        let mut decoder = Decoder::new(encoded);
        let window_id = WindowId::from_bytes(decoder.fixed()?);
        let remembered_target = decode_target(&mut decoder)?;
        let selected_thread = decode_selection(&mut decoder)?;
        let placement = decode_placement(&mut decoder)?;
        let revision = decoder.record_revision()?;
        decoder.finish()?;
        Ok(SessionWindowRecord {
            window_id,
            remembered_target,
            selected_thread,
            placement,
            revision,
        })
    }
}

macro_rules! claim_codec {
    ($codec:ident, $key:ty, $family:literal, $kind:literal) => {
        impl RecordCodec<SessionDomain> for $codec {
            type Key = $key;
            type Value = ThreadClaimRecord;
            type Error = CodecError;

            const FAMILY: &'static str = $family;
            const VERSION: RecordVersion = RecordVersion::new(1);
            const MAX_KEY_BYTES: usize = 16;
            const MAX_VALUE_BYTES: usize = CLAIM_V1_BYTES;

            fn encode_key(key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
                Ok(key.as_bytes().to_vec())
            }

            fn decode_key(encoded: &[u8]) -> Result<Self::Key, Self::Error> {
                identity(encoded, $kind).map(<$key>::from_bytes)
            }

            fn encode_value(value: &Self::Value) -> Result<Vec<u8>, Self::Error> {
                encode_claim(*value)
            }

            fn decode_value(encoded: &[u8]) -> Result<Self::Value, Self::Error> {
                decode_claim(encoded)
            }
        }
    };
}

claim_codec!(
    ClaimByWindowCodec,
    WindowId,
    "claims-by-window",
    "claim window identity"
);
claim_codec!(
    ClaimByThreadCodec,
    SyndicThreadId,
    "claims-by-thread",
    "claim thread identity"
);

fn encode_claim(value: ThreadClaimRecord) -> Result<Vec<u8>, CodecError> {
    let mut encoder = Encoder::new();
    encoder.fixed(value.window_id.as_bytes());
    encoder.fixed(value.thread_id.as_bytes());
    encoder.u64(value.generation.get());
    encoder.u8(match value.state {
        ThreadClaimState::Active => 0,
        ThreadClaimState::Restoring => 1,
    });
    encoder.u64(value.revision.get());
    let encoded = encoder.finish();
    debug_assert_eq!(encoded.len(), CLAIM_V1_BYTES);
    Ok(encoded)
}

fn decode_claim(encoded: &[u8]) -> Result<ThreadClaimRecord, CodecError> {
    let mut decoder = Decoder::new(encoded);
    let window_id = WindowId::from_bytes(decoder.fixed()?);
    let thread_id = SyndicThreadId::from_bytes(decoder.fixed()?);
    let generation = session_revision(&mut decoder)?;
    let state = match decoder.u8()? {
        0 => ThreadClaimState::Active,
        1 => ThreadClaimState::Restoring,
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "thread claim state",
                tag,
            });
        }
    };
    let revision =
        ClaimRevision::new(decoder.u64()?).map_err(|source| invalid("claim revision", source))?;
    decoder.finish()?;
    Ok(ThreadClaimRecord::new(
        window_id, thread_id, generation, state, revision,
    ))
}

fn encode_target(encoder: &mut Encoder, value: Option<RememberedTarget>) {
    match value {
        Some(value) => {
            encoder.u8(1);
            encoder.fixed(value.runtime_id.as_bytes());
            encoder.fixed(value.root_id.as_bytes());
        }
        None => {
            encoder.u8(0);
            encoder.padded(&[], 32);
        }
    }
}

fn decode_target(decoder: &mut Decoder<'_>) -> Result<Option<RememberedTarget>, CodecError> {
    let tag = decoder.u8()?;
    let runtime = decoder.fixed()?;
    let root = decoder.fixed()?;
    match tag {
        0 if runtime == [0; 16] && root == [0; 16] => Ok(None),
        0 => Err(invariant("absent remembered target has nonzero padding")),
        1 => Ok(Some(RememberedTarget::new(
            RuntimeId::from_bytes(runtime),
            RootId::from_bytes(root),
        ))),
        tag => Err(CodecError::InvalidTag {
            kind: "optional remembered target",
            tag,
        }),
    }
}

fn encode_selection(encoder: &mut Encoder, value: Option<WindowClaimSelection>) {
    match value {
        Some(value) => {
            encoder.u8(1);
            encoder.fixed(value.thread_id.as_bytes());
            encoder.u64(value.generation.get());
            encoder.u64(value.revision.get());
        }
        None => {
            encoder.u8(0);
            encoder.padded(&[], 32);
        }
    }
}

fn decode_selection(decoder: &mut Decoder<'_>) -> Result<Option<WindowClaimSelection>, CodecError> {
    let tag = decoder.u8()?;
    let thread = decoder.fixed()?;
    let raw_generation = decoder.u64()?;
    let raw_revision = decoder.u64()?;
    match tag {
        0 if thread == [0; 16] && raw_generation == 0 && raw_revision == 0 => Ok(None),
        0 => Err(invariant("absent selected thread has nonzero padding")),
        1 => Ok(Some(WindowClaimSelection::new(
            SyndicThreadId::from_bytes(thread),
            SessionRevision::new(raw_generation)
                .map_err(|source| invalid("claim generation", source))?,
            ClaimRevision::new(raw_revision).map_err(|source| invalid("claim revision", source))?,
        ))),
        tag => Err(CodecError::InvalidTag {
            kind: "optional selected thread",
            tag,
        }),
    }
}

fn session_revision(decoder: &mut Decoder<'_>) -> Result<SessionRevision, CodecError> {
    SessionRevision::new(decoder.u64()?).map_err(|source| invalid("session revision", source))
}

fn identity(encoded: &[u8], kind: &'static str) -> Result<[u8; 16], CodecError> {
    encoded
        .try_into()
        .map_err(|_| CodecError::InvalidLength { kind })
}

fn invariant(message: &'static str) -> CodecError {
    invalid(
        "session record",
        io::Error::new(io::ErrorKind::InvalidData, message),
    )
}

fn invalid(
    kind: &'static str,
    source: impl std::error::Error + Send + Sync + 'static,
) -> CodecError {
    CodecError::InvalidValue {
        kind,
        source: Box::new(source),
    }
}
