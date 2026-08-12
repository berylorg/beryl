use std::{error::Error, fmt};

use beryl_home_store::{RecordCodec, RecordVersion};
use beryl_model::{AdmittedHostPath, PathFlavor};

use crate::RecordRevision;

use super::{
    SETTINGS_RECORD_LIMIT, SettingKey, SettingRecord, SettingSchemaVersion, SettingValue,
    SettingsDomain, value::SettingValueKind,
};

pub(super) struct SettingRecordCodec;

impl RecordCodec<SettingsDomain> for SettingRecordCodec {
    type Key = SettingKey;
    type Value = SettingRecord;
    type Error = SettingsCodecError;

    const FAMILY: &'static str = "records";
    const VERSION: RecordVersion = RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 1;
    const MAX_VALUE_BYTES: usize = SETTINGS_RECORD_LIMIT;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
        Ok(vec![encode_key_tag(*key)])
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, Self::Error> {
        let [tag] = encoded else {
            return Err(SettingsCodecError::InvalidLength {
                kind: "setting key",
            });
        };
        decode_key_tag(*tag)
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, Self::Error> {
        if value.schema_version != value.key.schema_version() {
            return Err(SettingsCodecError::UnsupportedSettingSchema {
                key: value.key,
                supported: value.key.schema_version(),
                found: value.schema_version.get(),
            });
        }
        if value.key != value.value.key() {
            return Err(SettingsCodecError::KeyValueMismatch {
                key: value.key,
                value_key: value.value.key(),
            });
        }

        let mut encoder = Encoder::new();
        encoder.u8(encode_key_tag(value.key));
        encoder.u32(value.schema_version.get());
        encode_setting_value(&mut encoder, &value.value);
        encoder.u64(value.revision.get());
        Ok(encoder.finish())
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, Self::Error> {
        let mut decoder = Decoder::new(encoded);
        let key = decode_key_tag(decoder.u8()?)?;
        let schema = decoder.u32()?;
        let supported = key.schema_version();
        if schema != supported.get() {
            return Err(SettingsCodecError::UnsupportedSettingSchema {
                key,
                supported,
                found: schema,
            });
        }
        let value = decode_setting_value(&mut decoder)?;
        if key != value.key() {
            return Err(SettingsCodecError::KeyValueMismatch {
                key,
                value_key: value.key(),
            });
        }
        let revision = RecordRevision::new(decoder.u64()?)
            .map_err(|source| SettingsCodecError::invalid("record revision", source))?;
        decoder.finish()?;
        Ok(SettingRecord {
            key,
            schema_version: supported,
            value,
            revision,
        })
    }
}

fn encode_setting_value(encoder: &mut Encoder, value: &SettingValue) {
    encoder.u8(encode_key_tag(value.key()));
    match &value.kind {
        SettingValueKind::ActiveThemeId(value) | SettingValueKind::DeveloperInstructions(value) => {
            encoder.text(value)
        }
        SettingValueKind::ContextCompactionTimeoutMillis(value)
        | SettingValueKind::DraftAutosaveIntervalSeconds(value) => encoder.u64(*value),
        SettingValueKind::EndTurnSound(path) => match path {
            Some(path) => {
                encoder.u8(1);
                encoder.path_flavor(path.flavor());
                encoder.text(path.as_str());
            }
            None => encoder.u8(0),
        },
    }
}

fn decode_setting_value(decoder: &mut Decoder<'_>) -> Result<SettingValue, SettingsCodecError> {
    let key = decode_key_tag(decoder.u8()?)?;
    let kind = match key {
        SettingKey::ActiveThemeId => {
            SettingValue::active_theme_id(decoder.text("active theme id")?)
                .map_err(|source| SettingsCodecError::invalid("active theme id", source))?
                .kind
        }
        SettingKey::ContextCompactionTimeout => {
            SettingValueKind::ContextCompactionTimeoutMillis(decoder.u64()?)
        }
        SettingKey::DraftAutosaveInterval => {
            SettingValueKind::DraftAutosaveIntervalSeconds(decoder.u64()?)
        }
        SettingKey::DeveloperInstructions => {
            SettingValue::developer_instructions(decoder.text("developer instructions")?)
                .map_err(|source| SettingsCodecError::invalid("developer instructions", source))?
                .kind
        }
        SettingKey::EndTurnSound => {
            let path = match decoder.u8()? {
                0 => None,
                1 => {
                    let flavor = decoder.path_flavor()?;
                    let value = decoder.text("end-turn sound path")?;
                    Some(
                        AdmittedHostPath::from_admitted(flavor, value)
                            .map_err(|source| SettingsCodecError::invalid("sound path", source))?,
                    )
                }
                tag => {
                    return Err(SettingsCodecError::InvalidTag {
                        kind: "end-turn sound option",
                        tag,
                    });
                }
            };
            SettingValueKind::EndTurnSound(path)
        }
    };
    Ok(SettingValue::from_kind(kind))
}

const fn encode_key_tag(key: SettingKey) -> u8 {
    match key {
        SettingKey::ActiveThemeId => 0,
        SettingKey::ContextCompactionTimeout => 1,
        SettingKey::DraftAutosaveInterval => 2,
        SettingKey::DeveloperInstructions => 3,
        SettingKey::EndTurnSound => u8::MAX,
    }
}

fn decode_key_tag(tag: u8) -> Result<SettingKey, SettingsCodecError> {
    match tag {
        0 => Ok(SettingKey::ActiveThemeId),
        1 => Ok(SettingKey::ContextCompactionTimeout),
        2 => Ok(SettingKey::DraftAutosaveInterval),
        3 => Ok(SettingKey::DeveloperInstructions),
        u8::MAX => Ok(SettingKey::EndTurnSound),
        tag => Err(SettingsCodecError::InvalidTag {
            kind: "setting key",
            tag,
        }),
    }
}

struct Encoder {
    bytes: Vec<u8>,
}

impl Encoder {
    fn new() -> Self {
        Self { bytes: Vec::new() }
    }

    fn finish(self) -> Vec<u8> {
        self.bytes
    }

    fn u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    fn u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    fn text(&mut self, value: &str) {
        let length = u32::try_from(value.len()).expect("bounded setting text fits u32");
        self.u32(length);
        self.bytes.extend_from_slice(value.as_bytes());
    }

    fn path_flavor(&mut self, flavor: PathFlavor) {
        self.u8(match flavor {
            PathFlavor::Windows => 0,
            PathFlavor::Posix => 1,
        });
    }
}

struct Decoder<'a> {
    remaining: &'a [u8],
}

impl<'a> Decoder<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { remaining: bytes }
    }

    fn finish(self) -> Result<(), SettingsCodecError> {
        if self.remaining.is_empty() {
            Ok(())
        } else {
            Err(SettingsCodecError::TrailingBytes)
        }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], SettingsCodecError> {
        if self.remaining.len() < length {
            return Err(SettingsCodecError::Truncated);
        }
        let (value, remaining) = self.remaining.split_at(length);
        self.remaining = remaining;
        Ok(value)
    }

    fn u8(&mut self) -> Result<u8, SettingsCodecError> {
        Ok(self.take(1)?[0])
    }

    fn u32(&mut self) -> Result<u32, SettingsCodecError> {
        let bytes = self
            .take(4)?
            .try_into()
            .map_err(|_| SettingsCodecError::Truncated)?;
        Ok(u32::from_be_bytes(bytes))
    }

    fn u64(&mut self) -> Result<u64, SettingsCodecError> {
        let bytes = self
            .take(8)?
            .try_into()
            .map_err(|_| SettingsCodecError::Truncated)?;
        Ok(u64::from_be_bytes(bytes))
    }

    fn text(&mut self, kind: &'static str) -> Result<&'a str, SettingsCodecError> {
        let length = self.u32()? as usize;
        std::str::from_utf8(self.take(length)?)
            .map_err(|_| SettingsCodecError::InvalidUtf8 { kind })
    }

    fn path_flavor(&mut self) -> Result<PathFlavor, SettingsCodecError> {
        match self.u8()? {
            0 => Ok(PathFlavor::Windows),
            1 => Ok(PathFlavor::Posix),
            tag => Err(SettingsCodecError::InvalidTag {
                kind: "path flavor",
                tag,
            }),
        }
    }
}

#[derive(Debug)]
pub(super) enum SettingsCodecError {
    Truncated,
    TrailingBytes,
    InvalidLength {
        kind: &'static str,
    },
    InvalidTag {
        kind: &'static str,
        tag: u8,
    },
    InvalidUtf8 {
        kind: &'static str,
    },
    UnsupportedSettingSchema {
        key: SettingKey,
        supported: SettingSchemaVersion,
        found: u32,
    },
    KeyValueMismatch {
        key: SettingKey,
        value_key: SettingKey,
    },
    InvalidValue {
        kind: &'static str,
        source: Box<dyn Error + Send + Sync>,
    },
}

impl SettingsCodecError {
    fn invalid(
        kind: &'static str,
        source: impl Error + Send + Sync + 'static,
    ) -> SettingsCodecError {
        Self::InvalidValue {
            kind,
            source: Box::new(source),
        }
    }
}

impl fmt::Display for SettingsCodecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated => formatter.write_str("setting record payload is truncated"),
            Self::TrailingBytes => formatter.write_str("setting record payload has trailing bytes"),
            Self::InvalidLength { kind } => write!(formatter, "invalid {kind} byte length"),
            Self::InvalidTag { kind, tag } => write!(formatter, "invalid {kind} tag {tag}"),
            Self::InvalidUtf8 { kind } => write!(formatter, "{kind} is not valid UTF-8"),
            Self::UnsupportedSettingSchema {
                key,
                supported,
                found,
            } => write!(
                formatter,
                "setting `{}` uses schema {found}, but this codec accepts {}",
                key.stable_id(),
                supported.get()
            ),
            Self::KeyValueMismatch { key, value_key } => write!(
                formatter,
                "setting record key `{}` carries scalar `{}`",
                key.stable_id(),
                value_key.stable_id()
            ),
            Self::InvalidValue { kind, source } => {
                write!(formatter, "invalid {kind}: {source}")
            }
        }
    }
}

impl Error for SettingsCodecError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidValue { source, .. } => Some(source.as_ref()),
            _ => None,
        }
    }
}
