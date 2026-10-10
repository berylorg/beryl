use super::*;

macro_rules! enum_codec {
    ($enc:ident, $dec:ident, $kind:literal, $ty:ident, $($tag:literal => $variant:ident),+ $(,)?) => {
        fn $enc(e: &mut Encoder, value: $ty) {
            e.u8(match value { $($ty::$variant => $tag),+ });
        }
        fn $dec(d: &mut Decoder<'_>) -> Result<$ty, CodecError> {
            match d.u8()? { $($tag => Ok($ty::$variant)),+, tag => Err(CodecError::InvalidTag { kind: $kind, tag }) }
        }
    };
}

enum_codec!(enc_lifecycle, dec_lifecycle, "resource build lifecycle", Lifecycle,
    0 => Building, 1 => Completed, 2 => Cancelled, 3 => Superseded, 4 => Refused);
enum_codec!(enc_pass, dec_pass, "resource construction pass", Pass,
    0 => CodeLines, 1 => TableRows, 2 => TableCells, 3 => Columns, 4 => Sealed);
enum_codec!(enc_phase, dec_phase, "resource parse phase", ParsePhase,
    0 => Consuming, 1 => DrainingSyntax, 2 => Eof);
enum_codec!(enc_command, dec_command, "resource construction command", Command,
    0 => Advance, 1 => PassChange, 2 => Complete, 3 => Cancel, 4 => Supersede, 5 => Refuse);

fn dec_reference(d: &mut Decoder<'_>) -> Result<Reference, CodecError> {
    Ok(Reference {
        key: dec_sequence_key(d)?,
        digest: d.fixed32()?,
    })
}

fn enc_optional_reference(
    e: &mut Encoder,
    value: Option<Reference>,
    omit_digest: bool,
) -> Result<(), CodecError> {
    e.u8(u8::from(value.is_some()));
    if let Some(value) = value {
        enc_sequence_key(e, value.key)?;
        if !omit_digest {
            e.fixed32(&value.digest);
        }
    }
    Ok(())
}

fn dec_optional_reference(d: &mut Decoder<'_>) -> Result<Option<Reference>, CodecError> {
    match d.u8()? {
        0 => Ok(None),
        1 => Ok(Some(dec_reference(d)?)),
        tag => Err(CodecError::InvalidTag {
            kind: "resource reference option",
            tag,
        }),
    }
}

fn valid_utf8_carry(bytes: &[u8]) -> bool {
    bytes.is_empty()
        || (bytes.len() <= 3
            && std::str::from_utf8(bytes)
                .is_err_and(|error| error.valid_up_to() == 0 && error.error_len().is_none()))
}

pub(crate) fn validate_checkpoint(value: &Checkpoint, length: u64) -> Result<(), CodecError> {
    require(value.next_byte <= length && value.line_start <= value.next_byte)?;
    require(valid_utf8_carry(&value.utf8_carry) && value.syntax.len() <= 16_384)?;
    require(
        value.syntax_start <= value.next_byte && value.output_position <= value.syntax.len() as u64,
    )?;
    require(
        value
            .syntax_start
            .checked_add(value.syntax.len() as u64)
            .is_some_and(|end| end <= value.next_byte),
    )?;
    require(value.utf8_carry.len() as u64 <= value.next_byte)?;
    require(std::str::from_utf8(&value.syntax).is_ok())?;
    if !matches!(value.pass, Pass::TableRows | Pass::TableCells) {
        require(value.role == RowRole::Header)?;
    }
    if value.pass == Pass::CodeLines {
        require(
            value.next_column == 0
                && value.syntax_start == 0
                && value.syntax.is_empty()
                && value.output_position == 0
                && !value.escaped
                && !value.in_code,
        )?;
        require(value.line_start <= value.next_byte - value.utf8_carry.len() as u64)?;
        require(
            !value.pending_cr
                || (value.line_start < value.next_byte && value.utf8_carry.is_empty()),
        )?;
        if value.phase == ParsePhase::Eof {
            require(value.next_byte == length && value.line_start == 0 && !value.pending_cr)?;
        }
    }
    if value.phase == ParsePhase::Eof {
        require(value.utf8_carry.is_empty())?;
    }
    if value.pass == Pass::Sealed {
        require(
            value.next_byte == length
                && value.line_start == 0
                && value.next_row == 0
                && value.next_column == 0
                && value.phase == ParsePhase::Eof
                && !value.pending_cr
                && value.utf8_carry.is_empty()
                && value.syntax_start == 0
                && value.syntax.is_empty()
                && value.output_position == 0
                && !value.escaped
                && !value.in_code,
        )?;
    }
    Ok(())
}

fn enc_checkpoint(e: &mut Encoder, value: &Checkpoint) -> Result<(), CodecError> {
    enc_pass(e, value.pass);
    e.u64(value.next_byte);
    e.u64(value.line_start);
    enc_role(e, value.role);
    e.u64(value.next_row);
    e.u64(value.next_column);
    enc_phase(e, value.phase);
    e.u8(u8::from(value.pending_cr));
    lp(e, &value.utf8_carry)?;
    e.u64(value.syntax_start);
    lp(e, &value.syntax)?;
    e.u64(value.output_position);
    e.u8(u8::from(value.escaped));
    e.u8(u8::from(value.in_code));
    Ok(())
}

fn dec_checkpoint(d: &mut Decoder<'_>) -> Result<Checkpoint, CodecError> {
    Ok(Checkpoint {
        pass: dec_pass(d)?,
        next_byte: d.u64()?,
        line_start: d.u64()?,
        role: dec_role(d)?,
        next_row: d.u64()?,
        next_column: d.u64()?,
        phase: dec_phase(d)?,
        pending_cr: dec_bool(d, "resource pending CR")?,
        utf8_carry: take_lp(d, 3)?.to_vec(),
        syntax_start: d.u64()?,
        syntax: take_lp(d, 16_384)?.to_vec(),
        output_position: d.u64()?,
        escaped: dec_bool(d, "resource escape")?,
        in_code: dec_bool(d, "resource backtick")?,
    })
}

pub(super) fn enc_counts(e: &mut Encoder, counts: [u64; 4]) {
    for count in counts {
        e.u64(count);
    }
}

pub(super) fn dec_counts(d: &mut Decoder<'_>) -> Result<[u64; 4], CodecError> {
    Ok([d.u64()?, d.u64()?, d.u64()?, d.u64()?])
}

pub(crate) fn validate_build(value: &Build) -> Result<(), CodecError> {
    require(
        value.key.resource == value.binding.resource
            && value.binding_digest == binding_digest(&value.binding)?,
    )?;
    require(
        value.revision == value.transition.checked_add(1).ok_or_else(malformed)?
            && value.next_page != 0,
    )?;
    require(value.progress.is_some() == (value.transition != 0))?;
    if let Some(progress) = value.progress {
        require(progress.key.build == value.key && progress.key.ordinal == value.transition)?;
    }
    let length = value
        .binding
        .range
        .end
        .checked_sub(value.binding.range.start)
        .ok_or_else(malformed)?;
    validate_checkpoint(&value.checkpoint, length)?;
    match value.binding.metadata.structure() {
        ResourceStructure::Code { .. } => {
            require(
                matches!(value.checkpoint.pass, Pass::CodeLines | Pass::Sealed)
                    && value.counts[1..] == [0, 0, 0],
            )?;
            if value.checkpoint.pass == Pass::CodeLines {
                require(value.counts[0] == value.checkpoint.next_row)?;
            }
        }
        ResourceStructure::Table { .. } => {
            require(value.checkpoint.pass != Pass::CodeLines && value.counts[0] == 0)?;
        }
        ResourceStructure::Opaque => return Err(malformed()),
    }
    if value.transition == 0 {
        require(
            value.lifecycle == Lifecycle::Building
                && value.next_page == 1
                && value.counts == [0; 4]
                && value.chain == empty_chain(value.key, &value.binding_digest),
        )?;
        let initial_pass = if matches!(
            value.binding.metadata.structure(),
            ResourceStructure::Code { .. }
        ) {
            Pass::CodeLines
        } else {
            Pass::TableRows
        };
        require(
            value.checkpoint
                == Checkpoint {
                    pass: initial_pass,
                    next_byte: 0,
                    line_start: 0,
                    role: RowRole::Header,
                    next_row: 0,
                    next_column: 0,
                    phase: ParsePhase::Consuming,
                    pending_cr: false,
                    utf8_carry: Vec::new(),
                    syntax_start: 0,
                    syntax: Vec::new(),
                    output_position: 0,
                    escaped: false,
                    in_code: false,
                },
        )?;
    }
    require(value.lifecycle != Lifecycle::Completed || value.checkpoint.pass == Pass::Sealed)?;
    Ok(())
}

fn build_payload(value: &Build, omit_progress_digest: bool) -> Result<Vec<u8>, CodecError> {
    validate_build(value)?;
    let binding = binding_bytes(&value.binding)?;
    require(
        binding.len() <= 4_608
            && ResourcesFamily::encode_value(&value.binding.metadata)?.len() <= 4_096,
    )?;
    let mut e = Encoder::new();
    enc_build_key(&mut e, value.key);
    lp(&mut e, &binding)?;
    e.fixed32(&value.binding_digest);
    e.u64(value.revision);
    enc_lifecycle(&mut e, value.lifecycle);
    e.u64(value.transition);
    enc_optional_reference(&mut e, value.progress, omit_progress_digest)?;
    e.u64(value.next_page);
    e.fixed32(&value.chain);
    enc_counts(&mut e, value.counts);
    enc_checkpoint(&mut e, &value.checkpoint)?;
    require(e.len() <= 24_576)?;
    Ok(e.finish())
}

pub(crate) fn encode_build(value: &Build) -> Result<Vec<u8>, CodecError> {
    build_payload(value, false)
}

pub(crate) fn build_digest(value: &Build) -> Result<[u8; 32], CodecError> {
    Ok(hash(
        b"syndic/resource-structure-build/v1",
        &[&build_key(value.key), &build_payload(value, true)?],
    ))
}

pub(crate) fn decode_build(bytes: &[u8]) -> Result<Build, CodecError> {
    require(bytes.len() <= 24_576)?;
    let mut d = Decoder::new(bytes);
    let value = Build {
        key: dec_build_key(&mut d)?,
        binding: decode_binding(take_lp(&mut d, 4_608)?)?,
        binding_digest: d.fixed32()?,
        revision: d.u64()?,
        lifecycle: dec_lifecycle(&mut d)?,
        transition: d.u64()?,
        progress: dec_optional_reference(&mut d)?,
        next_page: d.u64()?,
        chain: d.fixed32()?,
        counts: dec_counts(&mut d)?,
        checkpoint: dec_checkpoint(&mut d)?,
    };
    d.finish()?;
    validate_build(&value)?;
    require(ResourcesFamily::encode_value(&value.binding.metadata)?.len() <= 4_096)?;
    Ok(value)
}

fn progress_payload(value: &Progress) -> Result<Vec<u8>, CodecError> {
    require(
        value.key.ordinal != 0
            && value.before_revision == value.key.ordinal
            && value.after_revision
                == value.before_revision.checked_add(1).ok_or_else(malformed)?,
    )?;
    require(value.prior.is_none() == (value.key.ordinal == 1))?;
    if let Some(prior) = value.prior {
        require(
            prior.key.build == value.key.build
                && prior.key.ordinal.checked_add(1) == Some(value.key.ordinal),
        )?;
    }
    if let Some(page) = value.page {
        require(page.key.build == value.key.build)?;
    }
    require(value.root.is_some() == (value.command == Command::Complete))?;
    require(
        !matches!(
            value.command,
            Command::Cancel | Command::Supersede | Command::Refuse
        ) || value.page.is_none(),
    )?;
    let mut e = Encoder::new();
    enc_sequence_key(&mut e, value.key)?;
    enc_command(&mut e, value.command);
    enc_optional_reference(&mut e, value.prior, false)?;
    e.u64(value.before_revision);
    e.u64(value.after_revision);
    e.fixed32(&value.before_digest);
    e.fixed32(&value.after_digest);
    enc_optional_reference(&mut e, value.page, false)?;
    e.u8(u8::from(value.root.is_some()));
    if let Some(root) = value.root {
        e.fixed32(&root);
    }
    Ok(e.finish())
}

pub(crate) fn progress_digest(value: &Progress) -> Result<[u8; 32], CodecError> {
    Ok(hash(
        b"syndic/resource-structure-progress/v1",
        &[&sequence_key(value.key)?, &progress_payload(value)?],
    ))
}

pub(crate) fn encode_progress(value: &Progress) -> Result<Vec<u8>, CodecError> {
    require(value.digest == progress_digest(value)?)?;
    let mut bytes = progress_payload(value)?;
    bytes.extend_from_slice(&value.digest);
    require(bytes.len() <= 512)?;
    Ok(bytes)
}

pub(crate) fn decode_progress(bytes: &[u8]) -> Result<Progress, CodecError> {
    require(bytes.len() <= 512)?;
    let mut d = Decoder::new(bytes);
    let value = Progress {
        key: dec_sequence_key(&mut d)?,
        command: dec_command(&mut d)?,
        prior: dec_optional_reference(&mut d)?,
        before_revision: d.u64()?,
        after_revision: d.u64()?,
        before_digest: d.fixed32()?,
        after_digest: d.fixed32()?,
        page: dec_optional_reference(&mut d)?,
        root: match d.u8()? {
            0 => None,
            1 => Some(d.fixed32()?),
            tag => {
                return Err(CodecError::InvalidTag {
                    kind: "resource root option",
                    tag,
                });
            }
        },
        digest: d.fixed32()?,
    };
    d.finish()?;
    encode_progress(&value)?;
    Ok(value)
}

fn decode_build_key(bytes: &[u8]) -> Result<BuildKey, CodecError> {
    let mut d = Decoder::new(bytes);
    let key = dec_build_key(&mut d)?;
    d.finish()?;
    Ok(key)
}

fn decode_resource_key(bytes: &[u8]) -> Result<ResourceKey, CodecError> {
    let mut d = Decoder::new(bytes);
    let key = dec_resource_key(&mut d)?;
    d.finish()?;
    Ok(key)
}

structure_family!(
    BuildsFamily,
    BuildsCodec,
    "resource-structure-builds",
    BuildKey,
    Build,
    40,
    24_576,
    |key| Ok(build_key(key)),
    decode_build_key,
    encode_build,
    decode_build
);
structure_family!(
    ProgressFamily,
    ProgressCodec,
    "resource-structure-progress",
    SequenceKey,
    Progress,
    48,
    512,
    sequence_key,
    decode_sequence_key,
    encode_progress,
    decode_progress
);
structure_family!(
    RootsFamily,
    RootsCodec,
    "resource-structure-roots",
    BuildKey,
    Root,
    40,
    4_608,
    |key| Ok(build_key(key)),
    decode_build_key,
    encode_root,
    decode_root
);
structure_family!(
    HeadsFamily,
    HeadsCodec,
    "resource-structure-heads",
    ResourceKey,
    Head,
    24,
    168,
    |key| Ok(resource_key(key)),
    decode_resource_key,
    encode_head,
    decode_head
);
