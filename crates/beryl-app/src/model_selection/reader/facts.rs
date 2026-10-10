#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum ModelReasoningEffort {
    None,
    Minimal,
    Low,
    Medium,
    High,
    XHigh,
    Max,
    Ultra,
}

impl ModelReasoningEffort {
    pub(crate) const ALL: [Self; 8] = [
        Self::None,
        Self::Minimal,
        Self::Low,
        Self::Medium,
        Self::High,
        Self::XHigh,
        Self::Max,
        Self::Ultra,
    ];
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Minimal => "minimal",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::XHigh => "xhigh",
            Self::Max => "max",
            Self::Ultra => "ultra",
        }
    }
    pub(crate) fn backend(self) -> beryl_backend::ReasoningEffort {
        match self {
            Self::None => beryl_backend::ReasoningEffort::None,
            Self::Minimal => beryl_backend::ReasoningEffort::Minimal,
            Self::Low => beryl_backend::ReasoningEffort::Low,
            Self::Medium => beryl_backend::ReasoningEffort::Medium,
            Self::High => beryl_backend::ReasoningEffort::High,
            Self::XHigh => beryl_backend::ReasoningEffort::XHigh,
            Self::Max => beryl_backend::ReasoningEffort::Max,
            Self::Ultra => beryl_backend::ReasoningEffort::Ultra,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ModelSupportedEfforts(u16);
impl ModelSupportedEfforts {
    pub(crate) fn contains(self, effort: ModelReasoningEffort) -> bool {
        self.0 & (1 << effort as u8) != 0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ModelDefaults {
    pub(crate) model: Option<String>,
    pub(crate) reasoning: Option<String>,
}

impl ModelDefaults {
    pub(super) fn from_backend(defaults: beryl_backend::BackendConfigDefaults) -> Self {
        Self {
            model: defaults.model().map(str::to_owned),
            reasoning: defaults.model_reasoning_effort().map(str::to_owned),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ModelOptionRecord {
    pub(crate) id: String,
    pub(crate) model: String,
    pub(crate) label: String,
    pub(crate) hidden: bool,
    pub(crate) is_default: bool,
    pub(crate) efforts: ModelSupportedEfforts,
    pub(crate) default_reasoning: Option<ModelReasoningEffort>,
}

impl ModelOptionRecord {
    pub(super) fn from_backend(record: &beryl_backend::ModelRecord) -> Self {
        use beryl_backend::DefaultReasoningEffort as D;
        let default_reasoning = match record.default_reasoning_effort() {
            D::None => Some(ModelReasoningEffort::None),
            D::Minimal => Some(ModelReasoningEffort::Minimal),
            D::Low => Some(ModelReasoningEffort::Low),
            D::Medium => Some(ModelReasoningEffort::Medium),
            D::High => Some(ModelReasoningEffort::High),
            D::XHigh => Some(ModelReasoningEffort::XHigh),
            D::Max => Some(ModelReasoningEffort::Max),
            D::Ultra => Some(ModelReasoningEffort::Ultra),
            D::Other => None,
        };
        Self {
            id: record.id().to_owned(),
            model: record.model().to_owned(),
            label: record.display_name().to_owned(),
            hidden: record.hidden(),
            is_default: record.is_default(),
            efforts: ModelSupportedEfforts(record.supported_reasoning_efforts().bits()),
            default_reasoning,
        }
    }
}
