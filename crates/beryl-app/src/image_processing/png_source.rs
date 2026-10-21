use beryl_home_store::CommandCancellation;
use std::io::{BufRead, Seek, SeekFrom};

use beryl_model::AssetId;
use sha2::{Digest, Sha256};

use super::{ImagePreparationError as Error, MediaResourceLimits, check_cancelled};

#[derive(Clone, Copy, Debug)]
pub(crate) struct PngPreflight {
    pub(super) source_extent: [u32; 2],
    pub(super) fitted_extent: [u32; 2],
    pub(super) source_channels: usize,
}

impl PngPreflight {
    pub(crate) fn fitted_extent(self) -> [u32; 2] {
        self.fitted_extent
    }

    pub(crate) fn output_bytes(self) -> Result<usize, Error> {
        (self.fitted_extent[0] as usize)
            .checked_mul(self.fitted_extent[1] as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or(Error::Capacity)
    }
}

pub(crate) fn preflight_png<R: BufRead + Seek>(
    source: &mut R,
    asset: AssetId,
    expected_extent: [u32; 2],
    limits: MediaResourceLimits,
    cancelled: &CommandCancellation,
) -> Result<PngPreflight, Error> {
    let config = limits.configuration();
    if asset.length().get() > config.rendition_source_max_bytes as u64 {
        return Err(Error::Capacity);
    }
    check_cancelled(cancelled)?;
    source.seek(SeekFrom::Start(0))?;
    let mut input = CheckedPngInput {
        source,
        cancelled,
        page_bytes: config.sidecar_io_page_bytes,
        digest: Sha256::new(),
        remaining: asset.length().get(),
    };
    if input.fixed::<8>()? != *b"\x89PNG\r\n\x1a\n" {
        return Err(Error::Unsupported);
    }
    let mut preflight = None;
    let mut saw_data = false;
    let mut ended_data = false;
    let mut saw_palette = false;
    let mut saw_transparency = false;
    loop {
        let header = input.fixed::<8>()?;
        let length =
            u32::from_be_bytes(header[..4].try_into().map_err(|_| Error::Malformed)?) as usize;
        let kind: [u8; 4] = header[4..].try_into().map_err(|_| Error::Malformed)?;
        if !kind.iter().all(u8::is_ascii_alphabetic) || kind[2] & 0x20 != 0 {
            return Err(Error::Malformed);
        }
        if u64::try_from(length)
            .map_err(|_| Error::Capacity)?
            .checked_add(4)
            .is_none_or(|required| required > input.remaining)
        {
            return Err(Error::Malformed);
        }
        if preflight.is_none() && kind != *b"IHDR" {
            return Err(Error::Malformed);
        }
        let mut crc = crc32fast::Hasher::new();
        crc.update(&kind);
        match &kind {
            b"IHDR" => {
                if preflight.is_some() || length != 13 {
                    return Err(Error::Malformed);
                }
                let bytes = input.fixed::<13>()?;
                crc.update(&bytes);
                let width =
                    u32::from_be_bytes(bytes[..4].try_into().map_err(|_| Error::Malformed)?);
                let height =
                    u32::from_be_bytes(bytes[4..8].try_into().map_err(|_| Error::Malformed)?);
                if width == 0 || height == 0 {
                    return Err(Error::Malformed);
                }
                if [width, height] != expected_extent {
                    return Err(Error::Identity);
                }
                let channels = match (bytes[8], bytes[9], bytes[10], bytes[11], bytes[12]) {
                    (8, 2, 0, 0, 0) => 3,
                    (8, 6, 0, 0, 0) => 4,
                    _ => return Err(Error::Unsupported),
                };
                let row_bytes = (width as usize)
                    .checked_mul(channels)
                    .and_then(|bytes| bytes.checked_add(1))
                    .ok_or(Error::Capacity)?;
                let pixels = u64::from(width)
                    .checked_mul(u64::from(height))
                    .ok_or(Error::Capacity)?;
                if width.max(height) as usize > config.rendition_source_max_dimension
                    || pixels > config.rendition_source_max_pixels as u64
                    || row_bytes > config.decoder_row_max_bytes
                    || (width as usize)
                        .checked_mul(4)
                        .is_none_or(|row| row > config.decoder_row_max_bytes)
                {
                    return Err(Error::Capacity);
                }
                preflight = Some(PngPreflight {
                    source_extent: [width, height],
                    fitted_extent: limits.fitted_extent(width, height)?,
                    source_channels: channels,
                });
            }
            b"IDAT" => {
                if ended_data {
                    return Err(Error::Malformed);
                }
                saw_data = true;
                input.chunk(length, &mut crc)?;
            }
            b"IEND" => {
                if !saw_data || length != 0 {
                    return Err(Error::Malformed);
                }
            }
            b"PLTE" => {
                if saw_data || saw_palette || length == 0 || length > 768 || length % 3 != 0 {
                    return Err(Error::Malformed);
                }
                saw_palette = true;
                input.chunk(length, &mut crc)?;
            }
            b"tRNS" => {
                if saw_data
                    || saw_transparency
                    || length != 6
                    || preflight
                        .as_ref()
                        .is_none_or(|source| source.source_channels != 3)
                {
                    return Err(Error::Malformed);
                }
                saw_transparency = true;
                input.chunk(length, &mut crc)?;
            }
            b"acTL" | b"fcTL" | b"fdAT" | b"eXIf" => return Err(Error::Unsupported),
            _ => {
                if kind[0] & 0x20 == 0 {
                    return Err(Error::Unsupported);
                }
                if !matches!(&kind, b"tEXt" | b"zTXt" | b"iTXt" | b"iCCP")
                    && length > config.decoder_processed_chunk_max_bytes
                {
                    return Err(Error::Capacity);
                }
                input.chunk(length, &mut crc)?;
            }
        }
        let recorded_crc = u32::from_be_bytes(input.fixed::<4>()?);
        if recorded_crc != crc.finalize() {
            return Err(Error::Malformed);
        }
        if kind == *b"IEND" {
            break;
        }
        if saw_data && kind != *b"IDAT" {
            ended_data = true;
        }
    }
    check_cancelled(cancelled)?;
    if input.remaining != 0 {
        return Err(Error::Identity);
    }
    let mut trailing = [0];
    if input.source.read(&mut trailing)? != 0 {
        return Err(Error::Identity);
    }
    if input.digest.finalize().as_slice() != asset.digest() {
        return Err(Error::Identity);
    }
    input.source.seek(SeekFrom::Start(0))?;
    preflight.ok_or(Error::Malformed)
}

struct CheckedPngInput<'a, R> {
    source: &'a mut R,
    cancelled: &'a CommandCancellation,
    page_bytes: usize,
    digest: Sha256,
    remaining: u64,
}

impl<R: BufRead> CheckedPngInput<'_, R> {
    fn fixed<const N: usize>(&mut self) -> Result<[u8; N], Error> {
        check_cancelled(self.cancelled)?;
        if N as u64 > self.remaining {
            return Err(Error::Malformed);
        }
        let mut bytes = [0; N];
        self.source.read_exact(&mut bytes)?;
        self.digest.update(bytes);
        self.remaining -= N as u64;
        Ok(bytes)
    }

    fn chunk(&mut self, mut remaining: usize, crc: &mut crc32fast::Hasher) -> Result<(), Error> {
        while remaining > 0 {
            check_cancelled(self.cancelled)?;
            let available = self.source.fill_buf()?;
            let count = available.len().min(self.page_bytes).min(remaining);
            if count == 0 {
                return Err(Error::Malformed);
            }
            if count as u64 > self.remaining {
                return Err(Error::Malformed);
            }
            crc.update(&available[..count]);
            self.digest.update(&available[..count]);
            self.source.consume(count);
            self.remaining -= count as u64;
            remaining -= count;
        }
        Ok(())
    }
}
