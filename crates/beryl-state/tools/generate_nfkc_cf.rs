//! Generates the compact catalog-owned Unicode NFKC_CF lookup table.
//!
//! Usage:
//! `rustc tools/generate_nfkc_cf.rs -o <temporary-executable>`
//! `<temporary-executable> <DerivedNormalizationProps.txt> <output.bin>`

use std::{
    convert::TryFrom,
    env,
    error::Error,
    fs,
    path::PathBuf,
};

const MAGIC: &[u8; 8] = b"BNKCF170";

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = env::args_os().skip(1);
    let input = required_path(arguments.next(), "Unicode input")?;
    let output = required_path(arguments.next(), "binary output")?;
    if arguments.next().is_some() {
        return Err("expected exactly two paths".into());
    }

    let source = fs::read_to_string(&input)?;
    require_unicode_17(&source)?;
    let mappings = parse_mappings(&source)?;
    let encoded = encode(&mappings)?;
    fs::write(&output, encoded)?;
    println!(
        "generated {} mappings at {}",
        mappings.len(),
        output.display()
    );
    Ok(())
}

fn required_path(value: Option<std::ffi::OsString>, kind: &str) -> Result<PathBuf, Box<dyn Error>> {
    value.map(PathBuf::from).ok_or_else(|| format!("missing {kind} path").into())
}

fn require_unicode_17(source: &str) -> Result<(), Box<dyn Error>> {
    if !source.lines().take(24).any(|line| {
        line.contains("DerivedNormalizationProps-17.0.0.txt")
            || line.contains("DerivedNormalizationProps-17.0.0")
    }) {
        return Err("input is not Unicode 17 DerivedNormalizationProps data".into());
    }
    Ok(())
}

#[derive(Debug)]
struct Mapping {
    scalar: u32,
    replacement: Vec<u32>,
}

fn parse_mappings(source: &str) -> Result<Vec<Mapping>, Box<dyn Error>> {
    let mut mappings = Vec::new();
    for (index, raw_line) in source.lines().enumerate() {
        let body = raw_line.split_once('#').map_or(raw_line, |(body, _)| body);
        let mut fields = body.split(';').map(str::trim);
        let Some(range) = fields.next().filter(|field| !field.is_empty()) else {
            continue;
        };
        let property = fields.next().unwrap_or_default();
        if property != "NFKC_CF" {
            continue;
        }
        let replacement = parse_replacement(fields.next().unwrap_or_default(), index + 1)?;
        let (start, end) = parse_range(range, index + 1)?;
        for scalar in start..=end {
            if char::from_u32(scalar).is_none() {
                return Err(format!("line {} maps a non-scalar value", index + 1).into());
            }
            mappings.push(Mapping {
                scalar,
                replacement: replacement.clone(),
            });
        }
    }
    mappings.sort_unstable_by_key(|mapping| mapping.scalar);
    if mappings.is_empty() {
        return Err("Unicode input contains no NFKC_CF mappings".into());
    }
    if mappings
        .windows(2)
        .any(|pair| pair[0].scalar == pair[1].scalar)
    {
        return Err("Unicode input contains duplicate NFKC_CF mappings".into());
    }
    Ok(mappings)
}

fn parse_range(value: &str, line: usize) -> Result<(u32, u32), Box<dyn Error>> {
    let (start, end) = value
        .split_once("..")
        .map_or((value, value), |(start, end)| (start, end));
    let start = u32::from_str_radix(start, 16)
        .map_err(|source| format!("line {line} has invalid range start: {source}"))?;
    let end = u32::from_str_radix(end, 16)
        .map_err(|source| format!("line {line} has invalid range end: {source}"))?;
    if end < start {
        return Err(format!("line {line} has a descending range").into());
    }
    Ok((start, end))
}

fn parse_replacement(value: &str, line: usize) -> Result<Vec<u32>, Box<dyn Error>> {
    value
        .split_whitespace()
        .map(|scalar| {
            let scalar = u32::from_str_radix(scalar, 16)
                .map_err(|source| format!("line {line} has invalid replacement: {source}"))?;
            char::from_u32(scalar)
                .map(|_| scalar)
                .ok_or_else(|| format!("line {line} replacement is not a scalar"))
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(Into::into)
}

fn encode(mappings: &[Mapping]) -> Result<Vec<u8>, Box<dyn Error>> {
    let mapping_count = u32::try_from(mappings.len())?;
    let value_count = mappings.iter().try_fold(0_u32, |total, mapping| {
        total
            .checked_add(u32::try_from(mapping.replacement.len()).ok()?)
    });
    let value_count = value_count.ok_or("mapping value count overflow")?;
    let capacity = 16_usize
        .checked_add(mappings.len().checked_mul(12).ok_or("entry size overflow")?)
        .and_then(|value| value.checked_add(value_count as usize * 4))
        .ok_or("table size overflow")?;
    let mut output = Vec::with_capacity(capacity);
    output.extend_from_slice(MAGIC);
    push_u32(&mut output, mapping_count);
    push_u32(&mut output, value_count);

    let mut offset = 0_u32;
    for mapping in mappings {
        let length = u16::try_from(mapping.replacement.len())?;
        push_u32(&mut output, mapping.scalar);
        push_u32(&mut output, offset);
        output.extend_from_slice(&length.to_le_bytes());
        output.extend_from_slice(&0_u16.to_le_bytes());
        offset = offset
            .checked_add(u32::from(length))
            .ok_or("mapping offset overflow")?;
    }
    if offset != value_count {
        return Err("mapping value count disagrees".into());
    }
    for mapping in mappings {
        for scalar in &mapping.replacement {
            push_u32(&mut output, *scalar);
        }
    }
    if output.len() != capacity {
        return Err("encoded table length disagrees".into());
    }
    Ok(output)
}

fn push_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}
