use super::super::*;
use super::{
    super::import::read::authenticate_manifest,
    super::import::stream::marker_frame,
    advance::{
        DraftComposerRecallImageSourceV1, PreparedDraftComposerRecallAdvanceV1, prepare_step,
    },
    command::custody,
    request::DraftComposerRecallErrorV1,
    source::{authenticate_basis, required, source_owner},
};
use crate::{SyndicStorage, domain::SyndicDomain};
use beryl_home_store::{
    DomainReader, ExecutableHomeProofCommand, FixedDigestHomeProofProtocol, HomeProofCommand,
    HomeProofReceipt, HomeStore, ProofCorrelationBytes, ProofReceiptConsumer,
    ProofWitnessContribution,
};
use beryl_model::{AssetId, ComposerRecallMarkerPageV1};

pub type ComposerRecallMarkerProofProtocolV1 =
    FixedDigestHomeProofProtocol<0x5343524d50525631, 0x524543414c4c5631>;

pub struct ComposerRecallProofSourceV1 {
    build: Box<DraftPieceBuildRecordV1>,
    session: Box<DraftEditorCandidateSessionV1>,
    page: ComposerRecallMarkerPageV1,
}
pub struct DraftComposerRecallImageProofAttemptV1 {
    command: Option<ExecutableHomeProofCommand<ComposerRecallMarkerProofProtocolV1>>,
    consumer: ProofReceiptConsumer<ComposerRecallMarkerProofProtocolV1>,
    image: DraftComposerRecallImageSourceV1,
    page: ComposerRecallMarkerPageV1,
}
impl DraftComposerRecallImageProofAttemptV1 {
    pub fn take_command(
        &mut self,
    ) -> Option<ExecutableHomeProofCommand<ComposerRecallMarkerProofProtocolV1>> {
        self.command.take()
    }
    pub fn consume(
        self,
        storage: &SyndicStorage,
        store: &HomeStore,
        receipt: HomeProofReceipt<ComposerRecallMarkerProofProtocolV1>,
    ) -> Result<PreparedDraftComposerRecallAdvanceV1, DraftComposerRecallErrorV1> {
        self.image.prepared.scope.require(store)?;
        if self.command.is_some() {
            return Err(DraftComposerRecallErrorV1::Proof);
        }
        store
            .consume_proof_receipt(self.consumer, receipt)
            .map_err(|_| DraftComposerRecallErrorV1::Proof)?;
        let revision = storage.revision(store)?;
        let next = prepare_step(
            storage,
            store,
            self.image.build,
            self.image.session,
            Some(self.page.association().into()),
        )?;
        if storage.revision(store)? != revision {
            return Err(DraftComposerRecallErrorV1::Conflict);
        }
        Ok(next)
    }
}
impl SyndicStorage {
    pub fn prepare_composer_recall_image_proof(
        &self,
        store: &HomeStore,
        image: DraftComposerRecallImageSourceV1,
        asset: AssetId,
        witness: ProofWitnessContribution<ComposerRecallMarkerProofProtocolV1>,
    ) -> Result<DraftComposerRecallImageProofAttemptV1, DraftComposerRecallErrorV1> {
        image.prepared.scope.require(store)?;
        if image.prepared.home_generation != self.home_generation
            || store.health().generation() != Some(self.home_generation)
        {
            return Err(DraftComposerRecallErrorV1::Proof);
        }
        let page = image
            .descriptive_page(asset)
            .ok_or(DraftComposerRecallErrorV1::Proof)?;
        let source = image
            .prepared
            .scope
            .handle
            .proof_source::<ComposerRecallMarkerProofProtocolV1>(
                self.revision(store)?,
                super::super::proof_dispatch::SourceInput::ComposerRecall(
                    ComposerRecallProofSourceV1 {
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
                .ok_or(DraftComposerRecallErrorV1::Unavailable)?,
            store.home_revision()?,
            source,
        )
        .map_err(|_| DraftComposerRecallErrorV1::Proof)?;
        command
            .add_witness(witness)
            .map_err(|_| DraftComposerRecallErrorV1::Proof)?;
        let (command, consumer) = command
            .seal()
            .map_err(|_| DraftComposerRecallErrorV1::Proof)?;
        Ok(DraftComposerRecallImageProofAttemptV1 {
            command: Some(command),
            consumer,
            image,
            page,
        })
    }
}

pub(crate) fn expected(input: &ComposerRecallProofSourceV1) -> ProofCorrelationBytes {
    ProofCorrelationBytes::new(correlation(&input.page))
}
pub(crate) fn prove(
    input: &ComposerRecallProofSourceV1,
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<ProofCorrelationBytes, DraftComposerRecallErrorV1> {
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
    let DraftPieceBuildFrontierV1::OrdinaryRecall(f) = build.frontier() else {
        return Err(DraftComposerRecallErrorV1::Proof);
    };
    if actual != **build
        || session != DraftEditorCandidateSessionRecordV1::Head(*input.session.clone())
        || input.session.active_operation() != Some(&custody(build))
        || !recall_build_is_exact(build)
        || !progress_receipt_matches_build(&receipt, build)
        || !progress_receipt_is_exact(&receipt)
        || build.lifecycle() != DraftPieceBuildLifecycleV1::Open
        || f.phase != DraftSealedImportPhaseV1::Streaming
        || f.decoder != DraftSealedImportDecoderV1::BetweenAtoms
    {
        return Err(DraftComposerRecallErrorV1::Proof);
    }
    authenticate_basis(reader, &f.basis)?;
    authenticate_manifest(reader, f.basis.content)?;
    let (id, label) = marker_frame(reader, f.clone())?;
    let p = &input.page;
    if p.draft_id() != build.draft_id()
        || p.session_id() != *build.session_id().as_bytes()
        || p.operation_id() != *build.operation_id().as_bytes()
        || p.canonical_header() != build.canonical_header()
        || p.receipt_ordinal() != build.progress_receipt().key().transition_ordinal()
        || p.receipt_digest() != *build.progress_receipt().digest().as_bytes()
        || p.source_owner() != source_owner(reader, &f.basis)?
        || p.source_draft_id() != f.basis.key.draft_id()
        || p.source_session_id() != *f.basis.key.session_id().as_bytes()
        || p.source_operation_id()
            != *f
                .basis
                .operation_id()
                .ok_or(DraftComposerRecallErrorV1::Proof)?
                .as_bytes()
        || p.acceptance_digest() != f.basis.acceptance_digest
        || Some(p.proof()) != f.basis.assets
        || p.encoded_start() != f.encoded_cursor
        || p.source_piece() != f.next_piece
        || p.association().ordinal()
            != f.markers
                .checked_add(1)
                .ok_or(DraftComposerRecallErrorV1::Proof)?
        || p.association().marker_id() != id
        || p.association().label() != label
    {
        return Err(DraftComposerRecallErrorV1::Proof);
    }
    Ok(expected(input))
}

fn correlation(page: &ComposerRecallMarkerPageV1) -> [u8; 32] {
    beryl_model::composer_recall_marker_correlation(page)
}
