use super::*;
use crate::canonical_empty_draft_marker_admission_root_v1;

#[derive(Clone, Copy)]
pub(super) enum SearchKey {
    Source(DraftMarkerAdmissionSourceKeyV1),
    Target(SyndicDraftMarkerId),
}

struct PathStep {
    node: DraftMarkerAdmissionNodeV1,
    child_index: usize,
}

struct AuthenticatedPath {
    leaf: DraftMarkerAdmissionNodeV1,
    steps: Vec<PathStep>,
    exact: bool,
}

enum RootPath {
    Empty,
    Occupied(AuthenticatedPath),
}

pub(super) struct TreeEdit {
    pub(super) root: DraftMarkerAdmissionRootV1,
    pub(super) puts: Vec<DraftMarkerAdmissionNodeV1>,
    pub(super) predecessor: Vec<DraftMarkerAdmissionChildV1>,
    pub(super) path_keys: BTreeSet<DraftMarkerAdmissionNodeKeyV1>,
}

pub(super) struct NodeIdFactory {
    pub(super) owner: DraftMarkerAdmissionOwnerV1,
    pub(super) page: DraftMarkerAdmissionPageIdentityV1,
    pub(super) association_index: u64,
    pub(super) tree: DraftMarkerAdmissionTreeV1,
    pub(super) next: u16,
}

impl NodeIdFactory {
    fn key(
        &mut self,
        kind: DraftMarkerAdmissionNodeKindV1,
    ) -> Result<DraftMarkerAdmissionNodeKeyV1, DraftMarkerAdmissionIndexPreparationErrorV1> {
        let sequence = self.next;
        self.next = self
            .next
            .checked_add(1)
            .ok_or(DraftMarkerAdmissionSchemaErrorV1::ArithmeticOverflow)?;
        let mut digest = Sha256::new();
        digest.update(NODE_ID_DOMAIN);
        digest.update(self.owner.draft_id().as_bytes());
        digest.update(self.owner.session_id().as_bytes());
        digest.update(self.owner.operation_id().as_bytes());
        digest.update(self.page.command_id().as_bytes());
        digest.update(self.page.page_ordinal().get().to_le_bytes());
        digest.update(self.association_index.to_le_bytes());
        digest.update([match self.tree {
            DraftMarkerAdmissionTreeV1::SourceOrder => 0,
            DraftMarkerAdmissionTreeV1::TargetId => 1,
        }]);
        digest.update([match kind {
            DraftMarkerAdmissionNodeKindV1::Internal => 0,
            DraftMarkerAdmissionNodeKindV1::Leaf => 1,
        }]);
        digest.update(sequence.to_le_bytes());
        let digest: [u8; 32] = digest.finalize().into();
        let mut id = [0; 16];
        id.copy_from_slice(&digest[..16]);
        Ok(DraftMarkerAdmissionNodeKeyV1::new(
            self.owner,
            kind,
            DraftMarkerAdmissionNodeIdV1::from_bytes(id),
        ))
    }
}

pub(super) fn edit_tree<R: AdmissionNodeReader, F>(
    ledger: &mut ReadLedger<'_, R>,
    owner: DraftMarkerAdmissionOwnerV1,
    root: DraftMarkerAdmissionRootV1,
    key: SearchKey,
    mut ids: NodeIdFactory,
    maximum_height: u8,
    leaf: F,
) -> Result<TreeEdit, DraftMarkerAdmissionIndexPreparationErrorV1>
where
    F: FnOnce() -> Result<DraftMarkerAdmissionNodeV1, DraftMarkerAdmissionSchemaErrorV1>,
{
    let path = authenticate_path(ledger, owner, root, key)?;
    if let RootPath::Occupied(path) = &path
        && path.exact
    {
        return Err(match key {
            SearchKey::Source(_) => DraftMarkerAdmissionIndexPreparationErrorV1::DuplicateSource,
            SearchKey::Target(_) => DraftMarkerAdmissionIndexPreparationErrorV1::DuplicateTarget,
        });
    }
    let template = leaf()?;
    let new_leaf = rebuild_leaf(template, ids.key(DraftMarkerAdmissionNodeKindV1::Leaf)?)?;
    let new_leaf_child = child(&new_leaf)?;
    let mut puts = vec![new_leaf];
    let predecessor = Vec::new();
    let mut path_keys = BTreeSet::new();

    let RootPath::Occupied(path) = path else {
        return Ok(TreeEdit {
            root: root_from_child(root.tree(), 1, new_leaf_child)?,
            puts,
            predecessor,
            path_keys,
        });
    };
    path_keys.insert(path.leaf.key());
    if path.steps.is_empty() {
        let mut children = vec![child(&path.leaf)?, new_leaf_child];
        sort_children(&mut children, root.tree())?;
        let node = DraftMarkerAdmissionNodeV1::internal(
            ids.key(DraftMarkerAdmissionNodeKindV1::Internal)?,
            root.tree(),
            2,
            children,
        )?;
        let root = root_from_node(&node)?;
        puts.push(node);
        return Ok(TreeEdit {
            root,
            puts,
            predecessor,
            path_keys,
        });
    }

    let predecessor = path
        .steps
        .iter()
        .map(|step| child(&step.node))
        .collect::<Result<_, _>>()?;
    let mut replacements = vec![new_leaf_child];
    for (reverse_index, step) in path.steps.iter().enumerate().rev() {
        path_keys.insert(step.node.key());
        let DraftMarkerAdmissionNodePayloadV1::Internal { height, children } = step.node.payload()
        else {
            return Err(DraftMarkerAdmissionIndexPreparationErrorV1::PathAuthentication);
        };
        let mut next = children.to_vec();
        if reverse_index + 1 == path.steps.len() {
            next.extend(replacements);
            sort_children(&mut next, root.tree())?;
        } else {
            next.splice(step.child_index..=step.child_index, replacements);
        }
        replacements = make_internal_level(&mut ids, root.tree(), *height, next, &mut puts)?;
    }
    let root = match replacements.as_slice() {
        [only] => root_from_child(root.tree(), root.height(), *only)?,
        [left, right] => {
            if root.height() >= maximum_height {
                return Err(DraftMarkerAdmissionSchemaErrorV1::TreeHeight.into());
            }
            let node = DraftMarkerAdmissionNodeV1::internal(
                ids.key(DraftMarkerAdmissionNodeKindV1::Internal)?,
                root.tree(),
                root.height() + 1,
                vec![*left, *right],
            )?;
            let root = root_from_node(&node)?;
            puts.push(node);
            root
        }
        _ => return Err(DraftMarkerAdmissionIndexPreparationErrorV1::PathAuthentication),
    };
    Ok(TreeEdit {
        root,
        puts,
        predecessor,
        path_keys,
    })
}

pub(super) fn rewrite_tree<R: AdmissionNodeReader, F>(
    ledger: &mut ReadLedger<'_, R>,
    owner: DraftMarkerAdmissionOwnerV1,
    root: DraftMarkerAdmissionRootV1,
    key: SearchKey,
    mut ids: NodeIdFactory,
    replacement: F,
) -> Result<TreeEdit, DraftMarkerAdmissionIndexPreparationErrorV1>
where
    F: FnOnce(
        &DraftMarkerAdmissionNodeV1,
    ) -> Result<Option<DraftMarkerAdmissionNodeV1>, DraftMarkerAdmissionSchemaErrorV1>,
{
    let RootPath::Occupied(path) = authenticate_path(ledger, owner, root, key)? else {
        return Err(DraftMarkerAdmissionIndexPreparationErrorV1::MissingNode);
    };
    if !path.exact {
        return Err(DraftMarkerAdmissionIndexPreparationErrorV1::MissingNode);
    }

    let mut puts = Vec::new();
    let mut path_keys = BTreeSet::new();
    path_keys.insert(path.leaf.key());
    let mut predecessor = path
        .steps
        .iter()
        .map(|step| child(&step.node))
        .collect::<Result<Vec<_>, _>>()?;
    predecessor.push(child(&path.leaf)?);

    let mut replacements = match replacement(&path.leaf)? {
        Some(template) => {
            let leaf = rebuild_leaf(template, ids.key(DraftMarkerAdmissionNodeKindV1::Leaf)?)?;
            let child = child(&leaf)?;
            puts.push(leaf);
            vec![child]
        }
        None => Vec::new(),
    };
    if path.steps.is_empty() {
        let root = match replacements.as_slice() {
            [] => canonical_empty_draft_marker_admission_root_v1(root.tree()),
            [only] => root_from_child(root.tree(), 1, *only)?,
            _ => return Err(DraftMarkerAdmissionIndexPreparationErrorV1::PathAuthentication),
        };
        return Ok(TreeEdit {
            root,
            puts,
            predecessor,
            path_keys,
        });
    }

    for step in path.steps.iter().rev() {
        path_keys.insert(step.node.key());
        let DraftMarkerAdmissionNodePayloadV1::Internal { height, children } = step.node.payload()
        else {
            return Err(DraftMarkerAdmissionIndexPreparationErrorV1::PathAuthentication);
        };
        let mut next = children.to_vec();
        next.splice(step.child_index..=step.child_index, replacements);
        replacements = if next.is_empty() {
            Vec::new()
        } else {
            make_internal_level(&mut ids, root.tree(), *height, next, &mut puts)?
        };
    }
    let root = match replacements.as_slice() {
        [] => canonical_empty_draft_marker_admission_root_v1(root.tree()),
        [only] => root_from_child(root.tree(), root.height(), *only)?,
        _ => return Err(DraftMarkerAdmissionIndexPreparationErrorV1::PathAuthentication),
    };
    Ok(TreeEdit {
        root,
        puts,
        predecessor,
        path_keys,
    })
}

pub(super) fn least_leaf<R: AdmissionNodeReader>(
    ledger: &mut ReadLedger<'_, R>,
    owner: DraftMarkerAdmissionOwnerV1,
    root: DraftMarkerAdmissionRootV1,
) -> Result<DraftMarkerAdmissionNodeV1, DraftMarkerAdmissionIndexPreparationErrorV1> {
    root.validate_shape()?;
    let root_key = root
        .node()
        .ok_or(DraftMarkerAdmissionIndexPreparationErrorV1::MissingNode)?;
    if root_key.owner() != owner {
        return Err(DraftMarkerAdmissionIndexPreparationErrorV1::PathAuthentication);
    }
    let mut node = required_node(ledger, &root_key)?;
    if node.tree() != root.tree()
        || node.height() != root.height()
        || node.digest() != root.digest()
        || node.count()? != root.count()
    {
        return Err(DraftMarkerAdmissionIndexPreparationErrorV1::PathAuthentication);
    }
    loop {
        let DraftMarkerAdmissionNodePayloadV1::Internal { height, children } = node.payload()
        else {
            return Ok(node);
        };
        let expected = *children
            .first()
            .ok_or(DraftMarkerAdmissionIndexPreparationErrorV1::PathAuthentication)?;
        let child_node = required_node(ledger, &expected.key())?;
        if child_node.key().owner() != owner
            || child_node.tree() != root.tree()
            || child_node.height().checked_add(1) != Some(*height)
            || child(&child_node)? != expected
        {
            return Err(DraftMarkerAdmissionIndexPreparationErrorV1::PathAuthentication);
        }
        node = child_node;
    }
}

fn authenticate_path<R: AdmissionNodeReader>(
    ledger: &mut ReadLedger<'_, R>,
    owner: DraftMarkerAdmissionOwnerV1,
    root: DraftMarkerAdmissionRootV1,
    key: SearchKey,
) -> Result<RootPath, DraftMarkerAdmissionIndexPreparationErrorV1> {
    root.validate_shape()?;
    let Some(root_key) = root.node() else {
        return Ok(RootPath::Empty);
    };
    if root_key.owner() != owner {
        return Err(DraftMarkerAdmissionIndexPreparationErrorV1::PathAuthentication);
    }
    let mut node = required_node(ledger, &root_key)?;
    node.validate()?;
    if node.key() != root_key
        || node.tree() != root.tree()
        || node.height() != root.height()
        || node.digest() != root.digest()
        || node.count()? != root.count()
    {
        return Err(DraftMarkerAdmissionIndexPreparationErrorV1::PathAuthentication);
    }
    let mut steps = Vec::new();
    let mut is_root = true;
    loop {
        match node.payload() {
            DraftMarkerAdmissionNodePayloadV1::Internal { height, children } => {
                if (!is_root && children.len() < 2)
                    || children.is_empty()
                    || children.len() > DRAFT_MARKER_ADMISSION_TREE_FANOUT
                {
                    return Err(DraftMarkerAdmissionSchemaErrorV1::NodeFanout.into());
                }
                let child_index = select_child(children, key)?;
                let expected = children[child_index];
                let child_node = required_node(ledger, &expected.key())?;
                child_node.validate()?;
                if child_node.key() != expected.key()
                    || child_node.key().owner() != owner
                    || child_node.tree() != root.tree()
                    || child_node.height().checked_add(1) != Some(*height)
                    || child_node.digest() != expected.digest()
                    || child_node.count()? != expected.count()
                    || child_node.envelope()? != expected.envelope()
                {
                    return Err(DraftMarkerAdmissionIndexPreparationErrorV1::PathAuthentication);
                }
                steps.push(PathStep { node, child_index });
                node = child_node;
                is_root = false;
            }
            _ => {
                let exact = leaf_key(&node)? == key;
                return Ok(RootPath::Occupied(AuthenticatedPath {
                    leaf: node,
                    steps,
                    exact,
                }));
            }
        }
    }
}

fn required_node<R: AdmissionNodeReader>(
    ledger: &mut ReadLedger<'_, R>,
    key: &DraftMarkerAdmissionNodeKeyV1,
) -> Result<DraftMarkerAdmissionNodeV1, DraftMarkerAdmissionIndexPreparationErrorV1> {
    ledger
        .point(key)?
        .ok_or(DraftMarkerAdmissionIndexPreparationErrorV1::MissingNode)
}

pub(super) fn authenticate_replay_deletions<R: AdmissionNodeReader>(
    ledger: &mut ReadLedger<'_, R>,
    owner: DraftMarkerAdmissionOwnerV1,
    prior: &[DraftMarkerAdmissionChildV1],
    protected: &BTreeSet<DraftMarkerAdmissionNodeKeyV1>,
) -> Result<Vec<DraftMarkerAdmissionNodeV1>, DraftMarkerAdmissionIndexPreparationErrorV1> {
    if prior.len() > usize::from(DRAFT_MARKER_ADMISSION_TREE_MAX_HEIGHT) * 2 + 2 {
        return Err(DraftMarkerAdmissionSchemaErrorV1::InvalidCount.into());
    }
    let mut seen = BTreeSet::new();
    let mut deletions = Vec::with_capacity(prior.len());
    for expected in prior {
        if expected.key().owner() != owner
            || protected.contains(&expected.key())
            || !seen.insert(expected.key())
        {
            return Err(DraftMarkerAdmissionIndexPreparationErrorV1::PathAuthentication);
        }
        let node = required_node(ledger, &expected.key())?;
        node.validate()?;
        if child(&node)? != *expected {
            return Err(DraftMarkerAdmissionIndexPreparationErrorV1::PathAuthentication);
        }
        deletions.push(node);
    }
    Ok(deletions)
}

pub(super) fn authenticate_fresh_put_keys<R: AdmissionNodeReader>(
    ledger: &mut ReadLedger<'_, R>,
    puts: &[DraftMarkerAdmissionNodeV1],
) -> Result<(), DraftMarkerAdmissionIndexPreparationErrorV1> {
    let mut seen = BTreeSet::new();
    for put in puts {
        if !seen.insert(put.key()) || ledger.point(&put.key())?.is_some() {
            return Err(DraftMarkerAdmissionIndexPreparationErrorV1::NodeIdOccupied);
        }
    }
    Ok(())
}

fn make_internal_level(
    ids: &mut NodeIdFactory,
    tree: DraftMarkerAdmissionTreeV1,
    height: u8,
    children: Vec<DraftMarkerAdmissionChildV1>,
    puts: &mut Vec<DraftMarkerAdmissionNodeV1>,
) -> Result<Vec<DraftMarkerAdmissionChildV1>, DraftMarkerAdmissionIndexPreparationErrorV1> {
    let split = if children.len() > DRAFT_MARKER_ADMISSION_TREE_FANOUT {
        Some(children.len() / 2)
    } else {
        None
    };
    let groups = match split {
        Some(split) => vec![children[..split].to_vec(), children[split..].to_vec()],
        None => vec![children],
    };
    let mut replacements = Vec::with_capacity(groups.len());
    for group in groups {
        let node = DraftMarkerAdmissionNodeV1::internal(
            ids.key(DraftMarkerAdmissionNodeKindV1::Internal)?,
            tree,
            height,
            group,
        )?;
        replacements.push(child(&node)?);
        puts.push(node);
    }
    Ok(replacements)
}

fn rebuild_leaf(
    template: DraftMarkerAdmissionNodeV1,
    key: DraftMarkerAdmissionNodeKeyV1,
) -> Result<DraftMarkerAdmissionNodeV1, DraftMarkerAdmissionSchemaErrorV1> {
    match template.payload() {
        DraftMarkerAdmissionNodePayloadV1::SourceLeaf {
            source_key,
            evidence,
            asset_id,
        } => DraftMarkerAdmissionNodeV1::source_leaf(key, *source_key, evidence.clone(), *asset_id),
        DraftMarkerAdmissionNodePayloadV1::TargetLeaf {
            target_marker_id,
            page,
            evidence,
            source_label,
            asset_id,
            disposition,
        } => DraftMarkerAdmissionNodeV1::target_leaf(
            key,
            *target_marker_id,
            *page,
            evidence.clone(),
            *source_label,
            *asset_id,
            *disposition,
        ),
        DraftMarkerAdmissionNodePayloadV1::Internal { .. } => {
            Err(DraftMarkerAdmissionSchemaErrorV1::InvalidTree)
        }
    }
}

fn child(
    node: &DraftMarkerAdmissionNodeV1,
) -> Result<DraftMarkerAdmissionChildV1, DraftMarkerAdmissionSchemaErrorV1> {
    Ok(DraftMarkerAdmissionChildV1::new(
        node.key(),
        node.digest(),
        node.count()?,
        node.envelope()?,
    ))
}

fn root_from_node(
    node: &DraftMarkerAdmissionNodeV1,
) -> Result<DraftMarkerAdmissionRootV1, DraftMarkerAdmissionSchemaErrorV1> {
    DraftMarkerAdmissionRootV1::new(
        node.tree(),
        node.key(),
        node.height(),
        node.digest(),
        node.count()?,
    )
}

fn root_from_child(
    tree: DraftMarkerAdmissionTreeV1,
    height: u8,
    child: DraftMarkerAdmissionChildV1,
) -> Result<DraftMarkerAdmissionRootV1, DraftMarkerAdmissionSchemaErrorV1> {
    DraftMarkerAdmissionRootV1::new(tree, child.key(), height, child.digest(), child.count())
}

fn leaf_key(
    node: &DraftMarkerAdmissionNodeV1,
) -> Result<SearchKey, DraftMarkerAdmissionIndexPreparationErrorV1> {
    match node.payload() {
        DraftMarkerAdmissionNodePayloadV1::SourceLeaf { source_key, .. }
            if node.tree() == DraftMarkerAdmissionTreeV1::SourceOrder =>
        {
            Ok(SearchKey::Source(*source_key))
        }
        DraftMarkerAdmissionNodePayloadV1::TargetLeaf {
            target_marker_id, ..
        } if node.tree() == DraftMarkerAdmissionTreeV1::TargetId => {
            Ok(SearchKey::Target(*target_marker_id))
        }
        _ => Err(DraftMarkerAdmissionIndexPreparationErrorV1::PathAuthentication),
    }
}

fn select_child(
    children: &[DraftMarkerAdmissionChildV1],
    key: SearchKey,
) -> Result<usize, DraftMarkerAdmissionIndexPreparationErrorV1> {
    children
        .iter()
        .position(|child| key.compare_to_envelope_last(child.envelope()) != Ordering::Greater)
        .or_else(|| children.len().checked_sub(1))
        .ok_or(DraftMarkerAdmissionIndexPreparationErrorV1::PathAuthentication)
}

fn sort_children(
    children: &mut [DraftMarkerAdmissionChildV1],
    tree: DraftMarkerAdmissionTreeV1,
) -> Result<(), DraftMarkerAdmissionIndexPreparationErrorV1> {
    children.sort_by(|left, right| envelope_first_cmp(left.envelope(), right.envelope(), tree));
    if children.windows(2).any(|pair| {
        envelope_first_cmp(pair[0].envelope(), pair[1].envelope(), tree) != Ordering::Less
    }) {
        return Err(DraftMarkerAdmissionSchemaErrorV1::InvalidEnvelope.into());
    }
    Ok(())
}

impl SearchKey {
    fn compare_to_envelope_last(self, envelope: crate::DraftMarkerAdmissionEnvelopeV1) -> Ordering {
        match (self, envelope) {
            (
                Self::Source(key),
                crate::DraftMarkerAdmissionEnvelopeV1::SourceOrder { last, .. },
            ) => source_cmp(key, last),
            (Self::Target(key), crate::DraftMarkerAdmissionEnvelopeV1::TargetId { last, .. }) => {
                key.cmp(&last)
            }
            _ => Ordering::Greater,
        }
    }
}

impl PartialEq for SearchKey {
    fn eq(&self, other: &Self) -> bool {
        match (*self, *other) {
            (Self::Source(left), Self::Source(right)) => left == right,
            (Self::Target(left), Self::Target(right)) => left == right,
            _ => false,
        }
    }
}

fn source_cmp(
    left: DraftMarkerAdmissionSourceKeyV1,
    right: DraftMarkerAdmissionSourceKeyV1,
) -> Ordering {
    if left == right {
        Ordering::Equal
    } else if source_key_less(left, right) {
        Ordering::Less
    } else {
        Ordering::Greater
    }
}

fn envelope_first_cmp(
    left: crate::DraftMarkerAdmissionEnvelopeV1,
    right: crate::DraftMarkerAdmissionEnvelopeV1,
    tree: DraftMarkerAdmissionTreeV1,
) -> Ordering {
    match (tree, left, right) {
        (
            DraftMarkerAdmissionTreeV1::SourceOrder,
            crate::DraftMarkerAdmissionEnvelopeV1::SourceOrder { first: left, .. },
            crate::DraftMarkerAdmissionEnvelopeV1::SourceOrder { first: right, .. },
        ) => source_cmp(left, right),
        (
            DraftMarkerAdmissionTreeV1::TargetId,
            crate::DraftMarkerAdmissionEnvelopeV1::TargetId { first: left, .. },
            crate::DraftMarkerAdmissionEnvelopeV1::TargetId { first: right, .. },
        ) => left.cmp(&right),
        _ => Ordering::Equal,
    }
}

pub(super) fn sum_node_charges(
    nodes: &[DraftMarkerAdmissionNodeV1],
) -> Result<u64, DraftMarkerAdmissionSchemaErrorV1> {
    nodes.iter().try_fold(0_u64, |sum, node| {
        sum.checked_add(encoded_node_record_charge(&node.key(), node)?)
            .ok_or(DraftMarkerAdmissionSchemaErrorV1::ArithmeticOverflow)
    })
}
