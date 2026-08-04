#[derive(Clone, Copy)]
enum Classification {
    Target(ClassifiedTarget),
    Discard(DiscardDisposition),
    ResponseSuccess { actual_id: Option<u64> },
    ResponseFailure,
}

#[derive(Clone, Copy)]
enum ClassifiedTarget {
    Provider(TargetMethod),
    Approval(ApprovalRequestKind),
    DynamicTool,
    ThreadClosed,
    ThreadStatus,
    TurnStarted,
    NormalTerminal,
}

struct Classifier {
    state: ClassifierState,
}

enum ClassifierState {
    Start,
    RootName,
    FirstName(ClassifierProbe),
    MethodValue,
    MethodString(MethodProbe),
    SuccessIdValue,
    SuccessIdNumber(NumberBytes),
    SuccessIdDiscard(ValueTracker),
    SuccessRootName {
        actual_id: Option<u64>,
    },
    SuccessName {
        actual_id: Option<u64>,
        probe: ClassifierProbe,
    },
    Quarantine,
}

impl Classifier {
    const fn new() -> Self {
        Self {
            state: ClassifierState::Start,
        }
    }

    fn bytes(&mut self, bytes: &[u8]) {
        match &mut self.state {
            ClassifierState::FirstName(probe) => {
                probe.push(bytes, &FIRST_ROOT_NAMES);
            }
            ClassifierState::MethodString(probe) => probe.push(bytes),
            ClassifierState::SuccessIdNumber(number) => number.push(bytes),
            ClassifierState::SuccessName { probe, .. } => {
                probe.push(bytes, &SUCCESS_NAMES);
            }
            _ => {}
        }
    }

    fn event(&mut self, event: Event) -> Option<Classification> {
        let state = std::mem::replace(&mut self.state, ClassifierState::Quarantine);
        self.state = match state {
            ClassifierState::Start => match event {
                Event::ContainerStart(ContainerKind::Object) => ClassifierState::RootName,
                _ => ClassifierState::Quarantine,
            },
            ClassifierState::RootName => match event {
                Event::ScalarStart(ScalarKind::Name) => {
                    let mut probe = ClassifierProbe::new();
                    probe.reset((1_u16 << FIRST_ROOT_NAMES.len()) - 1);
                    ClassifierState::FirstName(probe)
                }
                _ => ClassifierState::Quarantine,
            },
            ClassifierState::FirstName(probe) => {
                if matches!(event, Event::ScalarFragment(ScalarKind::Name)) {
                    ClassifierState::FirstName(probe)
                } else if matches!(event, Event::ScalarEnd(ScalarKind::Name)) {
                    if probe.exact(0, FIRST_ROOT_NAMES[0].len()) {
                        ClassifierState::MethodValue
                    } else if probe.exact(1, FIRST_ROOT_NAMES[1].len()) {
                        ClassifierState::SuccessIdValue
                    } else if probe.exact(2, FIRST_ROOT_NAMES[2].len()) {
                        self.state = ClassifierState::Quarantine;
                        return Some(Classification::ResponseFailure);
                    } else {
                        ClassifierState::Quarantine
                    }
                } else {
                    ClassifierState::Quarantine
                }
            }
            ClassifierState::MethodValue => match event {
                Event::ScalarStart(ScalarKind::String) => {
                    ClassifierState::MethodString(MethodProbe::new())
                }
                _ => ClassifierState::Quarantine,
            },
            ClassifierState::MethodString(probe) => {
                if matches!(event, Event::ScalarFragment(ScalarKind::String)) {
                    ClassifierState::MethodString(probe)
                } else if matches!(event, Event::ScalarEnd(ScalarKind::String)) {
                    self.state = ClassifierState::Quarantine;
                    return Some(probe.classify());
                } else {
                    ClassifierState::Quarantine
                }
            }
            ClassifierState::SuccessIdValue => {
                return self.start_success_id(event);
            }
            ClassifierState::SuccessIdNumber(number) => {
                if matches!(event, Event::ScalarFragment(ScalarKind::Number)) {
                    ClassifierState::SuccessIdNumber(number)
                } else if matches!(event, Event::ScalarEnd(ScalarKind::Number)) {
                    ClassifierState::SuccessRootName {
                        actual_id: number.parse_u64(),
                    }
                } else {
                    ClassifierState::Quarantine
                }
            }
            ClassifierState::SuccessIdDiscard(mut value) => {
                if !value.event(event) {
                    ClassifierState::Quarantine
                } else if value.is_complete() {
                    ClassifierState::SuccessRootName { actual_id: None }
                } else {
                    ClassifierState::SuccessIdDiscard(value)
                }
            }
            ClassifierState::SuccessRootName { actual_id } => match event {
                Event::ScalarStart(ScalarKind::Name) => {
                    let mut probe = ClassifierProbe::new();
                    probe.reset(1);
                    ClassifierState::SuccessName { actual_id, probe }
                }
                _ => ClassifierState::Quarantine,
            },
            ClassifierState::SuccessName { actual_id, probe } => {
                if matches!(event, Event::ScalarFragment(ScalarKind::Name)) {
                    ClassifierState::SuccessName { actual_id, probe }
                } else if matches!(event, Event::ScalarEnd(ScalarKind::Name))
                    && probe.exact(0, SUCCESS_NAMES[0].len())
                {
                    self.state = ClassifierState::Quarantine;
                    return Some(Classification::ResponseSuccess { actual_id });
                } else {
                    ClassifierState::Quarantine
                }
            }
            ClassifierState::Quarantine => ClassifierState::Quarantine,
        };
        None
    }

    fn start_success_id(&mut self, event: Event) -> Option<Classification> {
        self.state = match event {
            Event::ScalarStart(ScalarKind::Number) => {
                ClassifierState::SuccessIdNumber(NumberBytes::new())
            }
            Event::ContainerStart(_) | Event::ScalarStart(_) => {
                let mut value = ValueTracker::new();
                if value.event(event) {
                    ClassifierState::SuccessIdDiscard(value)
                } else {
                    ClassifierState::Quarantine
                }
            }
            Event::Boolean(_) | Event::Null => ClassifierState::SuccessRootName { actual_id: None },
            _ => ClassifierState::Quarantine,
        };
        None
    }

    const fn is_quarantined(&self) -> bool {
        matches!(self.state, ClassifierState::Quarantine)
    }

    fn resolve_prefix_pressure(&mut self) {
        self.state = ClassifierState::Quarantine;
    }
}

struct MethodProbe {
    targets: ClassifierProbe,
    controls: ClassifierProbe,
}

impl MethodProbe {
    fn new() -> Self {
        let mut targets = ClassifierProbe::new();
        targets.reset(u16::MAX);
        let mut controls = ClassifierProbe::new();
        controls.reset((1_u16 << CONTROL_METHOD_WIRES.len()) - 1);
        Self { targets, controls }
    }

    fn push(&mut self, bytes: &[u8]) {
        self.targets.push(bytes, &TARGET_METHOD_WIRES);
        self.controls.push(bytes, &CONTROL_METHOD_WIRES);
    }

    fn classify(self) -> Classification {
        if let Some(target) = TARGET_METHOD_WIRES
            .iter()
            .enumerate()
            .find_map(|(index, wire)| {
                self.targets
                    .exact(index, wire.len())
                    .then_some(TARGET_METHODS[index])
            })
        {
            return Classification::Target(target);
        }
        if let Some(classification) =
            CONTROL_METHOD_WIRES
                .iter()
                .enumerate()
                .find_map(|(index, wire)| {
                    self.controls
                        .exact(index, wire.len())
                        .then_some(CONTROL_CLASSIFICATIONS[index])
                })
        {
            return classification;
        }
        Classification::Discard(DiscardDisposition::UnknownNotification)
    }
}

const FIRST_ROOT_NAMES: [&[u8]; 3] = [b"method", b"id", b"error"];
const SUCCESS_NAMES: [&[u8]; 1] = [b"result"];

const TARGET_METHOD_WIRES: [&[u8]; 16] = [
    b"item/started",
    b"item/completed",
    b"item/agentMessage/delta",
    b"item/plan/delta",
    b"item/reasoning/summaryPartAdded",
    b"item/reasoning/summaryTextDelta",
    b"item/reasoning/textDelta",
    b"item/commandExecution/outputDelta",
    b"item/fileChange/outputDelta",
    b"item/fileChange/patchUpdated",
    b"item/mcpToolCall/progress",
    COMMAND_EXECUTION_REQUEST_APPROVAL_METHOD.as_bytes(),
    FILE_CHANGE_REQUEST_APPROVAL_METHOD.as_bytes(),
    PERMISSIONS_REQUEST_APPROVAL_METHOD.as_bytes(),
    crate::dynamic_tool::DYNAMIC_TOOL_CALL_METHOD.as_bytes(),
    b"turn/completed",
];

const TARGET_METHODS: [ClassifiedTarget; 16] = [
    ClassifiedTarget::Provider(TargetMethod::Lifecycle(ProviderItemLifecycle::Started)),
    ClassifiedTarget::Provider(TargetMethod::Lifecycle(ProviderItemLifecycle::Completed)),
    ClassifiedTarget::Provider(TargetMethod::Delta(ProviderDeltaKind::AgentMessage)),
    ClassifiedTarget::Provider(TargetMethod::Delta(ProviderDeltaKind::Plan)),
    ClassifiedTarget::Provider(TargetMethod::Delta(
        ProviderDeltaKind::ReasoningSummaryPartAdded,
    )),
    ClassifiedTarget::Provider(TargetMethod::Delta(ProviderDeltaKind::ReasoningSummaryText)),
    ClassifiedTarget::Provider(TargetMethod::Delta(
        ProviderDeltaKind::ReasoningTextObserved,
    )),
    ClassifiedTarget::Provider(TargetMethod::Delta(
        ProviderDeltaKind::CommandExecutionOutput,
    )),
    ClassifiedTarget::Provider(TargetMethod::Delta(ProviderDeltaKind::FileChangeOutput)),
    ClassifiedTarget::Provider(TargetMethod::Delta(
        ProviderDeltaKind::FileChangePatchUpdated,
    )),
    ClassifiedTarget::Provider(TargetMethod::Delta(ProviderDeltaKind::McpToolCallProgress)),
    ClassifiedTarget::Approval(ApprovalRequestKind::CommandExecution),
    ClassifiedTarget::Approval(ApprovalRequestKind::FileChange),
    ClassifiedTarget::Approval(ApprovalRequestKind::Permissions),
    ClassifiedTarget::DynamicTool,
    ClassifiedTarget::NormalTerminal,
];

const CONTROL_METHOD_WIRES: [&[u8]; 9] = [
    b"thread/started",
    b"thread/status/changed",
    b"thread/closed",
    b"thread/tokenUsage/updated",
    b"account/rateLimits/updated",
    b"turn/started",
    b"codex/event/collab_agent_spawn_end",
    b"thread/name/updated",
    b"turn/diff/updated",
];

const CONTROL_CLASSIFICATIONS: [Classification; 9] = [
    Classification::Discard(DiscardDisposition::Unavailable(KnownControlFamily::Compact)),
    Classification::Target(ClassifiedTarget::ThreadStatus),
    Classification::Target(ClassifiedTarget::ThreadClosed),
    Classification::Discard(DiscardDisposition::Unavailable(KnownControlFamily::Compact)),
    Classification::Discard(DiscardDisposition::Unavailable(KnownControlFamily::Compact)),
    Classification::Target(ClassifiedTarget::TurnStarted),
    Classification::Discard(DiscardDisposition::Unavailable(KnownControlFamily::Compact)),
    Classification::Discard(DiscardDisposition::NoOwnerNotification),
    Classification::Discard(DiscardDisposition::NoOwnerNotification),
];
