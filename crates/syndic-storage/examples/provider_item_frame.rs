use std::{convert::Infallible, io::Cursor};

use beryl_model::CasItemId;
use syndic_storage::{
    ProviderAgentMessageV1, ProviderFrameSinkV1, ProviderFrameTextSpanV1,
    ProviderFrameTextSpanValidatorV1, ProviderItemFrameV1, ProviderItemObservationV1,
    ProviderItemV1, ProviderLifecycleTimestampMsV1, ProviderTextV1, encode_provider_item_frame_v1,
    validate_streaming_provider_item_frame_v1,
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
    let mut spans = ProviderFrameTextSpanValidatorV1::new(reference.ordinal());
    let structural = validate_streaming_provider_item_frame_v1(
        &mut Cursor::new(&bytes.0),
        0,
        bytes.0.len() as u64,
        reference.encoded_digest(),
        &mut spans,
    )
    .unwrap();
    spans.finish(structural.reference()).unwrap();

    assert_eq!(structural.reference(), &reference);
    assert_eq!(reference.encoded_len(), bytes.0.len() as u64);
    Ok(())
}
