use crate::WorkspaceGraphUpkeepPolicy;

const GRAPH_UPKEEP_FIXED_GUIDANCE: &str = "\
Beryl graph upkeep guidance:
- Treat the semantic graph as navigational workspace state, not as the transcript, source of truth, or a search index.
- Use Beryl graph tools conservatively and obey their schemas, graph invariants, and provenance behavior.
- Source documents and backend conversation history remain authoritative when they conflict with graph summaries.
- Do not use filesystem watches, scanner loops, or hook-driven source sync as graph-upkeep authority.";

const WORKSPACE_GRAPH_UPKEEP_HEADER: &str = "Workspace graph-upkeep instructions:";

pub(crate) fn graph_upkeep_hidden_context(
    policy: Option<&WorkspaceGraphUpkeepPolicy>,
) -> Option<String> {
    let instructions = policy.and_then(WorkspaceGraphUpkeepPolicy::instructions)?;
    Some(format!(
        "{GRAPH_UPKEEP_FIXED_GUIDANCE}\n\n{WORKSPACE_GRAPH_UPKEEP_HEADER}\n{instructions}"
    ))
}

pub(crate) fn compose_hidden_developer_instructions(
    graph_upkeep_policy: Option<&WorkspaceGraphUpkeepPolicy>,
    global_developer_instructions: Option<String>,
) -> Option<String> {
    let graph_context = graph_upkeep_hidden_context(graph_upkeep_policy);
    let global_developer_instructions =
        global_developer_instructions.and_then(|value| (!value.trim().is_empty()).then_some(value));

    match (graph_context, global_developer_instructions) {
        (Some(graph_context), Some(global_developer_instructions)) => Some(format!(
            "{graph_context}\n\n{global_developer_instructions}"
        )),
        (Some(graph_context), None) => Some(graph_context),
        (None, Some(global_developer_instructions)) => Some(global_developer_instructions),
        (None, None) => None,
    }
}
