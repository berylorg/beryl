use super::*;

fn root_payload(value: &Root) -> Result<Vec<u8>, CodecError> {
    require(value.progress.build == value.key && value.progress.ordinal != 0)?;
    let total = value
        .counts
        .iter()
        .try_fold(0_u64, |total, count| total.checked_add(*count))
        .ok_or_else(malformed)?;
    let minimum_pages = total / 256 + u64::from(total % 256 != 0);
    require(minimum_pages <= value.pages && value.pages <= total)?;
    if let Some(header) = value.header {
        require(
            header.start < header.end
                && value.language.is_none()
                && value.code_lines == 0
                && value.columns != 0
                && value.counts[0] == 0
                && value.counts[1] == value.body_rows.checked_add(1).ok_or_else(malformed)?
                && value.counts[3] == value.columns,
        )?;
    } else {
        require(
            value.body_rows == 0
                && value.columns == 0
                && value.counts[0] == value.code_lines
                && value.counts[1..] == [0, 0, 0],
        )?;
    }
    if let Some(language) = &value.language {
        require(language.len() <= 4_096)?;
    }
    let mut e = Encoder::new();
    enc_build_key(&mut e, value.key);
    e.fixed32(&value.binding_digest);
    e.u64(value.pages);
    e.fixed32(&value.chain);
    enc_counts(&mut e, value.counts);
    e.u64(value.code_lines);
    e.u64(value.body_rows);
    e.u64(value.columns);
    e.u8(u8::from(value.header.is_some()));
    if let Some(header) = value.header {
        enc_range(&mut e, header)?;
    }
    e.u8(u8::from(value.language.is_some()));
    if let Some(language) = &value.language {
        lp(&mut e, language.as_bytes())?;
    }
    enc_sequence_key(&mut e, value.progress)?;
    Ok(e.finish())
}

pub(crate) fn root_digest(value: &Root) -> Result<[u8; 32], CodecError> {
    Ok(hash(
        b"syndic/resource-structure-root/v1",
        &[&build_key(value.key), &root_payload(value)?],
    ))
}

pub(crate) fn encode_root(value: &Root) -> Result<Vec<u8>, CodecError> {
    require(value.digest == root_digest(value)?)?;
    let mut bytes = root_payload(value)?;
    bytes.extend_from_slice(&value.digest);
    require(bytes.len() <= 4_608)?;
    Ok(bytes)
}

pub(crate) fn decode_root(bytes: &[u8]) -> Result<Root, CodecError> {
    require(bytes.len() <= 4_608)?;
    let mut d = Decoder::new(bytes);
    let value = Root {
        key: dec_build_key(&mut d)?,
        binding_digest: d.fixed32()?,
        pages: d.u64()?,
        chain: d.fixed32()?,
        counts: dec_counts(&mut d)?,
        code_lines: d.u64()?,
        body_rows: d.u64()?,
        columns: d.u64()?,
        header: match d.u8()? {
            0 => None,
            1 => Some(dec_range(&mut d)?),
            tag => {
                return Err(CodecError::InvalidTag {
                    kind: "resource header option",
                    tag,
                });
            }
        },
        language: match d.u8()? {
            0 => None,
            1 => Some(
                std::str::from_utf8(take_lp(&mut d, 4_096)?)
                    .map_err(|_| CodecError::InvalidUtf8("resource language"))?
                    .into(),
            ),
            tag => {
                return Err(CodecError::InvalidTag {
                    kind: "resource language option",
                    tag,
                });
            }
        },
        progress: dec_sequence_key(&mut d)?,
        digest: d.fixed32()?,
    };
    d.finish()?;
    encode_root(&value)?;
    Ok(value)
}

fn head_payload(value: &Head) -> Result<Vec<u8>, CodecError> {
    require(value.key == value.build.resource)?;
    let mut e = Encoder::new();
    enc_resource_key(&mut e, value.key);
    e.u64(1);
    e.fixed32(&value.binding_digest);
    enc_build_key(&mut e, value.build);
    e.fixed32(&value.root);
    Ok(e.finish())
}

pub(crate) fn head_digest(value: &Head) -> Result<[u8; 32], CodecError> {
    Ok(hash(
        b"syndic/resource-structure-head/v1",
        &[&resource_key(value.key), &head_payload(value)?],
    ))
}

pub(crate) fn encode_head(value: &Head) -> Result<Vec<u8>, CodecError> {
    require(value.digest == head_digest(value)?)?;
    let mut bytes = head_payload(value)?;
    bytes.extend_from_slice(&value.digest);
    Ok(bytes)
}

pub(crate) fn decode_head(bytes: &[u8]) -> Result<Head, CodecError> {
    require(bytes.len() == 168)?;
    let mut d = Decoder::new(bytes);
    let key = dec_resource_key(&mut d)?;
    require(d.u64()? == 1)?;
    let value = Head {
        key,
        binding_digest: d.fixed32()?,
        build: dec_build_key(&mut d)?,
        root: d.fixed32()?,
        digest: d.fixed32()?,
    };
    d.finish()?;
    encode_head(&value)?;
    Ok(value)
}
