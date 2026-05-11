use std::{
    collections::{HashMap, HashSet},
    ops::Range,
};

use beryl_backend::UserInput;

use super::{
    TranscriptImagePathResolver, TranscriptImageSource, transcript_image_source_for_input,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct GeneratedImageAnchor {
    record_index: usize,
    pub(super) range: Range<usize>,
    label: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct GeneratedImageAnchorKey {
    pub(super) record_index: usize,
    pub(super) start: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct BoundGeneratedImageAnchor {
    pub(super) label: String,
    pub(super) source: TranscriptImageSource,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct GeneratedImageBindings {
    pub(super) anchors: HashMap<GeneratedImageAnchorKey, BoundGeneratedImageAnchor>,
    pub(super) image_record_indexes: HashSet<usize>,
}

pub(super) fn generated_image_bindings_for_records(
    records: &[UserInput],
    resolver: &TranscriptImagePathResolver,
) -> GeneratedImageBindings {
    let anchors = generated_image_anchors_for_records(records);
    let mut next_anchor_index = anchors.len();
    let mut bindings = GeneratedImageBindings::default();

    for (record_index, input) in records.iter().enumerate().rev() {
        let Some(source) = transcript_image_source_for_input(input, resolver) else {
            continue;
        };
        while next_anchor_index > 0 && anchors[next_anchor_index - 1].record_index >= record_index {
            next_anchor_index -= 1;
        }
        if next_anchor_index == 0 {
            continue;
        }

        next_anchor_index -= 1;
        let anchor = &anchors[next_anchor_index];

        bindings.anchors.insert(
            GeneratedImageAnchorKey {
                record_index: anchor.record_index,
                start: anchor.range.start,
            },
            BoundGeneratedImageAnchor {
                label: anchor.label.clone(),
                source,
            },
        );
        bindings.image_record_indexes.insert(record_index);
    }

    bindings
}

fn generated_image_anchors_for_records(records: &[UserInput]) -> Vec<GeneratedImageAnchor> {
    records
        .iter()
        .enumerate()
        .filter_map(|(record_index, input)| match input {
            UserInput::Text { text } => Some((record_index, text.as_str())),
            UserInput::Image { .. }
            | UserInput::LocalImage { .. }
            | UserInput::Skill { .. }
            | UserInput::Mention { .. } => None,
        })
        .flat_map(|(record_index, text)| {
            generated_image_label_anchors_in_text(text)
                .into_iter()
                .map(move |mut anchor| {
                    anchor.record_index = record_index;
                    anchor
                })
        })
        .collect()
}

pub(super) fn generated_image_label_anchors_in_text(text: &str) -> Vec<GeneratedImageAnchor> {
    let bytes = text.as_bytes();
    let mut anchors = Vec::new();
    let mut cursor = 0usize;

    while let Some(relative_start) = text[cursor..].find("Image ") {
        let start = cursor + relative_start;
        let label_start = start + "Image ".len();
        let mut label_end = label_start;
        while label_end < bytes.len() && bytes[label_end].is_ascii_uppercase() {
            label_end += 1;
        }
        if label_end == label_start || bytes.get(label_end) != Some(&b':') {
            cursor = label_start;
            continue;
        }

        anchors.push(GeneratedImageAnchor {
            record_index: 0,
            range: start..label_end + ':'.len_utf8(),
            label: text[label_start..label_end].to_string(),
        });
        cursor = label_end + ':'.len_utf8();
    }

    anchors
}
