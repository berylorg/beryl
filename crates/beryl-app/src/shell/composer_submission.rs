use beryl_backend::UserInput;

use super::{
    composer_draft::AcceptedComposerDraftPart,
    composer_image_delivery::PreparedComposerDraft,
    execution_detail::{
        TranscriptImageMarkerSpec, TranscriptImagePreviewState, UserInputFragment,
        transcript_image_source_from_local_image,
        transcript_image_source_from_local_image_with_format,
    },
};

pub(super) fn prepared_composer_draft_fragment(
    prepared: &PreparedComposerDraft,
) -> Result<UserInputFragment, String> {
    let mut backend_input = Vec::new();

    for part in prepared.draft().parts() {
        match part {
            AcceptedComposerDraftPart::Text(text) => {
                if !text.is_empty() {
                    backend_input.push(UserInput::text(text.clone()));
                }
            }
            AcceptedComposerDraftPart::Image(image) => {
                let label = image.label();
                let Some(path) = prepared.backend_path_for_label(label) else {
                    return Err(format!(
                        "Beryl could not submit image {label} because image preparation did not return a backend-readable path for it."
                    ));
                };
                backend_input.push(UserInput::text(generated_image_label_text(label)));
                backend_input.push(UserInput::local_image(path.to_string()));
            }
        }
    }

    let image_markers = prepared
        .draft()
        .image_occurrences()
        .iter()
        .filter_map(|occurrence| {
            let image = prepared.image_for_label(occurrence.label())?;
            let source = if let Some(draft_image) = prepared
                .draft()
                .images()
                .find(|draft_image| draft_image.label() == occurrence.label())
            {
                transcript_image_source_from_local_image_with_format(
                    image.backend_path().to_string(),
                    image.asset_id().to_string(),
                    draft_image.data().format(),
                )
            } else {
                transcript_image_source_from_local_image(
                    image.backend_path().to_string(),
                    Some(image.asset_id().to_string()),
                    TranscriptImagePreviewState::Available,
                )
            };
            Some(TranscriptImageMarkerSpec::new(
                occurrence.label().to_string(),
                occurrence.range(),
                source,
            ))
        })
        .collect();

    Ok(UserInputFragment::from_backend_input_with_image_markers(
        prepared.draft().display_text().to_string(),
        backend_input,
        image_markers,
    ))
}

pub(super) fn generated_image_label_text(label: &str) -> String {
    format!("Image {label}:")
}
