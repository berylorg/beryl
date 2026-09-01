use gpui::{AnyView, KeyDownEvent, SharedString, div, px, rgb};

use super::*;
use crate::cas_projection::NativeLineageRecoveryCommand;

const DISPOSAL_FAILURE_RETRY_DISABLED: &str = "Retry is unavailable because composer disposal did not complete. Your preserved draft has not been discarded.";
const DISPOSAL_FAILURE_RECOVER_DISABLED: &str = "Syndic-history recovery is unavailable because composer disposal did not complete. Your preserved draft has not been discarded.";
const DISPOSAL_ACTIVE_RETRY_DISABLED: &str =
    "Retry is unavailable while composer disposal is in progress.";
const DISPOSAL_ACTIVE_RECOVER_DISABLED: &str =
    "Syndic-history recovery is unavailable while composer disposal is in progress.";

struct NativeLineageDisabledTooltip {
    id: &'static str,
    explanation: SharedString,
}

impl Render for NativeLineageDisabledTooltip {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id(self.id)
            .debug_selector({
                let id = self.id;
                move || id.to_owned()
            })
            .max_w(px(320.0))
            .px_2()
            .py_1()
            .rounded_sm()
            .border_1()
            .border_color(rgb(0x5f6875))
            .bg(rgb(0x20242b))
            .text_sm()
            .text_color(rgb(0xf2f4f7))
            .child(self.explanation.clone())
    }
}

impl MainWindowConversationComposerMount {
    #[cfg(feature = "test-faults")]
    pub fn test_native_lineage_prompt_diagnostics(
        &self,
    ) -> super::super::MainWindowNativeLineagePromptDiagnostics {
        use super::super::MainWindowNativeLineagePromptCommandPresentation::{
            Disabled, Enabled, Running,
        };

        if self.native_lineage_disposal_active {
            return super::super::MainWindowNativeLineagePromptDiagnostics {
                retry: Disabled,
                recover_from_syndic: Disabled,
                failed_command: None,
                retry_label: "Retry",
                recover_label: "Recover from Syndic history",
                retry_disabled_explanation: if self.native_lineage_failure.is_some() {
                    DISPOSAL_FAILURE_RETRY_DISABLED
                } else {
                    DISPOSAL_ACTIVE_RETRY_DISABLED
                },
                recover_disabled_explanation: if self.native_lineage_failure.is_some() {
                    DISPOSAL_FAILURE_RECOVER_DISABLED
                } else {
                    DISPOSAL_ACTIVE_RECOVER_DISABLED
                },
                local_failure_present: self.native_lineage_failure.is_some(),
                disposal_failure_present: self.native_lineage_failure.is_some(),
            };
        }

        let status = self
            .native_lineage_snapshot
            .map(NativeLineageRecoverySnapshot::status)
            .unwrap_or(NativeLineageRecoveryStatus::Unavailable);
        let (
            retry,
            recover_from_syndic,
            failed_command,
            retry_label,
            recover_label,
            retry_disabled_explanation,
            recover_disabled_explanation,
        ) = match status {
            NativeLineageRecoveryStatus::Ready { recovery_available } => (
                Enabled,
                if recovery_available {
                    Enabled
                } else {
                    Disabled
                },
                None,
                "Retry",
                "Recover from Syndic history",
                "",
                if recovery_available {
                    ""
                } else {
                    "Recover from Syndic history is unavailable because the selected history cannot be represented safely or contains a repair-pending turn."
                },
            ),
            NativeLineageRecoveryStatus::Running { command } => (
                if matches!(command, NativeLineageRecoveryCommand::Retry) {
                    Running
                } else {
                    Disabled
                },
                if matches!(command, NativeLineageRecoveryCommand::RecoverFromSyndic) {
                    Running
                } else {
                    Disabled
                },
                None,
                if matches!(command, NativeLineageRecoveryCommand::Retry) {
                    "Retrying…"
                } else {
                    "Retry"
                },
                if matches!(command, NativeLineageRecoveryCommand::RecoverFromSyndic) {
                    "Recovering…"
                } else {
                    "Recover from Syndic history"
                },
                match command {
                    NativeLineageRecoveryCommand::Retry => {
                        "Retry is already running for the exact native source."
                    }
                    NativeLineageRecoveryCommand::RecoverFromSyndic => {
                        "Retry is unavailable while Syndic-history recovery is running."
                    }
                },
                match command {
                    NativeLineageRecoveryCommand::Retry => {
                        "Syndic-history recovery is unavailable while the exact-source retry is running."
                    }
                    NativeLineageRecoveryCommand::RecoverFromSyndic => {
                        "Syndic-history recovery is already running."
                    }
                },
            ),
            NativeLineageRecoveryStatus::Failed {
                command,
                recovery_available,
            } => (
                Enabled,
                if recovery_available {
                    Enabled
                } else {
                    Disabled
                },
                Some(command),
                "Retry",
                "Recover from Syndic history",
                "",
                if recovery_available {
                    ""
                } else {
                    "Recover from Syndic history is unavailable because the selected history cannot be represented safely or contains a repair-pending turn."
                },
            ),
            NativeLineageRecoveryStatus::Loading
            | NativeLineageRecoveryStatus::Unavailable
            | NativeLineageRecoveryStatus::Leaving { .. } => (
                Disabled,
                Disabled,
                None,
                "Retry",
                "Recover from Syndic history",
                "Recovery commands are unavailable because this recovery decision is not currently actionable.",
                "Recovery commands are unavailable because this recovery decision is not currently actionable.",
            ),
        };
        super::super::MainWindowNativeLineagePromptDiagnostics {
            retry,
            recover_from_syndic,
            failed_command,
            retry_label,
            recover_label,
            retry_disabled_explanation,
            recover_disabled_explanation,
            local_failure_present: self.native_lineage_failure.is_some(),
            disposal_failure_present: false,
        }
    }

    fn invoke_native_lineage_command(
        &mut self,
        command: NativeLineageRecoveryCommand,
        cx: &mut Context<Self>,
    ) {
        if self.native_lineage_disposal_active {
            return;
        }
        let result = self
            .native_lineage_recovery
            .as_ref()
            .zip(self.native_lineage_snapshot)
            .ok_or(())
            .and_then(|(control, snapshot)| {
                control.submit(snapshot.key(), command).map_err(|_| ())
            });
        if result.is_err() {
            self.native_lineage_failure = Some(
                "That recovery command is no longer available for the selected thread.".to_owned(),
            );
        } else {
            self.native_lineage_failure = None;
        }
        cx.notify();
    }

    fn render_native_lineage_command(
        &self,
        id: &'static str,
        tooltip_id: &'static str,
        label: &'static str,
        command: NativeLineageRecoveryCommand,
        enabled: bool,
        disabled_explanation: &'static str,
        focus: &FocusHandle,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let mut button = div()
            .id(id)
            .debug_selector(move || id.to_owned())
            .track_focus(focus)
            .tab_stop(enabled)
            .px_3()
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(if enabled {
                rgb(0x7c8797)
            } else {
                rgb(0x4b5563)
            })
            .bg(if enabled {
                rgb(0x273142)
            } else {
                rgb(0x1f2937)
            })
            .text_sm()
            .text_color(if enabled {
                rgb(0xf8fafc)
            } else {
                rgb(0x94a3b8)
            })
            .when(enabled, |button| {
                button
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(0x374151)))
                    .focus(|style| style.border_color(rgb(0xf59e0b)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.invoke_native_lineage_command(command, cx);
                    }))
                    .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                        if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                            cx.stop_propagation();
                            this.invoke_native_lineage_command(command, cx);
                        }
                    }))
            })
            .child(label);
        if !enabled {
            let explanation = SharedString::from(disabled_explanation);
            button = button.tooltip(move |_, cx| -> AnyView {
                cx.new(|_| NativeLineageDisabledTooltip {
                    id: tooltip_id,
                    explanation: explanation.clone(),
                })
                .into()
            });
        }
        button
    }

    pub(super) fn render_native_lineage_prompt(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let status = self
            .native_lineage_snapshot
            .map(NativeLineageRecoverySnapshot::status)
            .unwrap_or(NativeLineageRecoveryStatus::Unavailable);
        let (
            retry_enabled,
            recover_enabled,
            retry_label,
            recover_label,
            retry_disabled,
            recover_disabled,
            explanation,
            backend_failure,
        ) = if self.native_lineage_disposal_active {
            (
                false,
                false,
                "Retry",
                "Recover from Syndic history",
                if self.native_lineage_failure.is_some() {
                    DISPOSAL_FAILURE_RETRY_DISABLED
                } else {
                    DISPOSAL_ACTIVE_RETRY_DISABLED
                },
                if self.native_lineage_failure.is_some() {
                    DISPOSAL_FAILURE_RECOVER_DISABLED
                } else {
                    DISPOSAL_ACTIVE_RECOVER_DISABLED
                },
                if self.native_lineage_failure.is_some() {
                    "Composer disposal did not complete. Recovery commands are unavailable, and your preserved draft has not been discarded."
                } else {
                    "Composer disposal is in progress. Recovery commands are temporarily unavailable."
                },
                None,
            )
        } else {
            match status {
                NativeLineageRecoveryStatus::Ready { recovery_available } => (
                    true,
                    recovery_available,
                    "Retry",
                    "Recover from Syndic history",
                    "",
                    if recovery_available {
                        ""
                    } else {
                        "Recover from Syndic history is unavailable because the selected history cannot be represented safely or contains a repair-pending turn."
                    },
                    "The selected conversation cannot continue from its current native source. Retry the exact source, or recover from durable Syndic history when available.",
                    None,
                ),
                NativeLineageRecoveryStatus::Failed {
                    command,
                    recovery_available,
                } => (
                    true,
                    recovery_available,
                    "Retry",
                    "Recover from Syndic history",
                    "",
                    if recovery_available {
                        ""
                    } else {
                        "Recover from Syndic history is unavailable because the selected history cannot be represented safely or contains a repair-pending turn."
                    },
                    match command {
                        NativeLineageRecoveryCommand::Retry => {
                            "Retrying the exact native source failed. Your draft remains preserved; retry again or recover from durable Syndic history when available."
                        }
                        NativeLineageRecoveryCommand::RecoverFromSyndic => {
                            "Recovering from durable Syndic history failed. Your draft remains preserved; retry the exact source or try history recovery again when available."
                        }
                    },
                    Some(match command {
                        NativeLineageRecoveryCommand::Retry => {
                            "The exact-source retry failed without changing your preserved draft."
                        }
                        NativeLineageRecoveryCommand::RecoverFromSyndic => {
                            "The Syndic-history recovery failed without changing your preserved draft."
                        }
                    }),
                ),
                NativeLineageRecoveryStatus::Loading => (
                    false,
                    false,
                    "Retry",
                    "Recover from Syndic history",
                    "Recovery commands are unavailable while the exact recovery decision is loading.",
                    "Recovery commands are unavailable while the exact recovery decision is loading.",
                    "Loading the exact recovery choices for this conversation.",
                    None,
                ),
                NativeLineageRecoveryStatus::Running { command } => (
                    false,
                    false,
                    if matches!(command, NativeLineageRecoveryCommand::Retry) {
                        "Retrying…"
                    } else {
                        "Retry"
                    },
                    if matches!(command, NativeLineageRecoveryCommand::RecoverFromSyndic) {
                        "Recovering…"
                    } else {
                        "Recover from Syndic history"
                    },
                    match command {
                        NativeLineageRecoveryCommand::Retry => {
                            "Retry is already running for the exact native source."
                        }
                        NativeLineageRecoveryCommand::RecoverFromSyndic => {
                            "Retry is unavailable while Syndic-history recovery is running."
                        }
                    },
                    match command {
                        NativeLineageRecoveryCommand::Retry => {
                            "Syndic-history recovery is unavailable while the exact-source retry is running."
                        }
                        NativeLineageRecoveryCommand::RecoverFromSyndic => {
                            "Syndic-history recovery is already running."
                        }
                    },
                    match command {
                        NativeLineageRecoveryCommand::Retry => {
                            "Retrying the exact native source. Your draft remains preserved."
                        }
                        NativeLineageRecoveryCommand::RecoverFromSyndic => {
                            "Recovering from durable Syndic history. Your draft remains preserved."
                        }
                    },
                    None,
                ),
                NativeLineageRecoveryStatus::Unavailable => (
                    false,
                    false,
                    "Retry",
                    "Recover from Syndic history",
                    "Recovery commands are unavailable because this recovery decision is no longer actionable.",
                    "Recovery commands are unavailable because this recovery decision is no longer actionable.",
                    "The recovery decision is no longer actionable for this conversation.",
                    None,
                ),
                NativeLineageRecoveryStatus::Leaving { .. } => (
                    false,
                    false,
                    "Retry",
                    "Recover from Syndic history",
                    "Recovery commands are unavailable while the recovered conversation is being published.",
                    "Recovery commands are unavailable while the recovered conversation is being published.",
                    "Publishing the recovered conversation. Your preserved draft remains unchanged.",
                    None,
                ),
            }
        };
        let failure = self
            .native_lineage_failure
            .clone()
            .or_else(|| backend_failure.map(str::to_owned));
        div()
            .id("native-lineage-recovery-prompt")
            .debug_selector(|| "native-lineage-recovery-prompt".to_owned())
            .w_full()
            .min_h(px(64.0))
            .px_3()
            .py_2()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_3()
            .rounded_lg()
            .border_1()
            .border_color(rgb(0xf59e0b))
            .bg(rgb(0x111827))
            .text_color(rgb(0xe5e7eb))
            .when(
                matches!(status, NativeLineageRecoveryStatus::Leaving { .. }),
                |root| root.opacity(0.7),
            )
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "enter" && retry_enabled {
                    cx.stop_propagation();
                    this.invoke_native_lineage_command(NativeLineageRecoveryCommand::Retry, cx);
                }
            }))
            .child(
                div()
                    .id("native-lineage-recovery-message")
                    .debug_selector(|| "native-lineage-recovery-message".to_owned())
                    .min_w_0()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .id("native-lineage-recovery-heading")
                            .debug_selector(|| "native-lineage-recovery-heading".to_owned())
                            .text_sm()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(0xf8fafc))
                            .child("Conversation recovery needs attention"),
                    )
                    .child(div().text_sm().text_color(rgb(0xcbd5e1)).child(explanation))
                    .when_some(failure, |message, failure| {
                        message.child(
                            div()
                                .id("native-lineage-recovery-failure")
                                .debug_selector(|| "native-lineage-recovery-failure".to_owned())
                                .text_sm()
                                .text_color(rgb(0xfca5a5))
                                .child(failure),
                        )
                    }),
            )
            .child(
                div()
                    .id("native-lineage-recovery-commands")
                    .debug_selector(|| "native-lineage-recovery-commands".to_owned())
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_end()
                    .gap_2()
                    .child(self.render_native_lineage_command(
                        "native-lineage-retry-command",
                        "native-lineage-retry-disabled-tooltip",
                        retry_label,
                        NativeLineageRecoveryCommand::Retry,
                        retry_enabled,
                        retry_disabled,
                        &self.native_lineage_retry_focus,
                        cx,
                    ))
                    .child(self.render_native_lineage_command(
                        "native-lineage-recover-from-syndic",
                        "native-lineage-recover-disabled-tooltip",
                        recover_label,
                        NativeLineageRecoveryCommand::RecoverFromSyndic,
                        recover_enabled,
                        recover_disabled,
                        &self.native_lineage_recovery_focus,
                        cx,
                    )),
            )
    }
}
