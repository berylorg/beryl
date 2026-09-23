use crate::{
    DiscussionHandoffGateRecord, codec::DiscussionHandoffGatesCodec, domain::SyndicDomain,
};
use beryl_home_store::RecordCodec;

pub fn encode_discussion_gate_fixture(record: &DiscussionHandoffGateRecord) -> Vec<u8> {
    <DiscussionHandoffGatesCodec as RecordCodec<SyndicDomain>>::encode_value(record)
        .expect("gate fixture encodes")
}

pub fn decode_discussion_gate_fixture(bytes: &[u8]) -> Result<DiscussionHandoffGateRecord, String> {
    <DiscussionHandoffGatesCodec as RecordCodec<SyndicDomain>>::decode_value(bytes)
        .map_err(|error| error.to_string())
}
