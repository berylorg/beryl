use serde::{Deserialize, Serialize};
use serde_json::Value;

pub(crate) const DYNAMIC_TOOL_CALL_METHOD: &str = "item/tool/call";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DynamicToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DynamicToolCallRequest {
    request_id: Value,
    params: DynamicToolCallParams,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DynamicToolCallParams {
    arguments: Value,
    call_id: String,
    #[serde(default)]
    namespace: Option<String>,
    thread_id: String,
    tool: String,
    turn_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DynamicToolCallResponse {
    pub content_items: Vec<DynamicToolCallOutputContentItem>,
    pub success: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum DynamicToolCallOutputContentItem {
    #[serde(rename = "inputText")]
    InputText { text: String },
    #[serde(rename = "inputImage", rename_all = "camelCase")]
    InputImage { image_url: String },
}

impl DynamicToolSpec {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: Value,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            input_schema,
            namespace: None,
            defer_loading: None,
        }
    }

    pub fn with_namespace(mut self, namespace: impl Into<String>) -> Self {
        self.namespace = Some(namespace.into());
        self
    }

    pub fn with_defer_loading(mut self, defer_loading: bool) -> Self {
        self.defer_loading = Some(defer_loading);
        self
    }
}

impl DynamicToolCallRequest {
    fn new(request_id: Value, params: DynamicToolCallParams) -> Self {
        Self { request_id, params }
    }

    pub fn request_id(&self) -> &Value {
        &self.request_id
    }

    pub fn method(&self) -> &'static str {
        DYNAMIC_TOOL_CALL_METHOD
    }

    pub fn thread_id(&self) -> &str {
        &self.params.thread_id
    }

    pub fn turn_id(&self) -> &str {
        &self.params.turn_id
    }

    pub fn call_id(&self) -> &str {
        &self.params.call_id
    }

    pub fn tool(&self) -> &str {
        &self.params.tool
    }

    pub fn namespace(&self) -> Option<&str> {
        self.params.namespace.as_deref()
    }

    pub fn arguments(&self) -> &Value {
        &self.params.arguments
    }

    pub fn pretty_arguments(&self) -> String {
        serde_json::to_string_pretty(&self.params.arguments)
            .unwrap_or_else(|_| self.params.arguments.to_string())
    }

    pub fn summary(&self) -> String {
        format!(
            "requestId={}, threadId={}, turnId={}, callId={}, namespace={}, tool={}",
            self.request_id,
            self.thread_id(),
            self.turn_id(),
            self.call_id(),
            self.namespace().unwrap_or("<none>"),
            self.tool()
        )
    }
}

impl DynamicToolCallResponse {
    pub fn success(content_items: Vec<DynamicToolCallOutputContentItem>) -> Self {
        Self {
            content_items,
            success: true,
        }
    }

    pub fn failure(content_items: Vec<DynamicToolCallOutputContentItem>) -> Self {
        Self {
            content_items,
            success: false,
        }
    }

    pub fn success_text(text: impl Into<String>) -> Self {
        Self::success(vec![DynamicToolCallOutputContentItem::text(text)])
    }

    pub fn failure_text(text: impl Into<String>) -> Self {
        Self::failure(vec![DynamicToolCallOutputContentItem::text(text)])
    }
}

impl DynamicToolCallOutputContentItem {
    pub fn text(text: impl Into<String>) -> Self {
        Self::InputText { text: text.into() }
    }

    pub fn image_url(image_url: impl Into<String>) -> Self {
        Self::InputImage {
            image_url: image_url.into(),
        }
    }
}

pub fn parse_dynamic_tool_call_request(
    request_id: Value,
    method: &str,
    params: Option<Value>,
) -> Result<Option<DynamicToolCallRequest>, serde_json::Error> {
    if method != DYNAMIC_TOOL_CALL_METHOD {
        return Ok(None);
    }

    let params = serde_json::from_value(params.unwrap_or(Value::Null))?;
    Ok(Some(DynamicToolCallRequest::new(request_id, params)))
}

pub(crate) fn is_dynamic_tool_call_method(method: &str) -> bool {
    method == DYNAMIC_TOOL_CALL_METHOD
}
