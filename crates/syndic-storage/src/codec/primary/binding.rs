use super::*;

fn encode_usable(e: &mut Encoder, value: &UsableCasBinding) {
    enc_execution(e, value.execution());
    enc_external(e, value.cas_thread_id().as_str());
    enc_represented_prefix(e, value.represented_prefix());
    enc_native_turn_count(e, value.native_turn_count());
    enc_tool_profile(e, value.tool_profile());
    enc_lineage(e, value.lineage());
}

fn decode_usable(d: &mut Decoder<'_>) -> Result<UsableCasBinding, CodecError> {
    Ok(UsableCasBinding::new(
        dec_execution(d)?,
        dec_cas_thread(d)?,
        dec_represented_prefix(d)?,
        dec_native_turn_count(d)?,
        dec_tool_profile(d)?,
        dec_lineage(d)?,
    ))
}

fn encode_binding_state(e: &mut Encoder, value: &BindingState) {
    match value {
        BindingState::Unbound { reason } => {
            e.u8(0);
            e.text(reason)
        }
        BindingState::Valid(binding) => {
            e.u8(1);
            encode_usable(e, binding)
        }
        BindingState::Active(binding) => {
            e.u8(2);
            encode_usable(e, binding.usable());
            enc_snapshot(e, binding.snapshot_id());
            enc_turn(e, binding.turn_id());
            enc_input_gate_rev(e, binding.activation_gate_revision());
            enc_timestamp(e, binding.started_at());
        }
        BindingState::Stale(stale) => {
            e.u8(3);
            enc_execution(e, stale.execution());
            enc_external(e, stale.cas_thread_id().as_str());
            enc_opt(e, stale.observed_tool_profile(), enc_tool_profile);
            enc_opt(e, stale.observed_prefix(), enc_represented_prefix);
            match stale.observed_lineage() {
                Some(lineage) => {
                    e.u8(1);
                    enc_lineage(e, lineage);
                }
                None => e.u8(0),
            }
            enc_opt(e, stale.observed_native_turn_count(), enc_native_turn_count);
            match stale.loaded_generation() {
                Some(generation) => {
                    e.u8(1);
                    enc_loaded_generation(e, generation);
                }
                None => e.u8(0),
            }
            e.text(stale.reason());
            enc_timestamp(e, stale.observed_at());
        }
    }
}

fn decode_binding_state(d: &mut Decoder<'_>) -> Result<BindingState, CodecError> {
    match d.u8()? {
        0 => BindingState::unbound(d.text("unbound reason")?)
            .map_err(|source| invalid("unbound binding", source)),
        1 => Ok(BindingState::valid(decode_usable(d)?)),
        2 => {
            let usable = decode_usable(d)?;
            let snapshot = dec_snapshot(d)?;
            let turn = dec_turn(d)?;
            Ok(BindingState::active(ActiveCasBinding::new(
                usable,
                snapshot,
                turn,
                dec_input_gate_rev(d)?,
                dec_timestamp(d)?,
            )))
        }
        3 => {
            let execution = dec_execution(d)?;
            let cas = dec_cas_thread(d)?;
            let tool_profile = dec_opt(d, "stale tool profile", dec_tool_profile)?;
            let prefix = dec_opt(d, "stale represented prefix", dec_represented_prefix)?;
            let lineage = dec_opt(d, "stale lineage", dec_lineage)?;
            let native_turn_count = dec_opt(d, "stale native turn count", dec_native_turn_count)?;
            let loaded = dec_opt(d, "stale loaded generation", dec_loaded_generation)?;
            let reason = d.text("stale reason")?;
            let observed_at = dec_timestamp(d)?;
            StaleCasBinding::new(
                execution,
                cas,
                tool_profile,
                prefix,
                lineage,
                native_turn_count,
                loaded,
                reason,
                observed_at,
            )
            .map(BindingState::stale)
            .map_err(|source| invalid("stale binding", source))
        }
        tag => Err(CodecError::InvalidTag {
            kind: "binding state",
            tag,
        }),
    }
}

pub(super) fn encode_binding_record(value: &BindingRecord) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_thread(&mut e, value.thread_id());
    enc_binding_rev(&mut e, value.revision());
    enc_selected_path(&mut e, value.selected_path());
    encode_binding_state(&mut e, value.state());
    Ok(e.finish())
}

pub(super) fn decode_binding_record(bytes: &[u8]) -> Result<BindingRecord, CodecError> {
    let mut d = Decoder::new(bytes);
    let value = BindingRecord::new(
        dec_thread(&mut d)?,
        dec_binding_rev(&mut d)?,
        dec_selected_path(&mut d)?,
        decode_binding_state(&mut d)?,
    );
    d.finish()?;
    Ok(value)
}

pub(super) fn encode_execution_snapshot(
    value: &ExecutionSnapshotRecord,
) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    e.u8(match value.kind() {
        ExecutionSnapshotKind::OrdinaryConversation => 0,
        ExecutionSnapshotKind::ProviderOperation(ProviderOperationKind::ContextCompaction) => 1,
    });
    enc_snapshot(&mut e, value.id());
    enc_thread(&mut e, value.thread_id());
    enc_binding_rev(&mut e, value.binding_revision());
    enc_input_gate_rev(&mut e, value.activation_gate_revision());
    enc_turn(&mut e, value.active_turn_id());
    enc_external(&mut e, value.cas_thread_id().as_str());
    enc_selected_path(&mut e, value.selected_path());
    enc_represented_prefix(&mut e, value.represented_base_prefix());
    enc_native_turn_count(&mut e, value.represented_base_native_turn_count());
    enc_tool_profile(&mut e, value.tool_profile());
    enc_lineage(&mut e, value.lineage());
    enc_execution(&mut e, value.execution());
    enc_loaded_generation(&mut e, value.loaded_generation());
    enc_timestamp(&mut e, value.started_at());
    Ok(e.finish())
}

pub(super) fn decode_execution_snapshot(
    bytes: &[u8],
) -> Result<ExecutionSnapshotRecord, CodecError> {
    let mut d = Decoder::new(bytes);
    let kind = match d.u8()? {
        0 => ExecutionSnapshotKind::OrdinaryConversation,
        1 => ExecutionSnapshotKind::ProviderOperation(ProviderOperationKind::ContextCompaction),
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "execution-snapshot kind",
                tag,
            });
        }
    };
    let id = dec_snapshot(&mut d)?;
    let thread = dec_thread(&mut d)?;
    let revision = dec_binding_rev(&mut d)?;
    let activation_gate_revision = dec_input_gate_rev(&mut d)?;
    let turn = dec_turn(&mut d)?;
    let cas_thread = dec_cas_thread(&mut d)?;
    let selected = dec_selected_path(&mut d)?;
    let represented = dec_represented_prefix(&mut d)?;
    let represented_native_turn_count = dec_native_turn_count(&mut d)?;
    let tool_profile = dec_tool_profile(&mut d)?;
    let lineage = dec_lineage(&mut d)?;
    let execution = dec_execution(&mut d)?;
    let loaded = dec_loaded_generation(&mut d)?;
    let started = dec_timestamp(&mut d)?;
    let value = match kind {
        ExecutionSnapshotKind::OrdinaryConversation => ExecutionSnapshotRecord::new(
            id,
            thread,
            revision,
            activation_gate_revision,
            turn,
            cas_thread,
            selected,
            represented,
            represented_native_turn_count,
            tool_profile,
            lineage,
            execution,
            loaded,
            started,
        ),
        ExecutionSnapshotKind::ProviderOperation(ProviderOperationKind::ContextCompaction) => {
            ExecutionSnapshotRecord::provider_operation(
                id,
                thread,
                revision,
                activation_gate_revision,
                turn,
                cas_thread,
                selected,
                represented,
                represented_native_turn_count,
                tool_profile,
                lineage,
                execution,
                loaded,
                started,
            )
        }
    };
    d.finish()?;
    Ok(value)
}

pub(super) fn encode_active_cas_turn(value: &ActiveCasTurnRecord) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_snapshot(&mut e, value.snapshot_id());
    enc_thread(&mut e, value.thread_id());
    enc_turn(&mut e, value.turn_id());
    enc_binding_rev(&mut e, value.binding_revision());
    enc_external(&mut e, value.cas_thread_id().as_str());
    enc_external(&mut e, value.cas_turn_id().as_str());
    enc_timestamp(&mut e, value.published_at());
    Ok(e.finish())
}

pub(super) fn decode_active_cas_turn(bytes: &[u8]) -> Result<ActiveCasTurnRecord, CodecError> {
    let mut d = Decoder::new(bytes);
    let value = ActiveCasTurnRecord::new(
        dec_snapshot(&mut d)?,
        dec_thread(&mut d)?,
        dec_turn(&mut d)?,
        dec_binding_rev(&mut d)?,
        dec_cas_thread(&mut d)?,
        dec_cas_turn(&mut d)?,
        dec_timestamp(&mut d)?,
    );
    d.finish()?;
    Ok(value)
}
