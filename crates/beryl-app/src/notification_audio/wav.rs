use super::{Control, MAX_DECODED_BYTES, MAX_ENCODED_BYTES};
use std::{
    fs::File,
    io::{Cursor, Read},
    num::{NonZeroU16, NonZeroU32},
    path::Path,
    sync::Arc,
};

pub(crate) struct Samples {
    pub(crate) data: Vec<f32>,
    pub(crate) channels: NonZeroU16,
    pub(crate) rate: NonZeroU32,
}

struct Header {
    channels: u16,
    rate: u32,
    count: usize,
}

fn invalid() -> String {
    "unsupported, malformed or oversized notification WAV".into()
}
fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap())
}
fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn validate(bytes: &[u8], control: &Control) -> Result<Header, String> {
    if bytes.len() < 12
        || &bytes[..4] != b"RIFF"
        || &bytes[8..12] != b"WAVE"
        || u32_at(bytes, 4) as usize != bytes.len() - 8
    {
        return Err(invalid());
    }
    let mut offset = 12usize;
    let mut format = None;
    while offset < bytes.len() {
        control.check()?;
        let header = bytes
            .get(offset..offset.checked_add(8).ok_or_else(invalid)?)
            .ok_or_else(invalid)?;
        let len = u32_at(header, 4) as usize;
        offset += 8;
        let end = offset.checked_add(len).ok_or_else(invalid)?;
        let chunk = bytes.get(offset..end).ok_or_else(invalid)?;
        match &header[..4] {
            b"fmt " => {
                if format.is_some() || !matches!(len, 16 | 18) {
                    return Err(invalid());
                }
                let tag = u16_at(chunk, 0);
                let channels = u16_at(chunk, 2);
                let rate = u32_at(chunk, 4);
                let bits = u16_at(chunk, 14);
                let supported =
                    (tag == 1 && matches!(bits, 8 | 16 | 24 | 32)) || (tag == 3 && bits == 32);
                if !supported
                    || !(1..=2).contains(&channels)
                    || !(8_000..=192_000).contains(&rate)
                    || (len == 18 && (u16_at(chunk, 16) != 0 || (tag == 1 && bits == 32)))
                {
                    return Err(invalid());
                }
                let align = channels * (bits / 8);
                if u16_at(chunk, 12) != align || u32_at(chunk, 8) != rate * u32::from(align) {
                    return Err(invalid());
                }
                format = Some((channels, rate, bits, align));
            }
            b"data" => {
                let (channels, rate, bits, align) = format.ok_or_else(invalid)?;
                if end.checked_add(len % 2) != Some(bytes.len())
                    || len == 0
                    || len % usize::from(align) != 0
                {
                    return Err(invalid());
                }
                let count = len / usize::from(bits / 8);
                if count > MAX_DECODED_BYTES / size_of::<f32>()
                    || count as u64 > u64::from(rate) * u64::from(channels) * 30
                {
                    return Err(invalid());
                }
                return Ok(Header {
                    channels,
                    rate,
                    count,
                });
            }
            b"fact" if len != 4 => return Err(invalid()),
            _ if len % 2 != 0 => return Err(invalid()),
            _ => {}
        }
        offset = end;
    }
    Err(invalid())
}

pub(crate) fn acquire(path: &Path, control: &Arc<Control>) -> Result<Samples, String> {
    acquire_with_cut(path, control, |_| {})
}

pub(crate) fn acquire_with_cut(
    path: &Path,
    control: &Arc<Control>,
    mut cut: impl FnMut(&str),
) -> Result<Samples, String> {
    cut("before-open");
    control.check()?;
    if !super::ordinary_file_path(path) {
        return Err("unsupported notification sound path".into());
    }
    let mut file = File::open(path).map_err(|e| format!("sound open failed: {e}"))?;
    cut("after-open");
    control.check()?;
    let metadata = file
        .metadata()
        .map_err(|e| format!("sound metadata failed: {e}"))?;
    cut("after-metadata");
    control.check()?;
    if !metadata.is_file() || metadata.len() > MAX_ENCODED_BYTES as u64 {
        return Err(invalid());
    }
    let len = metadata.len() as usize;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(len)
        .map_err(|_| "sound encoded allocation unavailable")?;
    bytes.resize(len, 0);
    for block in bytes.chunks_mut(64 * 1024) {
        control.check()?;
        file.read_exact(block)
            .map_err(|e| format!("sound read failed: {e}"))?;
        cut("after-read");
        control.check()?;
    }
    let mut extra = [0u8; 1];
    control.check()?;
    if file
        .read(&mut extra)
        .map_err(|e| format!("sound read failed: {e}"))?
        != 0
    {
        return Err(invalid());
    }
    control.check()?;
    drop(file);
    cut("before-decode");
    decode(bytes, control)
}

pub(crate) fn decode(bytes: Vec<u8>, control: &Arc<Control>) -> Result<Samples, String> {
    control.check()?;
    if bytes.len() > MAX_ENCODED_BYTES {
        return Err(invalid());
    }
    let header = validate(&bytes, control)?;
    let mut decoder = rodio::Decoder::new_wav(Cursor::new(bytes))
        .map_err(|e| format!("sound decode failed: {e}"))?;
    let mut data = Vec::new();
    data.try_reserve_exact(header.count)
        .map_err(|_| "sound decoded allocation unavailable")?;
    for index in 0..header.count {
        if index % 4096 == 0 {
            control.check()?;
        }
        let sample = decoder.next().ok_or_else(invalid)?;
        if !sample.is_finite() {
            return Err(invalid());
        }
        data.push(sample.clamp(-1.0, 1.0));
    }
    control.check()?;
    if decoder.next().is_some() {
        return Err(invalid());
    }
    Ok(Samples {
        data,
        channels: NonZeroU16::new(header.channels).unwrap(),
        rate: NonZeroU32::new(header.rate).unwrap(),
    })
}
