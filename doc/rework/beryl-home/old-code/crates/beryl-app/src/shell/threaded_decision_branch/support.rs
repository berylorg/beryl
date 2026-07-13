use super::*;

pub(super) fn title_seed_for_turn_or_node(
    turn: Option<&std::sync::Arc<TurnExecutionRecord>>,
    graph: &SemanticGraph,
    node_id: &SemanticNodeId,
) -> String {
    let fragments = turn
        .into_iter()
        .flat_map(|turn| turn.user_input_fragments().iter())
        .filter(|fragment| !fragment.is_blank())
        .map(|fragment| fragment.text.trim())
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>();
    if !fragments.is_empty() {
        return fragments.join("\n\n");
    }

    graph
        .node(node_id)
        .map(|node| node.title().trim())
        .filter(|title| !title.is_empty())
        .unwrap_or("Decision branch")
        .to_string()
}

pub(super) fn parent_context_source_for_turn(turn: &TurnExecutionRecord) -> Option<String> {
    if !turn.has_resident_payload() {
        return None;
    }

    let mut sections = Vec::new();
    for entry in turn.narrative_entries() {
        match entry {
            TurnNarrativeEntry::UserInput { fragment_id } => {
                let Some((_, fragment)) = turn.user_input_fragment_by_id(*fragment_id) else {
                    continue;
                };
                if fragment.is_blank() {
                    continue;
                }
                let text = fragment.text.trim();
                if text.is_empty() {
                    sections.push("User: [non-text user input]".to_string());
                } else {
                    sections.push(format!("User:\n{text}"));
                }
            }
            TurnNarrativeEntry::Item { item_id } => {
                let Some(ExecutionItem::AgentMessage(message)) = turn.item_by_id(item_id) else {
                    continue;
                };
                let text = message.text.trim();
                if !text.is_empty() {
                    sections.push(format!("Assistant:\n{text}"));
                }
            }
        }
    }

    if sections.is_empty() {
        return None;
    }
    Some(bounded_parent_context_source(sections.join("\n\n")))
}

fn bounded_parent_context_source(mut text: String) -> String {
    if text.len() <= DECISION_PARENT_CONTEXT_SOURCE_MAX_BYTES {
        return text;
    }

    let marker = format!(
        "\n[Beryl omitted additional parent context source after {} retained bytes]",
        DECISION_PARENT_CONTEXT_SOURCE_MAX_BYTES
    );
    let retained_limit = DECISION_PARENT_CONTEXT_SOURCE_MAX_BYTES.saturating_sub(marker.len());
    text.truncate(floor_char_boundary(&text, retained_limit));
    text.push_str(&marker);
    text
}

fn floor_char_boundary(text: &str, limit: usize) -> usize {
    if limit >= text.len() {
        return text.len();
    }
    let mut boundary = limit;
    while boundary > 0 && !text.is_char_boundary(boundary) {
        boundary -= 1;
    }
    boundary
}

pub(super) fn workspace_action_provenance(action: &str) -> MutationProvenance {
    MutationProvenance::new(
        "operator",
        token_usage_snapshot::current_unix_millis(),
        MutationSource::workspace_action(action).expect("workspace action provenance is valid"),
        Some(100),
    )
    .expect("workspace action provenance is valid")
}

pub(super) fn dynamic_tool_provenance(
    request: &DynamicToolCallRequest,
    tool_name: &'static str,
) -> Result<MutationProvenance, beryl_model::provenance::ProvenanceError> {
    MutationSource::dynamic_tool_call(
        ConversationThreadId::new(request.thread_id().to_string()),
        ConversationTurnId::new(request.turn_id().to_string()),
        tool_name,
        request.call_id(),
    )
    .and_then(|source| {
        MutationProvenance::new(
            "codex",
            token_usage_snapshot::current_unix_millis(),
            source,
            Some(100),
        )
    })
}

pub(super) fn next_decision_record_id(
    node_id: &SemanticNodeId,
    parent_thread_id: &str,
    timestamp: u64,
) -> Option<ThreadedDecisionRecordId> {
    ThreadedDecisionRecordId::new(format!(
        "decision_branch_{}_{}_{}",
        sanitize_id_part(node_id.as_str()),
        sanitize_id_part(parent_thread_id),
        timestamp
    ))
    .ok()
}

pub(super) fn next_decision_operation_id(
    node_id: &SemanticNodeId,
    timestamp: u64,
) -> Option<ThreadedDecisionOperationId> {
    ThreadedDecisionOperationId::new(format!(
        "start_decision_branch_{}_{}",
        sanitize_id_part(node_id.as_str()),
        timestamp
    ))
    .ok()
}

fn sanitize_id_part(value: &str) -> String {
    let sanitized: String = value
        .chars()
        .map(|ch| match ch {
            'a'..='z' | '0'..='9' | '-' | '_' => ch,
            'A'..='Z' => ch.to_ascii_lowercase(),
            _ => '_',
        })
        .collect();
    if sanitized.is_empty() {
        "untitled".to_string()
    } else {
        sanitized
    }
}
