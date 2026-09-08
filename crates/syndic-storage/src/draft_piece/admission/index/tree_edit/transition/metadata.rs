use super::*;
use crate::draft_piece::DraftMarkerAdmissionAssignmentGroupV1;

pub(super) struct IngestionEntry {
    pub(super) association_index: u64,
    pub(super) source_key: DraftMarkerAdmissionSourceKeyV1,
    pub(super) evidence: DraftMarkerAdmissionEvidenceV1,
    pub(super) asset: AssetId,
}

pub(super) fn ingestion(
    receipt: &DraftMarkerAdmissionReplayReceiptV1,
    selected_head: Option<&DraftMarkerAdmissionHeadV1>,
) -> Result<Option<IngestionEntry>, TransitionError> {
    let mut source = Bytes(receipt.source_head_bytes());
    let mut target = Bytes(receipt.target_head_bytes());
    let authority = target.take(504)?;
    if source.take(504)? != authority {
        return Err(invalid());
    }
    let mut digest = Sha256::new();
    digest.update(b"syndic/draft-marker-label-readiness-request/v1");
    digest.update(authority);
    let digest: [u8; 32] = digest.finalize().into();
    if &digest != receipt.request_commitment().as_bytes() {
        return Err(invalid());
    }
    let header = target.take(81)?;
    if source.take(81)? != header {
        return Err(invalid());
    }
    let mut header = Bytes(header);
    if &header.fixed::<16>()? != receipt.owner().draft_id().as_bytes()
        || &header.fixed::<16>()? != receipt.owner().session_id().as_bytes()
        || &header.fixed::<16>()? != receipt.owner().operation_id().as_bytes()
        || &header.fixed::<16>()? != receipt.command_id().as_bytes()
        || header.u64_le()? != receipt.page_ordinal().get()
    {
        return Err(invalid());
    }
    let eof = match header.fixed::<1>()? {
        [0] => false,
        [1] => true,
        _ => return Err(invalid()),
    };
    let count = header.u64_le()?;
    if count > 256 || (count == 0 && !eof) {
        return Err(invalid());
    }
    let selected_index = match selected_head {
        Some(head) if head.ingestion_association_cursor() != 0 => {
            if head.ingestion_association_cursor() >= count
                || head.next_page_ordinal() != receipt.page_ordinal()
                || head.evidence_eof()
            {
                return Err(invalid());
            }
            head.ingestion_association_cursor().checked_sub(1)
        }
        Some(head) => {
            if receipt.page_ordinal().get().checked_add(1) != Some(head.next_page_ordinal().get())
                || head.evidence_eof() != eof
            {
                return Err(invalid());
            }
            count.checked_sub(1)
        }
        None if eof => count.checked_sub(1),
        None => return Err(invalid()),
    };
    let mut selected = None;
    let mut markers = BTreeSet::new();
    for association_index in 0..count {
        let group = target.group()?;
        if source.take(group.canonical_bytes().len())? != group.canonical_bytes() {
            return Err(invalid());
        }
        let marker = SyndicDraftMarkerId::from_bytes(target.fixed()?);
        if !markers.insert(marker) {
            return Err(invalid());
        }
        let evidence_len = match target.0 {
            [0, 0, ..] => 434,
            [0, 1, ..] => 450,
            [1, ..] => 194,
            [2, ..] => 42,
            _ => return Err(invalid()),
        };
        let evidence = target.take(evidence_len)?;
        if source.take(evidence_len)? != evidence {
            return Err(invalid());
        }
        let mut asset = Bytes(&evidence[evidence_len - 41..]);
        if asset.fixed::<1>()? != [1] {
            return Err(invalid());
        }
        let asset_digest = asset.fixed()?;
        let asset_length = NonZeroU64::new(asset.u64_le()?).ok_or_else(invalid)?;
        let asset = AssetId::sha256_v1(asset_digest, asset_length);
        let evidence = DraftMarkerAdmissionEvidenceV1::new(evidence)?;
        evidence.validate_group(group, asset)?;
        if Some(association_index) == selected_index {
            selected = Some(IngestionEntry {
                association_index,
                source_key: DraftMarkerAdmissionSourceKeyV1::new(group, marker),
                evidence,
                asset,
            });
        }
    }
    if !source.0.is_empty() || !target.0.is_empty() {
        return Err(invalid());
    }
    Ok(selected)
}

pub(super) fn assignment_label(
    receipt: &DraftMarkerAdmissionReplayReceiptV1,
    group: DraftMarkerAdmissionAssignmentGroupV1,
    asset: AssetId,
) -> Result<ImageLabelOrdinal, TransitionError> {
    let mut source = Bytes(receipt.source_head_bytes());
    source.take(32)?;
    if source.take(group.canonical_bytes().len())? != group.canonical_bytes()
        || source.fixed::<32>()? != asset.digest()
        || source.u64_le()? != asset.length().get()
        || !source.0.is_empty()
    {
        return Err(invalid());
    }
    let mut target = Bytes(receipt.target_head_bytes());
    if &target.fixed::<32>()? != receipt.target_before().digest().as_bytes()
        || target.u64_le()? != receipt.target_before().count()
    {
        return Err(invalid());
    }
    let label = ImageLabelOrdinal::new(target.u64_le()?).map_err(|_| invalid())?;
    if !target.0.is_empty()
        || matches!(group, DraftMarkerAdmissionAssignmentGroupV1::PreserveLabel(expected) if label != expected)
    {
        return Err(invalid());
    }
    Ok(label)
}

pub(super) fn empty_assignment(
    receipt: &DraftMarkerAdmissionReplayReceiptV1,
) -> Result<(), TransitionError> {
    let source = receipt
        .source_head_bytes()
        .strip_prefix(b"syndic/draft-marker-empty-assignment-source/v1")
        .ok_or_else(invalid)?;
    let target = receipt
        .target_head_bytes()
        .strip_prefix(b"syndic/draft-marker-empty-assignment-target/v1")
        .ok_or_else(invalid)?;
    if source.len() != 32 || source != target {
        return Err(invalid());
    }
    Ok(())
}

struct Bytes<'a>(&'a [u8]);

impl<'a> Bytes<'a> {
    fn take(&mut self, count: usize) -> Result<&'a [u8], TransitionError> {
        let value = self.0.get(..count).ok_or_else(invalid)?;
        self.0 = &self.0[count..];
        Ok(value)
    }

    fn fixed<const N: usize>(&mut self) -> Result<[u8; N], TransitionError> {
        self.take(N)?.try_into().map_err(|_| invalid())
    }

    fn u64_le(&mut self) -> Result<u64, TransitionError> {
        Ok(u64::from_le_bytes(self.fixed()?))
    }

    fn group(&mut self) -> Result<DraftMarkerAdmissionAssignmentGroupV1, TransitionError> {
        Ok(match self.fixed::<1>()? {
            [0] => DraftMarkerAdmissionAssignmentGroupV1::PreserveLabel(
                ImageLabelOrdinal::new(u64::from_be_bytes(self.fixed()?)).map_err(|_| invalid())?,
            ),
            [1] => DraftMarkerAdmissionAssignmentGroupV1::AllocateLabel(
                beryl_model::SyndicThreadId::from_bytes(self.fixed()?),
                ImageLabelOrdinal::new(u64::from_be_bytes(self.fixed()?)).map_err(|_| invalid())?,
            ),
            [2] => {
                if self.fixed::<1>()? != [1] {
                    return Err(invalid());
                }
                let digest = self.fixed()?;
                let length =
                    NonZeroU64::new(u64::from_be_bytes(self.fixed()?)).ok_or_else(invalid)?;
                DraftMarkerAdmissionAssignmentGroupV1::FreshAsset(AssetId::sha256_v1(
                    digest, length,
                ))
            }
            _ => return Err(invalid()),
        })
    }
}
