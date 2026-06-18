use std::sync::atomic::{AtomicU64, Ordering};

use beryl_backend::UserInput;

use super::transcript_images::{
    TranscriptImageMarker, TranscriptImageMarkerSpec, TranscriptImagePathResolver,
    transcript_image_marker_specs_from_markers, transcript_image_markers_from_specs,
    transcript_image_parts_for_backend_records,
};

static NEXT_USER_INPUT_FRAGMENT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct UserInputFragment {
    pub id: u64,
    pub text: String,
    backend_input: Vec<UserInput>,
    image_markers: Vec<TranscriptImageMarker>,
}

impl UserInputFragment {
    pub(super) fn text(text: impl Into<String>) -> Self {
        let text = text.into();
        Self::from_backend_input(text.clone(), vec![UserInput::text(text)])
    }

    pub(super) fn from_backend_input(
        text: impl Into<String>,
        backend_input: Vec<UserInput>,
    ) -> Self {
        let text = text.into();
        let parts = transcript_image_parts_for_backend_records(
            &backend_input,
            &TranscriptImagePathResolver::default(),
        );
        let marker_specs = (parts.display_text() == text)
            .then(|| parts.into_image_markers())
            .unwrap_or_default();
        Self::from_backend_input_with_image_markers(text, backend_input, marker_specs)
    }

    pub(super) fn from_backend_input_with_image_markers(
        text: impl Into<String>,
        backend_input: Vec<UserInput>,
        image_markers: Vec<TranscriptImageMarkerSpec>,
    ) -> Self {
        let id = NEXT_USER_INPUT_FRAGMENT_ID.fetch_add(1, Ordering::Relaxed);
        Self {
            id,
            text: text.into(),
            backend_input,
            image_markers: transcript_image_markers_from_specs(id, image_markers),
        }
    }

    pub(super) fn backend_input(&self) -> &[UserInput] {
        &self.backend_input
    }

    #[allow(dead_code)]
    pub(super) fn image_markers(&self) -> &[TranscriptImageMarker] {
        &self.image_markers
    }

    pub(super) fn image_marker_specs(&self) -> Vec<TranscriptImageMarkerSpec> {
        transcript_image_marker_specs_from_markers(&self.image_markers)
    }

    pub(super) fn retained_payload_bytes_lower_bound(&self) -> usize {
        self.text
            .len()
            .saturating_add(
                self.backend_input
                    .iter()
                    .map(user_input_payload_bytes)
                    .sum::<usize>(),
            )
            .saturating_add(self.image_markers.len().saturating_mul(32))
    }

    pub(super) fn is_blank(&self) -> bool {
        self.text.trim().is_empty() && self.backend_input.is_empty()
    }
}

pub(super) fn user_input_payload_bytes(input: &UserInput) -> usize {
    match input {
        UserInput::Text { text } => text.len(),
        UserInput::Image { url } => url.len(),
        UserInput::LocalImage { path } => path.len(),
        UserInput::Skill { name, path } | UserInput::Mention { name, path } => {
            name.len().saturating_add(path.len())
        }
    }
}
