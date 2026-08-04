/// Protocol response body for one dynamic-tool server request.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DynamicToolCallResponse {
    /// Text or image content returned to the backend.
    pub content_items: Vec<DynamicToolCallOutputContentItem>,
    /// Whether the installed tool completed successfully.
    pub success: bool,
}

/// One content item in a dynamic-tool response.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum DynamicToolCallOutputContentItem {
    /// Model-visible input text.
    #[serde(rename = "inputText")]
    InputText {
        /// Decoded text content.
        text: String,
    },
    /// Model-visible input image referenced by URL.
    #[serde(rename = "inputImage", rename_all = "camelCase")]
    InputImage {
        /// Image URL passed back to the backend.
        image_url: String,
    },
}

impl DynamicToolCallResponse {
    /// Creates a successful response containing `content_items`.
    #[must_use]
    pub fn success(content_items: Vec<DynamicToolCallOutputContentItem>) -> Self {
        Self {
            content_items,
            success: true,
        }
    }

    /// Creates a failed response containing `content_items`.
    #[must_use]
    pub fn failure(content_items: Vec<DynamicToolCallOutputContentItem>) -> Self {
        Self {
            content_items,
            success: false,
        }
    }

    /// Creates a successful response containing one text item.
    #[must_use]
    pub fn success_text(text: impl Into<String>) -> Self {
        Self::success(vec![DynamicToolCallOutputContentItem::text(text)])
    }

    /// Creates a failed response containing one text item.
    #[must_use]
    pub fn failure_text(text: impl Into<String>) -> Self {
        Self::failure(vec![DynamicToolCallOutputContentItem::text(text)])
    }
}

impl DynamicToolCallOutputContentItem {
    /// Creates one text response item.
    #[must_use]
    pub fn text(text: impl Into<String>) -> Self {
        Self::InputText { text: text.into() }
    }

    /// Creates one image-URL response item.
    #[must_use]
    pub fn image_url(image_url: impl Into<String>) -> Self {
        Self::InputImage {
            image_url: image_url.into(),
        }
    }
}
