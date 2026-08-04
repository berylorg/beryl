struct TargetMachine<'a> {
    method: TargetMethod,
    verifier: Option<StreamedUserMessageVerifierHandle<'a>>,
    sink: Option<&'a mut dyn OrderedTurnStreamSink>,
    capture: Option<ObservationCapture<'a>>,
    steering_capture: Option<SteeringUserMessageCapture<'a>>,
    frames: [Frame; STACK_CAPACITY],
    depth: usize,
    expected: Option<Expected>,
    scalar: ScalarHandler,
    thread_id: Option<CasThreadId>,
    turn_id: Option<CasTurnId>,
    item_id: Option<CasItemId>,
    timestamp: Option<u64>,
    stats: DecodeStats,
    root_complete: bool,
}

#[derive(Clone, Copy)]
enum Frame {
    Unused,
    Root {
        params_seen: bool,
    },
    LifecycleParams {
        lifecycle: ProviderItemLifecycle,
        seen: u8,
        after: After,
    },
    DeltaParams {
        kind: ProviderDeltaKind,
        common: u8,
        payload: u8,
        after: After,
    },
    ItemSelect {
        lifecycle: ProviderItemLifecycle,
        after: After,
    },
    ItemProvider {
        fields: &'static [schema::FieldSpec],
        seen: u64,
        after: After,
    },
    ItemUser {
        lifecycle: UserMessageEchoLifecycle,
        seen: u8,
        after: After,
    },
    FixedObject {
        owner: ProviderField,
        fields: &'static [schema::FieldSpec],
        seen: u64,
        emit_container: bool,
        after: After,
    },
    DiscriminatedObject {
        schema: schema::ObjectSchema,
        owner: ProviderField,
        variant: Option<ProviderEnumValue>,
        seen: u64,
        after: After,
    },
    WebOther {
        owner: ProviderField,
        after: After,
    },
    OtherDiscard {
        container: ContainerKind,
        structured_depth: u8,
    },
    List {
        context: ProviderValueContext,
        kind: schema::ListKind,
        next: u64,
        after: After,
    },
    DiscardTextList {
        after: After,
    },
    AgentStates {
        context: ProviderValueContext,
        next: u64,
        after: After,
    },
    Structured {
        root: ProviderField,
        context: ProviderValueContext,
        container: ProviderContainer,
        next: u64,
        structured_depth: u8,
        mcp: McpState,
        after: After,
    },
    UserContent {
        next: u64,
        after: After,
    },
    UserInput {
        index: u64,
        kind: Option<UserInputKind>,
        seen: u8,
        after: After,
    },
    EmptyUserList {
        item_index: u64,
        after: After,
        had_value: bool,
    },
}

#[derive(Clone, Copy)]
enum UserInputKind {
    Text,
    LocalImage,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum McpState {
    None,
    Unsafe,
    Safe,
}

#[derive(Clone, Copy)]
enum After {
    None,
    Element {
        context: ProviderValueContext,
        index: u64,
    },
    ObjectEntry {
        root: ProviderField,
        depth: u8,
        entry: u64,
    },
}

#[derive(Clone, Copy)]
enum Expected {
    Params(TargetMethod),
    Item(ProviderItemLifecycle, After),
    Schema(schema::FieldSpec, After),
    Route(RouteValue, After),
    DeltaText(ProviderField, bool, After),
    DeltaChanges(After),
    ItemType(ProviderItemLifecycle),
    Discriminant {
        schema: schema::ObjectSchema,
        owner: ProviderField,
    },
    OtherValue,
    Structured {
        root: ProviderField,
        context: ProviderValueContext,
        depth: u8,
        mcp: bool,
        after: After,
    },
    FixedObject {
        owner: ProviderField,
        schema: schema::ObjectSchema,
        after: After,
    },
    AgentStates(After),
    AgentStateValue {
        entry: u64,
    },
    UserContent(After),
    UserClientId(After),
    UserInput {
        index: u64,
        after: After,
    },
    UserType {
        index: u64,
    },
    UserText {
        index: u64,
        after: After,
    },
    UserPath {
        index: u64,
        after: After,
    },
    UserDetail {
        index: u64,
        after: After,
    },
    EmptyUserList {
        index: u64,
        after: After,
    },
    McpType {
        root: ProviderField,
        depth: u8,
        entry: u64,
    },
}

#[derive(Clone, Copy)]
enum RouteValue {
    ThreadId,
    TurnId,
    Timestamp,
    ItemId,
    Unsigned(ProviderField, IntegerWidth),
    Signed(ProviderField, IntegerWidth),
}

#[derive(Clone, Copy)]
enum IntegerWidth {
    Any,
    Bits32,
}

// Exact decimal rounding and bounded fixed scalars deliberately keep their scratch storage inline.
#[allow(clippy::large_enum_variant)]
enum ScalarHandler {
    None,
    Fixed {
        purpose: FixedPurpose,
        bytes: FixedBytes,
        after: After,
    },
    Stream {
        context: ProviderValueContext,
        end: StreamEnd,
    },
    ThreadId {
        context: ProviderValueContext,
        end: StreamEnd,
        bytes: FixedBytes,
    },
    Number {
        purpose: NumberPurpose,
        number: NumberAccumulator,
        after: After,
    },
    Discard {
        reason: DiscardReason,
        after: After,
    },
    WebAction(WebActionProbe),
    OtherName(OtherNameProbe),
    UserText {
        index: u64,
        after: After,
    },
    UserPath {
        index: u64,
        after: After,
    },
    McpKey {
        root: ProviderField,
        depth: u8,
        entry: u64,
        bytes: FixedBytes,
        streaming: bool,
    },
    McpType {
        root: ProviderField,
        depth: u8,
        entry: u64,
        bytes: FixedBytes,
        streaming: bool,
    },
}

#[derive(Clone, Copy)]
enum StreamEnd {
    Value(After),
    StructuredKey {
        root: ProviderField,
        depth: u8,
        entry: u64,
    },
    AgentStateKey {
        entry: u64,
    },
}

#[derive(Clone, Copy)]
enum FixedPurpose {
    Name,
    ItemType(ProviderItemLifecycle),
    Enum {
        context: ProviderValueContext,
        values: &'static [(&'static str, ProviderEnumValue)],
    },
    Identity(RouteValue),
    UserClientId,
    UserType {
        index: u64,
    },
    UserDetail {
        index: u64,
    },
}

#[derive(Clone, Copy)]
enum NumberPurpose {
    Route(RouteValue),
    Structured(ProviderValueContext),
}

#[derive(Clone, Copy)]
enum DiscardReason {
    ImageResult,
    ReasoningText,
    OtherPayload,
}
