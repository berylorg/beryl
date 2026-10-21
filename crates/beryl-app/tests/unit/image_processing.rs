use beryl_home_store::CommandCancellation;
use std::{
    cell::Cell,
    io::{Cursor, Read, Seek, SeekFrom},
    num::NonZeroU64,
    rc::Rc,
};

use beryl_model::AssetId;
use sha2::{Digest, Sha256};

use super::*;

fn limits() -> MediaResourceLimits {
    MediaResourceLimits::new(MediaResourceConfiguration::default()).unwrap()
}

fn cancelled_signal() -> CommandCancellation {
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    cancellation
}

fn identity(bytes: &[u8]) -> AssetId {
    AssetId::sha256_v1(
        Sha256::digest(bytes).into(),
        NonZeroU64::new(bytes.len() as u64).unwrap(),
    )
}

fn image(width: u32, height: u32, color: png::ColorType) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, width, height);
    encoder.set_color(color);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    let channels = color.samples();
    let pixels: Vec<_> = (0..width as usize * height as usize * channels)
        .map(|index| (index % 251) as u8)
        .collect();
    writer.write_image_data(&pixels).unwrap();
    writer.finish().unwrap();
    bytes
}

fn admit(bytes: &[u8], extent: [u32; 2]) -> Result<PngPreflight, ImagePreparationError> {
    preflight_png(
        &mut Cursor::new(bytes),
        identity(bytes),
        extent,
        limits(),
        &CommandCancellation::new(),
    )
}

fn insert_chunk(bytes: &mut Vec<u8>, kind: [u8; 4], payload: &[u8]) {
    let mut chunk = Vec::new();
    chunk.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    chunk.extend_from_slice(&kind);
    chunk.extend_from_slice(payload);
    let mut crc = crc32fast::Hasher::new();
    crc.update(&kind);
    crc.update(payload);
    chunk.extend_from_slice(&crc.finalize().to_be_bytes());
    bytes.splice(33..33, chunk);
}

#[test]
fn rgba_original_decodes_to_exact_admitted_pixels() {
    let bytes = image(3, 2, png::ColorType::Rgba);
    let prepared = admit(&bytes, [3, 2]).unwrap();
    let mut output = zeroed_bytes(prepared.output_bytes().unwrap()).unwrap();
    decode_png_into(
        Cursor::new(bytes),
        prepared,
        limits(),
        &CommandCancellation::new(),
        &mut output,
    )
    .unwrap();
    assert_eq!(output, (0..24).map(|index| index as u8).collect::<Vec<_>>());
}

#[test]
fn rgb_rows_expand_alpha_without_changing_colour_samples() {
    let bytes = image(2, 1, png::ColorType::Rgb);
    let prepared = admit(&bytes, [2, 1]).unwrap();
    let mut output = zeroed_bytes(prepared.output_bytes().unwrap()).unwrap();
    decode_png_into(
        Cursor::new(bytes),
        prepared,
        limits(),
        &CommandCancellation::new(),
        &mut output,
    )
    .unwrap();
    assert_eq!(output, [0, 1, 2, 255, 3, 4, 5, 255]);
}

#[test]
fn source_height_does_not_require_a_full_original_output_or_read() {
    let bytes = image(64, 16_384, png::ColorType::Rgba);
    let greatest_read = Rc::new(Cell::new(0));
    let source = CountedSource {
        inner: Cursor::new(bytes.clone()),
        greatest_read: greatest_read.clone(),
    };
    let cancelled = CommandCancellation::new();
    let mut source = super::test_input::PagedImageInput::new(source, 4_096, &cancelled).unwrap();
    let prepared = preflight_png(
        &mut source,
        identity(&bytes),
        [64, 16_384],
        limits(),
        &cancelled,
    )
    .unwrap();
    assert_eq!(prepared.fitted_extent(), [4, 1_024]);
    let mut output = zeroed_bytes(prepared.output_bytes().unwrap()).unwrap();
    decode_png_into(
        source,
        prepared,
        limits(),
        &CommandCancellation::new(),
        &mut output,
    )
    .unwrap();
    assert_eq!(output.len(), 16_384);
    assert!(greatest_read.get() <= 4_096);
    for y in 0..1_024usize {
        for x in 0..4usize {
            let source_x = (2 * x + 1) * 64 / 8;
            let source_y = (2 * y + 1) * 16_384 / 2_048;
            let source_start = (source_y * 64 + source_x) * 4;
            let output_start = (y * 4 + x) * 4;
            assert_eq!(
                &output[output_start..output_start + 4],
                &(0..4)
                    .map(|channel| ((source_start + channel) % 251) as u8)
                    .collect::<Vec<_>>()
            );
        }
    }
}

#[test]
fn exact_identity_and_metadata_extent_are_required() {
    let bytes = image(2, 2, png::ColorType::Rgba);
    assert!(matches!(
        admit(&bytes, [1, 2]),
        Err(ImagePreparationError::Identity)
    ));
    let wrong = AssetId::sha256_v1([0; 32], NonZeroU64::new(bytes.len() as u64).unwrap());
    assert!(matches!(
        preflight_png(
            &mut Cursor::new(&bytes),
            wrong,
            [2, 2],
            limits(),
            &CommandCancellation::new()
        ),
        Err(ImagePreparationError::Identity)
    ));
}

#[test]
fn every_chunk_crc_is_checked_even_for_ignored_text() {
    let mut bytes = image(1, 1, png::ColorType::Rgba);
    insert_chunk(&mut bytes, *b"tEXt", b"name\0text");
    bytes[45] ^= 1;
    assert!(matches!(
        admit(&bytes, [1, 1]),
        Err(ImagePreparationError::Malformed)
    ));
}

#[test]
fn physical_trailing_bytes_are_not_hidden_by_iend() {
    let mut bytes = image(1, 1, png::ColorType::Rgba);
    bytes.push(0);
    assert!(matches!(
        admit(&bytes, [1, 1]),
        Err(ImagePreparationError::Identity)
    ));
}

#[test]
fn unsupported_formats_never_allocate_an_original_output() {
    let bytes = image(2, 2, png::ColorType::Grayscale);
    assert!(matches!(
        admit(&bytes, [2, 2]),
        Err(ImagePreparationError::Unsupported)
    ));
    for kind in [*b"acTL", *b"fcTL", *b"fdAT", *b"eXIf", *b"ABCD"] {
        let mut bytes = image(1, 1, png::ColorType::Rgba);
        insert_chunk(&mut bytes, kind, &[]);
        assert!(
            matches!(
                admit(&bytes, [1, 1]),
                Err(ImagePreparationError::Unsupported)
            ),
            "{kind:?}"
        );
    }
}

#[test]
fn malformed_and_truncated_chunk_framing_never_prepares_pixels() {
    let bytes = image(2, 2, png::ColorType::Rgba);
    for length in [0, 7, 8, 20, 32, bytes.len() - 1] {
        let source = &bytes[..length];
        if source.is_empty() {
            continue;
        }
        assert!(admit(source, [2, 2]).is_err());
    }
    let mut huge = bytes;
    huge[8..12].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(matches!(
        admit(&huge, [2, 2]),
        Err(ImagePreparationError::Malformed)
    ));
}

#[test]
fn source_and_row_capacity_refuse_before_decode() {
    let bytes = image(2, 2, png::ColorType::Rgba);
    let mut config = MediaResourceConfiguration::default();
    config.rendition_source_max_bytes = bytes.len() - 1;
    let bounded = MediaResourceLimits::new(config).unwrap();
    assert!(matches!(
        preflight_png(
            &mut Cursor::new(&bytes),
            identity(&bytes),
            [2, 2],
            bounded,
            &CommandCancellation::new()
        ),
        Err(ImagePreparationError::Capacity)
    ));
    config = MediaResourceConfiguration::default();
    config.decoder_row_max_bytes = 8;
    let bounded = MediaResourceLimits::new(config).unwrap();
    assert!(matches!(
        preflight_png(
            &mut Cursor::new(&bytes),
            identity(&bytes),
            [2, 2],
            bounded,
            &CommandCancellation::new()
        ),
        Err(ImagePreparationError::Capacity)
    ));
}

#[test]
fn preflight_and_decode_cancel_without_accepting_partial_output() {
    let bytes = image(2, 2, png::ColorType::Rgba);
    assert!(matches!(
        preflight_png(
            &mut Cursor::new(&bytes),
            identity(&bytes),
            [2, 2],
            limits(),
            &cancelled_signal()
        ),
        Err(ImagePreparationError::Cancelled)
    ));
    let prepared = admit(&bytes, [2, 2]).unwrap();
    let mut output = vec![17; prepared.output_bytes().unwrap()];
    assert!(matches!(
        decode_png_into(
            Cursor::new(bytes),
            prepared,
            limits(),
            &cancelled_signal(),
            &mut output
        ),
        Err(ImagePreparationError::Cancelled)
    ));
    assert!(output.iter().all(|byte| *byte == 17));
}

#[test]
fn output_capacity_is_exact_and_configuration_rejects_invalid_limits() {
    let bytes = image(2, 2, png::ColorType::Rgba);
    let prepared = admit(&bytes, [2, 2]).unwrap();
    assert!(matches!(
        decode_png_into(
            Cursor::new(bytes),
            prepared,
            limits(),
            &CommandCancellation::new(),
            &mut [0; 15]
        ),
        Err(ImagePreparationError::Capacity)
    ));
    let mut config = MediaResourceConfiguration::default();
    config.decoder_slots = 0;
    assert!(matches!(
        MediaResourceLimits::new(config),
        Err(ImagePreparationError::Configuration)
    ));
    config = MediaResourceConfiguration::default();
    config.rendition_max_dimension = usize::MAX;
    assert!(matches!(
        MediaResourceLimits::new(config),
        Err(ImagePreparationError::Configuration)
    ));
    config = MediaResourceConfiguration::default();
    config.cpu_surface_bytes = config.surface_max_bytes - 1;
    assert!(matches!(
        MediaResourceLimits::new(config),
        Err(ImagePreparationError::Configuration)
    ));
}

struct CountedSource {
    inner: Cursor<Vec<u8>>,
    greatest_read: Rc<Cell<usize>>,
}

fn surface_key(attachment: u64, digest: u8) -> super::surfaces::RenditionIdentity {
    super::surfaces::RenditionIdentity {
        attachment,
        asset: AssetId::sha256_v1([digest; 32], NonZeroU64::MIN),
        extent: [1, 1],
    }
}

fn surface_cache(entries: usize) -> super::surfaces::CpuSurfaceCache {
    let config = MediaResourceConfiguration {
        rendition_max_dimension: 1,
        surface_max_bytes: 4,
        cpu_surface_entries: entries,
        cpu_surface_bytes: entries * 4,
        ..MediaResourceConfiguration::default()
    };
    super::surfaces::CpuSurfaceCache::new(MediaResourceLimits::new(config).unwrap())
}

#[test]
fn unpublished_and_published_surfaces_transfer_one_allocation_charge() {
    let cache = surface_cache(1);
    let key = surface_key(1, 4);
    let mut buffer = cache.reserve(key).unwrap();
    assert_eq!(cache.diagnostics().unwrap().entries, 1);
    assert_eq!(cache.diagnostics().unwrap().bytes, 4);
    assert!(matches!(
        cache.reserve(surface_key(1, 5)),
        Err(ImagePreparationError::Capacity)
    ));
    buffer.pixels_mut().copy_from_slice(&[1, 2, 3, 4]);
    let surface = cache.publish(buffer).unwrap();
    assert_eq!(surface.pixels(), [1, 2, 3, 4]);
    assert_eq!(cache.diagnostics().unwrap().entries, 1);
    assert_eq!(cache.diagnostics().unwrap().bytes, 4);
    cache.retire_attachment(1);
    assert_eq!(cache.diagnostics().unwrap().bytes, 4);
    drop(surface);
    assert_eq!(cache.diagnostics().unwrap().bytes, 0);
    assert_eq!(cache.diagnostics().unwrap().entries, 0);
}

#[test]
fn cpu_lru_preserves_pinned_peers_and_evicts_only_an_unpinned_surface() {
    let cache = surface_cache(2);
    let first = surface_key(1, 1);
    let second = surface_key(1, 2);
    let third = surface_key(1, 3);
    let pinned = cache.publish(cache.reserve(first).unwrap()).unwrap();
    drop(cache.publish(cache.reserve(second).unwrap()).unwrap());
    drop(cache.publish(cache.reserve(third).unwrap()).unwrap());
    assert!(cache.get(second).unwrap().is_none());
    assert!(cache.get(first).unwrap().is_some());
    assert!(cache.get(third).unwrap().is_some());
    assert_eq!(cache.diagnostics().unwrap().evictions, 1);
    assert_eq!(pinned.identity(), first);
    let also_pinned = cache.get(third).unwrap().unwrap();
    assert!(matches!(
        cache.reserve(surface_key(1, 9)),
        Err(ImagePreparationError::Capacity)
    ));
    drop(also_pinned);
}

#[test]
fn dropped_unpublished_surface_releases_exact_count_and_bytes() {
    let cache = surface_cache(1);
    drop(cache.reserve(surface_key(1, 1)).unwrap());
    assert_eq!(
        cache.diagnostics().unwrap(),
        super::surfaces::CpuSurfaceDiagnostics::default()
    );
    assert!(cache.reserve(surface_key(2, 1)).is_ok());
}

#[test]
fn native_rendition_keys_preserve_full_asset_and_attachment_identity() {
    let key = surface_key(1, 1);
    assert_eq!(key.native_key(), key.native_key());
    assert_ne!(key.native_key(), surface_key(2, 1).native_key());
    assert_ne!(key.native_key(), surface_key(1, 2).native_key());
    let changed_length = super::surfaces::RenditionIdentity {
        asset: AssetId::sha256_v1(key.asset.digest(), NonZeroU64::new(2).unwrap()),
        ..key
    };
    assert_ne!(key.native_key(), changed_length.native_key());
}

impl Read for CountedSource {
    fn read(&mut self, destination: &mut [u8]) -> std::io::Result<usize> {
        self.greatest_read
            .set(self.greatest_read.get().max(destination.len()));
        self.inner.read(destination)
    }
}

impl Seek for CountedSource {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        self.inner.seek(position)
    }
}
