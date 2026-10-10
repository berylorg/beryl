use super::*;

fn enc_coordinate(e: &mut Encoder, key: CoordinateKey) -> Result<(), CodecError> {
    match key {
        CoordinateKey::CodeLine(build, ordinal) | CoordinateKey::TableColumn(build, ordinal) => {
            enc_build_key(e, build);
            e.u64(ordinal);
        }
        CoordinateKey::TableRow(build, role, row) => {
            require(role != RowRole::Header || row == 0)?;
            enc_build_key(e, build);
            enc_role(e, role);
            e.u64(row);
        }
        CoordinateKey::TableCell(build, role, row, column) => {
            require(role != RowRole::Header || row == 0)?;
            enc_build_key(e, build);
            enc_role(e, role);
            e.u64(row);
            e.u64(column);
        }
    }
    Ok(())
}

fn dec_coordinate(d: &mut Decoder<'_>, tag: u8) -> Result<CoordinateKey, CodecError> {
    let build = dec_build_key(d)?;
    let key = match tag {
        0 => CoordinateKey::CodeLine(build, d.u64()?),
        1 => CoordinateKey::TableRow(build, dec_role(d)?, d.u64()?),
        2 => CoordinateKey::TableCell(build, dec_role(d)?, d.u64()?, d.u64()?),
        3 => CoordinateKey::TableColumn(build, d.u64()?),
        _ => return Err(malformed()),
    };
    enc_coordinate(&mut Encoder::new(), key)?;
    Ok(key)
}

pub(crate) fn coordinate_key(key: CoordinateKey) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_coordinate(&mut e, key)?;
    Ok(e.finish())
}

fn coordinate_tag(key: CoordinateKey) -> u8 {
    match key {
        CoordinateKey::CodeLine(..) => 0,
        CoordinateKey::TableRow(..) => 1,
        CoordinateKey::TableCell(..) => 2,
        CoordinateKey::TableColumn(..) => 3,
    }
}

fn decode_coordinate_key(bytes: &[u8], tag: u8) -> Result<CoordinateKey, CodecError> {
    let mut d = Decoder::new(bytes);
    let key = dec_coordinate(&mut d, tag)?;
    d.finish()?;
    Ok(key)
}

fn encode_locator(value: &Locator, tag: u8) -> Result<Vec<u8>, CodecError> {
    require(coordinate_tag(value.key) == tag && value.page != 0 && value.slot < 256)?;
    let mut e = Encoder::new();
    enc_coordinate(&mut e, value.key)?;
    e.u64(value.page);
    e.u64(value.slot);
    e.fixed32(&value.digest);
    Ok(e.finish())
}

fn decode_locator(bytes: &[u8], tag: u8) -> Result<Locator, CodecError> {
    let mut d = Decoder::new(bytes);
    let value = Locator {
        key: dec_coordinate(&mut d, tag)?,
        page: d.u64()?,
        slot: d.u64()?,
        digest: d.fixed32()?,
    };
    d.finish()?;
    encode_locator(&value, tag)?;
    Ok(value)
}

macro_rules! coordinate_family {
    ($family:ident, $codec:ident, $name:literal, $tag:expr, $key_max:expr) => {
        structure_family!(
            $family,
            $codec,
            $name,
            CoordinateKey,
            Locator,
            $key_max,
            $key_max + 48,
            |key| {
                require(coordinate_tag(key) == $tag)?;
                coordinate_key(key)
            },
            |bytes| decode_coordinate_key(bytes, $tag),
            |value| encode_locator(value, $tag),
            |bytes| decode_locator(bytes, $tag)
        );
    };
}

coordinate_family!(
    CodeLinesFamily,
    CodeLinesCodec,
    "resource-code-lines",
    0,
    48
);
coordinate_family!(
    TableRowsFamily,
    TableRowsCodec,
    "resource-table-rows",
    1,
    49
);
coordinate_family!(
    TableCellsFamily,
    TableCellsCodec,
    "resource-table-cells",
    2,
    57
);
coordinate_family!(
    TableColumnsFamily,
    TableColumnsCodec,
    "resource-table-columns",
    3,
    48
);

fn enc_anchor(e: &mut Encoder, key: AnchorKey) -> Result<(), CodecError> {
    require(matches!(key.tag, 0..=2))?;
    require(key.tag != 0 || (key.role == RowRole::Header && key.column == 0))?;
    require(key.tag != 1 || key.column == 0)?;
    require(key.tag == 0 || key.role != RowRole::Header || key.row == 0)?;
    enc_build_key(e, key.build);
    e.u64(key.start);
    e.u8(key.tag);
    enc_role(e, key.role);
    e.u64(key.row);
    e.u64(key.column);
    Ok(())
}

fn dec_anchor(d: &mut Decoder<'_>) -> Result<AnchorKey, CodecError> {
    let key = AnchorKey {
        build: dec_build_key(d)?,
        start: d.u64()?,
        tag: d.u8()?,
        role: dec_role(d)?,
        row: d.u64()?,
        column: d.u64()?,
    };
    enc_anchor(&mut Encoder::new(), key)?;
    Ok(key)
}

pub(crate) fn anchor_key(key: AnchorKey) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_anchor(&mut e, key)?;
    Ok(e.finish())
}

fn decode_anchor_key(bytes: &[u8]) -> Result<AnchorKey, CodecError> {
    let mut d = Decoder::new(bytes);
    let key = dec_anchor(&mut d)?;
    d.finish()?;
    Ok(key)
}

pub(crate) fn encode_anchor(value: &AnchorLocator) -> Result<Vec<u8>, CodecError> {
    require(value.end >= value.key.start && value.page != 0 && value.slot < 256)?;
    require(value.key.tag == 2 || value.key.start < value.end)?;
    let mut e = Encoder::new();
    enc_anchor(&mut e, value.key)?;
    e.u64(value.end);
    e.u64(value.page);
    e.u64(value.slot);
    e.fixed32(&value.digest);
    Ok(e.finish())
}

pub(crate) fn decode_anchor(bytes: &[u8]) -> Result<AnchorLocator, CodecError> {
    let mut d = Decoder::new(bytes);
    let value = AnchorLocator {
        key: dec_anchor(&mut d)?,
        end: d.u64()?,
        page: d.u64()?,
        slot: d.u64()?,
        digest: d.fixed32()?,
    };
    d.finish()?;
    encode_anchor(&value)?;
    Ok(value)
}

structure_family!(
    SourceAnchorsFamily,
    SourceAnchorsCodec,
    "resource-source-anchors",
    AnchorKey,
    AnchorLocator,
    66,
    122,
    anchor_key,
    decode_anchor_key,
    encode_anchor,
    decode_anchor
);
