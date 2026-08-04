use beryl_backend::ClientUserMessageId;
use beryl_model::SyndicAcceptedInputId;

use super::{
    AcceptedInputSteeringCorrelationError,
    decode_accepted_input_steering_correlation,
    encode_accepted_input_steering_correlation,
};

#[test]
fn accepted_input_correlation_is_canonical_and_strict() {
    let input_id = SyndicAcceptedInputId::from_bytes([
        0x00, 0x01, 0x0a, 0x0f, 0x10, 0x11, 0x7f, 0x80, 0x9a, 0xaf, 0xb0, 0xcd, 0xde, 0xef,
        0xfe, 0xff,
    ]);
    let encoded = encode_accepted_input_steering_correlation(input_id);
    assert_eq!(
        encoded.as_str(),
        "beryl.accepted-input.v1:00010a0f10117f809aafb0cddeeffeff"
    );
    assert_eq!(
        decode_accepted_input_steering_correlation(&encoded).unwrap(),
        input_id
    );
    let distinct = encode_accepted_input_steering_correlation(
        SyndicAcceptedInputId::from_bytes([0xff; 16]),
    );
    assert_ne!(distinct.as_str(), encoded.as_str());

    let wrong_prefix =
        ClientUserMessageId::try_new("beryl.accepted-input.v2:00010a0f10117f809aafb0cddeeffeff")
            .unwrap();
    assert_eq!(
        decode_accepted_input_steering_correlation(&wrong_prefix),
        Err(AcceptedInputSteeringCorrelationError::WrongPrefix)
    );
    let wrong_length =
        ClientUserMessageId::try_new("beryl.accepted-input.v1:00010a0f10117f809aafb0cddeeffef")
            .unwrap();
    assert!(matches!(
        decode_accepted_input_steering_correlation(&wrong_length),
        Err(AcceptedInputSteeringCorrelationError::WrongLength { .. })
    ));
    let too_long =
        ClientUserMessageId::try_new("beryl.accepted-input.v1:00010a0f10117f809aafb0cddeeffeff0")
            .unwrap();
    assert!(matches!(
        decode_accepted_input_steering_correlation(&too_long),
        Err(AcceptedInputSteeringCorrelationError::WrongLength { .. })
    ));
    for noncanonical in [
        "beryl.accepted-input.v1:A0010a0f10117f809aafb0cddeeffeff",
        "beryl.accepted-input.v1:g0010a0f10117f809aafb0cddeeffeff",
        "beryl.accepted-input.v1:éééééééééééééééé",
    ] {
        let value = ClientUserMessageId::try_new(noncanonical).unwrap();
        assert!(matches!(
            decode_accepted_input_steering_correlation(&value),
            Err(AcceptedInputSteeringCorrelationError::InvalidHex { index: 0 })
        ));
    }
}
