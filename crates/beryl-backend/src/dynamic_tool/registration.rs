/// One dynamic-tool registration accepted by the backend protocol.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DynamicToolSpec {
    /// A directly registered function tool.
    Function(DynamicToolFunctionSpec),
    /// A namespace containing one or more function tools.
    Namespace(DynamicToolNamespaceSpec),
}

/// Registration metadata for one dynamic function tool.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DynamicToolFunctionSpec {
    #[serde(rename = "type")]
    kind: DynamicToolFunctionSpecType,
    /// Protocol-visible tool name.
    pub name: String,
    /// Human-readable tool description supplied to the model.
    pub description: String,
    /// JSON Schema describing the tool's input value.
    pub input_schema: Value,
    /// Whether the backend may defer loading this tool definition.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
}

/// Registration metadata for one dynamic-tool namespace.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DynamicToolNamespaceSpec {
    #[serde(rename = "type")]
    kind: DynamicToolNamespaceSpecType,
    /// Protocol-visible namespace name.
    pub name: String,
    /// Human-readable namespace description supplied to the model.
    pub description: String,
    /// Function tools installed under this namespace.
    pub tools: Vec<DynamicToolFunctionSpec>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum DynamicToolFunctionSpecType {
    Function,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum DynamicToolNamespaceSpecType {
    Namespace,
}

impl DynamicToolFunctionSpec {
    /// Creates one dynamic function registration.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: Value,
    ) -> Self {
        Self {
            kind: DynamicToolFunctionSpecType::Function,
            name: name.into(),
            description: description.into(),
            input_schema,
            defer_loading: None,
        }
    }

    /// Sets whether the backend may defer loading this definition.
    #[must_use]
    pub fn with_defer_loading(mut self, defer_loading: bool) -> Self {
        self.defer_loading = Some(defer_loading);
        self
    }
}

impl DynamicToolNamespaceSpec {
    /// Creates one namespace registration containing `tools`.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        tools: Vec<DynamicToolFunctionSpec>,
    ) -> Self {
        Self {
            kind: DynamicToolNamespaceSpecType::Namespace,
            name: name.into(),
            description: description.into(),
            tools,
        }
    }
}

impl From<DynamicToolFunctionSpec> for DynamicToolSpec {
    fn from(spec: DynamicToolFunctionSpec) -> Self {
        Self::Function(spec)
    }
}

impl From<DynamicToolNamespaceSpec> for DynamicToolSpec {
    fn from(spec: DynamicToolNamespaceSpec) -> Self {
        Self::Namespace(spec)
    }
}
