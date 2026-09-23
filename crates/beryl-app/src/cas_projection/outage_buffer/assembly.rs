use super::{
    OutageBuffer, OutageConnectionIdentity, OutageFact, OutagePriority, OutageTarget,
    OutageTextKind,
};
use beryl_backend::{
    ProviderEnumValue, ProviderField, ProviderItemKind, ProviderObservationBegin,
    ProviderObservationControl, ProviderObservationRoute, ProviderValueContext,
};
use beryl_model::{CasItemId, ProviderObservationId};

#[derive(Clone, Copy, Debug)]
pub struct OutageAssemblyLimits {
    pub max_bytes: usize,
    pub max_field_bytes: usize,
    pub max_entries: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutageAssemblyError {
    Overflow,
    Malformed,
    MissingRoute,
    UnqualifiedRoute,
    RetentionLoss,
}

enum Entry {
    Control(ProviderObservationControl),
    Field(ProviderValueContext, String),
}

pub struct OutageAssembly {
    connection: OutageConnectionIdentity,
    observation: ProviderObservationId,
    begin: ProviderObservationBegin,
    limits: OutageAssemblyLimits,
    entries: Vec<Entry>,
    open: Option<(ProviderValueContext, String)>,
    retained_bytes: usize,
    loss: Option<OutageAssemblyError>,
}

impl OutageAssembly {
    pub fn new(
        connection: OutageConnectionIdentity,
        observation: ProviderObservationId,
        begin: ProviderObservationBegin,
        limits: OutageAssemblyLimits,
    ) -> Self {
        Self {
            connection,
            observation,
            begin,
            limits,
            entries: Vec::new(),
            open: None,
            retained_bytes: 0,
            loss: None,
        }
    }

    pub fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }

    pub fn retained_entries(&self) -> usize {
        self.entries.len() + usize::from(self.open.is_some())
    }

    pub fn control(
        &mut self,
        control: ProviderObservationControl,
    ) -> Result<(), OutageAssemblyError> {
        if let Some(loss) = self.loss {
            return Err(loss);
        }
        match control {
            ProviderObservationControl::BeginField(context) => {
                if self.open.is_some() {
                    return self.fail(OutageAssemblyError::Malformed);
                }
                self.reserve_entry()?;
                self.open = Some((context, String::new()));
            }
            ProviderObservationControl::EndField(context) => {
                let Some((expected, value)) = self.open.take() else {
                    return self.fail(OutageAssemblyError::Malformed);
                };
                if expected != context {
                    return self.fail(OutageAssemblyError::Malformed);
                }
                self.entries.push(Entry::Field(context, value));
            }
            _ => {
                if self.open.is_some() {
                    return self.fail(OutageAssemblyError::Malformed);
                }
                self.reserve_entry()?;
                self.entries.push(Entry::Control(control));
            }
        }
        Ok(())
    }

    pub fn fragment(
        &mut self,
        context: ProviderValueContext,
        offset: usize,
        text: &str,
    ) -> Result<(), OutageAssemblyError> {
        if let Some(loss) = self.loss {
            return Err(loss);
        }
        let Some((expected, value)) = &self.open else {
            return self.fail(OutageAssemblyError::Malformed);
        };
        if *expected != context || offset != value.len() {
            return self.fail(OutageAssemblyError::Malformed);
        }
        if value
            .len()
            .checked_add(text.len())
            .is_none_or(|length| length > self.limits.max_field_bytes)
            || self
                .retained_bytes
                .checked_add(text.len())
                .is_none_or(|length| length > self.limits.max_bytes)
        {
            return self.fail(OutageAssemblyError::Overflow);
        }
        let value = &mut self.open.as_mut().expect("open field checked above").1;
        if value.try_reserve_exact(text.len()).is_err() {
            return self.fail(OutageAssemblyError::Overflow);
        }
        value.push_str(text);
        self.retained_bytes += text.len();
        Ok(())
    }

    pub fn discard(&mut self) {
        let _ = self.fail(OutageAssemblyError::RetentionLoss);
    }

    pub(super) fn validate_seal(
        &self,
        route: &ProviderObservationRoute,
    ) -> Result<(), OutageAssemblyError> {
        if let Some(loss) = self.loss {
            return Err(loss);
        }
        if self.open.is_some() {
            return Err(OutageAssemblyError::Malformed);
        }
        let route_bytes = route
            .thread_id()
            .as_str()
            .len()
            .checked_add(route.turn_id().as_str().len())
            .and_then(|bytes| bytes.checked_add(std::mem::size_of::<ProviderObservationRoute>()));
        if route.thread_id().as_str().len() > self.limits.max_field_bytes
            || route.turn_id().as_str().len() > self.limits.max_field_bytes
            || self.retained_entries() >= self.limits.max_entries
            || route_bytes
                .and_then(|bytes| self.retained_bytes.checked_add(bytes))
                .is_none_or(|bytes| bytes > self.limits.max_bytes)
        {
            return Err(OutageAssemblyError::Overflow);
        }
        self.item_id().map(|_| ())
    }

    fn item_id(&self) -> Result<CasItemId, OutageAssemblyError> {
        let mut item = None;
        for entry in &self.entries {
            if let Entry::Field(ProviderValueContext::Field(ProviderField::ItemId), text) = entry {
                if item.is_some() || text.len() > 256 {
                    return Err(OutageAssemblyError::Malformed);
                }
                item =
                    Some(CasItemId::new(text.clone()).map_err(|_| OutageAssemblyError::Malformed)?);
            }
        }
        item.ok_or(OutageAssemblyError::Malformed)
    }

    pub fn abandon(self, buffer: &mut OutageBuffer) {
        self.gap_connection(buffer);
    }

    pub fn seal(
        self,
        buffer: &mut OutageBuffer,
        route: Option<&ProviderObservationRoute>,
    ) -> Result<(), OutageAssemblyError> {
        let Some(route) = route else {
            self.gap_connection(buffer);
            return Err(OutageAssemblyError::MissingRoute);
        };
        let mut matches = buffer.targets.iter().enumerate().filter(|(_, state)| {
            self.same_connection(&state.target)
                && state.target.identity.cas_thread_id() == route.thread_id()
                && &state.target.cas_turn == route.turn_id()
        });
        let index = matches.next().map(|(index, _)| index);
        if index.is_none() || matches.next().is_some() {
            self.gap_connection(buffer);
            return Err(OutageAssemblyError::UnqualifiedRoute);
        }
        let index = index.expect("unique route checked above");
        if let Some(loss) = self.loss {
            buffer.targets[index].gap = true;
            return Err(loss);
        }
        if self.open.is_some() {
            buffer.targets[index].gap = true;
            return Err(OutageAssemblyError::Malformed);
        }
        let item = match self.item_id() {
            Ok(item) => item,
            Err(error) => {
                buffer.targets[index].gap = true;
                return Err(error);
            }
        };
        let target = buffer.targets[index].target.clone();
        let mut lost = buffer
            .offer(
                &target,
                OutageFact::Identity {
                    observation: self.observation,
                    item: &item,
                },
            )
            .is_err();
        lost |= buffer
            .offer(
                &target,
                OutageFact::Lifecycle {
                    observation: self.observation,
                    begin: self.begin,
                },
            )
            .is_err();
        let final_answer = self.entries.iter().any(|entry| {
            matches!(
                entry,
                Entry::Control(ProviderObservationControl::Enum {
                    context: ProviderValueContext::Field(ProviderField::MessagePhase),
                    value: ProviderEnumValue::FinalAnswer,
                })
            )
        });
        for (ordinal, entry) in self.entries.iter().enumerate() {
            let fact = match entry {
                Entry::Field(context, text) => OutageFact::CompleteField {
                    observation: self.observation,
                    ordinal: ordinal as u64,
                    context: *context,
                    kind: text_kind(*context, self.begin, final_answer),
                    text,
                },
                Entry::Control(control) => OutageFact::Control {
                    observation: self.observation,
                    ordinal: ordinal as u64,
                    priority: control_priority(*control, self.begin, final_answer),
                    control: *control,
                },
            };
            lost |= buffer.offer(&target, fact).is_err();
        }
        if lost || buffer.targets[index].gap {
            Err(OutageAssemblyError::RetentionLoss)
        } else {
            Ok(())
        }
    }

    fn reserve_entry(&mut self) -> Result<(), OutageAssemblyError> {
        let size = std::mem::size_of::<Entry>();
        if self.retained_entries() >= self.limits.max_entries
            || self
                .retained_bytes
                .checked_add(size)
                .is_none_or(|bytes| bytes > self.limits.max_bytes)
            || self.entries.try_reserve_exact(1).is_err()
        {
            return self.fail(OutageAssemblyError::Overflow);
        }
        self.retained_bytes += size;
        Ok(())
    }

    fn fail(&mut self, error: OutageAssemblyError) -> Result<(), OutageAssemblyError> {
        self.entries = Vec::new();
        self.open = None;
        self.retained_bytes = 0;
        self.loss = Some(self.loss.unwrap_or(error));
        Err(self.loss.expect("loss just installed"))
    }

    fn same_connection(&self, target: &OutageTarget) -> bool {
        self.connection == target.connection()
    }

    fn gap_connection(&self, buffer: &mut OutageBuffer) {
        self.connection.record_gap(buffer);
    }
}

fn text_kind(
    context: ProviderValueContext,
    begin: ProviderObservationBegin,
    final_answer: bool,
) -> OutageTextKind {
    let kind = match begin {
        ProviderObservationBegin::Item { kind, .. } => kind,
        ProviderObservationBegin::Delta { kind } => kind.expected_item_kind(),
    };
    match root(context) {
        ProviderField::ItemId | ProviderField::LifecycleObservedAt => {
            OutageTextKind::IdentityCorrelation
        }
        ProviderField::ClientId => OutageTextKind::UserCorrelation,
        ProviderField::ImageGenerationSavedPath => OutageTextKind::MediaHandoff,
        ProviderField::AgentMessageText if final_answer => OutageTextKind::AssistantFinal,
        ProviderField::AgentMessageText
        | ProviderField::PlanText
        | ProviderField::ReasoningSummaries
        | ProviderField::ReasoningSummary => OutageTextKind::TranscriptNarrative,
        ProviderField::DeltaText
            if matches!(
                kind,
                ProviderItemKind::AgentMessage
                    | ProviderItemKind::Plan
                    | ProviderItemKind::Reasoning
            ) =>
        {
            OutageTextKind::TranscriptNarrative
        }
        _ => OutageTextKind::Operational,
    }
}

fn root(context: ProviderValueContext) -> ProviderField {
    match context {
        ProviderValueContext::Field(field) => field,
        ProviderValueContext::Structured { root, .. } => root,
    }
}

fn control_priority(
    control: ProviderObservationControl,
    begin: ProviderObservationBegin,
    final_answer: bool,
) -> OutagePriority {
    let context = match control {
        ProviderObservationControl::BeginField(context)
        | ProviderObservationControl::EndField(context)
        | ProviderObservationControl::BeginContainer { context, .. }
        | ProviderObservationControl::EndContainer { context, .. }
        | ProviderObservationControl::BeginElement { context, .. }
        | ProviderObservationControl::EndElement { context, .. }
        | ProviderObservationControl::Enum { context, .. }
        | ProviderObservationControl::Scalar { context, .. } => context,
        ProviderObservationControl::BeginObjectEntry { root, .. }
        | ProviderObservationControl::EndObjectEntry { root, .. } => {
            ProviderValueContext::Field(root)
        }
    };
    match root(context) {
        ProviderField::ItemId
        | ProviderField::LifecycleObservedAt
        | ProviderField::DeltaSummaryIndex
        | ProviderField::DeltaContentIndex
        | ProviderField::MessagePhase => OutagePriority::IdentityCorrelation,
        ProviderField::ClientId => OutagePriority::UserCorrelation,
        ProviderField::ImageGenerationSavedPath => OutagePriority::MediaHandoff,
        _ => match text_kind(context, begin, final_answer) {
            OutageTextKind::IdentityCorrelation => OutagePriority::IdentityCorrelation,
            OutageTextKind::UserCorrelation => OutagePriority::UserCorrelation,
            OutageTextKind::MediaHandoff => OutagePriority::MediaHandoff,
            OutageTextKind::AssistantFinal => OutagePriority::AssistantFinal,
            OutageTextKind::TranscriptNarrative => OutagePriority::TranscriptNarrative,
            OutageTextKind::Operational => OutagePriority::Operational,
        },
    }
}
