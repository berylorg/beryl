use std::num::NonZeroU64;

use beryl_home_store::HomeStore;
use beryl_model::SyndicDraftMarkerId;
use gpui::px;
use gpui_text_input::{
    ByteOffset, ByteRange, InlineObjectFact, InlineObjectId, InlineObjectOrder,
    InlineObjectPresentation, ObjectCursor, ObjectDemandEnvelope, ObjectDirection, ObjectPage,
    ObjectPageEdgeFact, ObjectPageId, ObjectPurpose, ObjectRequest, PageDemandEnvelope,
    PageDirection, PageEdgeFact, PageId, PagePurpose, PageRequest, RangePage,
};
use syndic_storage::{
    DraftCompositeSearchKeyV1, DraftPieceMarkerDemandResultV1, DraftPieceMarkerDemandV1,
    DraftPieceMarkerDirectionV1, DraftPieceMarkerScopeV1, DraftPieceTextDemandResultV1,
    DraftPieceTextDemandV1, DraftPieceTextEdgeFactV1,
};

use crate::composer_host::{
    ComposerHostBinding, ComposerHostReadTarget, ComposerHostRequestId, ComposerHostRequestKey,
    ComposerHostRequestKind, ComposerHostRequestPurpose, ComposerHostResponse,
    ComposerHostResponseValue, SyndicComposerHost,
};

use super::MainWindowComposerDispatchError;

pub(super) fn text_page(
    host: &mut SyndicComposerHost,
    store: &HomeStore,
    binding: ComposerHostBinding,
    host_request_id: u64,
    request: PageRequest,
) -> Result<RangePage, MainWindowComposerDispatchError> {
    let key = request.key();
    validate_binding(binding, key.binding(), key.revision())?;
    let demand = match key.demand() {
        PageDemandEnvelope::Adjacent {
            anchor,
            direction: PageDirection::Forward,
            ..
        } => DraftPieceTextDemandV1::Forward(anchor.get()),
        PageDemandEnvelope::Adjacent {
            anchor,
            direction: PageDirection::Backward,
            ..
        } => DraftPieceTextDemandV1::Backward(anchor.get()),
        PageDemandEnvelope::Validation { candidate, .. } => {
            DraftPieceTextDemandV1::Validate(candidate.get())
        }
    };
    let host_key = host_key(binding, host_request_id, page_purpose(key.purpose())?)?;
    let pending = host.begin_request(
        host_key,
        ComposerHostRequestKind::Text {
            target: ComposerHostReadTarget::Candidate,
            demand,
            max_bytes: usize::try_from(key.max_payload_bytes())
                .map_err(|_| MainWindowComposerDispatchError::Malformed)?,
        },
    )?;
    let execution = host.execute_pending(store, pending);
    let response = host.complete_request(execution)?;
    let ComposerHostResponseValue::CandidateText(candidate) = response.value() else {
        return Err(MainWindowComposerDispatchError::Malformed);
    };
    if candidate.binding() != binding.candidate() {
        return Err(MainWindowComposerDispatchError::StaleSelection);
    }
    translate_text_result(key, candidate.value())
}

pub(super) fn object_page(
    host: &mut SyndicComposerHost,
    store: &HomeStore,
    binding: ComposerHostBinding,
    host_request_id: u64,
    request: ObjectRequest,
) -> Result<ObjectPage, MainWindowComposerDispatchError> {
    object_page_for_target(
        host,
        store,
        binding,
        host_request_id,
        request,
        ComposerHostReadTarget::Candidate,
    )
}

pub(super) fn historical_object_page(
    host: &mut SyndicComposerHost,
    store: &HomeStore,
    binding: ComposerHostBinding,
    host_request_id: u64,
    request: ObjectRequest,
) -> Result<ObjectPage, MainWindowComposerDispatchError> {
    object_page_for_target(
        host,
        store,
        binding,
        host_request_id,
        request,
        ComposerHostReadTarget::Historical(binding.root()),
    )
}

fn object_page_for_target(
    host: &mut SyndicComposerHost,
    store: &HomeStore,
    binding: ComposerHostBinding,
    host_request_id: u64,
    request: ObjectRequest,
    target: ComposerHostReadTarget,
) -> Result<ObjectPage, MainWindowComposerDispatchError> {
    let key = request.key();
    validate_binding(binding, key.binding(), key.revision())?;
    if key.presentation_generation().get() != binding.presentation_generation().get() {
        return Err(MainWindowComposerDispatchError::StaleSelection);
    }
    let envelope = key.demand();
    let (scope, cursor) = match envelope {
        ObjectDemandEnvelope::Range { range, cursor, .. } => (
            DraftPieceMarkerScopeV1::Range {
                start: range.start().get(),
                end: range.end().get(),
            },
            cursor.map(storage_cursor).transpose()?,
        ),
        ObjectDemandEnvelope::Anchor { anchor, cursor, .. } => (
            DraftPieceMarkerScopeV1::ExactAnchor(anchor.get()),
            cursor.map(storage_cursor).transpose()?,
        ),
    };
    let direction = match envelope.direction() {
        ObjectDirection::Forward => DraftPieceMarkerDirectionV1::Forward,
        ObjectDirection::Backward => DraftPieceMarkerDirectionV1::Backward,
    };
    let host_key = host_key(binding, host_request_id, object_purpose(key.purpose())?)?;
    let pending = host.begin_request(
        host_key,
        ComposerHostRequestKind::Markers {
            target,
            demand: DraftPieceMarkerDemandV1::new(
                scope,
                direction,
                cursor,
                envelope.max_objects(),
                envelope.max_retained_bytes(),
            ),
        },
    )?;
    let execution = host.execute_pending(store, pending);
    let response = host.complete_request(execution)?;
    let result = match (target, response.value()) {
        (
            ComposerHostReadTarget::Candidate,
            ComposerHostResponseValue::CandidateMarkers(result),
        ) => {
            if result.binding() != binding.candidate() {
                return Err(MainWindowComposerDispatchError::StaleSelection);
            }
            result.value()
        }
        (
            ComposerHostReadTarget::Historical(root),
            ComposerHostResponseValue::HistoricalMarkers(result),
        ) if result.root() == root => result,
        _ => return Err(MainWindowComposerDispatchError::Malformed),
    };
    translate_marker_result(key, result)
}

pub(in crate::main_window) fn initial_response(
    binding: ComposerHostBinding,
    request: gpui_text_input::RangeTextInputRequest,
    response: &ComposerHostResponse,
) -> Result<super::MainWindowComposerDispatchOutcome, MainWindowComposerDispatchError> {
    if response.key().binding() != binding {
        return Err(MainWindowComposerDispatchError::StaleSelection);
    }
    match (request, response.value()) {
        (
            gpui_text_input::RangeTextInputRequest::Page(request),
            ComposerHostResponseValue::CandidateText(candidate),
        ) if response.key().purpose() == page_purpose(request.key().purpose())? => {
            if candidate.binding() != binding.candidate() {
                return Err(MainWindowComposerDispatchError::StaleSelection);
            }
            Ok(super::MainWindowComposerDispatchOutcome::Page(
                translate_text_result(request.key(), candidate.value())?,
            ))
        }
        (
            gpui_text_input::RangeTextInputRequest::ObjectPage(request),
            ComposerHostResponseValue::CandidateMarkers(candidate),
        ) if response.key().purpose() == object_purpose(request.key().purpose())? => {
            if candidate.binding() != binding.candidate() {
                return Err(MainWindowComposerDispatchError::StaleSelection);
            }
            Ok(super::MainWindowComposerDispatchOutcome::ObjectPage(
                translate_marker_result(request.key(), candidate.value())?,
            ))
        }
        _ => Err(MainWindowComposerDispatchError::Malformed),
    }
}

fn translate_text_result(
    key: gpui_text_input::PageRequestKey,
    result: &DraftPieceTextDemandResultV1,
) -> Result<RangePage, MainWindowComposerDispatchError> {
    let preceding = match result.preceding() {
        DraftPieceTextEdgeFactV1::DocumentStart => PageEdgeFact::DocumentBoundary,
        DraftPieceTextEdgeFactV1::Continuation(_) => PageEdgeFact::Continues,
        DraftPieceTextEdgeFactV1::DocumentEnd => {
            return Err(MainWindowComposerDispatchError::Malformed);
        }
    };
    let following = match result.following() {
        DraftPieceTextEdgeFactV1::DocumentEnd => PageEdgeFact::DocumentBoundary,
        DraftPieceTextEdgeFactV1::Continuation(_) => PageEdgeFact::Continues,
        DraftPieceTextEdgeFactV1::DocumentStart => {
            return Err(MainWindowComposerDispatchError::Malformed);
        }
    };
    RangePage::new(
        PageId::new(key.id().get()),
        key,
        ByteRange::from_u64(result.start(), result.end())
            .map_err(|_| MainWindowComposerDispatchError::Malformed)?,
        String::from_utf8(result.bytes().to_vec())
            .map_err(|_| MainWindowComposerDispatchError::Malformed)?,
        Vec::new(),
        preceding,
        following,
        matches!(result.following(), DraftPieceTextEdgeFactV1::DocumentEnd),
    )
    .map_err(|_| MainWindowComposerDispatchError::Malformed)
}

fn translate_marker_result(
    key: gpui_text_input::ObjectRequestKey,
    result: &DraftPieceMarkerDemandResultV1,
) -> Result<ObjectPage, MainWindowComposerDispatchError> {
    let objects = result
        .markers()
        .iter()
        .map(|at| {
            let marker = at.marker();
            let label = marker.label().to_string();
            let presentation = InlineObjectPresentation::new(
                key.presentation_generation().get(),
                format!("[{label}]"),
                px(18.0 + label.len() as f32 * 8.0),
                px(22.0),
                px(17.0),
                None,
                marker.label().get(),
                true,
            )
            .map_err(|_| {
                MainWindowComposerDispatchError::ResponseTranslation("object presentation")
            })?;
            Ok(InlineObjectFact::new(
                InlineObjectId::new(u128::from_be_bytes(*marker.marker_id().as_bytes())),
                ByteOffset::new(at.anchor()),
                InlineObjectOrder::new(u128::from(marker.order_key())),
                format!("[Image {label}]"),
                presentation,
            ))
        })
        .collect::<Result<Vec<_>, MainWindowComposerDispatchError>>()?;
    let continuation = result
        .continuation()
        .map(widget_cursor)
        .transpose()
        .map_err(|_| MainWindowComposerDispatchError::ResponseTranslation("marker continuation"))?;
    let request_edge = key.demand().cursor().map_or(
        ObjectPageEdgeFact::EnvelopeBoundary,
        ObjectPageEdgeFact::Continues,
    );
    let continuation_edge = continuation.map_or(
        ObjectPageEdgeFact::EnvelopeBoundary,
        ObjectPageEdgeFact::Continues,
    );
    let (preceding, following) = match key.demand().direction() {
        ObjectDirection::Forward => (request_edge, continuation_edge),
        ObjectDirection::Backward => (continuation_edge, request_edge),
    };
    ObjectPage::new(
        ObjectPageId::new(key.id().get()),
        key,
        objects,
        preceding,
        following,
        result.requested_side_complete(),
        continuation,
    )
    .map_err(|_| MainWindowComposerDispatchError::ObjectPage("object page was rejected".into()))
}

fn storage_cursor(
    cursor: ObjectCursor,
) -> Result<DraftCompositeSearchKeyV1, MainWindowComposerDispatchError> {
    Ok(DraftCompositeSearchKeyV1::Marker {
        anchor: cursor.anchor().get(),
        order_key: u64::try_from(cursor.order().get())
            .map_err(|_| MainWindowComposerDispatchError::Malformed)?,
        marker_id: SyndicDraftMarkerId::from_bytes(cursor.id().get().to_be_bytes()),
    })
}

fn widget_cursor(
    cursor: DraftCompositeSearchKeyV1,
) -> Result<ObjectCursor, MainWindowComposerDispatchError> {
    let DraftCompositeSearchKeyV1::Marker {
        anchor,
        order_key,
        marker_id,
    } = cursor
    else {
        return Err(MainWindowComposerDispatchError::Malformed);
    };
    Ok(ObjectCursor::new(
        ByteOffset::new(anchor),
        InlineObjectOrder::new(u128::from(order_key)),
        InlineObjectId::new(u128::from_be_bytes(*marker_id.as_bytes())),
    ))
}

fn validate_binding(
    binding: ComposerHostBinding,
    widget_binding: gpui_text_input::BindingId,
    revision: gpui_text_input::SourceRevision,
) -> Result<(), MainWindowComposerDispatchError> {
    let expected = binding.range_binding();
    if widget_binding != expected.binding() || revision != expected.revision() {
        return Err(MainWindowComposerDispatchError::StaleSelection);
    }
    Ok(())
}

fn host_key(
    binding: ComposerHostBinding,
    request_id: u64,
    purpose: ComposerHostRequestPurpose,
) -> Result<ComposerHostRequestKey, MainWindowComposerDispatchError> {
    let request_id =
        NonZeroU64::new(request_id).ok_or(MainWindowComposerDispatchError::Malformed)?;
    Ok(ComposerHostRequestKey::new(
        binding,
        ComposerHostRequestId::new(request_id),
        purpose,
    ))
}

fn page_purpose(
    purpose: PagePurpose,
) -> Result<ComposerHostRequestPurpose, MainWindowComposerDispatchError> {
    Ok(match purpose {
        PagePurpose::Viewport => ComposerHostRequestPurpose::Viewport,
        PagePurpose::Caret => ComposerHostRequestPurpose::Caret,
        PagePurpose::Selection | PagePurpose::PlatformRange => {
            ComposerHostRequestPurpose::Selection
        }
        PagePurpose::Segmentation => ComposerHostRequestPurpose::Segmentation,
        PagePurpose::Clipboard => ComposerHostRequestPurpose::Clipboard,
        PagePurpose::Restoration => ComposerHostRequestPurpose::Restoration,
        PagePurpose::GeometryIndex | PagePurpose::GeometryTarget => {
            ComposerHostRequestPurpose::Geometry
        }
        _ => return Err(MainWindowComposerDispatchError::Malformed),
    })
}

fn object_purpose(
    purpose: ObjectPurpose,
) -> Result<ComposerHostRequestPurpose, MainWindowComposerDispatchError> {
    Ok(match purpose {
        ObjectPurpose::Viewport => ComposerHostRequestPurpose::Viewport,
        ObjectPurpose::Caret => ComposerHostRequestPurpose::Caret,
        ObjectPurpose::Selection
        | ObjectPurpose::MutationSuccessor
        | ObjectPurpose::PlatformRange => ComposerHostRequestPurpose::Selection,
        ObjectPurpose::Clipboard => ComposerHostRequestPurpose::Clipboard,
        ObjectPurpose::Restoration => ComposerHostRequestPurpose::Restoration,
        ObjectPurpose::GeometryIndex | ObjectPurpose::GeometryTarget => {
            ComposerHostRequestPurpose::Geometry
        }
        _ => return Err(MainWindowComposerDispatchError::Malformed),
    })
}
