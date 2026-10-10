use beryl_home_store::RecordVersion;
use sha2::{Digest, Sha256};

use crate::codec::{CodecError, ExactCodec, Family, ResourcesFamily, parts::*};
use crate::{ResourceBacking, ResourceStructure};

use super::*;

mod control;
mod index;
mod selection;

pub(crate) use control::*;
pub(crate) use index::*;
pub(crate) use selection::*;

pub(super) fn malformed() -> CodecError {
    CodecError::InvalidLength("resource structure")
}

pub(super) fn require(condition: bool) -> Result<(), CodecError> {
    if condition { Ok(()) } else { Err(malformed()) }
}

pub(super) fn lp(e: &mut Encoder, value: &[u8]) -> Result<(), CodecError> {
    e.u64(u64::try_from(value.len()).map_err(|_| malformed())?);
    for byte in value {
        e.u8(*byte);
    }
    Ok(())
}

pub(super) fn take_lp<'a>(d: &mut Decoder<'a>, maximum: usize) -> Result<&'a [u8], CodecError> {
    let length = usize::try_from(d.u64()?).map_err(|_| malformed())?;
    require(length <= maximum)?;
    d.take(length)
}

pub(super) fn hash(domain: &[u8], fields: &[&[u8]]) -> [u8; 32] {
    let mut hash = Sha256::new();
    for bytes in std::iter::once(domain).chain(fields.iter().copied()) {
        hash.update((bytes.len() as u64).to_be_bytes());
        hash.update(bytes);
    }
    hash.finalize().into()
}

pub(super) fn enc_resource_key(e: &mut Encoder, key: ResourceKey) {
    enc_resource(e, key.id);
    enc_projection_rev(e, key.revision);
}

pub(super) fn dec_resource_key(d: &mut Decoder<'_>) -> Result<ResourceKey, CodecError> {
    Ok(ResourceKey {
        id: dec_resource(d)?,
        revision: dec_projection_rev(d)?,
    })
}

pub(super) fn enc_build_key(e: &mut Encoder, key: BuildKey) {
    enc_resource_key(e, key.resource);
    e.fixed16(&key.operation);
}

pub(super) fn dec_build_key(d: &mut Decoder<'_>) -> Result<BuildKey, CodecError> {
    Ok(BuildKey {
        resource: dec_resource_key(d)?,
        operation: d.fixed16()?,
    })
}

pub(super) fn enc_sequence_key(e: &mut Encoder, key: SequenceKey) -> Result<(), CodecError> {
    require(key.ordinal != 0)?;
    enc_build_key(e, key.build);
    e.u64(key.ordinal);
    Ok(())
}

pub(super) fn dec_sequence_key(d: &mut Decoder<'_>) -> Result<SequenceKey, CodecError> {
    let key = SequenceKey {
        build: dec_build_key(d)?,
        ordinal: d.u64()?,
    };
    require(key.ordinal != 0)?;
    Ok(key)
}

pub(crate) fn resource_key(key: ResourceKey) -> Vec<u8> {
    let mut e = Encoder::new();
    enc_resource_key(&mut e, key);
    e.finish()
}

pub(crate) fn build_key(key: BuildKey) -> Vec<u8> {
    let mut e = Encoder::new();
    enc_build_key(&mut e, key);
    e.finish()
}

pub(crate) fn sequence_key(key: SequenceKey) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_sequence_key(&mut e, key)?;
    Ok(e.finish())
}

pub(super) fn enc_range(e: &mut Encoder, value: ByteRange) -> Result<(), CodecError> {
    require(value.is_valid())?;
    e.u64(value.start);
    e.u64(value.end);
    Ok(())
}

pub(super) fn dec_range(d: &mut Decoder<'_>) -> Result<ByteRange, CodecError> {
    let value = ByteRange {
        start: d.u64()?,
        end: d.u64()?,
    };
    require(value.is_valid())?;
    Ok(value)
}

pub(super) fn enc_role(e: &mut Encoder, role: RowRole) {
    e.u8(match role {
        RowRole::Header => 0,
        RowRole::Body => 1,
    });
}

pub(super) fn dec_role(d: &mut Decoder<'_>) -> Result<RowRole, CodecError> {
    match d.u8()? {
        0 => Ok(RowRole::Header),
        1 => Ok(RowRole::Body),
        tag => Err(CodecError::InvalidTag {
            kind: "resource row role",
            tag,
        }),
    }
}

pub(crate) fn binding_bytes(value: &Binding) -> Result<Vec<u8>, CodecError> {
    require(
        value.resource.id == value.metadata.id()
            && value.resource.revision == value.metadata.revision(),
    )?;
    let ResourceBacking::TextRange { source, range } = value.metadata.backing() else {
        return Err(malformed());
    };
    require(
        source == value.source
            && range.start() == value.range.start
            && range.end() == value.range.end,
    )?;
    require(matches!(
        value.metadata.structure(),
        ResourceStructure::Code { .. } | ResourceStructure::Table { .. }
    ))?;
    let metadata = ResourcesFamily::encode_value(&value.metadata)?;
    require(metadata.len() <= 65_536)?;
    let mut e = Encoder::new();
    enc_resource_key(&mut e, value.resource);
    lp(&mut e, &metadata)?;
    enc_projection_text_source(&mut e, value.source);
    enc_range(&mut e, value.range)?;
    e.u64(1);
    e.u64(1);
    require(e.len() <= 66_048)?;
    Ok(e.finish())
}

pub(crate) fn decode_binding(bytes: &[u8]) -> Result<Binding, CodecError> {
    require(bytes.len() <= 66_048)?;
    let mut d = Decoder::new(bytes);
    let value = Binding {
        resource: dec_resource_key(&mut d)?,
        metadata: ResourcesFamily::decode_value(take_lp(&mut d, 65_536)?)?,
        source: dec_projection_text_source(&mut d)?,
        range: dec_range(&mut d)?,
    };
    require(d.u64()? == 1 && d.u64()? == 1)?;
    d.finish()?;
    require(binding_bytes(&value)?.as_slice() == bytes)?;
    Ok(value)
}

pub(crate) fn binding_digest(value: &Binding) -> Result<[u8; 32], CodecError> {
    Ok(hash(
        b"syndic/resource-structure-binding/v1",
        &[&binding_bytes(value)?],
    ))
}

pub(crate) fn empty_chain(key: BuildKey, binding: &[u8; 32]) -> [u8; 32] {
    hash(
        b"syndic/resource-structure-chain/v1/empty",
        &[&build_key(key), binding],
    )
}

pub(crate) fn chain_link(prior: &[u8; 32], page: &[u8; 32]) -> [u8; 32] {
    hash(b"syndic/resource-structure-chain/v1/link", &[prior, page])
}

pub(crate) fn validate_descriptor(value: &Descriptor) -> Result<(), CodecError> {
    if let Some(raw) = value.raw() {
        require(raw.is_valid())?;
    }
    match *value {
        Descriptor::CodeLine {
            raw,
            content,
            terminator,
            ..
        } => {
            require(
                raw.start < raw.end
                    && raw.start == content.start
                    && content.end == terminator.start
                    && terminator.end == raw.end,
            )?;
            require(
                raw.contains(content)
                    && raw.contains(terminator)
                    && terminator.end - terminator.start <= 2,
            )?;
        }
        Descriptor::TableRow {
            role,
            ordinal,
            raw,
            authored_cells,
            ..
        } => {
            require(
                raw.start < raw.end
                    && authored_cells != 0
                    && (role != RowRole::Header || ordinal == 0),
            )?;
        }
        Descriptor::TableCell {
            role,
            row,
            raw,
            trimmed,
            ..
        } => {
            require(raw.contains(trimmed) && (role != RowRole::Header || row == 0))?;
        }
        Descriptor::TableColumn { .. } => {}
    }
    Ok(())
}

pub(crate) fn enc_descriptor(e: &mut Encoder, value: &Descriptor) -> Result<(), CodecError> {
    validate_descriptor(value)?;
    e.u8(value.coordinate().0);
    match *value {
        Descriptor::CodeLine {
            ordinal,
            raw,
            content,
            terminator,
        } => {
            e.u64(ordinal);
            enc_range(e, raw)?;
            enc_range(e, content)?;
            enc_range(e, terminator)?;
        }
        Descriptor::TableRow {
            role,
            ordinal,
            raw,
            authored_cells,
            disposition,
        } => {
            enc_role(e, role);
            e.u64(ordinal);
            enc_range(e, raw)?;
            e.u64(authored_cells);
            e.u8(match disposition {
                RowDisposition::Ordinary => 0,
                RowDisposition::WholeRowFallback => 1,
            });
        }
        Descriptor::TableCell {
            role,
            row,
            column,
            raw,
            trimmed,
            presentation,
        } => {
            enc_role(e, role);
            e.u64(row);
            e.u64(column);
            enc_range(e, raw)?;
            enc_range(e, trimmed)?;
            e.u8(match presentation {
                CellPresentation::Literal => 0,
                CellPresentation::LiteralFallback => 1,
            });
        }
        Descriptor::TableColumn { ordinal, alignment } => {
            e.u64(ordinal);
            e.u8(match alignment {
                Alignment::Unspecified => 0,
                Alignment::Left => 1,
                Alignment::Center => 2,
                Alignment::Right => 3,
            });
        }
    }
    Ok(())
}

pub(crate) fn dec_descriptor(d: &mut Decoder<'_>) -> Result<Descriptor, CodecError> {
    let value = match d.u8()? {
        0 => Descriptor::CodeLine {
            ordinal: d.u64()?,
            raw: dec_range(d)?,
            content: dec_range(d)?,
            terminator: dec_range(d)?,
        },
        1 => Descriptor::TableRow {
            role: dec_role(d)?,
            ordinal: d.u64()?,
            raw: dec_range(d)?,
            authored_cells: d.u64()?,
            disposition: match d.u8()? {
                0 => RowDisposition::Ordinary,
                1 => RowDisposition::WholeRowFallback,
                tag => {
                    return Err(CodecError::InvalidTag {
                        kind: "resource row disposition",
                        tag,
                    });
                }
            },
        },
        2 => Descriptor::TableCell {
            role: dec_role(d)?,
            row: d.u64()?,
            column: d.u64()?,
            raw: dec_range(d)?,
            trimmed: dec_range(d)?,
            presentation: match d.u8()? {
                0 => CellPresentation::Literal,
                1 => CellPresentation::LiteralFallback,
                tag => {
                    return Err(CodecError::InvalidTag {
                        kind: "resource cell presentation",
                        tag,
                    });
                }
            },
        },
        3 => Descriptor::TableColumn {
            ordinal: d.u64()?,
            alignment: match d.u8()? {
                0 => Alignment::Unspecified,
                1 => Alignment::Left,
                2 => Alignment::Center,
                3 => Alignment::Right,
                tag => {
                    return Err(CodecError::InvalidTag {
                        kind: "resource column alignment",
                        tag,
                    });
                }
            },
        },
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "resource descriptor",
                tag,
            });
        }
    };
    validate_descriptor(&value)?;
    Ok(value)
}

fn descriptor_bytes(values: &[Descriptor]) -> Result<Vec<u8>, CodecError> {
    require(!values.is_empty() && values.len() <= 256)?;
    require(
        values
            .windows(2)
            .all(|pair| pair[0].coordinate() < pair[1].coordinate()),
    )?;
    let mut e = Encoder::new();
    e.u64(values.len() as u64);
    for value in values {
        enc_descriptor(&mut e, value)?;
    }
    Ok(e.finish())
}

pub(crate) fn page_digest(value: &Page) -> Result<[u8; 32], CodecError> {
    Ok(hash(
        b"syndic/resource-structure-page/v1",
        &[
            &sequence_key(value.key)?,
            &value.binding_digest,
            &value.prior_chain,
            &descriptor_bytes(&value.descriptors)?,
        ],
    ))
}

pub(crate) fn encode_page(value: &Page) -> Result<Vec<u8>, CodecError> {
    require(value.digest == page_digest(value)?)?;
    let mut e = Encoder::new();
    enc_sequence_key(&mut e, value.key)?;
    e.fixed32(&value.binding_digest);
    e.fixed32(&value.prior_chain);
    for byte in descriptor_bytes(&value.descriptors)? {
        e.u8(byte);
    }
    e.fixed32(&value.digest);
    require(e.len() <= 65_536)?;
    Ok(e.finish())
}

pub(crate) fn decode_page(bytes: &[u8]) -> Result<Page, CodecError> {
    require(bytes.len() <= 65_536)?;
    let mut d = Decoder::new(bytes);
    let key = dec_sequence_key(&mut d)?;
    let binding_digest = d.fixed32()?;
    let prior_chain = d.fixed32()?;
    let count = usize::try_from(d.u64()?).map_err(|_| malformed())?;
    require((1..=256).contains(&count))?;
    let mut descriptors = Vec::with_capacity(count);
    for _ in 0..count {
        descriptors.push(dec_descriptor(&mut d)?);
    }
    let value = Page {
        key,
        binding_digest,
        prior_chain,
        descriptors,
        digest: d.fixed32()?,
    };
    d.finish()?;
    encode_page(&value)?;
    Ok(value)
}

macro_rules! structure_family {
    ($family:ident, $codec:ident, $name:literal, $key:ty, $value:ty, $key_max:expr, $value_max:expr, $enc_key:expr, $dec_key:expr, $enc:expr, $dec:expr) => {
        pub(crate) struct $family;
        pub(crate) type $codec = ExactCodec<$family>;
        impl Family for $family {
            type Key = $key;
            type Value = $value;
            const NAME: &'static str = $name;
            const RECORD_VERSION: RecordVersion = RecordVersion::new(1);
            const MAX_KEY_BYTES: usize = $key_max;
            const MAX_VALUE_BYTES: usize = $value_max;
            fn encode_key(key: &Self::Key) -> Result<Vec<u8>, CodecError> {
                ($enc_key)(*key)
            }
            fn decode_key(bytes: &[u8]) -> Result<Self::Key, CodecError> {
                ($dec_key)(bytes)
            }
            fn encode_value(value: &Self::Value) -> Result<Vec<u8>, CodecError> {
                ($enc)(value)
            }
            fn decode_value(bytes: &[u8]) -> Result<Self::Value, CodecError> {
                ($dec)(bytes)
            }
        }
    };
}

pub(super) use structure_family;

pub(super) fn decode_sequence_key(bytes: &[u8]) -> Result<SequenceKey, CodecError> {
    let mut d = Decoder::new(bytes);
    let key = dec_sequence_key(&mut d)?;
    d.finish()?;
    Ok(key)
}

structure_family!(
    PagesFamily,
    PagesCodec,
    "resource-structure-pages",
    SequenceKey,
    Page,
    48,
    65_536,
    sequence_key,
    decode_sequence_key,
    encode_page,
    decode_page
);
