use super::super::*;
use super::{
    admission::{authenticate_source, required},
    advance::{DraftReplacementImageSourceV1, PreparedDraftReplacementAdvanceV1, prepare_step},
    command::custody,
    read::authenticate_manifest,
    request::DraftReplacementErrorV1,
    stream::marker_frame,
};
use crate::{SyndicStorage, codec::*, domain::SyndicDomain};
use beryl_home_store::{
    DomainReader, ExecutableHomeProofCommand, FixedDigestHomeProofProtocol, HomeProofCommand,
    HomeProofReceipt, HomeStore, ProofCorrelationBytes, ProofReceiptConsumer,
    ProofWitnessContribution,
};
use beryl_model::{AssetId, HistoricalInputMarkerPageV1};
use sha2::{Digest, Sha256};

pub type HistoricalInputMarkerProofProtocolV1 =
    FixedDigestHomeProofProtocol<0x5348494d50525631, 0x494d504147455631>;

pub struct HistoricalImportSource {
    build: Box<DraftPieceBuildRecordV1>,
    session: Box<DraftEditorCandidateSessionV1>,
    page: HistoricalInputMarkerPageV1,
}
pub struct DraftReplacementImageProofAttemptV1 {
    command: Option<ExecutableHomeProofCommand<HistoricalInputMarkerProofProtocolV1>>,
    consumer: ProofReceiptConsumer<HistoricalInputMarkerProofProtocolV1>,
    image: DraftReplacementImageSourceV1,
    page: HistoricalInputMarkerPageV1,
}
impl DraftReplacementImageProofAttemptV1 {
    pub fn take_command(
        &mut self,
    ) -> Option<ExecutableHomeProofCommand<HistoricalInputMarkerProofProtocolV1>> {
        self.command.take()
    }
    pub fn consume(
        self,
        storage: &SyndicStorage,
        store: &HomeStore,
        receipt: HomeProofReceipt<HistoricalInputMarkerProofProtocolV1>,
    ) -> Result<PreparedDraftReplacementAdvanceV1, DraftReplacementErrorV1> {
        self.image.prepared.scope.require_current(store)?;
        if self.command.is_some() {
            return Err(DraftReplacementErrorV1::Proof);
        }
        store
            .consume_proof_receipt(self.consumer, receipt)
            .map_err(|_| DraftReplacementErrorV1::Proof)?;
        let revision = storage.revision(store)?;
        let next = prepare_step(
            storage,
            store,
            self.image.build,
            self.image.session,
            Some(self.page.association()),
        )?;
        if storage.revision(store)? != revision {
            return Err(DraftReplacementErrorV1::Conflict);
        }
        Ok(next)
    }
}
impl SyndicStorage {
    pub fn prepare_replacement_image_proof(
        &self,
        store: &HomeStore,
        image: DraftReplacementImageSourceV1,
        asset: AssetId,
        witness: ProofWitnessContribution<HistoricalInputMarkerProofProtocolV1>,
    ) -> Result<DraftReplacementImageProofAttemptV1, DraftReplacementErrorV1> {
        image.prepared.scope.require_current(store)?;
        if image.prepared.home_generation != self.home_generation
            || store.health().generation() != Some(self.home_generation)
        {
            return Err(DraftReplacementErrorV1::Proof);
        }
        let page = image
            .descriptive_page(asset)
            .ok_or(DraftReplacementErrorV1::Proof)?;
        let source = image
            .prepared
            .scope
            .handle
            .proof_source::<HistoricalInputMarkerProofProtocolV1>(
                self.revision(store)?,
                super::super::proof_dispatch::SourceInput::HistoricalInput(
                    HistoricalImportSource {
                        build: image.build.clone(),
                        session: image.session.clone(),
                        page: page.clone(),
                    },
                ),
            );
        let mut command = HomeProofCommand::new(
            store
                .health()
                .generation()
                .ok_or(DraftReplacementErrorV1::Unavailable)?,
            store.home_revision()?,
            source,
        )
        .map_err(|_| DraftReplacementErrorV1::Proof)?;
        command
            .add_witness(witness)
            .map_err(|_| DraftReplacementErrorV1::Proof)?;
        let (command, consumer) = command.seal().map_err(|_| DraftReplacementErrorV1::Proof)?;
        Ok(DraftReplacementImageProofAttemptV1 {
            command: Some(command),
            consumer,
            image,
            page,
        })
    }
}

pub(crate) fn expected(input: &HistoricalImportSource) -> ProofCorrelationBytes {
    ProofCorrelationBytes::new(correlation(&input.page))
}
pub(crate) fn prove(
    input: &HistoricalImportSource,
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<ProofCorrelationBytes, DraftReplacementErrorV1> {
    let build = &input.build;
    let key =
        DraftPieceSettlementKeyV1::new(build.draft_id(), build.session_id(), build.operation_id());
    let actual = required::<DraftPieceBuildsFamily>(reader, &key)?;
    let session = required::<DraftEditorCandidateSessionsFamily>(
        reader,
        &DraftEditorCandidateSessionRecordKeyV1::head(build.draft_id(), build.session_id()),
    )?;
    let receipt =
        required::<DraftPieceBuildProgressFamily>(reader, &build.progress_receipt().key())?;
    let DraftPieceBuildFrontierV1::SealedImport(f) = build.frontier() else {
        return Err(DraftReplacementErrorV1::Proof);
    };
    if actual != **build
        || session != DraftEditorCandidateSessionRecordV1::Head(*input.session.clone())
        || input.session.active_operation() != Some(&custody(build))
        || !import_build_is_exact(build)
        || !progress_receipt_matches_build(&receipt, build)
        || !progress_receipt_is_exact(&receipt)
        || build.lifecycle() != DraftPieceBuildLifecycleV1::Open
        || f.phase != DraftSealedImportPhaseV1::Streaming
        || f.decoder != DraftSealedImportDecoderV1::BetweenAtoms
    {
        return Err(DraftReplacementErrorV1::Proof);
    }
    authenticate_source(reader, f.basis)?;
    authenticate_manifest(reader, f.basis.content)?;
    let (id, label) = marker_frame(reader, f)?;
    let p = &input.page;
    if p.draft_id() != build.draft_id()
        || p.session_id() != *build.session_id().as_bytes()
        || p.operation_id() != *build.operation_id().as_bytes()
        || p.canonical_header() != build.canonical_header()
        || p.receipt_ordinal() != build.progress_receipt().key().transition_ordinal()
        || p.receipt_digest() != *build.progress_receipt().digest().as_bytes()
        || p.item_id() != f.basis.item_id
        || Some(p.proof()) != f.basis.assets
        || p.encoded_start() != f.encoded_cursor
        || p.source_piece() != f.next_piece
        || p.association().ordinal()
            != f.markers
                .checked_add(1)
                .ok_or(DraftReplacementErrorV1::Proof)?
        || p.association().marker_id() != id
        || p.association().label() != label
    {
        return Err(DraftReplacementErrorV1::Proof);
    }
    Ok(expected(input))
}

fn correlation(page: &HistoricalInputMarkerPageV1) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"syndic/historical-input-marker-window/v1");
    hash.update(page.draft_id().as_bytes());
    hash.update(page.session_id());
    hash.update(page.operation_id());
    hash.update(page.receipt_ordinal().to_be_bytes());
    hash.update(page.receipt_digest());
    hash.update((page.canonical_header().len() as u64).to_be_bytes());
    hash.update(page.canonical_header());
    hash.update(page.item_id().as_bytes());
    hash.update(page.proof().set_id().as_bytes());
    hash.update(page.proof().sequential().marker_digest());
    hash.update(page.proof().sequential().marker_count().to_be_bytes());
    hash.update(
        page.proof()
            .sequential()
            .maximum_image_label()
            .map_or(0, |v| v.get())
            .to_be_bytes(),
    );
    hash.update(page.proof().ordered_assets().marker_asset_digest());
    hash.update(page.proof().ordered_assets().marker_count().to_be_bytes());
    hash.update(page.proof().entry_frontier().to_be_bytes());
    hash.update(page.proof().asset_chain_digest().as_bytes());
    hash.update(page.encoded_start().to_be_bytes());
    hash.update(page.source_piece().to_be_bytes());
    let a = page.association();
    hash.update(a.ordinal().to_be_bytes());
    hash.update(a.marker_id().as_bytes());
    hash.update(a.label().get().to_be_bytes());
    hash.update([a.asset_id().version() as u8]);
    hash.update(a.asset_id().digest());
    hash.update(a.asset_id().length().get().to_be_bytes());
    hash.finalize().into()
}
