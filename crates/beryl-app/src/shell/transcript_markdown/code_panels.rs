use super::{BlockRenderList, BlockRenderNode, BlockRenderPlan};

const PANEL_ID_PREFIX: &str = "transcript-code-panel";

pub(crate) fn markdown_code_panel_id(
    row_identity: &str,
    block_path: &str,
    code_path: &str,
) -> String {
    format!(
        "{PANEL_ID_PREFIX}:r{}:{row_identity}:b{}:{block_path}:c{}:{code_path}",
        row_identity.len(),
        block_path.len(),
        code_path.len(),
    )
}

pub(crate) fn markdown_code_panel_ids(
    row_identity: &str,
    block_path: &str,
    plan: &BlockRenderPlan,
) -> Vec<String> {
    let mut ids = Vec::new();
    collect_code_panel_ids(
        &mut ids,
        row_identity,
        block_path,
        plan.blocks.as_slice(),
        "",
    );
    ids
}

pub(crate) fn markdown_code_panel_block_path(parent: &str, index: usize) -> String {
    child_path(parent, format!("b{index}"))
}

pub(crate) fn markdown_code_panel_list_item_path(parent: &str, index: usize) -> String {
    child_path(parent, format!("i{index}"))
}

pub(crate) fn markdown_code_panel_block_quote_path(parent: &str) -> String {
    child_path(parent, "q")
}

fn collect_code_panel_ids(
    ids: &mut Vec<String>,
    row_identity: &str,
    block_path: &str,
    blocks: &[BlockRenderNode],
    structural_parent_path: &str,
) {
    for (index, block) in blocks.iter().enumerate() {
        let structural_path = markdown_code_panel_block_path(structural_parent_path, index);
        match block {
            BlockRenderNode::Code(_) => ids.push(markdown_code_panel_id(
                row_identity,
                block_path,
                structural_path.as_str(),
            )),
            BlockRenderNode::List(list) => collect_list_code_panel_ids(
                ids,
                row_identity,
                block_path,
                list,
                structural_path.as_str(),
            ),
            BlockRenderNode::BlockQuote { blocks, .. } => collect_code_panel_ids(
                ids,
                row_identity,
                block_path,
                blocks.as_slice(),
                markdown_code_panel_block_quote_path(structural_path.as_str()).as_str(),
            ),
            BlockRenderNode::Paragraph { .. }
            | BlockRenderNode::Heading { .. }
            | BlockRenderNode::Math { .. }
            | BlockRenderNode::ThematicBreak
            | BlockRenderNode::Unsupported { .. } => {}
        }
    }
}

fn collect_list_code_panel_ids(
    ids: &mut Vec<String>,
    row_identity: &str,
    block_path: &str,
    list: &BlockRenderList,
    structural_list_path: &str,
) {
    for (index, item) in list.items.iter().enumerate() {
        collect_code_panel_ids(
            ids,
            row_identity,
            block_path,
            item.blocks.as_slice(),
            markdown_code_panel_list_item_path(structural_list_path, index).as_str(),
        );
    }
}

fn child_path(parent: &str, child: impl AsRef<str>) -> String {
    if parent.is_empty() {
        child.as_ref().to_string()
    } else {
        format!("{parent}.{}", child.as_ref())
    }
}
