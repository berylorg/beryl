mod tool;

use crate::provider_observation::{ProviderEnumValue as E, ProviderField as F};
use crate::{ProviderFrameSinkV1, ProviderLogicalTextRoleV1};

use super::super::{Encoder, ObservationEncodeError};
use crate::provider_observation::compiler::replay::FieldSelector;

impl<S: ProviderFrameSinkV1> Encoder<'_, '_, S> {
    pub(super) fn command_execution(&mut self) -> Result<(), ObservationEncodeError<S::Error>> {
        self.text(
            FieldSelector::top(F::Command).into(),
            Some(ProviderLogicalTextRoleV1::Operational),
        )?;
        self.text(FieldSelector::top(F::WorkingDirectory).into(), None)?;
        self.optional_text(FieldSelector::top(F::ProcessId), None)?;

        let source = self
            .optional_enum(FieldSelector::top(F::CommandSource))?
            .unwrap_or(E::Agent);
        self.enum_tag(
            source,
            &[
                E::Agent,
                E::UserShell,
                E::UnifiedExecStartup,
                E::UnifiedExecInteraction,
            ],
        )?;
        let status = self.required_enum(FieldSelector::top(F::CommandStatus))?;
        self.status4(status)?;

        let count = self.list_count(FieldSelector::top(F::CommandActions), true)?;
        self.u64(count)?;
        for index in 0..count {
            self.command_action(index)?;
        }
        self.optional_text(
            FieldSelector::top(F::AggregatedOutput),
            Some(ProviderLogicalTextRoleV1::Operational),
        )?;
        let exit = self.optional_signed(FieldSelector::top(F::ExitCode))?;
        self.option(exit.is_some(), |encoder| {
            encoder
                .i32(i32::try_from(exit.unwrap()).map_err(|_| super::value_mismatch(F::ExitCode))?)
        })?;
        let duration = self.optional_signed(FieldSelector::top(F::DurationMs))?;
        self.option(duration.is_some(), |encoder| encoder.i64(duration.unwrap()))
    }

    fn command_action(&mut self, index: u64) -> Result<(), ObservationEncodeError<S::Error>> {
        let discriminant = FieldSelector::in_list(F::CommandActionKind, F::CommandActions, index)
            .with_object(F::CommandActions);
        let variant = self.required_enum(discriminant)?;
        let field = |value| {
            FieldSelector::in_list(value, F::CommandActions, index).with_object(F::CommandActions)
        };
        match variant {
            E::Read => {
                self.u8(0)?;
                self.text(field(F::CommandActionCommand).into(), None)?;
                self.text(field(F::CommandActionName).into(), None)?;
                self.text(field(F::CommandActionPath).into(), None)
            }
            E::ListFiles => {
                self.u8(1)?;
                self.text(field(F::CommandActionCommand).into(), None)?;
                self.optional_text(field(F::CommandActionPath), None)
            }
            E::Search => {
                self.u8(2)?;
                self.text(field(F::CommandActionCommand).into(), None)?;
                self.optional_text(field(F::CommandActionQuery), None)?;
                self.optional_text(field(F::CommandActionPath), None)
            }
            E::Unknown => {
                self.u8(3)?;
                self.text(field(F::CommandActionCommand).into(), None)
            }
            _ => Err(super::value_mismatch(F::CommandActionKind)),
        }
    }

    pub(super) fn file_change(
        &mut self,
        list_field: F,
    ) -> Result<(), ObservationEncodeError<S::Error>> {
        let status = self.required_enum(FieldSelector::top(F::FileChangeStatus))?;
        self.status4(status)?;
        self.file_changes(list_field)
    }

    pub(super) fn file_changes(
        &mut self,
        list_field: F,
    ) -> Result<(), ObservationEncodeError<S::Error>> {
        let count = self.list_count(FieldSelector::top(list_field), true)?;
        self.u64(count)?;
        for index in 0..count {
            let field = |value| FieldSelector::in_list(value, list_field, index);
            self.text(field(F::FileChangePath).into(), None)?;
            self.text(
                field(F::FileChangeDiff).into(),
                Some(ProviderLogicalTextRoleV1::Operational),
            )?;
            let kind =
                self.required_enum(field(F::FileChangeKind).with_object(F::FileChangeKind))?;
            match kind {
                E::Add => self.u8(0)?,
                E::Delete => self.u8(1)?,
                E::Update => {
                    self.u8(2)?;
                    self.optional_text(
                        field(F::FileChangeMovePath).with_object(F::FileChangeKind),
                        None,
                    )?;
                }
                _ => return Err(super::value_mismatch(F::FileChangeKind)),
            }
        }
        Ok(())
    }

    pub(super) fn status4(&mut self, value: E) -> Result<(), ObservationEncodeError<S::Error>> {
        self.enum_tag(
            value,
            &[E::InProgress, E::Completed, E::Failed, E::Declined],
        )
    }

    pub(super) fn status3(&mut self, value: E) -> Result<(), ObservationEncodeError<S::Error>> {
        self.enum_tag(value, &[E::InProgress, E::Completed, E::Failed])
    }
}
