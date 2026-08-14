use super::{
    SemanticGraph, SemanticNode, SemanticNodeId, SoftLink, ThreadRef, ThreadRefDraft, ThreadRefId,
};

impl SemanticGraph {
    pub fn nodes(&self) -> impl Iterator<Item = &SemanticNode> {
        self.nodes.values()
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn child_nodes_of(&self, parent_id: &SemanticNodeId) -> Vec<&SemanticNode> {
        self.child_ids_of(parent_id)
            .into_iter()
            .flatten()
            .filter_map(|child_id| self.node(child_id))
            .collect()
    }

    pub fn checklist_items(&self, checklist_node_id: &SemanticNodeId) -> Vec<&SemanticNode> {
        self.child_nodes_of(checklist_node_id)
    }

    pub fn path_to_root(&self, node_id: &SemanticNodeId) -> Option<Vec<&SemanticNode>> {
        let mut path = Vec::new();
        let mut current_id = node_id;

        loop {
            let node = self.node(current_id)?;
            path.push(node);

            let Some(parent_id) = self.parent_id_of(current_id) else {
                break;
            };
            current_id = parent_id;
        }

        path.reverse();
        Some(path)
    }

    pub fn soft_links(&self) -> impl Iterator<Item = &SoftLink> {
        self.soft_links.values()
    }

    pub fn soft_link_count(&self) -> usize {
        self.soft_links.len()
    }

    pub fn soft_links_from(&self, source_id: &SemanticNodeId) -> impl Iterator<Item = &SoftLink> {
        self.soft_links()
            .filter(move |link| link.source_id() == source_id)
    }

    pub fn thread_refs(&self) -> impl Iterator<Item = &ThreadRef> {
        self.thread_refs.values()
    }

    pub fn thread_ref_count(&self) -> usize {
        self.thread_refs.len()
    }

    pub fn thread_refs_for_node(
        &self,
        node_id: &SemanticNodeId,
    ) -> impl Iterator<Item = &ThreadRef> {
        self.thread_refs()
            .filter(move |thread_ref| thread_ref.node_id() == node_id)
    }
}

impl ThreadRefDraft {
    pub fn id(&self) -> &ThreadRefId {
        &self.id
    }
}
