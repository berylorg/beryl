use std::convert::Infallible;

use beryl_model::CasItemId;
use syndic_storage::{
    PROVIDER_FRAME_BOUNDED_DECODE_MAX_BYTES, ProviderAgentMessageV1, ProviderFrameSinkV1,
    ProviderFrameTextSpanV1, ProviderItemFrameV1, ProviderItemObservationV1, ProviderItemV1,
    ProviderLifecycleTimestampMsV1, ProviderTextV1, decode_bounded_provider_item_frame_v1,
    encode_provider_item_frame_v1,
};

#[derive(Default)]
struct FrameBytes(Vec<u8>);

impl ProviderFrameSinkV1 for FrameBytes {
    type Error = Infallible;

    fn write_chunk(&mut self, chunk: &[u8]) -> Result<(), Self::Error> {
        self.0.extend_from_slice(chunk);
        Ok(())
    }

    fn write_text_span(&mut self, _span: ProviderFrameTextSpanV1) -> Result<(), Self::Error> {
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let frame = ProviderItemFrameV1::new(
        syndic_storage::ProviderFrameOrdinalV1::FIRST,
        CasItemId::new("provider-item")?,
        ProviderItemObservationV1::Started {
            observed_at: ProviderLifecycleTimestampMsV1::new(1),
            item: ProviderItemV1::AgentMessage(ProviderAgentMessageV1 {
                text: ProviderTextV1::inline("hello"),
                phase: None,
                memory_citation: None,
            }),
        },
    );
    let mut bytes = FrameBytes::default();
    let reference = encode_provider_item_frame_v1(&frame, 0, &mut bytes).unwrap();
    let decoded = decode_bounded_provider_item_frame_v1(
        &bytes.0,
        PROVIDER_FRAME_BOUNDED_DECODE_MAX_BYTES,
        0,
    )?;

    assert_eq!(decoded, frame);
    assert_eq!(reference.encoded_len(), bytes.0.len() as u64);
    Ok(())
}
