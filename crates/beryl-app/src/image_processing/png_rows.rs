use beryl_home_store::CommandCancellation;
use std::io::{BufRead, Seek, SeekFrom};

use super::{
    ImagePreparationError as Error, MediaResourceLimits, PngPreflight, check_cancelled,
    zeroed_bytes,
};

pub(crate) fn decode_png_into<R: BufRead + Seek>(
    mut source: R,
    preflight: PngPreflight,
    limits: MediaResourceLimits,
    cancelled: &CommandCancellation,
    output: &mut [u8],
) -> Result<(), Error> {
    check_cancelled(cancelled)?;
    let config = limits.configuration();
    let expected_bytes = preflight.output_bytes()?;
    if output.len() != expected_bytes || expected_bytes > config.surface_max_bytes {
        return Err(Error::Capacity);
    }
    source.seek(SeekFrom::Start(0))?;
    let mut options = png::DecodeOptions::default();
    options.set_ignore_adler32(false);
    options.set_ignore_crc(false);
    options.set_skip_ancillary_crc_failures(false);
    options.set_ignore_text_chunk(true);
    options.set_ignore_iccp_chunk(true);
    let mut decoder = png::Decoder::new_with_options(source, options);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::ALPHA);
    decoder.set_limits(png::Limits {
        bytes: config.decoder_processed_chunk_max_bytes,
    });
    let mut reader = decoder.read_info()?;
    let info = reader.info();
    if [info.width, info.height] != preflight.source_extent
        || info.interlaced
        || info.bit_depth != png::BitDepth::Eight
        || !matches!(info.color_type, png::ColorType::Rgb | png::ColorType::Rgba)
        || info.animation_control.is_some()
    {
        return Err(Error::Malformed);
    }
    let [width, height] = preflight.source_extent;
    let [fitted_width, fitted_height] = preflight.fitted_extent;
    let line_bytes = (width as usize).checked_mul(4).ok_or(Error::Capacity)?;
    if line_bytes > config.decoder_row_max_bytes
        || reader.output_line_size(width) != Some(line_bytes)
    {
        return Err(Error::Capacity);
    }
    let mut row = zeroed_bytes(line_bytes)?;
    let mut fitted_y = 0u32;
    for source_y in 0..height {
        check_cancelled(cancelled)?;
        if reader.read_row(&mut row)?.is_none() {
            return Err(Error::Malformed);
        }
        while fitted_y < fitted_height
            && sample_coordinate(fitted_y, height, fitted_height)? == source_y
        {
            let output_start = (fitted_y as usize)
                .checked_mul(fitted_width as usize)
                .and_then(|pixels| pixels.checked_mul(4))
                .ok_or(Error::Capacity)?;
            for fitted_x in 0..fitted_width {
                let source_x = sample_coordinate(fitted_x, width, fitted_width)? as usize;
                let source_start = source_x.checked_mul(4).ok_or(Error::Capacity)?;
                let target_start = output_start
                    .checked_add((fitted_x as usize).checked_mul(4).ok_or(Error::Capacity)?)
                    .ok_or(Error::Capacity)?;
                output[target_start..target_start + 4]
                    .copy_from_slice(&row[source_start..source_start + 4]);
            }
            fitted_y += 1;
        }
    }
    check_cancelled(cancelled)?;
    if fitted_y != fitted_height || reader.read_row(&mut row)?.is_some() {
        return Err(Error::Malformed);
    }
    reader.finish()?;
    check_cancelled(cancelled)
}

fn sample_coordinate(position: u32, source: u32, target: u32) -> Result<u32, Error> {
    let numerator = u64::from(position)
        .checked_mul(2)
        .and_then(|position| position.checked_add(1))
        .and_then(|position| position.checked_mul(u64::from(source)))
        .ok_or(Error::Capacity)?;
    let denominator = u64::from(target)
        .checked_mul(2)
        .filter(|value| *value > 0)
        .ok_or(Error::Capacity)?;
    u32::try_from(numerator / denominator)
        .map(|value| value.min(source - 1))
        .map_err(|_| Error::Capacity)
}
