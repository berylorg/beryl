use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
};

use beryl_home_store::{DomainReader, PointReadLimit, ReadError};
use beryl_model::SyndicDraftMarkerId;
use sha2::{Digest, Sha256};

use crate::{codec::SMALL_MAX, domain::SyndicDomain};

use super::{
    DRAFT_MARKER_ADMISSION_MAX_ASSOCIATIONS, DRAFT_MARKER_ADMISSION_TREE_FANOUT,
    DRAFT_MARKER_ADMISSION_TREE_MAX_HEIGHT, DraftMarkerAdmissionChildV1,
    DraftMarkerAdmissionEnvelopeV1, DraftMarkerAdmissionEvidenceV1, DraftMarkerAdmissionNodeIdV1,
    DraftMarkerAdmissionNodeKeyV1, DraftMarkerAdmissionNodeKindV1,
    DraftMarkerAdmissionNodePayloadV1, DraftMarkerAdmissionNodeV1, DraftMarkerAdmissionNodesCodec,
    DraftMarkerAdmissionOwnerV1, DraftMarkerAdmissionPageIdentityV1,
    DraftMarkerAdmissionRetainedChargeV1, DraftMarkerAdmissionRootV1,
    DraftMarkerAdmissionSchemaErrorV1, DraftMarkerAdmissionSourceKeyV1,
    DraftMarkerAdmissionTargetDispositionV1, DraftMarkerAdmissionTreeV1,
    DraftMarkerLabelReadinessProvenPageV1, checked_draft_marker_admission_command_charge_v1,
    encoded_node_key_charge, encoded_node_record_charge, source_key_less,
};

const NODE_ID_DOMAIN: &[u8] = b"syndic/draft-marker-label-admission-index-node/v1";

#[derive(Debug)]
pub(crate) enum DraftMarkerAdmissionIndexPreparationErrorV1 {
    Read(ReadError),
    Schema(DraftMarkerAdmissionSchemaErrorV1),
    AssociationOutOfRange,
    DuplicateSource,
    DuplicateTarget,
    MissingNode,
    NodeIdOccupied,
    PathAuthentication,
    ProvenPageOwner,
}

impl From<ReadError> for DraftMarkerAdmissionIndexPreparationErrorV1 {
    fn from(value: ReadError) -> Self {
        Self::Read(value)
    }
}

impl From<DraftMarkerAdmissionSchemaErrorV1> for DraftMarkerAdmissionIndexPreparationErrorV1 {
    fn from(value: DraftMarkerAdmissionSchemaErrorV1) -> Self {
        Self::Schema(value)
    }
}

impl std::fmt::Display for DraftMarkerAdmissionIndexPreparationErrorV1 {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Read(error) => write!(
                formatter,
                "draft-marker admission index read failed: {error}"
            ),
            Self::Schema(error) => write!(
                formatter,
                "draft-marker admission index schema failed: {error}"
            ),
            Self::AssociationOutOfRange => {
                formatter.write_str("draft-marker association is out of range")
            }
            Self::DuplicateSource => formatter.write_str("draft-marker source is already admitted"),
            Self::DuplicateTarget => formatter.write_str("draft-marker target is already admitted"),
            Self::MissingNode => formatter.write_str("draft-marker admission path node is missing"),
            Self::NodeIdOccupied => {
                formatter.write_str("draft-marker successor node identity is occupied")
            }
            Self::PathAuthentication => {
                formatter.write_str("draft-marker admission path authentication failed")
            }
            Self::ProvenPageOwner => {
                formatter.write_str("draft-marker proven-page owner disagrees")
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DraftMarkerAdmissionIndexFootprintV1 {
    read_bytes: u64,
    write_bytes: u64,
    delete_bytes: u64,
    command_bytes: u64,
}

impl DraftMarkerAdmissionIndexFootprintV1 {
    pub(crate) const fn read_bytes(self) -> u64 {
        self.read_bytes
    }

    pub(crate) const fn write_bytes(self) -> u64 {
        self.write_bytes
    }

    pub(crate) const fn delete_bytes(self) -> u64 {
        self.delete_bytes
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DraftMarkerAdmissionRetainedChargeDeltaV1 {
    added: DraftMarkerAdmissionRetainedChargeV1,
    removed: DraftMarkerAdmissionRetainedChargeV1,
}

impl DraftMarkerAdmissionRetainedChargeDeltaV1 {
    pub(crate) const fn added(self) -> DraftMarkerAdmissionRetainedChargeV1 {
        self.added
    }

    pub(crate) const fn removed(self) -> DraftMarkerAdmissionRetainedChargeV1 {
        self.removed
    }
}

pub(crate) struct PreparedDraftMarkerAdmissionIndexSuccessorV1 {
    source_root: DraftMarkerAdmissionRootV1,
    target_root: DraftMarkerAdmissionRootV1,
    puts: Box<[DraftMarkerAdmissionNodeV1]>,
    deletions: Box<[DraftMarkerAdmissionNodeV1]>,
    retained_predecessor_nodes: Box<[DraftMarkerAdmissionChildV1]>,
    retained_charge_delta: DraftMarkerAdmissionRetainedChargeDeltaV1,
    footprint: DraftMarkerAdmissionIndexFootprintV1,
}

#[allow(dead_code)]
impl PreparedDraftMarkerAdmissionIndexSuccessorV1 {
    pub(crate) const fn source_root(&self) -> DraftMarkerAdmissionRootV1 {
        self.source_root
    }

    pub(crate) const fn target_root(&self) -> DraftMarkerAdmissionRootV1 {
        self.target_root
    }

    pub(crate) fn puts(&self) -> &[DraftMarkerAdmissionNodeV1] {
        &self.puts
    }

    pub(crate) fn deletions(&self) -> &[DraftMarkerAdmissionNodeV1] {
        &self.deletions
    }

    pub(crate) fn retained_predecessor_nodes(&self) -> &[DraftMarkerAdmissionChildV1] {
        &self.retained_predecessor_nodes
    }

    pub(crate) const fn retained_charge_delta(&self) -> DraftMarkerAdmissionRetainedChargeDeltaV1 {
        self.retained_charge_delta
    }

    pub(crate) const fn footprint(&self) -> DraftMarkerAdmissionIndexFootprintV1 {
        self.footprint
    }
}

trait AdmissionNodeReader {
    fn point(
        &self,
        key: &DraftMarkerAdmissionNodeKeyV1,
    ) -> Result<Option<DraftMarkerAdmissionNodeV1>, DraftMarkerAdmissionIndexPreparationErrorV1>;
}

struct DomainAdmissionNodeReader<'a, 'b> {
    reader: &'a DomainReader<'b, SyndicDomain>,
}

impl AdmissionNodeReader for DomainAdmissionNodeReader<'_, '_> {
    fn point(
        &self,
        key: &DraftMarkerAdmissionNodeKeyV1,
    ) -> Result<Option<DraftMarkerAdmissionNodeV1>, DraftMarkerAdmissionIndexPreparationErrorV1>
    {
        self.reader
            .point::<DraftMarkerAdmissionNodesCodec>(
                key,
                PointReadLimit::new(SMALL_MAX).expect("admission node point bound is nonzero"),
            )
            .map_err(Into::into)
    }
}

struct ReadLedger<'a, R> {
    reader: &'a R,
    read_bytes: u64,
    maximum_bytes: u64,
    cache: BTreeMap<DraftMarkerAdmissionNodeKeyV1, Option<DraftMarkerAdmissionNodeV1>>,
}

impl<R: AdmissionNodeReader> ReadLedger<'_, R> {
    fn point(
        &mut self,
        key: &DraftMarkerAdmissionNodeKeyV1,
    ) -> Result<Option<DraftMarkerAdmissionNodeV1>, DraftMarkerAdmissionIndexPreparationErrorV1>
    {
        if let Some(value) = self.cache.get(key) {
            return Ok(value.clone());
        }
        let value = self.reader.point(key)?;
        let charge = match value.as_ref() {
            Some(value) => encoded_node_record_charge(key, value)?,
            None => encoded_node_key_charge(key)?,
        };
        let read_bytes = self
            .read_bytes
            .checked_add(charge)
            .ok_or(DraftMarkerAdmissionSchemaErrorV1::ArithmeticOverflow)?;
        if read_bytes > self.maximum_bytes {
            return Err(DraftMarkerAdmissionSchemaErrorV1::CommandTooLarge.into());
        }
        self.read_bytes = read_bytes;
        self.cache.insert(*key, value.clone());
        Ok(value)
    }
}

mod tree_edit;

use tree_edit::{
    NodeIdFactory, SearchKey, authenticate_fresh_put_keys, authenticate_replay_deletions,
    edit_tree, sum_node_charges,
};

#[allow(dead_code)]
pub(crate) fn prepare_draft_marker_admission_index_successor_v1(
    reader: &DomainReader<'_, SyndicDomain>,
    owner: DraftMarkerAdmissionOwnerV1,
    source_root: DraftMarkerAdmissionRootV1,
    target_root: DraftMarkerAdmissionRootV1,
    prior_replay_nodes: &[DraftMarkerAdmissionChildV1],
    proven_page: &DraftMarkerLabelReadinessProvenPageV1,
    association_index: usize,
) -> Result<PreparedDraftMarkerAdmissionIndexSuccessorV1, DraftMarkerAdmissionIndexPreparationErrorV1>
{
    prepare_with_reader(
        &DomainAdmissionNodeReader { reader },
        owner,
        source_root,
        target_root,
        prior_replay_nodes,
        proven_page,
        association_index,
        DRAFT_MARKER_ADMISSION_TREE_MAX_HEIGHT,
        super::DRAFT_MARKER_ADMISSION_COMMAND_MAX_ENCODED_BYTES,
    )
}

#[allow(clippy::too_many_arguments)]
fn prepare_with_reader<R: AdmissionNodeReader>(
    reader: &R,
    owner: DraftMarkerAdmissionOwnerV1,
    source_root: DraftMarkerAdmissionRootV1,
    target_root: DraftMarkerAdmissionRootV1,
    prior_replay_nodes: &[DraftMarkerAdmissionChildV1],
    proven_page: &DraftMarkerLabelReadinessProvenPageV1,
    association_index: usize,
    maximum_height: u8,
    command_limit: u64,
) -> Result<PreparedDraftMarkerAdmissionIndexSuccessorV1, DraftMarkerAdmissionIndexPreparationErrorV1>
{
    if association_index >= proven_page.association_count() {
        return Err(DraftMarkerAdmissionIndexPreparationErrorV1::AssociationOutOfRange);
    }
    let page = proven_page.sealed_page();
    if page.owner != owner {
        return Err(DraftMarkerAdmissionIndexPreparationErrorV1::ProvenPageOwner);
    }
    let entry = &page.entries[association_index];
    if source_root.tree() != DraftMarkerAdmissionTreeV1::SourceOrder
        || target_root.tree() != DraftMarkerAdmissionTreeV1::TargetId
        || source_root.count() != target_root.count()
    {
        return Err(DraftMarkerAdmissionSchemaErrorV1::InvalidRoot.into());
    }
    if source_root.count() >= DRAFT_MARKER_ADMISSION_MAX_ASSOCIATIONS {
        return Err(DraftMarkerAdmissionSchemaErrorV1::CapacityExceeded.into());
    }
    source_root.validate_shape()?;
    target_root.validate_shape()?;

    let association_index = u64::try_from(association_index)
        .map_err(|_| DraftMarkerAdmissionSchemaErrorV1::ArithmeticOverflow)?;
    let page_identity =
        DraftMarkerAdmissionPageIdentityV1::new(proven_page.page_identity(), page.ordinal);
    let evidence = DraftMarkerAdmissionEvidenceV1::new(entry.evidence_bytes())?;
    let source_key = DraftMarkerAdmissionSourceKeyV1::new(entry.label, entry.target_marker_id);
    let mut ledger = ReadLedger {
        reader,
        read_bytes: 0,
        maximum_bytes: command_limit,
        cache: BTreeMap::new(),
    };
    authenticate_retained_predecessor_nodes(&mut ledger, owner, prior_replay_nodes)?;
    classify_page_occupancy(
        &mut ledger,
        owner,
        target_root,
        proven_page,
        association_index,
    )?;

    let target = edit_tree(
        &mut ledger,
        owner,
        target_root,
        SearchKey::Target(entry.target_marker_id),
        NodeIdFactory {
            owner,
            page: page_identity,
            association_index,
            tree: DraftMarkerAdmissionTreeV1::TargetId,
            next: 0,
        },
        maximum_height,
        || {
            DraftMarkerAdmissionNodeV1::target_leaf(
                DraftMarkerAdmissionNodeKeyV1::new(
                    owner,
                    DraftMarkerAdmissionNodeKindV1::Leaf,
                    DraftMarkerAdmissionNodeIdV1::from_bytes([0; 16]),
                ),
                entry.target_marker_id,
                page_identity,
                evidence.clone(),
                entry.label,
                entry.asset_id,
                DraftMarkerAdmissionTargetDispositionV1::Unassigned,
            )
        },
    )?;
    let source = edit_tree(
        &mut ledger,
        owner,
        source_root,
        SearchKey::Source(source_key),
        NodeIdFactory {
            owner,
            page: page_identity,
            association_index,
            tree: DraftMarkerAdmissionTreeV1::SourceOrder,
            next: 0,
        },
        maximum_height,
        || {
            DraftMarkerAdmissionNodeV1::source_leaf(
                DraftMarkerAdmissionNodeKeyV1::new(
                    owner,
                    DraftMarkerAdmissionNodeKindV1::Leaf,
                    DraftMarkerAdmissionNodeIdV1::from_bytes([0; 16]),
                ),
                source_key,
                evidence,
                entry.asset_id,
            )
        },
    )?;

    let mut protected = source.path_keys;
    protected.extend(target.path_keys);
    let deletions =
        authenticate_replay_deletions(&mut ledger, owner, prior_replay_nodes, &protected)?;
    let mut puts = source.puts;
    puts.extend(target.puts);
    authenticate_fresh_put_keys(&mut ledger, &puts)?;

    let write_bytes = sum_node_charges(&puts)?;
    let delete_bytes = sum_node_charges(&deletions)?;
    let command_bytes = ledger
        .read_bytes
        .checked_add(write_bytes)
        .and_then(|value| value.checked_add(delete_bytes))
        .ok_or(DraftMarkerAdmissionSchemaErrorV1::ArithmeticOverflow)?;
    if command_bytes > command_limit {
        return Err(DraftMarkerAdmissionSchemaErrorV1::CommandTooLarge.into());
    }
    checked_draft_marker_admission_command_charge_v1([
        ledger.read_bytes,
        write_bytes,
        delete_bytes,
    ])?;

    let mut retained_predecessor_nodes = source.predecessor;
    retained_predecessor_nodes.extend(target.predecessor);
    let added = DraftMarkerAdmissionRetainedChargeV1::new(0, 1, write_bytes);
    let removed = DraftMarkerAdmissionRetainedChargeV1::new(0, 0, delete_bytes);
    Ok(PreparedDraftMarkerAdmissionIndexSuccessorV1 {
        source_root: source.root,
        target_root: target.root,
        puts: puts.into_boxed_slice(),
        deletions: deletions.into_boxed_slice(),
        retained_predecessor_nodes: retained_predecessor_nodes.into_boxed_slice(),
        retained_charge_delta: DraftMarkerAdmissionRetainedChargeDeltaV1 { added, removed },
        footprint: DraftMarkerAdmissionIndexFootprintV1 {
            read_bytes: ledger.read_bytes,
            write_bytes,
            delete_bytes,
            command_bytes,
        },
    })
}

fn authenticate_retained_predecessor_nodes<R: AdmissionNodeReader>(
    ledger: &mut ReadLedger<'_, R>,
    owner: DraftMarkerAdmissionOwnerV1,
    retained: &[DraftMarkerAdmissionChildV1],
) -> Result<(), DraftMarkerAdmissionIndexPreparationErrorV1> {
    for expected in retained {
        let node = ledger
            .point(&expected.key())?
            .ok_or(DraftMarkerAdmissionIndexPreparationErrorV1::MissingNode)?;
        node.validate()?;
        if node.key() != expected.key()
            || node.key().owner() != owner
            || node.digest() != expected.digest()
            || node.count()? != expected.count()
            || node.envelope()? != expected.envelope()
        {
            return Err(DraftMarkerAdmissionIndexPreparationErrorV1::PathAuthentication);
        }
    }
    Ok(())
}

fn classify_page_occupancy<R: AdmissionNodeReader>(
    ledger: &mut ReadLedger<'_, R>,
    owner: DraftMarkerAdmissionOwnerV1,
    target_root: DraftMarkerAdmissionRootV1,
    proven_page: &DraftMarkerLabelReadinessProvenPageV1,
    consumed_prefix: u64,
) -> Result<(), DraftMarkerAdmissionIndexPreparationErrorV1> {
    let page = proven_page.sealed_page();
    let page_identity =
        DraftMarkerAdmissionPageIdentityV1::new(proven_page.page_identity(), page.ordinal);
    for (index, entry) in page.entries.iter().enumerate() {
        let occupied = point_target_leaf(ledger, owner, target_root, entry.target_marker_id)?;
        let index = u64::try_from(index)
            .map_err(|_| DraftMarkerAdmissionSchemaErrorV1::ArithmeticOverflow)?;
        if index < consumed_prefix {
            let expected_evidence = DraftMarkerAdmissionEvidenceV1::new(entry.evidence_bytes())?;
            match occupied.as_ref().map(DraftMarkerAdmissionNodeV1::payload) {
                Some(DraftMarkerAdmissionNodePayloadV1::TargetLeaf {
                    target_marker_id,
                    page,
                    evidence,
                    source_label,
                    asset_id,
                    disposition,
                }) if *target_marker_id == entry.target_marker_id
                    && *page == page_identity
                    && *evidence == expected_evidence
                    && *source_label == entry.label
                    && *asset_id == entry.asset_id
                    && *disposition == DraftMarkerAdmissionTargetDispositionV1::Unassigned => {}
                _ => return Err(DraftMarkerAdmissionIndexPreparationErrorV1::DuplicateTarget),
            }
        } else if occupied.is_some() {
            return Err(DraftMarkerAdmissionIndexPreparationErrorV1::DuplicateTarget);
        }
    }
    Ok(())
}

fn point_target_leaf<R: AdmissionNodeReader>(
    ledger: &mut ReadLedger<'_, R>,
    owner: DraftMarkerAdmissionOwnerV1,
    root: DraftMarkerAdmissionRootV1,
    target: SyndicDraftMarkerId,
) -> Result<Option<DraftMarkerAdmissionNodeV1>, DraftMarkerAdmissionIndexPreparationErrorV1> {
    if root.tree() != DraftMarkerAdmissionTreeV1::TargetId {
        return Err(DraftMarkerAdmissionIndexPreparationErrorV1::PathAuthentication);
    }
    root.validate_shape()?;
    let Some(root_key) = root.node() else {
        return Ok(None);
    };
    if root_key.owner() != owner {
        return Err(DraftMarkerAdmissionIndexPreparationErrorV1::PathAuthentication);
    }
    let mut node = ledger
        .point(&root_key)?
        .ok_or(DraftMarkerAdmissionIndexPreparationErrorV1::MissingNode)?;
    node.validate()?;
    if node.key() != root_key
        || node.tree() != DraftMarkerAdmissionTreeV1::TargetId
        || node.height() != root.height()
        || node.digest() != root.digest()
        || node.count()? != root.count()
    {
        return Err(DraftMarkerAdmissionIndexPreparationErrorV1::PathAuthentication);
    }
    loop {
        match node.payload() {
            DraftMarkerAdmissionNodePayloadV1::Internal { height, children } => {
                let mut selected = None;
                for (index, child) in children.iter().enumerate() {
                    let DraftMarkerAdmissionEnvelopeV1::TargetId { last, .. } = child.envelope()
                    else {
                        return Err(
                            DraftMarkerAdmissionIndexPreparationErrorV1::PathAuthentication,
                        );
                    };
                    if target <= last {
                        selected = Some(index);
                        break;
                    }
                }
                let selected = selected
                    .or_else(|| children.len().checked_sub(1))
                    .ok_or(DraftMarkerAdmissionIndexPreparationErrorV1::PathAuthentication)?;
                let expected = children[selected];
                let child = ledger
                    .point(&expected.key())?
                    .ok_or(DraftMarkerAdmissionIndexPreparationErrorV1::MissingNode)?;
                child.validate()?;
                if child.key() != expected.key()
                    || child.key().owner() != owner
                    || child.tree() != DraftMarkerAdmissionTreeV1::TargetId
                    || child.height().checked_add(1) != Some(*height)
                    || child.digest() != expected.digest()
                    || child.count()? != expected.count()
                    || child.envelope()? != expected.envelope()
                {
                    return Err(DraftMarkerAdmissionIndexPreparationErrorV1::PathAuthentication);
                }
                node = child;
            }
            DraftMarkerAdmissionNodePayloadV1::TargetLeaf {
                target_marker_id, ..
            } => return Ok((*target_marker_id == target).then_some(node)),
            DraftMarkerAdmissionNodePayloadV1::SourceLeaf { .. } => {
                return Err(DraftMarkerAdmissionIndexPreparationErrorV1::PathAuthentication);
            }
        }
    }
}

#[cfg(feature = "test-faults")]
mod test_fixture;

#[cfg(feature = "test-faults")]
pub use test_fixture::{
    DraftMarkerAdmissionIndexTestErrorV1, DraftMarkerAdmissionIndexTestStateV1,
    DraftMarkerAdmissionIndexTestStepV1,
};
