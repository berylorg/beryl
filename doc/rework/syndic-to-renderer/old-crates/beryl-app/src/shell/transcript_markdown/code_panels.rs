use super::{BlockRenderList, BlockRenderNode, BlockRenderPlan};

const PANEL_ID_PREFIX: &str = "transcript-code-panel";

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct TranscriptCodePanelLocalIdentity {
    block_path: String,
    code_path: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct TranscriptCodePanelIdentity {
    row_identity: String,
    local_identity: TranscriptCodePanelLocalIdentity,
    encoded: String,
}

impl TranscriptCodePanelLocalIdentity {
    pub(crate) fn new(block_path: impl Into<String>, code_path: impl Into<String>) -> Self {
        Self {
            block_path: block_path.into(),
            code_path: code_path.into(),
        }
    }

    pub(crate) fn block_path(&self) -> &str {
        self.block_path.as_str()
    }

    pub(crate) fn code_path(&self) -> &str {
        self.code_path.as_str()
    }
}

impl TranscriptCodePanelIdentity {
    pub(crate) fn new(
        row_identity: impl Into<String>,
        block_path: impl Into<String>,
        code_path: impl Into<String>,
    ) -> Self {
        Self::from_local_identity(
            row_identity,
            TranscriptCodePanelLocalIdentity::new(block_path, code_path),
        )
    }

    pub(crate) fn from_local_identity(
        row_identity: impl Into<String>,
        local_identity: TranscriptCodePanelLocalIdentity,
    ) -> Self {
        let row_identity = row_identity.into();
        let encoded = encode_panel_id(
            row_identity.as_str(),
            local_identity.block_path(),
            local_identity.code_path(),
        );
        Self {
            row_identity,
            local_identity,
            encoded,
        }
    }

    pub(crate) fn parse(panel_id: &str) -> Option<Self> {
        let mut remainder = panel_id.strip_prefix(PANEL_ID_PREFIX)?.strip_prefix(":r")?;
        let row_len_end = remainder.find(':')?;
        let row_len = remainder[..row_len_end].parse::<usize>().ok()?;
        remainder = &remainder[row_len_end + 1..];
        let row_identity = take_len_prefixed_segment(&mut remainder, row_len)?;
        remainder = remainder.strip_prefix(":b")?;
        let block_len_end = remainder.find(':')?;
        let block_len = remainder[..block_len_end].parse::<usize>().ok()?;
        remainder = &remainder[block_len_end + 1..];
        let block_path = take_len_prefixed_segment(&mut remainder, block_len)?;
        remainder = remainder.strip_prefix(":c")?;
        let code_len_end = remainder.find(':')?;
        let code_len = remainder[..code_len_end].parse::<usize>().ok()?;
        remainder = &remainder[code_len_end + 1..];
        let code_path = take_len_prefixed_segment(&mut remainder, code_len)?;
        if !remainder.is_empty() {
            return None;
        }

        Some(Self::new(row_identity, block_path, code_path))
    }

    pub(crate) fn row_identity(&self) -> &str {
        self.row_identity.as_str()
    }

    pub(crate) fn local_identity(&self) -> &TranscriptCodePanelLocalIdentity {
        &self.local_identity
    }

    pub(crate) fn as_str(&self) -> &str {
        self.encoded.as_str()
    }
}

impl std::fmt::Display for TranscriptCodePanelIdentity {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

pub(crate) fn markdown_code_panel_id(
    row_identity: &str,
    block_path: &str,
    code_path: &str,
) -> String {
    markdown_code_panel_identity(row_identity, block_path, code_path).to_string()
}

pub(crate) fn markdown_code_panel_identity(
    row_identity: &str,
    block_path: &str,
    code_path: &str,
) -> TranscriptCodePanelIdentity {
    TranscriptCodePanelIdentity::new(row_identity, block_path, code_path)
}

fn encode_panel_id(row_identity: &str, block_path: &str, code_path: &str) -> String {
    format!(
        "{PANEL_ID_PREFIX}:r{}:{row_identity}:b{}:{block_path}:c{}:{code_path}",
        row_identity.len(),
        block_path.len(),
        code_path.len(),
    )
}

pub(crate) fn markdown_code_panel_id_belongs_to_row(panel_id: &str, row_identity: &str) -> bool {
    TranscriptCodePanelIdentity::parse(panel_id)
        .is_some_and(|identity| identity.row_identity() == row_identity)
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

fn take_len_prefixed_segment(remainder: &mut &str, len: usize) -> Option<String> {
    if len > remainder.len() || !remainder.is_char_boundary(len) {
        return None;
    }
    let value = remainder[..len].to_string();
    *remainder = &remainder[len..];
    Some(value)
}
