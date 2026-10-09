use super::super::{PendingStage, SelectedComposer};
use super::*;

#[inline(never)]
pub(super) fn authenticate<'a>(
    slot: &'a mut MainWindowComposerSlot,
    store: &HomeStore,
    selection: &MainWindowComposerSelectionIdentity,
    request: &RangeTextInputRequest,
    marker_metadata: Box<[ComposerHostImageMarkerMetadata]>,
) -> Result<
    (
        &'a mut SelectedComposer,
        Box<[ComposerHostImageMarkerMetadata]>,
    ),
    MainWindowComposerDispatchError,
> {
    if slot
        .pending
        .as_ref()
        .is_some_and(|pending| !matches!(pending.stage, PendingStage::Ready))
        && matches!(
            request,
            RangeTextInputRequest::MutationBegin(_) | RangeTextInputRequest::HistoryIntent(_)
        )
    {
        return Err(MainWindowComposerDispatchError::PendingInteractionRejected);
    }
    let authenticated = slot
        .marker_authority
        .authenticate(store, *selection, request, marker_metadata)
        .map_err(MainWindowComposerDispatchError::MarkerMetadata)?;
    let marker_metadata = authenticated
        .into_metadata(*selection, request)
        .map_err(MainWindowComposerDispatchError::MarkerMetadata)?;
    let selected = slot
        .selected
        .as_mut()
        .filter(|selected| selected.identity == *selection)
        .ok_or(MainWindowComposerDispatchError::StaleSelection)?;
    if selected.dispatcher.binding != selection.binding()
        || selected.host.binding() != Some(selection.binding())
    {
        return Err(MainWindowComposerDispatchError::StaleSelection);
    }
    if selected.dispatcher.in_dispatch {
        return Err(MainWindowComposerDispatchError::Busy);
    }
    Ok((selected, marker_metadata))
}

#[inline(never)]
pub(super) fn adopt_binding(
    selected: &mut SelectedComposer,
) -> Result<(), MainWindowComposerDispatchError> {
    if let Some(binding) = selected.host.binding()
        && binding != selected.identity.binding
    {
        let predecessor = selected.identity.binding;
        selected
            .draft_state
            .adopt(predecessor, binding)
            .map_err(|_| MainWindowComposerDispatchError::StaleSelection)?;
        selected.identity.binding = binding;
        selected.dispatcher.replace_binding(binding);
    }
    Ok(())
}
