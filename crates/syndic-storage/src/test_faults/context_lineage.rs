use crate::{
    CasLineageProof,
    codec::parts::{Decoder, Encoder, dec_lineage, enc_lineage},
};

pub fn encode_lineage_fixture(lineage: CasLineageProof) -> Vec<u8> {
    let mut encoder = Encoder::new();
    enc_lineage(&mut encoder, lineage);
    encoder.finish()
}

pub fn decode_lineage_fixture(bytes: &[u8]) -> Result<CasLineageProof, String> {
    let mut decoder = Decoder::new(bytes);
    let lineage = dec_lineage(&mut decoder).map_err(|error| error.to_string())?;
    decoder.finish().map_err(|error| error.to_string())?;
    Ok(lineage)
}

pub fn foreign_home_seed_lineage_fixture(
    store: &beryl_home_store::HomeStore,
    storage: &crate::SyndicStorage,
    thread: beryl_model::SyndicThreadId,
    usable: &crate::UsableCasBinding,
    foreign_home: beryl_model::BerylHomeId,
) -> Result<CasLineageProof, String> {
    use crate::context_projection::{ContextEstablishment, frame, read};
    let CasLineageProof::ContextAware(mut proof) = usable.lineage() else {
        return Err("expected seed context lineage".into());
    };
    let ContextEstablishment::NativeSeed {
        source,
        cut,
        injection,
        ..
    } = proof.establishment
    else {
        return Err("expected native seed".into());
    };
    if cut.summary.count() != 0 || proof.summary.count() != 1 {
        return Err("expected one context entry".into());
    }
    let record = storage
        .context_record_for_execution(store, thread)
        .map_err(|error| error.to_string())?;
    let frame = frame::describe_for_home(foreign_home, thread, &record)
        .map_err(|error| error.to_string())?;
    proof.home = foreign_home;
    proof.summary = crate::ContextSummary::empty()
        .append(frame.entry, frame.utf8_bytes)
        .map_err(|error| error.to_string())?;
    proof.projection_digest = read::projection_digest(
        foreign_home,
        thread,
        proof.prefix,
        proof.summary,
        usable.execution(),
        usable.tool_profile(),
    );
    proof.establishment = ContextEstablishment::NativeSeed {
        source,
        cut,
        entry: frame.entry,
        injection,
    };
    proof
        .checked()
        .map(CasLineageProof::ContextAware)
        .map_err(|error| error.to_string())
}
