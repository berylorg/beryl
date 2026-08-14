use super::{
    transcript_markdown::ParsedTranscriptMarkdown,
    transcript_media::{TranscriptMediaLoadOutcome, TranscriptMediaSource},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TranscriptMediaRunSegment {
    Markdown(String),
    Media(TranscriptMediaSource),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptMediaRunCopyLine {
    pub(crate) display_text: String,
    pub(crate) copy_text: String,
}

pub(crate) fn markdown_media_run_segments(
    markdown: &ParsedTranscriptMarkdown,
) -> Vec<TranscriptMediaRunSegment> {
    let source = markdown.source();
    let requests = markdown.media_requests();
    if requests.is_empty() {
        return (!source.is_empty())
            .then(|| TranscriptMediaRunSegment::Markdown(source.to_string()))
            .into_iter()
            .collect();
    }

    let mut segments = Vec::new();
    let mut cursor = 0usize;
    let mut requests = requests.iter().collect::<Vec<_>>();
    requests.sort_by_key(|request| {
        request
            .source_span()
            .map(|span| (span.start(), span.end()))
            .unwrap_or((usize::MAX, usize::MAX))
    });

    for request in requests {
        let Some(span) = request.source_span() else {
            continue;
        };
        if span.start() < cursor
            || span.end() > source.len()
            || !source.is_char_boundary(span.start())
            || !source.is_char_boundary(span.end())
        {
            continue;
        }

        push_markdown_gap(&mut segments, &source[cursor..span.start()]);
        segments.push(TranscriptMediaRunSegment::Media(
            TranscriptMediaSource::markdown_image(
                request.alt(),
                request.destination(),
                request.title().map(str::to_string),
            ),
        ));
        cursor = span.end();
    }

    push_markdown_gap(&mut segments, &source[cursor..]);
    if segments.is_empty() && !source.is_empty() {
        segments.push(TranscriptMediaRunSegment::Markdown(source.to_string()));
    }
    segments
}

fn push_markdown_gap(segments: &mut Vec<TranscriptMediaRunSegment>, gap: &str) {
    if gap.is_empty() || gap.chars().all(char::is_whitespace) {
        return;
    }
    segments.push(TranscriptMediaRunSegment::Markdown(gap.to_string()));
}

pub(crate) fn media_run_copy_line<'a>(
    items: impl IntoIterator<
        Item = (
            &'a TranscriptMediaSource,
            Option<&'a TranscriptMediaLoadOutcome>,
        ),
    >,
) -> Option<TranscriptMediaRunCopyLine> {
    let mut display_parts = Vec::new();
    let mut copy_parts = Vec::new();

    for (source, outcome) in items {
        let display = media_display_text(source, outcome);
        if display.is_empty() {
            continue;
        }
        display_parts.push(display);
        copy_parts.push(media_copy_text(source, outcome));
    }

    (!display_parts.is_empty()).then(|| TranscriptMediaRunCopyLine {
        display_text: display_parts.join(" "),
        copy_text: copy_parts.join("\n"),
    })
}

fn media_display_text(
    source: &TranscriptMediaSource,
    outcome: Option<&TranscriptMediaLoadOutcome>,
) -> String {
    if let Some(fallback) = outcome.and_then(TranscriptMediaLoadOutcome::fallback_text) {
        return fallback;
    }

    match outcome {
        Some(TranscriptMediaLoadOutcome::Loaded(image)) => image.alt().to_string(),
        Some(TranscriptMediaLoadOutcome::Pending { alt }) => alt.to_string(),
        Some(_) | None => media_source_alt(source),
    }
}

fn media_copy_text(
    source: &TranscriptMediaSource,
    outcome: Option<&TranscriptMediaLoadOutcome>,
) -> String {
    if let Some(fallback) = outcome.and_then(TranscriptMediaLoadOutcome::fallback_text) {
        return fallback;
    }

    match source {
        TranscriptMediaSource::MarkdownImage {
            alt,
            destination,
            title,
        } => markdown_image_source(alt, destination, title.as_deref()),
        TranscriptMediaSource::NativeImageGeneration { .. } => media_display_text(source, outcome),
    }
}

fn media_source_alt(source: &TranscriptMediaSource) -> String {
    match source {
        TranscriptMediaSource::MarkdownImage { alt, .. } => fallback_media_alt(alt),
        TranscriptMediaSource::NativeImageGeneration { revised_prompt, .. } => revised_prompt
            .as_deref()
            .map(str::trim)
            .filter(|alt| !alt.is_empty())
            .unwrap_or("generated image")
            .to_string(),
    }
}

fn markdown_image_source(alt: &str, destination: &str, title: Option<&str>) -> String {
    let mut source = format!("![{alt}]({destination}");
    if let Some(title) = title {
        source.push(' ');
        source.push('"');
        source.push_str(title);
        source.push('"');
    }
    source.push(')');
    source
}

fn fallback_media_alt(alt: &str) -> String {
    let alt = alt.trim();
    if alt.is_empty() {
        "image".to_string()
    } else {
        alt.to_string()
    }
}
