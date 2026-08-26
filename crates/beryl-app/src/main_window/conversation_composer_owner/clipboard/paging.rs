use super::*;

pub(super) fn read_cut_page(
    slot: &mut MainWindowComposerSlot,
    store: &beryl_home_store::HomeStore,
    selection: MainWindowComposerSelectionIdentity,
    request: PropagatedCutPageRequest,
) -> Result<PreparedPropagatedCutPage, String> {
    let binding = selection.binding().range_binding();
    let range = ByteRange::new(
        request.selection.start().byte_offset,
        request.selection.end().byte_offset,
    )
    .map_err(|_| "composer cut byte range is malformed".to_owned())?;
    let demand = ObjectDemandEnvelope::range(
        range,
        request.cursor,
        ObjectDirection::Forward,
        request.max_objects,
        request.max_retained_bytes,
    )
    .map_err(|_| "composer cut object demand is malformed".to_owned())?;
    let key = ObjectRequestKey::new(
        ObjectRequestId::new(request.request_id),
        binding.binding(),
        binding.revision(),
        request.presentation_generation,
        ObjectPurpose::Clipboard,
        demand,
    )
    .map_err(|_| "composer cut object request is malformed".to_owned())?;
    let page = slot
        .read_selected_predecessor_object_page(store, selection, ObjectRequest::new(key))
        .map_err(|_| "composer cut object dispatch failed".to_owned())?;
    prepare_cut_page(request, page)
}

pub(super) fn prepare_cut_page(
    request: PropagatedCutPageRequest,
    page: ObjectPage,
) -> Result<PreparedPropagatedCutPage, String> {
    let selection = request.selection;
    let mut items = Vec::with_capacity(page.objects().len());
    let leading = request
        .pending_tail
        .map(PendingCutMarker::cursor)
        .or_else(|| edge_cursor(page.preceding()));
    let mut reached_end = false;
    if let Some(pending) = request.pending_tail {
        let following = page
            .objects()
            .first()
            .map(gpui_text_input::InlineObjectFact::cursor);
        if following.is_none() && !page.complete() {
            return Err("composer cut lookahead page made no progress".into());
        }
        items.push(MutationPageItem::Object(ObjectChange::Remove {
            target: removal_target(pending, following)?,
        }));
    }
    let mut pending_tail = None;
    for (index, fact) in page.objects().iter().enumerate() {
        let cursor = fact.cursor();
        if !object_follows_start(cursor, selection.start()) {
            continue;
        }
        if !object_precedes_end(cursor, selection.end()) {
            reached_end = true;
            break;
        }
        let preceding = index
            .checked_sub(1)
            .map(|prior| page.objects()[prior].cursor())
            .or(leading);
        if items.len() == request.max_objects {
            pending_tail = Some(PendingCutMarker::new(fact, preceding));
            break;
        }
        let following = page
            .objects()
            .get(index + 1)
            .map(gpui_text_input::InlineObjectFact::cursor);
        if following.is_none() && !page.complete() {
            pending_tail = Some(PendingCutMarker::new(fact, preceding));
            break;
        }
        let target = removal_target(PendingCutMarker::new(fact, preceding), following)?;
        items.push(MutationPageItem::Object(ObjectChange::Remove { target }));
    }
    let complete = reached_end || (page.complete() && pending_tail.is_none());
    let continuation = if complete {
        None
    } else {
        pending_tail
            .map(PendingCutMarker::cursor)
            .or_else(|| page.continuation())
    };
    Ok(PreparedPropagatedCutPage {
        items,
        continuation,
        pending_tail,
        complete,
    })
}

fn removal_target(
    fact: PendingCutMarker,
    following: Option<ObjectCursor>,
) -> Result<ObjectTarget, String> {
    let current = InlineObjectNeighbor::new(fact.id, fact.order);
    let preceding = fact
        .preceding
        .filter(|cursor| cursor.anchor() == fact.anchor)
        .map(ObjectCursor::neighbor);
    let following = following
        .filter(|cursor| cursor.anchor() == fact.anchor)
        .map(ObjectCursor::neighbor);
    let start_gap = preceding.map_or(InlineObjectGap::before(current), |preceding| {
        InlineObjectGap::between(preceding, current)
            .expect("authenticated marker order is increasing")
    });
    let end_gap = following.map_or(InlineObjectGap::after(current), |following| {
        InlineObjectGap::between(current, following)
            .expect("authenticated marker order is increasing")
    });
    let range = SourceRange::new(
        SourcePosition::new(fact.anchor, start_gap),
        SourcePosition::new(fact.anchor, end_gap),
    )
    .map_err(|_| "composer cut marker target is malformed".to_owned())?;
    ObjectTarget::new(range, fact.id, fact.order)
        .map_err(|_| "composer cut marker target was rejected".to_owned())
}

impl PendingCutMarker {
    fn new(fact: &gpui_text_input::InlineObjectFact, preceding: Option<ObjectCursor>) -> Self {
        Self {
            anchor: fact.anchor(),
            id: fact.id(),
            order: fact.order(),
            preceding,
        }
    }

    const fn cursor(self) -> ObjectCursor {
        ObjectCursor::new(self.anchor, self.order, self.id)
    }
}

const fn edge_cursor(edge: ObjectPageEdgeFact) -> Option<ObjectCursor> {
    match edge {
        ObjectPageEdgeFact::EnvelopeBoundary => None,
        ObjectPageEdgeFact::Continues(cursor) => Some(cursor),
    }
}

pub(super) const fn cursor_after_gap(position: SourcePosition) -> Option<ObjectCursor> {
    match position.gap {
        InlineObjectGap::Between { preceding, .. } | InlineObjectGap::After(preceding) => Some(
            ObjectCursor::new(position.byte_offset, preceding.order(), preceding.id()),
        ),
        InlineObjectGap::NoObjects | InlineObjectGap::Before(_) => None,
    }
}

fn object_follows_start(cursor: ObjectCursor, start: SourcePosition) -> bool {
    match cursor.anchor().cmp(&start.byte_offset) {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Less => false,
        std::cmp::Ordering::Equal => match start.gap {
            InlineObjectGap::NoObjects => false,
            InlineObjectGap::Before(first) => {
                cursor >= ObjectCursor::new(start.byte_offset, first.order(), first.id())
            }
            InlineObjectGap::Between { preceding, .. } | InlineObjectGap::After(preceding) => {
                cursor > ObjectCursor::new(start.byte_offset, preceding.order(), preceding.id())
            }
        },
    }
}

fn object_precedes_end(cursor: ObjectCursor, end: SourcePosition) -> bool {
    match cursor.anchor().cmp(&end.byte_offset) {
        std::cmp::Ordering::Less => true,
        std::cmp::Ordering::Greater => false,
        std::cmp::Ordering::Equal => match end.gap {
            InlineObjectGap::NoObjects => false,
            InlineObjectGap::Before(first) => {
                cursor < ObjectCursor::new(end.byte_offset, first.order(), first.id())
            }
            InlineObjectGap::Between { preceding, .. } | InlineObjectGap::After(preceding) => {
                cursor <= ObjectCursor::new(end.byte_offset, preceding.order(), preceding.id())
            }
        },
    }
}

pub(super) fn deletion_caret(range: SourceRange) -> SourcePosition {
    let preceding = match range.start().gap {
        InlineObjectGap::Between { preceding, .. } | InlineObjectGap::After(preceding) => {
            Some(preceding)
        }
        InlineObjectGap::NoObjects | InlineObjectGap::Before(_) => None,
    };
    let following = match range.end().gap {
        InlineObjectGap::Between { following, .. } | InlineObjectGap::Before(following) => {
            Some(following)
        }
        InlineObjectGap::NoObjects | InlineObjectGap::After(_) => None,
    };
    let gap = match (preceding, following) {
        (None, None) => InlineObjectGap::NoObjects,
        (None, Some(following)) => InlineObjectGap::before(following),
        (Some(preceding), None) => InlineObjectGap::after(preceding),
        (Some(preceding), Some(following)) => InlineObjectGap::between(preceding, following)
            .expect("authenticated cut boundaries preserve marker order"),
    };
    SourcePosition::new(range.start().byte_offset, gap)
}

pub(super) fn deletion_extent(
    deletion: gpui_text_input::CutDeletion,
) -> Result<gpui_text_input::LogicalExtent, String> {
    let base = deletion.binding().extent();
    let range = deletion.selection();
    let bytes = base
        .byte_len()
        .checked_sub(range.end().byte_offset.get() - range.start().byte_offset.get())
        .ok_or_else(|| "composer cut extent underflowed".to_owned())?;
    let base_breaks = base
        .line_count()
        .checked_sub(u64::from(base.byte_len() != 0))
        .ok_or_else(|| "composer cut line extent was incoherent".to_owned())?;
    let breaks = base_breaks
        .checked_sub(deletion.selection_line_breaks())
        .ok_or_else(|| "composer cut line count underflowed".to_owned())?;
    let lines = if bytes == 0 {
        if breaks != 0 {
            return Err("composer cut produced an incoherent empty extent".into());
        }
        0
    } else {
        breaks
            .checked_add(1)
            .ok_or_else(|| "composer cut line count overflowed".to_owned())?
    };
    Ok(gpui_text_input::LogicalExtent::new(bytes, lines))
}
