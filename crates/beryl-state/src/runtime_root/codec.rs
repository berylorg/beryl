use beryl_home_store::{RecordCodec, RecordVersion};
use beryl_model::{AdmittedHostPath, RootId, RuntimeId, RuntimeNativePath};

use crate::encoding::{
    CodecError, Decoder, Encoder, decode_root_id, decode_runtime_id, encode_root_id,
    encode_runtime_id,
};

use super::{ROOT_RECORD_LIMIT, RootRecord, RuntimeRecord, RuntimeRootDomain};

pub(super) struct RuntimeRecordCodec;
pub(super) struct ExecutableIndexCodec;
pub(super) struct RootRecordCodec;
pub(super) struct RootIdIndexCodec;
pub(super) struct RootPathIndexCodec;
pub(super) struct RuntimeHomeRootIndexCodec;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ExecutableKey {
    Lower,
    Value(AdmittedHostPath),
    Upper,
}

impl ExecutableKey {
    pub(super) const fn new(path: AdmittedHostPath) -> Self {
        Self::Value(path)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct RuntimeRootKey {
    runtime_id: RuntimeId,
    root_id: RootId,
}

impl RuntimeRootKey {
    pub(super) const fn new(runtime_id: RuntimeId, root_id: RootId) -> Self {
        Self {
            runtime_id,
            root_id,
        }
    }

    pub(super) const fn runtime_id(self) -> RuntimeId {
        self.runtime_id
    }

    pub(super) const fn root_id(self) -> RootId {
        self.root_id
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum RootPathKey {
    Lower,
    Value {
        runtime_id: RuntimeId,
        path: RuntimeNativePath,
    },
    Upper,
}

impl RootPathKey {
    pub(super) const fn new(runtime_id: RuntimeId, path: RuntimeNativePath) -> Self {
        Self::Value { runtime_id, path }
    }
}

impl RecordCodec<RuntimeRootDomain> for RuntimeRecordCodec {
    type Key = RuntimeId;
    type Value = RuntimeRecord;
    type Error = CodecError;

    const FAMILY: &'static str = "runtimes";
    const VERSION: RecordVersion = RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 16;
    const MAX_VALUE_BYTES: usize = super::RUNTIME_RECORD_LIMIT;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
        Ok(encode_runtime_id(*key))
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, Self::Error> {
        decode_runtime_id(encoded)
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, Self::Error> {
        let mut encoder = Encoder::new();
        encoder.fixed(value.runtime_id.as_bytes());
        encoder.host_path(&value.canonical_executable);
        encoder.runtime_mode(&value.mode);
        encoder.runtime_path(&value.runtime_native_executable);
        encoder.text(&value.environment_label);
        encoder.u64(value.created_at.get());
        encoder.availability(value.availability);
        encoder.u64(value.revision.get());
        Ok(encoder.finish())
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, Self::Error> {
        let mut decoder = Decoder::new(encoded);
        let runtime_id = RuntimeId::from_bytes(decoder.fixed()?);
        let canonical_executable = decoder.host_path()?;
        let mode = decoder.runtime_mode()?;
        let runtime_native_executable = decoder.runtime_path()?;
        let environment_label: Box<str> = decoder.text("runtime environment label")?.into();
        let created_at = crate::UnixMillis::new(decoder.u64()?);
        let availability = decoder.availability()?;
        let revision = decoder.record_revision()?;
        decoder.finish()?;
        if runtime_native_executable.mode() != &mode {
            return Err(invariant("runtime executable mode mismatch"));
        }
        let expected_label = match &mode {
            beryl_model::RuntimeMode::Host => "Host",
            beryl_model::RuntimeMode::Wsl(distribution) => distribution.as_str(),
        };
        if environment_label.as_ref() != expected_label {
            return Err(invariant("runtime environment label mismatch"));
        }
        Ok(RuntimeRecord {
            runtime_id,
            canonical_executable,
            mode,
            runtime_native_executable,
            environment_label,
            created_at,
            availability,
            revision,
        })
    }
}

impl RecordCodec<RuntimeRootDomain> for ExecutableIndexCodec {
    type Key = ExecutableKey;
    type Value = RuntimeId;
    type Error = CodecError;

    const FAMILY: &'static str = "runtime-executable-index";
    const VERSION: RecordVersion = RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = u16::MAX as usize;
    const MAX_VALUE_BYTES: usize = 16;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
        let mut encoder = Encoder::new();
        match key {
            ExecutableKey::Lower => encoder.u8(0),
            ExecutableKey::Value(path) => {
                encoder.u8(1);
                encoder.host_path(path);
            }
            ExecutableKey::Upper => encoder.u8(u8::MAX),
        }
        Ok(encoder.finish())
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, Self::Error> {
        let mut decoder = Decoder::new(encoded);
        let key = match decoder.u8()? {
            0 => ExecutableKey::Lower,
            1 => ExecutableKey::new(decoder.host_path()?),
            u8::MAX => ExecutableKey::Upper,
            tag => {
                return Err(CodecError::InvalidTag {
                    kind: "executable index key",
                    tag,
                });
            }
        };
        decoder.finish()?;
        Ok(key)
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, Self::Error> {
        Ok(encode_runtime_id(*value))
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, Self::Error> {
        decode_runtime_id(encoded)
    }
}

impl RecordCodec<RuntimeRootDomain> for RootRecordCodec {
    type Key = RuntimeRootKey;
    type Value = RootRecord;
    type Error = CodecError;

    const FAMILY: &'static str = "roots";
    const VERSION: RecordVersion = RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 32;
    const MAX_VALUE_BYTES: usize = ROOT_RECORD_LIMIT;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
        let mut encoded = Vec::with_capacity(32);
        encoded.extend_from_slice(key.runtime_id.as_bytes());
        encoded.extend_from_slice(key.root_id.as_bytes());
        Ok(encoded)
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, Self::Error> {
        if encoded.len() != 32 {
            return Err(CodecError::InvalidLength {
                kind: "runtime/root key",
            });
        }
        Ok(Self::Key::new(
            decode_runtime_id(&encoded[..16])?,
            decode_root_id(&encoded[16..])?,
        ))
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, Self::Error> {
        let mut encoder = Encoder::new();
        encoder.fixed(value.root_id.as_bytes());
        encoder.fixed(value.runtime_id.as_bytes());
        encoder.runtime_path(&value.canonical_path);
        encoder.host_path(&value.display_path);
        encoder.u8(u8::from(value.non_removable));
        encoder.u64(value.created_at.get());
        encoder.availability(value.availability);
        encoder.optional_time(value.last_activity_at);
        encoder.u64(value.revision.get());
        Ok(encoder.finish())
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, Self::Error> {
        let mut decoder = Decoder::new(encoded);
        let root_id = RootId::from_bytes(decoder.fixed()?);
        let runtime_id = RuntimeId::from_bytes(decoder.fixed()?);
        let canonical_path = decoder.runtime_path()?;
        let display_path = decoder.host_path()?;
        let non_removable = match decoder.u8()? {
            0 => false,
            1 => true,
            tag => {
                return Err(CodecError::InvalidTag {
                    kind: "non-removable root",
                    tag,
                });
            }
        };
        let created_at = crate::UnixMillis::new(decoder.u64()?);
        let availability = decoder.availability()?;
        let last_activity_at = decoder.optional_time()?;
        let revision = decoder.record_revision()?;
        decoder.finish()?;
        Ok(RootRecord {
            root_id,
            runtime_id,
            canonical_path,
            display_path,
            non_removable,
            created_at,
            availability,
            last_activity_at,
            revision,
        })
    }
}

impl RecordCodec<RuntimeRootDomain> for RootIdIndexCodec {
    type Key = RootId;
    type Value = RuntimeId;
    type Error = CodecError;

    const FAMILY: &'static str = "root-id-index";
    const VERSION: RecordVersion = RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 16;
    const MAX_VALUE_BYTES: usize = 16;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
        Ok(encode_root_id(*key))
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, Self::Error> {
        decode_root_id(encoded)
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, Self::Error> {
        Ok(encode_runtime_id(*value))
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, Self::Error> {
        decode_runtime_id(encoded)
    }
}

impl RecordCodec<RuntimeRootDomain> for RootPathIndexCodec {
    type Key = RootPathKey;
    type Value = RootId;
    type Error = CodecError;

    const FAMILY: &'static str = "root-path-index";
    const VERSION: RecordVersion = RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = u16::MAX as usize;
    const MAX_VALUE_BYTES: usize = 16;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
        let mut encoder = Encoder::new();
        match key {
            RootPathKey::Lower => encoder.u8(0),
            RootPathKey::Value { runtime_id, path } => {
                encoder.u8(1);
                encoder.fixed(runtime_id.as_bytes());
                encoder.runtime_path(path);
            }
            RootPathKey::Upper => encoder.u8(u8::MAX),
        }
        Ok(encoder.finish())
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, Self::Error> {
        let mut decoder = Decoder::new(encoded);
        let key = match decoder.u8()? {
            0 => RootPathKey::Lower,
            1 => {
                let runtime_id = RuntimeId::from_bytes(decoder.fixed()?);
                let path = decoder.runtime_path()?;
                Self::Key::new(runtime_id, path)
            }
            u8::MAX => RootPathKey::Upper,
            tag => {
                return Err(CodecError::InvalidTag {
                    kind: "root path index key",
                    tag,
                });
            }
        };
        decoder.finish()?;
        Ok(key)
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, Self::Error> {
        Ok(encode_root_id(*value))
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, Self::Error> {
        decode_root_id(encoded)
    }
}

impl RecordCodec<RuntimeRootDomain> for RuntimeHomeRootIndexCodec {
    type Key = RuntimeId;
    type Value = RootId;
    type Error = CodecError;

    const FAMILY: &'static str = "runtime-home-root-index";
    const VERSION: RecordVersion = RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 16;
    const MAX_VALUE_BYTES: usize = 16;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
        Ok(encode_runtime_id(*key))
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, Self::Error> {
        decode_runtime_id(encoded)
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, Self::Error> {
        Ok(encode_root_id(*value))
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, Self::Error> {
        decode_root_id(encoded)
    }
}

fn invariant(message: &'static str) -> CodecError {
    CodecError::InvalidValue {
        kind: "runtime/root record",
        source: Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            message,
        )),
    }
}
