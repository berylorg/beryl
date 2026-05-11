use std::{collections::HashMap, sync::Arc, time::Duration};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use beryl_model::workspace::WorkspaceId;
use gpui::ImageFormat;

#[path = "../src/shell/transcript_media.rs"]
mod transcript_media;

use transcript_media::{
    TranscriptMediaCache, TranscriptMediaCacheKey, TranscriptMediaFileReader, TranscriptMediaSource,
};

#[test]
fn markdown_png_target_resolves_relative_to_thread_execution_target() {
    let workspace = host_workspace();
    let source = TranscriptMediaSource::markdown_image("cat", "images/cat.png", None);
    let expected_path = r"c:\work\member\images\cat.png";
    let bytes = png_bytes();
    let mut reader = FakeReader::with_file(expected_path, bytes.clone());
    let mut cache = TranscriptMediaCache::new(8);

    let lookup = cache.lookup(
        cache_key("cat"),
        source.clone(),
        workspace.clone(),
        timeout(),
    );
    assert!(lookup.outcome.is_pending());
    let completion = lookup.load_request.unwrap().load(&mut reader);

    let result = cache.complete_load(completion);
    assert!(result.display_changed);
    assert!(!result.stale);
    assert_eq!(reader.calls, vec![expected_path.to_string()]);

    let ready = cache.lookup(cache_key("cat"), source, workspace, timeout());
    let image = ready.outcome.loaded().expect("relative PNG should load");
    assert_eq!(image.alt(), "cat");
    assert_eq!(image.format(), ImageFormat::Png);
    assert_eq!(image.bytes(), bytes.as_slice());
    assert_eq!(image.natural_dimensions().width(), 1);
    assert_eq!(image.natural_dimensions().height(), 1);
    assert_eq!(image.source_path(), Some(expected_path));
    assert_eq!(image.image().format(), ImageFormat::Png);
    assert!(ready.load_request.is_none());
}

#[test]
fn loaded_image_records_decoded_natural_dimensions() {
    let path = r"c:\work\member\images\wide.png";
    let bytes = png_bytes_with_dimensions(3, 2, [0, 0, 0, 255]);
    let source = TranscriptMediaSource::markdown_image("wide", "images/wide.png", None);
    let mut reader = FakeReader::with_file(path, bytes);
    let mut cache = TranscriptMediaCache::new(8);

    let lookup = cache.lookup(
        cache_key("wide"),
        source.clone(),
        host_workspace(),
        timeout(),
    );
    assert!(
        cache
            .complete_load(lookup.load_request.unwrap().load(&mut reader))
            .display_changed
    );
    let ready = cache.lookup(cache_key("wide"), source, host_workspace(), timeout());
    let image = ready.outcome.loaded().expect("wide PNG should load");

    assert_eq!(image.natural_dimensions().width(), 3);
    assert_eq!(image.natural_dimensions().height(), 2);
}

#[test]
fn media_cache_stats_report_loaded_image_bytes_and_decoded_estimate() {
    let path = r"c:\work\member\images\wide.png";
    let bytes = png_bytes_with_dimensions(3, 2, [0, 0, 0, 255]);
    let source = TranscriptMediaSource::markdown_image("wide", "images/wide.png", None);
    let mut reader = FakeReader::with_file(path, bytes.clone());
    let mut cache = TranscriptMediaCache::new(8);

    let lookup = cache.lookup(
        cache_key("wide-stats"),
        source.clone(),
        host_workspace(),
        timeout(),
    );
    assert!(
        cache
            .complete_load(lookup.load_request.unwrap().load(&mut reader))
            .display_changed
    );
    let _ready = cache.lookup(cache_key("wide-stats"), source, host_workspace(), timeout());

    let stats = cache.stats();
    assert_eq!(stats.entries, 1);
    assert_eq!(stats.pending_entries, 0);
    assert_eq!(stats.loaded_entries, 1);
    assert_eq!(stats.loaded_image_bytes, bytes.len());
    assert_eq!(stats.decoded_image_bytes_estimate, 3 * 2 * 4);
    assert_eq!(stats.thumbnail_count, 0);
}

#[test]
fn absolute_path_outside_bound_member_is_rejected_before_reading() {
    let source = TranscriptMediaSource::markdown_image("outside", r"C:\other\cat.png", None);
    let mut reader = FakeReader::default();
    let mut cache = TranscriptMediaCache::new(8);

    let lookup = cache.lookup(cache_key("outside"), source, host_workspace(), timeout());
    let result = lookup.load_request.unwrap().load(&mut reader);
    assert!(cache.complete_load(result).display_changed);

    let ready = cache.lookup(
        cache_key("outside"),
        TranscriptMediaSource::markdown_image("outside", r"C:\other\cat.png", None),
        host_workspace(),
        timeout(),
    );
    assert_eq!(
        ready.outcome.fallback_text().as_deref(),
        Some("outside (path not allowed)")
    );
    assert!(reader.calls.is_empty());
}

#[test]
fn missing_or_unreadable_markdown_file_renders_unavailable_fallback() {
    let source = TranscriptMediaSource::markdown_image("missing", "images/missing.png", None);
    let mut reader = FakeReader::default();
    let mut cache = TranscriptMediaCache::new(8);

    let lookup = cache.lookup(
        cache_key("missing"),
        source.clone(),
        host_workspace(),
        timeout(),
    );
    let completion = lookup.load_request.unwrap().load(&mut reader);
    assert!(cache.complete_load(completion).display_changed);

    let ready = cache.lookup(cache_key("missing"), source, host_workspace(), timeout());
    assert_eq!(
        ready.outcome.fallback_text().as_deref(),
        Some("missing (file unavailable)")
    );
    assert_eq!(
        reader.calls,
        vec![r"c:\work\member\images\missing.png".to_string()]
    );
}

#[test]
fn svg_and_non_image_markdown_targets_render_unsupported_without_reading() {
    let mut reader = FakeReader::default();
    let mut cache = TranscriptMediaCache::new(8);

    let svg = TranscriptMediaSource::markdown_image("vector", "images/vector.svg", None);
    let svg_lookup = cache.lookup(cache_key("svg"), svg.clone(), host_workspace(), timeout());
    assert!(
        cache
            .complete_load(svg_lookup.load_request.unwrap().load(&mut reader))
            .display_changed
    );
    let ready_svg = cache.lookup(cache_key("svg"), svg, host_workspace(), timeout());
    assert_eq!(
        ready_svg.outcome.fallback_text().as_deref(),
        Some("vector (render not supported)")
    );

    let text = TranscriptMediaSource::markdown_image("notes", "notes/readme.txt", None);
    let text_lookup = cache.lookup(cache_key("text"), text.clone(), host_workspace(), timeout());
    assert!(
        cache
            .complete_load(text_lookup.load_request.unwrap().load(&mut reader))
            .display_changed
    );
    let ready_text = cache.lookup(cache_key("text"), text, host_workspace(), timeout());
    assert_eq!(
        ready_text.outcome.fallback_text().as_deref(),
        Some("notes (render not supported)")
    );
    assert!(reader.calls.is_empty());
}

#[test]
fn native_generated_image_prefers_saved_path_over_inline_result() {
    let inline_bytes = png_bytes_with_pixel([255, 0, 0, 255]);
    let saved_bytes = png_bytes_with_pixel([0, 0, 255, 255]);
    let path = r"C:\outside\fresh.png";
    let source = TranscriptMediaSource::native_image_generation(
        "image_generation_1",
        Some("Cheshire cat".to_string()),
        Some(Arc::new(BASE64_STANDARD.encode(&inline_bytes))),
        Some(path.to_string()),
        true,
    );
    let mut reader = FakeReader::with_file(path, saved_bytes.clone());
    let mut cache = TranscriptMediaCache::new(8);

    let lookup = cache.lookup(
        cache_key("native"),
        source.clone(),
        host_workspace(),
        timeout(),
    );
    assert!(
        cache
            .complete_load(lookup.load_request.unwrap().load(&mut reader))
            .display_changed
    );
    let ready = cache.lookup(cache_key("native"), source, host_workspace(), timeout());

    let image = ready
        .outcome
        .loaded()
        .expect("native saved path should load before inline result bytes");
    assert_eq!(image.alt(), "Cheshire cat");
    assert_eq!(image.format(), ImageFormat::Png);
    assert_eq!(image.bytes(), saved_bytes.as_slice());
    assert_eq!(image.source_path(), Some(path));
    assert_eq!(reader.calls, vec![path.to_string()]);
}

#[test]
fn native_generated_image_saved_path_loads_without_markdown_path_policy() {
    let path = r"C:\codex\generated-images\cat.png";
    let bytes = png_bytes();
    let source = TranscriptMediaSource::native_image_generation(
        "image_generation_1",
        Some("Cheshire cat".to_string()),
        None::<Arc<String>>,
        Some(path.to_string()),
        true,
    );
    let mut reader = FakeReader::with_file(path, bytes.clone());
    let mut cache = TranscriptMediaCache::new(8);

    let lookup = cache.lookup(
        cache_key("native-path"),
        source.clone(),
        host_workspace(),
        timeout(),
    );
    assert!(
        cache
            .complete_load(lookup.load_request.unwrap().load(&mut reader))
            .display_changed
    );
    let ready = cache.lookup(
        cache_key("native-path"),
        source,
        host_workspace(),
        timeout(),
    );

    let image = ready
        .outcome
        .loaded()
        .expect("native saved path should load outside the workspace member");
    assert_eq!(image.alt(), "Cheshire cat");
    assert_eq!(image.bytes(), bytes.as_slice());
    assert_eq!(image.source_path(), Some(path));
    assert_eq!(reader.calls, vec![path.to_string()]);
}

#[test]
fn completed_native_generated_image_without_bytes_reports_unavailable() {
    let source = TranscriptMediaSource::native_image_generation(
        "image_generation_1",
        Some("Vanished image".to_string()),
        None::<Arc<String>>,
        None,
        true,
    );
    let mut reader = FakeReader::default();
    let mut cache = TranscriptMediaCache::new(8);

    let lookup = cache.lookup(
        cache_key("native-missing"),
        source.clone(),
        host_workspace(),
        timeout(),
    );
    assert!(lookup.outcome.is_pending());
    assert!(
        cache
            .complete_load(lookup.load_request.unwrap().load(&mut reader))
            .display_changed
    );
    let ready = cache.lookup(
        cache_key("native-missing"),
        source,
        host_workspace(),
        timeout(),
    );

    assert_eq!(
        ready.outcome.fallback_text().as_deref(),
        Some("Vanished image (file unavailable)")
    );
    assert!(reader.calls.is_empty());
}

#[test]
fn native_generated_image_cache_identity_ignores_inline_result_when_saved_path_is_present() {
    let path = r"C:\codex\generated-images\cat.png";
    let saved_bytes = png_bytes();
    let first_result = Arc::new(BASE64_STANDARD.encode(png_bytes_with_pixel([0, 0, 0, 255])));
    let second_result = Arc::new(BASE64_STANDARD.encode(png_bytes_with_pixel([255, 0, 0, 255])));
    let mut reader = FakeReader::with_file(path, saved_bytes.clone());
    let mut cache = TranscriptMediaCache::new(8);
    let key = cache_key("native-path-result");
    let first_source = TranscriptMediaSource::native_image_generation(
        "image_generation_1",
        Some("Cheshire cat".to_string()),
        Some(first_result),
        Some(path.to_string()),
        true,
    );

    let first_lookup = cache.lookup(
        key.clone(),
        first_source.clone(),
        host_workspace(),
        timeout(),
    );
    assert!(
        cache
            .complete_load(first_lookup.load_request.unwrap().load(&mut reader))
            .display_changed
    );
    let ready = cache.lookup(key.clone(), first_source, host_workspace(), timeout());
    assert_eq!(
        ready.outcome.loaded().map(|image| image.bytes()),
        Some(saved_bytes.as_slice())
    );

    let second_source = TranscriptMediaSource::native_image_generation(
        "image_generation_1",
        Some("Cheshire cat".to_string()),
        Some(second_result),
        Some(path.to_string()),
        true,
    );
    let unchanged = cache.lookup(key, second_source, host_workspace(), timeout());
    assert!(unchanged.load_request.is_none());
    assert_eq!(
        unchanged.outcome.loaded().map(|image| image.bytes()),
        Some(saved_bytes.as_slice())
    );
    assert_eq!(reader.calls, vec![path.to_string()]);
}

#[test]
fn native_generated_image_cache_identity_uses_shared_result_payload_without_saved_path() {
    let first_bytes = png_bytes_with_pixel([0, 0, 0, 255]);
    let second_bytes = png_bytes_with_pixel([255, 0, 0, 255]);
    let first_result = Arc::new(BASE64_STANDARD.encode(&first_bytes));
    let second_result = Arc::new(BASE64_STANDARD.encode(&second_bytes));
    assert_eq!(
        first_result.len(),
        second_result.len(),
        "fixture should isolate result identity from payload length"
    );
    let mut reader = FakeReader::default();
    let mut cache = TranscriptMediaCache::new(8);
    let key = cache_key("native-result");
    let first_source = TranscriptMediaSource::native_image_generation(
        "image_generation_1",
        Some("Cheshire cat".to_string()),
        Some(first_result.clone()),
        None,
        true,
    );

    let first_lookup = cache.lookup(
        key.clone(),
        first_source.clone(),
        host_workspace(),
        timeout(),
    );
    assert!(
        cache
            .complete_load(first_lookup.load_request.unwrap().load(&mut reader))
            .display_changed
    );
    let ready = cache.lookup(key.clone(), first_source, host_workspace(), timeout());
    assert!(ready.load_request.is_none());
    assert_eq!(
        ready.outcome.loaded().map(|image| image.bytes()),
        Some(first_bytes.as_slice())
    );

    let second_source = TranscriptMediaSource::native_image_generation(
        "image_generation_1",
        Some("Cheshire cat".to_string()),
        Some(second_result),
        None,
        true,
    );
    let changed = cache.lookup(
        key.clone(),
        second_source.clone(),
        host_workspace(),
        timeout(),
    );
    assert!(changed.outcome.is_pending());
    let changed_request = changed
        .load_request
        .expect("new generated-image result payload should invalidate cached media");
    assert!(
        cache
            .complete_load(changed_request.load(&mut reader))
            .display_changed
    );
    let ready = cache.lookup(key, second_source, host_workspace(), timeout());
    assert_eq!(
        ready.outcome.loaded().map(|image| image.bytes()),
        Some(second_bytes.as_slice())
    );
    assert!(reader.calls.is_empty());
}

#[test]
fn markdown_image_revalidation_updates_changed_or_deleted_file_state() {
    let path = r"c:\work\member\images\cat.png";
    let first_bytes = png_bytes_with_pixel([0, 0, 0, 255]);
    let second_bytes = png_bytes_with_pixel([255, 0, 0, 255]);
    let source = TranscriptMediaSource::markdown_image("cat", "images/cat.png", None);
    let mut reader = FakeReader::with_file(path, first_bytes.clone());
    let mut cache = TranscriptMediaCache::new_with_markdown_revalidate_after(8, Duration::ZERO);

    let first_lookup = cache.lookup(
        cache_key("markdown-refresh"),
        source.clone(),
        host_workspace(),
        timeout(),
    );
    assert!(
        cache
            .complete_load(first_lookup.load_request.unwrap().load(&mut reader))
            .display_changed
    );
    reader.replace_file(path, second_bytes.clone());
    let revalidate_changed = cache.lookup(
        cache_key("markdown-refresh"),
        source.clone(),
        host_workspace(),
        timeout(),
    );
    assert_eq!(
        revalidate_changed
            .outcome
            .loaded()
            .expect("old image should remain visible while revalidation runs")
            .bytes(),
        first_bytes.as_slice()
    );
    assert!(
        cache
            .complete_load(revalidate_changed.load_request.unwrap().load(&mut reader))
            .display_changed
    );
    let changed_ready = cache.lookup(
        cache_key("markdown-refresh"),
        source.clone(),
        host_workspace(),
        timeout(),
    );
    assert_eq!(
        changed_ready
            .outcome
            .loaded()
            .expect("changed bytes should become visible after revalidation")
            .bytes(),
        second_bytes.as_slice()
    );
    assert!(
        cache
            .complete_load(changed_ready.load_request.unwrap().load(&mut reader))
            .display_changed
    );

    reader.remove_file(path);
    let revalidate_deleted = cache.lookup(
        cache_key("markdown-refresh"),
        source.clone(),
        host_workspace(),
        timeout(),
    );
    assert!(
        cache
            .complete_load(revalidate_deleted.load_request.unwrap().load(&mut reader))
            .display_changed
    );
    let deleted_ready = cache.lookup(
        cache_key("markdown-refresh"),
        source,
        host_workspace(),
        timeout(),
    );
    assert_eq!(
        deleted_ready.outcome.fallback_text().as_deref(),
        Some("cat (file unavailable)")
    );
    assert_eq!(
        reader.calls,
        vec![
            path.to_string(),
            path.to_string(),
            path.to_string(),
            path.to_string()
        ]
    );
}

#[test]
fn empty_alt_markdown_fallbacks_omit_label_prefix() {
    let mut reader = FakeReader::default();
    let mut cache = TranscriptMediaCache::new(8);

    let unsupported = TranscriptMediaSource::markdown_image("", "images/vector.svg", None);
    let unsupported_lookup = cache.lookup(
        cache_key("empty-alt-unsupported"),
        unsupported.clone(),
        host_workspace(),
        timeout(),
    );
    assert!(
        cache
            .complete_load(unsupported_lookup.load_request.unwrap().load(&mut reader))
            .display_changed
    );
    let unsupported_ready = cache.lookup(
        cache_key("empty-alt-unsupported"),
        unsupported,
        host_workspace(),
        timeout(),
    );
    assert_eq!(
        unsupported_ready.outcome.fallback_text().as_deref(),
        Some("(render not supported)")
    );

    let unavailable = TranscriptMediaSource::markdown_image("", "images/missing.png", None);
    let unavailable_lookup = cache.lookup(
        cache_key("empty-alt-unavailable"),
        unavailable.clone(),
        host_workspace(),
        timeout(),
    );
    assert!(
        cache
            .complete_load(unavailable_lookup.load_request.unwrap().load(&mut reader))
            .display_changed
    );
    let unavailable_ready = cache.lookup(
        cache_key("empty-alt-unavailable"),
        unavailable,
        host_workspace(),
        timeout(),
    );
    assert_eq!(
        unavailable_ready.outcome.fallback_text().as_deref(),
        Some("(file unavailable)")
    );

    let rejected = TranscriptMediaSource::markdown_image("", r"C:\other\cat.png", None);
    let rejected_lookup = cache.lookup(
        cache_key("empty-alt-rejected"),
        rejected.clone(),
        host_workspace(),
        timeout(),
    );
    assert!(
        cache
            .complete_load(rejected_lookup.load_request.unwrap().load(&mut reader))
            .display_changed
    );
    let rejected_ready = cache.lookup(
        cache_key("empty-alt-rejected"),
        rejected,
        host_workspace(),
        timeout(),
    );
    assert_eq!(
        rejected_ready.outcome.fallback_text().as_deref(),
        Some("(path not allowed)")
    );
}

#[test]
fn stale_load_cannot_update_replaced_media_source() {
    let mut reader = FakeReader::default()
        .with_file_added(r"c:\work\member\images\old.png", png_bytes())
        .with_file_added(r"c:\work\member\images\new.png", png_bytes());
    let mut cache = TranscriptMediaCache::new(8);
    let key = cache_key("replace");
    let old = TranscriptMediaSource::markdown_image("old", "images/old.png", None);
    let new = TranscriptMediaSource::markdown_image("new", "images/new.png", None);

    let old_lookup = cache.lookup(key.clone(), old, host_workspace(), timeout());
    let new_lookup = cache.lookup(key.clone(), new.clone(), host_workspace(), timeout());
    assert!(new_lookup.load_request.is_none());
    assert!(new_lookup.outcome.is_pending());

    let stale = cache.complete_load(old_lookup.load_request.unwrap().load(&mut reader));
    assert!(stale.stale);
    let follow_up = stale
        .follow_up_request
        .expect("latest media source should be scheduled after stale completion");

    let pending = cache.lookup(key.clone(), new.clone(), host_workspace(), timeout());
    assert!(pending.outcome.is_pending());
    assert!(pending.load_request.is_none());

    let fresh = cache.complete_load(follow_up.load(&mut reader));
    assert!(fresh.display_changed);
    assert!(!fresh.stale);

    let ready = cache.lookup(key, new, host_workspace(), timeout());
    assert_eq!(ready.outcome.loaded().map(|image| image.alt()), Some("new"));
}

#[test]
fn stale_load_after_scope_clear_cannot_update_different_thread() {
    let mut reader = FakeReader::with_file(r"c:\work\member\images\cat.png", png_bytes());
    let mut cache = TranscriptMediaCache::new(8);
    let source = TranscriptMediaSource::markdown_image("cat", "images/cat.png", None);
    let lookup = cache.lookup(cache_key("thread-a"), source, host_workspace(), timeout());
    let completion = lookup.load_request.unwrap().load(&mut reader);

    cache.clear();
    let result = cache.complete_load(completion);
    assert!(result.stale);
    assert!(result.follow_up_request.is_none());
    assert_eq!(cache.stats().entries, 0);
}

#[derive(Default)]
struct FakeReader {
    files: HashMap<String, Vec<u8>>,
    calls: Vec<String>,
}

impl FakeReader {
    fn with_file(path: &str, bytes: Vec<u8>) -> Self {
        Self::default().with_file_added(path, bytes)
    }

    fn with_file_added(mut self, path: &str, bytes: Vec<u8>) -> Self {
        self.files.insert(path.to_string(), bytes);
        self
    }

    fn replace_file(&mut self, path: &str, bytes: Vec<u8>) {
        self.files.insert(path.to_string(), bytes);
    }

    fn remove_file(&mut self, path: &str) {
        self.files.remove(path);
    }
}

impl TranscriptMediaFileReader for FakeReader {
    type Error = String;

    fn read_file_bytes(&mut self, path: &str, _timeout: Duration) -> Result<Vec<u8>, Self::Error> {
        self.calls.push(path.to_string());
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| format!("missing {path}"))
    }
}

fn host_workspace() -> WorkspaceId {
    WorkspaceId::host_windows(r"C:\work\member")
}

fn cache_key(value: &str) -> TranscriptMediaCacheKey {
    TranscriptMediaCacheKey::new(value)
}

fn timeout() -> Duration {
    Duration::from_secs(1)
}

fn png_bytes() -> Vec<u8> {
    png_bytes_with_pixel([0, 0, 0, 0])
}

fn png_bytes_with_pixel(rgba: [u8; 4]) -> Vec<u8> {
    png_bytes_with_dimensions(1, 1, rgba)
}

fn png_bytes_with_dimensions(width: u32, height: u32, rgba: [u8; 4]) -> Vec<u8> {
    let image = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        width,
        height,
        image::Rgba(rgba),
    ));
    let mut bytes = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut bytes, image::ImageFormat::Png)
        .expect("embedded PNG fixture should encode");
    bytes.into_inner()
}
