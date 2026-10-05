use super::*;
use beryl_home_store::{HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion};
use beryl_state::BerylState;

fn png() -> Vec<u8> {
    let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x02\0\0\0\x03\x08\x06\0\0\0".to_vec();
    let crc = png_header_crc(&bytes[12..29]);
    bytes.extend_from_slice(&crc.to_be_bytes());
    let mut image_data = vec![0x78, 0x01, 0x01, 27, 0, 0xe4, 0xff];
    image_data.extend_from_slice(&[0; 27]);
    image_data.extend_from_slice(&[0, 27, 0, 1]);
    for (kind, data) in [(b"IDAT", image_data.as_slice()), (b"IEND", &[][..])] {
        bytes.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let start = bytes.len();
        bytes.extend_from_slice(kind);
        bytes.extend_from_slice(data);
        let crc = png_header_crc(&bytes[start..]);
        bytes.extend_from_slice(&crc.to_be_bytes());
    }
    bytes
}

fn open(path: &std::path::Path) -> (HomeStore, AssetState) {
    let mut candidate =
        HomeOpenCandidate::open(HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT)).unwrap();
    let state = BerylState::register(&mut candidate).unwrap();
    let store = candidate
        .prepare_publication(BerylState::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    (store, state.assets())
}

#[test]
fn bounded_image_headers_reject_zero_dimensions_crc_and_excessive_jpeg_scan() {
    let bytes = png();
    let (kind, dimensions) = image_header(&bytes).unwrap();
    assert_eq!(kind, "image/png");
    assert_eq!(
        (dimensions.width().get(), dimensions.height().get()),
        (2, 3)
    );
    let mut corrupt = bytes.clone();
    corrupt[20] ^= 1;
    assert!(image_header(&corrupt).is_none());
    corrupt = bytes;
    corrupt[16..20].fill(0);
    let crc = png_header_crc(&corrupt[12..29]);
    corrupt[29..33].copy_from_slice(&crc.to_be_bytes());
    assert!(image_header(&corrupt).is_none());
    let mut jpeg = vec![0xff, 0xd8];
    for _ in 0..17000 {
        jpeg.extend_from_slice(&[0xff, 0xe0, 0, 2]);
    }
    jpeg.extend_from_slice(&[0xff, 0xc0, 0, 11, 8, 0, 3, 0, 2, 1, 1, 0x11, 0]);
    assert!(image_header(&jpeg).is_none());
    assert!(image_header(&[0xff, 0xd8, 0xff, 0xc0, 0, 11, 8, 0, 3, 0, 2, 1, 1, 0x11, 0]).is_some());
}

#[test]
fn image_admission_streams_metadata_and_reuses_one_exact_asset() {
    let directory = tempfile::tempdir().unwrap();
    let (store, assets) = open(directory.path());
    let bytes = png();
    let cancellation = CommandCancellation::new();
    let limit = NonZeroU64::new(1024).unwrap();
    let page = NonZeroUsize::new(7).unwrap();
    let asset =
        admit_clipboard_image(&store, assets.clone(), &bytes, limit, page, &cancellation).unwrap();
    let revision = assets.revision(&store).unwrap();
    let metadata = assets.metadata(&store, asset).unwrap().unwrap();
    assert_eq!(metadata.dimensions().unwrap().width().get(), 2);
    assert_eq!(metadata.media_type().as_str(), "image/png");
    assert_eq!(
        admit_clipboard_image(&store, assets.clone(), &bytes, limit, page, &cancellation).unwrap(),
        asset
    );
    assert_eq!(assets.revision(&store).unwrap(), revision);
}

#[test]
fn image_size_and_cancellation_refuse_before_metadata() {
    let directory = tempfile::tempdir().unwrap();
    let (store, assets) = open(directory.path());
    let revision = assets.revision(&store).unwrap();
    let bytes = png();
    let cancellation = CommandCancellation::new();
    let page = NonZeroUsize::new(7).unwrap();
    assert!(matches!(
        admit_clipboard_image(
            &store,
            assets.clone(),
            &bytes,
            NonZeroU64::new(1).unwrap(),
            page,
            &cancellation
        ),
        Err(ClipboardImageAdmissionError::TooLarge)
    ));
    cancellation.cancel();
    assert!(matches!(
        admit_clipboard_image(
            &store,
            assets.clone(),
            &bytes,
            NonZeroU64::new(1024).unwrap(),
            page,
            &cancellation
        ),
        Err(ClipboardImageAdmissionError::Cancelled)
    ));
    assert_eq!(assets.revision(&store).unwrap(), revision);
    assert!(!directory.path().join("sidecars").exists());
}
