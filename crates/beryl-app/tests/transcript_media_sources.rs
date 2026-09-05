#[path = "support/tempdir.rs"]
mod tempdir_support;

use std::{fs, path::Path, time::Duration};

use beryl_model::workspace::WorkspaceId;
use gpui::ImageFormat;

#[path = "../src/shell/transcript_media.rs"]
mod transcript_media;

use transcript_media::{TranscriptMediaCache, TranscriptMediaCacheKey, TranscriptMediaSource};

#[test]
fn markdown_image_reads_the_direct_host_file_and_keeps_a_byte_backed_presentation() {
    let root = unique_temp_dir();
    let image_path = root.join("images/cat.png");
    write_png(&image_path, 3, 2);
    let workspace = WorkspaceId::host_windows(root.path());
    let source = TranscriptMediaSource::markdown_image("cat", "images/cat.png", None);
    let mut cache = TranscriptMediaCache::new(8);

    complete(
        &mut cache,
        cache_key("markdown"),
        source.clone(),
        workspace.clone(),
    );
    let ready = cache.lookup(cache_key("markdown"), source, workspace, timeout());
    let image = ready
        .outcome
        .loaded()
        .expect("directly readable Markdown image should load");

    assert_eq!(image.format(), ImageFormat::Png);
    assert_eq!(image.source_backed_file_path(), None);
    assert_eq!(image.action_file_path(), Some(&image_path));
    assert!(image.retained_bytes().is_some());
    assert_eq!(image.natural_dimensions().width(), 3);
    assert_eq!(image.natural_dimensions().height(), 2);
    assert_eq!(cache.stats().loaded_retained_byte_entries, 1);
    cleanup(root);
}

#[test]
fn markdown_image_rejects_outside_paths_and_marks_missing_direct_files_unavailable() {
    let root = unique_temp_dir();
    let workspace = WorkspaceId::host_windows(root.path());
    let mut cache = TranscriptMediaCache::new(8);
    let outside = TranscriptMediaSource::markdown_image("outside", r"C:\\other\\cat.png", None);
    complete(
        &mut cache,
        cache_key("outside"),
        outside.clone(),
        workspace.clone(),
    );
    assert_eq!(
        cache
            .lookup(cache_key("outside"), outside, workspace.clone(), timeout())
            .outcome
            .fallback_text()
            .as_deref(),
        Some("outside (path not allowed)")
    );

    let missing = TranscriptMediaSource::markdown_image("missing", "images/missing.png", None);
    complete(
        &mut cache,
        cache_key("missing"),
        missing.clone(),
        workspace.clone(),
    );
    assert_eq!(
        cache
            .lookup(cache_key("missing"), missing, workspace, timeout())
            .outcome
            .fallback_text()
            .as_deref(),
        Some("missing (file unavailable)")
    );
    cleanup(root);
}

#[test]
fn generated_image_uses_a_readable_saved_path_even_while_generating() {
    let root = unique_temp_dir();
    let image_path = root.join("generated.png");
    write_png(&image_path, 2, 1);
    let workspace = WorkspaceId::host_windows(root.path());
    let source = TranscriptMediaSource::native_image_generation(
        "generated_1",
        Some("a glass cat".to_string()),
        Some(image_path.display().to_string()),
        false,
    );
    let mut cache = TranscriptMediaCache::new(8);

    complete(
        &mut cache,
        cache_key("generated"),
        source.clone(),
        workspace.clone(),
    );
    let ready = cache.lookup(cache_key("generated"), source, workspace, timeout());
    let image = ready
        .outcome
        .loaded()
        .expect("saved generated source should load before completion");
    assert_eq!(image.source_backed_file_path(), Some(&image_path));
    assert_eq!(image.retained_bytes(), None);
    cleanup(root);
}

#[test]
fn direct_file_loading_rejects_oversized_compressed_sources_before_decoding() {
    let root = unique_temp_dir();
    let image_path = root.join("oversized.png");
    let file = fs::File::create(&image_path).unwrap();
    file.set_len((transcript_media::TRANSCRIPT_MEDIA_CACHE_MAX_COMPRESSED_IMAGE_BYTES + 1) as u64)
        .unwrap();
    let workspace = WorkspaceId::host_windows(root.path());
    let source = TranscriptMediaSource::native_image_generation(
        "oversized_generated",
        Some("oversized".to_string()),
        Some(image_path.display().to_string()),
        true,
    );
    let mut cache = TranscriptMediaCache::new(8);

    complete(
        &mut cache,
        cache_key("oversized"),
        source.clone(),
        workspace.clone(),
    );
    assert_eq!(
        cache
            .lookup(cache_key("oversized"), source, workspace, timeout())
            .outcome
            .fallback_text()
            .as_deref(),
        Some("oversized (image too large)")
    );
    cleanup(root);
}

#[test]
fn markdown_revalidation_reloads_changed_pixels_and_reports_deleted_files() {
    let root = unique_temp_dir();
    let image_path = root.join("images/cat.png");
    write_png_with_pixel(&image_path, 1, 1, [0, 0, 0, 255]);
    let workspace = WorkspaceId::host_windows(root.path());
    let source = TranscriptMediaSource::markdown_image("cat", "images/cat.png", None);
    let mut cache = TranscriptMediaCache::new_with_markdown_revalidate_after(8, Duration::ZERO);

    complete(
        &mut cache,
        cache_key("revalidate"),
        source.clone(),
        workspace.clone(),
    );
    let first_bytes = fs::read(&image_path).unwrap();
    write_png_with_pixel(&image_path, 1, 1, [255, 0, 0, 255]);
    let revalidation = cache.lookup(
        cache_key("revalidate"),
        source.clone(),
        workspace.clone(),
        timeout(),
    );
    assert!(revalidation.load_request.is_some());
    let completion = revalidation.load_request.unwrap().load();
    let refreshed = completion.loaded_image().unwrap();
    assert_eq!(refreshed.natural_dimensions().width(), 1);
    assert_eq!(refreshed.natural_dimensions().height(), 1);
    assert_ne!(refreshed.retained_bytes(), Some(first_bytes.as_slice()));
    assert!(cache.complete_load(completion).display_changed);

    fs::remove_file(&image_path).unwrap();
    let deleted = cache.lookup(
        cache_key("revalidate"),
        source.clone(),
        workspace.clone(),
        timeout(),
    );
    assert!(
        cache
            .complete_load(deleted.load_request.unwrap().load())
            .display_changed
    );
    assert_eq!(
        cache
            .lookup(cache_key("revalidate"), source, workspace, timeout())
            .outcome
            .fallback_text()
            .as_deref(),
        Some("cat (file unavailable)")
    );
    cleanup(root);
}

#[test]
fn direct_file_loading_rejects_malformed_and_pixel_oversized_rasters() {
    let root = unique_temp_dir();
    let malformed_path = root.join("malformed.png");
    let oversized_path = root.join("oversized.bmp");
    fs::write(&malformed_path, &[137, 80, 78, 71]).unwrap();
    fs::write(&oversized_path, oversized_bmp_header(6000, 6000)).unwrap();
    let workspace = WorkspaceId::host_windows(root.path());
    let malformed = TranscriptMediaSource::native_image_generation(
        "malformed",
        Some("malformed".to_string()),
        Some(malformed_path.display().to_string()),
        true,
    );
    let oversized = TranscriptMediaSource::native_image_generation(
        "oversized",
        Some("pixel oversized".to_string()),
        Some(oversized_path.display().to_string()),
        true,
    );
    let mut cache = TranscriptMediaCache::new(8);

    complete(
        &mut cache,
        cache_key("malformed"),
        malformed.clone(),
        workspace.clone(),
    );
    complete(
        &mut cache,
        cache_key("pixel-oversized"),
        oversized.clone(),
        workspace.clone(),
    );
    assert_eq!(
        cache
            .lookup(
                cache_key("malformed"),
                malformed,
                workspace.clone(),
                timeout()
            )
            .outcome
            .fallback_text()
            .as_deref(),
        Some("malformed (render not supported)")
    );
    assert_eq!(
        cache
            .lookup(
                cache_key("pixel-oversized"),
                oversized,
                workspace,
                timeout()
            )
            .outcome
            .fallback_text()
            .as_deref(),
        Some("pixel oversized (image too large)")
    );
    cleanup(root);
}

#[test]
fn stale_loads_cannot_replace_a_new_source_or_a_cleared_scope() {
    let root = unique_temp_dir();
    write_png(&root.join("images/old.png"), 1, 1);
    write_png(&root.join("images/new.png"), 2, 1);
    let workspace = WorkspaceId::host_windows(root.path());
    let old = TranscriptMediaSource::markdown_image("old", "images/old.png", None);
    let new = TranscriptMediaSource::markdown_image("new", "images/new.png", None);
    let mut cache = TranscriptMediaCache::new(8);
    let old_request = cache
        .lookup(cache_key("replacement"), old, workspace.clone(), timeout())
        .load_request
        .unwrap();
    let pending_new = cache.lookup(
        cache_key("replacement"),
        new.clone(),
        workspace.clone(),
        timeout(),
    );
    assert!(pending_new.load_request.is_none());
    let stale = cache.complete_load(old_request.load());
    assert!(stale.stale);
    let follow_up = stale.follow_up_request.unwrap();
    assert!(cache.complete_load(follow_up.load()).display_changed);
    assert_eq!(
        cache
            .lookup(cache_key("replacement"), new, workspace.clone(), timeout())
            .outcome
            .loaded()
            .unwrap()
            .alt(),
        "new"
    );

    let source = TranscriptMediaSource::markdown_image("cat", "images/old.png", None);
    let request = cache
        .lookup(cache_key("cleared"), source, workspace, timeout())
        .load_request
        .unwrap();
    cache.clear();
    assert!(cache.complete_load(request.load()).stale);
    cleanup(root);
}

#[test]
fn generated_image_without_a_saved_path_is_pending_then_unavailable() {
    let root = unique_temp_dir();
    let workspace = WorkspaceId::host_windows(root.path());
    let pending = TranscriptMediaSource::native_image_generation(
        "generated_pending",
        Some("pending cat".to_string()),
        None,
        false,
    );
    let complete_source = TranscriptMediaSource::native_image_generation(
        "generated_missing",
        Some("missing cat".to_string()),
        None,
        true,
    );
    let mut cache = TranscriptMediaCache::new(8);

    complete(
        &mut cache,
        cache_key("pending"),
        pending.clone(),
        workspace.clone(),
    );
    assert!(
        cache
            .lookup(cache_key("pending"), pending, workspace.clone(), timeout())
            .outcome
            .is_pending()
    );
    complete(
        &mut cache,
        cache_key("complete"),
        complete_source.clone(),
        workspace.clone(),
    );
    assert_eq!(
        cache
            .lookup(cache_key("complete"), complete_source, workspace, timeout())
            .outcome
            .fallback_text()
            .as_deref(),
        Some("missing cat (file unavailable)")
    );
    cleanup(root);
}

#[test]
fn generated_image_with_an_unreadable_or_unsupported_saved_path_is_unavailable_or_unsupported() {
    let root = unique_temp_dir();
    let workspace = WorkspaceId::host_windows(root.path());
    let missing_path = root.join("missing.png");
    let unsupported_path = root.join("generated.svg");
    fs::write(&unsupported_path, "<svg/>").unwrap();
    let mut cache = TranscriptMediaCache::new(8);
    let missing = TranscriptMediaSource::native_image_generation(
        "generated_missing_path",
        Some("missing".to_string()),
        Some(missing_path.display().to_string()),
        true,
    );
    let unsupported = TranscriptMediaSource::native_image_generation(
        "generated_unsupported_path",
        Some("unsupported".to_string()),
        Some(unsupported_path.display().to_string()),
        true,
    );
    complete(
        &mut cache,
        cache_key("missing-path"),
        missing.clone(),
        workspace.clone(),
    );
    complete(
        &mut cache,
        cache_key("unsupported-path"),
        unsupported.clone(),
        workspace.clone(),
    );
    assert_eq!(
        cache
            .lookup(
                cache_key("missing-path"),
                missing,
                workspace.clone(),
                timeout()
            )
            .outcome
            .fallback_text()
            .as_deref(),
        Some("missing (file unavailable)")
    );
    assert_eq!(
        cache
            .lookup(
                cache_key("unsupported-path"),
                unsupported,
                workspace,
                timeout()
            )
            .outcome
            .fallback_text()
            .as_deref(),
        Some("unsupported (render not supported)")
    );
    cleanup(root);
}

#[test]
fn empty_markdown_alt_keeps_path_and_file_fallbacks() {
    let root = unique_temp_dir();
    let workspace = WorkspaceId::host_windows(root.path());
    let rejected = TranscriptMediaSource::markdown_image("", r"C:\\other\\cat.png", None);
    let unavailable = TranscriptMediaSource::markdown_image("", "images/missing.png", None);
    let mut cache = TranscriptMediaCache::new(8);

    complete(
        &mut cache,
        cache_key("empty-rejected"),
        rejected.clone(),
        workspace.clone(),
    );
    complete(
        &mut cache,
        cache_key("empty-unavailable"),
        unavailable.clone(),
        workspace.clone(),
    );
    assert_eq!(
        cache
            .lookup(
                cache_key("empty-rejected"),
                rejected,
                workspace.clone(),
                timeout()
            )
            .outcome
            .fallback_text()
            .as_deref(),
        Some("(path not allowed)")
    );
    assert_eq!(
        cache
            .lookup(
                cache_key("empty-unavailable"),
                unavailable,
                workspace,
                timeout()
            )
            .outcome
            .fallback_text()
            .as_deref(),
        Some("(file unavailable)")
    );
    cleanup(root);
}

fn complete(
    cache: &mut TranscriptMediaCache,
    key: TranscriptMediaCacheKey,
    source: TranscriptMediaSource,
    workspace: WorkspaceId,
) {
    let request = cache
        .lookup(key, source, workspace, timeout())
        .load_request
        .unwrap();
    assert!(cache.complete_load(request.load()).display_changed);
}

fn cache_key(value: &str) -> TranscriptMediaCacheKey {
    TranscriptMediaCacheKey::new(value)
}

fn timeout() -> Duration {
    Duration::from_secs(1)
}

fn write_png(path: &Path, width: u32, height: u32) {
    write_png_with_pixel(path, width, height, [0, 0, 0, 255]);
}

fn write_png_with_pixel(path: &Path, width: u32, height: u32, pixel: [u8; 4]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let image = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        width,
        height,
        image::Rgba(pixel),
    ));
    image.save(path).unwrap();
}

fn oversized_bmp_header(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = vec![0_u8; 54];
    bytes[0] = b'B';
    bytes[1] = b'M';
    bytes[2..6].copy_from_slice(&(54_u32).to_le_bytes());
    bytes[10..14].copy_from_slice(&(54_u32).to_le_bytes());
    bytes[14..18].copy_from_slice(&(40_u32).to_le_bytes());
    bytes[18..22].copy_from_slice(&(width as i32).to_le_bytes());
    bytes[22..26].copy_from_slice(&(height as i32).to_le_bytes());
    bytes[26..28].copy_from_slice(&(1_u16).to_le_bytes());
    bytes[28..30].copy_from_slice(&(32_u16).to_le_bytes());
    bytes
}

fn unique_temp_dir() -> tempdir_support::TestTempDir {
    tempdir_support::temp_dir("beryl-transcript-media-sources-test-")
}

fn cleanup(root: tempdir_support::TestTempDir) {
    root.close().unwrap();
}
