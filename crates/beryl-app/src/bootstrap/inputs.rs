use super::BootstrapError;
use crate::{
    activity_service::ActivityServiceLimits,
    app_services::{AppServiceConfiguration, MainWindowServiceInputs},
    cas_projection::{
        MinimumTurnCaptureReserve, OrdinaryTurnExecutionRequest, ProjectionServiceConfig,
        RuntimeInterestConfig, ScheduledOrdinaryRequestPolicy,
    },
    composer_host::{
        ComposerHostActivationRequest, ComposerHostInitialDemand, ComposerHostRequestId,
        ComposerHostRequestPurpose,
    },
    composer_marker_seal::DraftMarkerSealServiceLimits,
    discussion_handoff_limits::{HandoffScanConfiguration, HandoffScanLimits},
    main_window::{MainWindowComposerSelectionIdentity, MainWindowConversationComposerConfig},
    theme_runtime::{AppearanceCoordinatorConfig, ThemeRuntimeConfig},
};
use beryl_backend::{ThreadStartOptions, TurnStartOptions};
use beryl_home_store::CursorReadLimits;
use beryl_model::{
    SyndicDraftId, SyndicThreadId, WindowBounds, WindowDisplayState, WindowPlacement,
};
use beryl_state::{ThemeManifestReadLimits, ThemePageLimits};
use gpui::{
    SharedString, StreamingLayoutBinding, StreamingLayoutLimits, StreamingLayoutPosition, TextRun,
    black, font, px,
};
use gpui_scrollbar::ScrollbarStyle;
use gpui_text_input::{
    ClipboardLimits, ExactGeometryLimits, MutationLimits, ObjectResidencyLimits,
    PresentationGeneration, RangeSettlementCoordinator, RangeTextInputConfig, RangeTextInputLimits,
    ResidencyLimits, SegmentationLimits, StreamingGeometryStyle, StreamingOversizePresentation,
    TextInputAtomClipboardPolicy, TextInputEnterKey, TextInputRichPastePolicy, TextInputTheme,
};
use std::{
    num::{NonZeroU64, NonZeroUsize},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use syndic_storage::{
    DraftEditHistoryPolicyV1, DraftEditorCandidateSessionIdV1, DraftPieceMarkerDemandV1,
    DraftPieceMarkerDirectionV1, DraftPieceMarkerScopeV1, DraftPieceOperationIdV1,
    DraftPieceTextDemandV1, SyndicTimestamp,
};

pub(super) fn identity() -> Result<[u8; 16], BootstrapError> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(|error| BootstrapError::Inputs(error.to_string()))?;
    Ok(bytes)
}

fn token_directory() -> Result<crate::cas_projection::RuntimeTokenDirectory, BootstrapError> {
    let path = std::fs::canonicalize(std::env::temp_dir())
        .map_err(|error| BootstrapError::Inputs(format!("temporary directory: {error}")))?;
    let path = path
        .to_str()
        .ok_or_else(|| BootstrapError::Inputs("temporary directory is not Unicode".into()))?;
    let host = beryl_model::AdmittedHostPath::from_admitted(beryl_model::PathFlavor::Windows, path)
        .map_err(|error| BootstrapError::Inputs(error.to_string()))?;
    Ok(crate::cas_projection::RuntimeTokenDirectory::from_admitted(
        host,
    ))
}

pub(super) fn placement() -> WindowPlacement {
    WindowPlacement::new(
        WindowBounds::new(40, 40, 1000, 760).unwrap(),
        WindowDisplayState::Normal,
        None,
        None,
    )
}

pub(super) fn services() -> Result<AppServiceConfiguration, BootstrapError> {
    let n = |value| NonZeroUsize::new(value).unwrap();
    fn configuration(error: impl std::fmt::Debug) -> BootstrapError {
        BootstrapError::Inputs(format!("{error:?}"))
    }
    Ok(AppServiceConfiguration {
        paste_resources: crate::main_window::MainWindowComposerPasteResources::new(1, 4096)
            .map_err(configuration)?,
        projection: ProjectionServiceConfig::try_new(
            64,
            16,
            MinimumTurnCaptureReserve::try_new(64 * 1024).map_err(configuration)?,
        )
        .map_err(configuration)?,
        runtime_interest: RuntimeInterestConfig::new(n(4), n(4), Duration::from_secs(10))
            .map_err(configuration)?,
        session_policy: ScheduledOrdinaryRequestPolicy::new(
            ThreadStartOptions::persistent(),
            None,
            Duration::from_secs(30),
            OrdinaryTurnExecutionRequest::new(TurnStartOptions::default(), Duration::from_secs(30)),
        ),
        token_directory: token_directory()?,
        wsl_supervisor_artifact: None,
        handoff: HandoffScanLimits::try_from(HandoffScanConfiguration {
            handoff_recovery_page_items: 16,
            handoff_recovery_page_encoded_bytes: beryl_state::HANDOFF_LIVE_RECORD_MAX_ENCODED_BYTES,
            handoff_job_record_encoded_bytes: beryl_state::HANDOFF_JOB_RECORD_MAX_ENCODED_BYTES,
            handoff_reconcile_slots: 4,
            handoff_ready_job_items: 16,
        })
        .map_err(configuration)?,
        marker: DraftMarkerSealServiceLimits::new(n(16), n(1)).map_err(configuration)?,
        activity: ActivityServiceLimits::new(
            n(4),
            n(4),
            CursorReadLimits::new(64, 256 * 1024).map_err(configuration)?,
        ),
        theme: ThemeRuntimeConfig::new(
            AppearanceCoordinatorConfig::new(n(beryl_state::MAX_RESTORABLE_WINDOWS + 2)),
            NonZeroU64::new(8 * 1024 * 1024).unwrap(),
            ThemeManifestReadLimits::new(n(4096), n(16 * 1024), n(256 * 1024))
                .map_err(configuration)?,
            ThemePageLimits::new(n(32), n(64 * 1024)).map_err(configuration)?,
            Duration::from_millis(250),
            n(16),
            n(32),
            NonZeroU64::new(1024 * 1024).unwrap(),
            n(16),
        ),
    })
}

pub(super) fn windows() -> MainWindowServiceInputs {
    MainWindowServiceInputs {
        request_source: Arc::new(|window, target, context| {
            let execution = context.execution_binding(target)?;
            let thread = SyndicThreadId::from_bytes(identity().map_err(|e| e.to_string())?);
            let draft = SyndicDraftId::from_bytes(identity().map_err(|e| e.to_string())?);
            let at = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| e.to_string())?;
            crate::window_acquisition::RuntimeBackedWindowAcquisitionRequest::new(
                window,
                target,
                placement(),
                thread,
                draft,
                execution,
                SyndicTimestamp::from_unix_millis(
                    u64::try_from(at.as_millis()).map_err(|e| e.to_string())?,
                ),
                DraftEditHistoryPolicyV1::new(8 * 1024 * 1024, 1)
                    .ok_or("draft history policy is unavailable")?,
            )
            .map_err(|e| format!("{e:?}"))
        }),
        activation_source: Arc::new(|acquisition| activation(acquisition.thread_id())),
        restored_activation_source: Arc::new(|record| {
            activation(
                record
                    .selected_thread()
                    .ok_or("restored window has no selected thread")?
                    .thread_id(),
            )
        }),
        configurator_source: Arc::new(|| Box::new(configure)),
    }
}

pub(super) fn activation(
    thread: SyndicThreadId,
) -> Result<(ComposerHostActivationRequest, DraftPieceOperationIdV1), String> {
    let demands = vec![
        ComposerHostInitialDemand::Text {
            request_id: ComposerHostRequestId::new(NonZeroU64::new(1).unwrap()),
            purpose: ComposerHostRequestPurpose::Geometry,
            demand: DraftPieceTextDemandV1::Forward(0),
            max_bytes: 4096,
        },
        ComposerHostInitialDemand::Markers {
            request_id: ComposerHostRequestId::new(NonZeroU64::new(2).unwrap()),
            purpose: ComposerHostRequestPurpose::Geometry,
            demand: DraftPieceMarkerDemandV1::new(
                DraftPieceMarkerScopeV1::ExactAnchor(0),
                DraftPieceMarkerDirectionV1::Forward,
                None,
                48,
                65536,
            ),
        },
    ];
    Ok((
        ComposerHostActivationRequest::new(
            thread,
            DraftEditorCandidateSessionIdV1::from_bytes(identity().map_err(|e| e.to_string())?),
            DraftPieceOperationIdV1::from_bytes(identity().map_err(|e| e.to_string())?),
            NonZeroU64::new(1).unwrap(),
            None,
            demands.into_boxed_slice(),
        ),
        DraftPieceOperationIdV1::from_bytes(identity().map_err(|e| e.to_string())?),
    ))
}

fn configure(
    selection: MainWindowComposerSelectionIdentity,
) -> Result<MainWindowConversationComposerConfig, String> {
    MainWindowConversationComposerConfig::new(selection, widget(selection))
        .map_err(|e| e.to_string())
}

fn widget(selection: MainWindowComposerSelectionIdentity) -> RangeTextInputConfig {
    let run = TextRun {
        len: 0,
        font: font(".SystemUIFont"),
        color: black(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    RangeTextInputConfig {
        binding: selection.binding().range_binding(),
        presentation_generation: PresentationGeneration::new(
            selection.binding().presentation_generation().get(),
        ),
        enter_key: TextInputEnterKey::Propagate,
        atom_clipboard_policy: TextInputAtomClipboardPolicy::Propagate,
        rich_paste_policy: TextInputRichPastePolicy::Propagate,
        layout: StreamingLayoutBinding {
            input_id: 1,
            segment_policy_id: 1,
            start_position: StreamingLayoutPosition::at(0),
            wrap_width: px(640.),
            font_size: px(14.),
            line_height: px(20.),
            limits: StreamingLayoutLimits {
                segment_bytes: 4096,
                runs: 16,
                decorations: 16,
                glyphs: 4096,
                wraps: 256,
                maps: 4097,
                fragments: 8,
                retained_items: 32768,
                retained_bytes: 2 * 1024 * 1024,
            },
        },
        style: StreamingGeometryStyle::new(
            run,
            StreamingOversizePresentation::new(
                SharedString::new_static(""),
                vec![],
                px(14.),
                px(20.),
                px(14.),
                None,
            ),
        ),
        geometry_limits: ExactGeometryLimits::new(4096, 16, 2 * 1024 * 1024, 32768).unwrap(),
        residency_limits: ResidencyLimits::new(9, 384 * 1024, 6, 384 * 1024).unwrap(),
        object_residency_limits: ObjectResidencyLimits::new(9, 48, 65536, 65536, 6, 48, 65536)
            .unwrap(),
        mutation_limits: MutationLimits::new(64, 65536).unwrap(),
        clipboard_limits: ClipboardLimits::new(64 * 1024, 4096).unwrap(),
        segmentation_limits: SegmentationLimits::new(4096, 4096).unwrap(),
        limits: RangeTextInputLimits::new(
            8 * 1024 * 1024,
            131072,
            64,
            px(80.),
            4096,
            4096,
            px(20.),
        )
        .unwrap(),
        settlement_coordinator: RangeSettlementCoordinator::new(4).unwrap(),
        viewport_extent: px(640.),
        overscan: px(40.),
        placeholder: SharedString::new_static("Message"),
        theme: TextInputTheme::default(),
        scrollbar_style: ScrollbarStyle::default(),
    }
}
