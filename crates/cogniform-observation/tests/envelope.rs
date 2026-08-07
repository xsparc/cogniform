//! Exact compatibility, bounds, and corruption coverage for payload envelopes.

use core::num::{NonZeroU32, NonZeroU64};

use cogniform_observation::{
    EntityVisibility, OBSERVATION_PAYLOAD_ENVELOPE_HEADER_BYTES,
    OBSERVATION_PAYLOAD_ENVELOPE_VERSION, ObservationEnvelopeError, ObservationPayload,
    ObservationPayloadLimits, decode_payload, encode_payload,
};
use cogniform_protocol::{
    FrameId, ImageDimensions, ObservationId, ObservationKind, ObservationMetadata,
    ObservationQuality, ObservationStaleness, RuntimeLimits, SceneRevision, SchemaVersion,
    StableEntityId,
};
use sha2::{Digest, Sha256};

const PREFIX_BYTES: usize = 28;
const DIGEST_OFFSET: usize = 28;

#[test]
fn every_payload_kind_round_trips_with_exact_framing() {
    let first = StableEntityId::new(1).unwrap();
    let second = StableEntityId::new(2).unwrap();
    let cases = [
        (
            metadata(ObservationKind::Color),
            ObservationPayload::Color(vec![[1, 2, 3, 4], [5, 6, 7, 8]]),
            1,
        ),
        (
            metadata(ObservationKind::Depth),
            ObservationPayload::Depth(vec![0.0, 1.0]),
            2,
        ),
        (
            metadata(ObservationKind::Normal),
            ObservationPayload::Normal(vec![None, Some([0.0, 0.0, 1.0])]),
            3,
        ),
        (
            metadata(ObservationKind::EntityId),
            ObservationPayload::EntityId(vec![None, Some(first)]),
            4,
        ),
        (
            metadata(ObservationKind::Visibility),
            ObservationPayload::Visibility(vec![
                EntityVisibility {
                    entity_id: first,
                    visible_pixels: 1,
                },
                EntityVisibility {
                    entity_id: second,
                    visible_pixels: 1,
                },
            ]),
            5,
        ),
    ];

    for (metadata, payload, kind_tag) in cases {
        let encoded = encode_payload(
            &metadata,
            &payload,
            &RuntimeLimits::default(),
            ObservationPayloadLimits::default(),
        )
        .unwrap();
        assert_eq!(&encoded[..8], b"COGOBS01");
        assert_eq!(
            u16::from_be_bytes(copy_array(&encoded[8..10])),
            OBSERVATION_PAYLOAD_ENVELOPE_VERSION
        );
        assert_eq!(encoded[10], kind_tag);
        assert_eq!(encoded[11], 0);
        assert_eq!(
            u64::from_be_bytes(copy_array(&encoded[12..20])),
            payload.item_count()
        );
        assert_eq!(
            u64::from_be_bytes(copy_array(&encoded[20..28])),
            u64::try_from(encoded.len() - OBSERVATION_PAYLOAD_ENVELOPE_HEADER_BYTES).unwrap()
        );
        assert_eq!(
            decode_payload(
                &metadata,
                &encoded,
                &RuntimeLimits::default(),
                ObservationPayloadLimits::default(),
            )
            .unwrap(),
            payload
        );
    }
}

#[test]
fn invalid_metadata_rejects_encode_and_decode() {
    let valid_metadata = metadata(ObservationKind::Color);
    let payload = ObservationPayload::Color(vec![[1, 2, 3, 4], [5, 6, 7, 8]]);
    let encoded = encode(&valid_metadata, &payload);
    let mut invalid_metadata = valid_metadata;
    invalid_metadata.dimensions = None;

    assert_eq!(
        encode_payload(
            &invalid_metadata,
            &payload,
            &RuntimeLimits::default(),
            ObservationPayloadLimits::default(),
        )
        .unwrap_err(),
        ObservationEnvelopeError::InvalidMetadata
    );
    assert_eq!(
        decode(&invalid_metadata, &encoded).unwrap_err(),
        ObservationEnvelopeError::InvalidMetadata
    );
}

#[test]
fn color_fixture_is_byte_stable() {
    const EXPECTED: &[u8] = &[
        0x43, 0x4f, 0x47, 0x4f, 0x42, 0x53, 0x30, 0x31, 0x00, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x61, 0x14,
        0xb4, 0x13, 0xed, 0xa0, 0x20, 0x18, 0x71, 0x1a, 0xf7, 0xd1, 0x59, 0xf0, 0xcb, 0x38, 0xec,
        0x7f, 0xbc, 0xc3, 0x03, 0xb5, 0xff, 0xc0, 0xe0, 0x30, 0xb5, 0x61, 0x2e, 0x54, 0x49, 0xd7,
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08,
    ];
    let metadata = metadata(ObservationKind::Color);
    let payload = ObservationPayload::Color(vec![[1, 2, 3, 4], [5, 6, 7, 8]]);
    let encoded = encode_payload(
        &metadata,
        &payload,
        &RuntimeLimits::default(),
        ObservationPayloadLimits::default(),
    )
    .unwrap();
    assert_eq!(encoded, EXPECTED);
    assert_eq!(encoded.len(), OBSERVATION_PAYLOAD_ENVELOPE_HEADER_BYTES + 8);
}

#[test]
fn numeric_presence_and_identity_layouts_are_fixed_big_endian() {
    let first = StableEntityId::new(1).unwrap();
    let second = StableEntityId::new(2).unwrap();
    let cases = [
        (
            metadata(ObservationKind::Depth),
            ObservationPayload::Depth(vec![0.0, 1.0]),
            vec![0, 0, 0, 0, 0x3f, 0x80, 0, 0],
        ),
        (
            metadata(ObservationKind::Normal),
            ObservationPayload::Normal(vec![None, Some([0.0, 0.0, 1.0])]),
            [
                vec![0; 13],
                vec![1, 0, 0, 0, 0, 0, 0, 0, 0, 0x3f, 0x80, 0, 0],
            ]
            .concat(),
        ),
        (
            metadata(ObservationKind::EntityId),
            ObservationPayload::EntityId(vec![None, Some(first)]),
            [
                vec![0; 17],
                vec![1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1],
            ]
            .concat(),
        ),
        (
            metadata(ObservationKind::Visibility),
            ObservationPayload::Visibility(vec![
                EntityVisibility {
                    entity_id: first,
                    visible_pixels: 1,
                },
                EntityVisibility {
                    entity_id: second,
                    visible_pixels: 2,
                },
            ]),
            [
                vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1],
                vec![0, 0, 0, 0, 0, 0, 0, 1],
                vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2],
                vec![0, 0, 0, 0, 0, 0, 0, 2],
            ]
            .concat(),
        ),
    ];

    for (metadata, payload, expected) in cases {
        let encoded = encode(&metadata, &payload);
        assert_eq!(
            &encoded[OBSERVATION_PAYLOAD_ENVELOPE_HEADER_BYTES..],
            expected
        );
    }
}

#[test]
fn metadata_and_every_envelope_byte_are_integrity_bound() {
    let metadata = metadata(ObservationKind::Color);
    let payload = ObservationPayload::Color(vec![[1, 2, 3, 4], [5, 6, 7, 8]]);
    let encoded = encode_payload(
        &metadata,
        &payload,
        &RuntimeLimits::default(),
        ObservationPayloadLimits::default(),
    )
    .unwrap();

    for length in 0..encoded.len() {
        assert!(decode(&metadata, &encoded[..length]).is_err());
    }
    let mut extended = encoded.clone();
    extended.push(0);
    assert!(matches!(
        decode(&metadata, &extended),
        Err(ObservationEnvelopeError::TrailingBytes { .. })
    ));

    for index in 0..encoded.len() {
        let mut corrupted = encoded.clone();
        corrupted[index] ^= 1;
        assert!(
            decode(&metadata, &corrupted).is_err(),
            "byte {index} was not protected"
        );
    }

    let mut other_metadata = metadata.clone();
    other_metadata.observation_id = ObservationId::new(2).unwrap();
    assert_eq!(
        decode(&other_metadata, &encoded).unwrap_err(),
        ObservationEnvelopeError::IntegrityMismatch
    );
}

#[test]
fn header_contract_rejects_unknown_noncanonical_and_inconsistent_values() {
    let color_metadata = metadata(ObservationKind::Color);
    let payload = ObservationPayload::Color(vec![[1, 2, 3, 4], [5, 6, 7, 8]]);
    let encoded = encode(&color_metadata, &payload);

    let mut invalid_magic = encoded.clone();
    invalid_magic[0] = b'X';
    assert_eq!(
        decode(&color_metadata, &invalid_magic).unwrap_err(),
        ObservationEnvelopeError::InvalidMagic
    );

    let mut invalid_version = encoded.clone();
    invalid_version[8..10].copy_from_slice(&2_u16.to_be_bytes());
    assert_eq!(
        decode(&color_metadata, &invalid_version).unwrap_err(),
        ObservationEnvelopeError::UnsupportedVersion { found: 2 }
    );

    let mut invalid_kind = encoded.clone();
    invalid_kind[10] = 99;
    assert_eq!(
        decode(&color_metadata, &invalid_kind).unwrap_err(),
        ObservationEnvelopeError::UnsupportedKind { found: 99 }
    );

    let mut reserved = encoded.clone();
    reserved[11] = 1;
    assert_eq!(
        decode(&color_metadata, &reserved).unwrap_err(),
        ObservationEnvelopeError::NonCanonicalEncoding
    );

    let mut wrong_length = encoded[..encoded.len() - 1].to_vec();
    wrong_length[20..28].copy_from_slice(&7_u64.to_be_bytes());
    assert_eq!(
        decode(&color_metadata, &wrong_length).unwrap_err(),
        ObservationEnvelopeError::PayloadLengthMismatch {
            expected: 8,
            actual: 7,
        }
    );

    let mut wrong_count = encoded[..OBSERVATION_PAYLOAD_ENVELOPE_HEADER_BYTES + 4].to_vec();
    wrong_count[12..20].copy_from_slice(&1_u64.to_be_bytes());
    wrong_count[20..28].copy_from_slice(&4_u64.to_be_bytes());
    update_digest(&color_metadata, &mut wrong_count);
    assert_eq!(
        decode(&color_metadata, &wrong_count).unwrap_err(),
        ObservationEnvelopeError::ItemCountMismatch {
            expected: 2,
            actual: 1,
        }
    );

    let depth_metadata = metadata(ObservationKind::Depth);
    assert_eq!(
        decode(&depth_metadata, &encoded).unwrap_err(),
        ObservationEnvelopeError::KindMismatch {
            metadata: ObservationKind::Depth,
            payload: ObservationKind::Color,
        }
    );
}

#[test]
fn encode_rejects_kind_count_and_image_value_violations() {
    let runtime = RuntimeLimits::default();
    let limits = ObservationPayloadLimits::default();

    assert!(matches!(
        encode_payload(
            &metadata(ObservationKind::Color),
            &ObservationPayload::Depth(vec![0.0, 1.0]),
            &runtime,
            limits,
        ),
        Err(ObservationEnvelopeError::KindMismatch { .. })
    ));
    assert_eq!(
        encode_payload(
            &metadata(ObservationKind::Color),
            &ObservationPayload::Color(vec![[0; 4]]),
            &runtime,
            limits,
        )
        .unwrap_err(),
        ObservationEnvelopeError::ItemCountMismatch {
            expected: 2,
            actual: 1,
        }
    );

    for invalid in [f32::NAN, f32::INFINITY, -0.0, -0.1, 1.1] {
        assert!(matches!(
            encode_payload(
                &metadata(ObservationKind::Depth),
                &ObservationPayload::Depth(vec![invalid, 1.0]),
                &runtime,
                limits,
            ),
            Err(ObservationEnvelopeError::InvalidPayloadValue {
                kind: ObservationKind::Depth,
                index: 0,
            })
        ));
    }

    for invalid in [
        [0.0, 0.0, 0.0],
        [2.0, 0.0, 0.0],
        [f32::NAN, 0.0, 1.0],
        [-0.0, 0.0, 1.0],
    ] {
        assert!(matches!(
            encode_payload(
                &metadata(ObservationKind::Normal),
                &ObservationPayload::Normal(vec![Some(invalid), None]),
                &runtime,
                limits,
            ),
            Err(ObservationEnvelopeError::InvalidPayloadValue {
                kind: ObservationKind::Normal,
                index: 0,
            })
        ));
    }
}

#[test]
fn encode_rejects_visibility_order_and_limit_violations() {
    let runtime = RuntimeLimits::default();
    let limits = ObservationPayloadLimits::default();

    let first = StableEntityId::new(1).unwrap();
    let second = StableEntityId::new(2).unwrap();
    for invalid in [
        vec![
            EntityVisibility {
                entity_id: second,
                visible_pixels: 1,
            },
            EntityVisibility {
                entity_id: first,
                visible_pixels: 1,
            },
        ],
        vec![EntityVisibility {
            entity_id: first,
            visible_pixels: 0,
        }],
    ] {
        assert!(matches!(
            encode_payload(
                &metadata(ObservationKind::Visibility),
                &ObservationPayload::Visibility(invalid),
                &runtime,
                limits,
            ),
            Err(ObservationEnvelopeError::InvalidPayloadValue {
                kind: ObservationKind::Visibility,
                ..
            })
        ));
    }

    let one_entry = ObservationPayload::Visibility(vec![EntityVisibility {
        entity_id: first,
        visible_pixels: 1,
    }]);
    let zero_visibility_entries =
        ObservationPayloadLimits::new(NonZeroU64::new(1024).unwrap(), NonZeroU32::new(1).unwrap());
    let two_entries = ObservationPayload::Visibility(vec![
        EntityVisibility {
            entity_id: first,
            visible_pixels: 1,
        },
        EntityVisibility {
            entity_id: second,
            visible_pixels: 1,
        },
    ]);
    assert!(matches!(
        encode_payload(
            &metadata(ObservationKind::Visibility),
            &two_entries,
            &runtime,
            zero_visibility_entries,
        ),
        Err(ObservationEnvelopeError::VisibilityEntryLimitExceeded { .. })
    ));

    let tiny_envelope = ObservationPayloadLimits::new(
        NonZeroU64::new(u64::try_from(OBSERVATION_PAYLOAD_ENVELOPE_HEADER_BYTES).unwrap()).unwrap(),
        NonZeroU32::new(1).unwrap(),
    );
    assert!(matches!(
        encode_payload(
            &metadata(ObservationKind::Visibility),
            &one_entry,
            &runtime,
            tiny_envelope,
        ),
        Err(ObservationEnvelopeError::EnvelopeLimitExceeded { .. })
    ));
}

#[test]
fn decode_applies_envelope_and_visibility_limits_before_return() {
    let color_metadata = metadata(ObservationKind::Color);
    let color = encode(
        &color_metadata,
        &ObservationPayload::Color(vec![[1; 4], [2; 4]]),
    );
    let byte_limit = ObservationPayloadLimits::new(
        NonZeroU64::new(u64::try_from(color.len() - 1).unwrap()).unwrap(),
        NonZeroU32::new(2).unwrap(),
    );
    assert!(matches!(
        decode_payload(
            &color_metadata,
            &color,
            &RuntimeLimits::default(),
            byte_limit,
        ),
        Err(ObservationEnvelopeError::EnvelopeLimitExceeded { .. })
    ));

    let visibility_metadata = metadata(ObservationKind::Visibility);
    let visibility = encode(
        &visibility_metadata,
        &ObservationPayload::Visibility(vec![
            EntityVisibility {
                entity_id: StableEntityId::new(1).unwrap(),
                visible_pixels: 1,
            },
            EntityVisibility {
                entity_id: StableEntityId::new(2).unwrap(),
                visible_pixels: 1,
            },
        ]),
    );
    let entry_limit =
        ObservationPayloadLimits::new(NonZeroU64::new(1024).unwrap(), NonZeroU32::new(1).unwrap());
    assert!(matches!(
        decode_payload(
            &visibility_metadata,
            &visibility,
            &RuntimeLimits::default(),
            entry_limit,
        ),
        Err(ObservationEnvelopeError::VisibilityEntryLimitExceeded { .. })
    ));
}

#[test]
fn decode_rejects_noncanonical_or_invalid_values_even_with_a_valid_digest() {
    let entity_metadata = metadata(ObservationKind::EntityId);
    let mut absent_entity = encode(
        &entity_metadata,
        &ObservationPayload::EntityId(vec![None, None]),
    );
    absent_entity[OBSERVATION_PAYLOAD_ENVELOPE_HEADER_BYTES + 1] = 1;
    update_digest(&entity_metadata, &mut absent_entity);
    assert_eq!(
        decode(&entity_metadata, &absent_entity).unwrap_err(),
        ObservationEnvelopeError::NonCanonicalEncoding
    );

    let mut zero_entity = encode(
        &entity_metadata,
        &ObservationPayload::EntityId(vec![None, None]),
    );
    zero_entity[OBSERVATION_PAYLOAD_ENVELOPE_HEADER_BYTES] = 1;
    update_digest(&entity_metadata, &mut zero_entity);
    assert!(matches!(
        decode(&entity_metadata, &zero_entity),
        Err(ObservationEnvelopeError::InvalidPayloadValue {
            kind: ObservationKind::EntityId,
            index: 0,
        })
    ));

    let normal_metadata = metadata(ObservationKind::Normal);
    let mut absent_normal = encode(
        &normal_metadata,
        &ObservationPayload::Normal(vec![None, None]),
    );
    absent_normal[OBSERVATION_PAYLOAD_ENVELOPE_HEADER_BYTES + 1] = 1;
    update_digest(&normal_metadata, &mut absent_normal);
    assert_eq!(
        decode(&normal_metadata, &absent_normal).unwrap_err(),
        ObservationEnvelopeError::NonCanonicalEncoding
    );

    let depth_metadata = metadata(ObservationKind::Depth);
    let mut negative_zero = encode(&depth_metadata, &ObservationPayload::Depth(vec![0.0, 1.0]));
    negative_zero
        [OBSERVATION_PAYLOAD_ENVELOPE_HEADER_BYTES..OBSERVATION_PAYLOAD_ENVELOPE_HEADER_BYTES + 4]
        .copy_from_slice(&(-0.0_f32).to_bits().to_be_bytes());
    update_digest(&depth_metadata, &mut negative_zero);
    assert!(matches!(
        decode(&depth_metadata, &negative_zero),
        Err(ObservationEnvelopeError::InvalidPayloadValue {
            kind: ObservationKind::Depth,
            index: 0,
        })
    ));
}

#[test]
fn visibility_sum_respects_the_runtime_pixel_bound() {
    let runtime = RuntimeLimits {
        max_observation_pixels: NonZeroU64::new(1).unwrap(),
        ..RuntimeLimits::default()
    };
    let payload = ObservationPayload::Visibility(vec![EntityVisibility {
        entity_id: StableEntityId::new(1).unwrap(),
        visible_pixels: 2,
    }]);
    assert_eq!(
        encode_payload(
            &metadata(ObservationKind::Visibility),
            &payload,
            &runtime,
            ObservationPayloadLimits::default(),
        )
        .unwrap_err(),
        ObservationEnvelopeError::VisibilityPixelLimitExceeded {
            actual: 2,
            limit: 1,
        }
    );
}

fn encode(metadata: &ObservationMetadata, payload: &ObservationPayload) -> Vec<u8> {
    encode_payload(
        metadata,
        payload,
        &RuntimeLimits::default(),
        ObservationPayloadLimits::default(),
    )
    .unwrap()
}

fn decode(
    metadata: &ObservationMetadata,
    encoded: &[u8],
) -> Result<ObservationPayload, ObservationEnvelopeError> {
    decode_payload(
        metadata,
        encoded,
        &RuntimeLimits::default(),
        ObservationPayloadLimits::default(),
    )
}

fn metadata(kind: ObservationKind) -> ObservationMetadata {
    ObservationMetadata {
        schema_version: SchemaVersion::V1,
        observation_id: ObservationId::new(1).unwrap(),
        scene_revision: SceneRevision::new(3),
        frame_id: FrameId::new(4).unwrap(),
        camera_id: StableEntityId::new(5).unwrap(),
        kind,
        dimensions: (kind != ObservationKind::Visibility).then(|| ImageDimensions {
            width: NonZeroU32::new(2).unwrap(),
            height: NonZeroU32::new(1).unwrap(),
        }),
        quality: ObservationQuality::Low,
        observed_at_unix_micros: 6,
        production_latency_micros: 7,
        staleness: ObservationStaleness {
            latest_known_revision: SceneRevision::new(3),
            revisions_behind: 0,
        },
    }
}

fn update_digest(metadata: &ObservationMetadata, encoded: &mut [u8]) {
    let metadata_json = metadata
        .to_canonical_json(&RuntimeLimits::default())
        .unwrap();
    let mut hasher = Sha256::new();
    hasher.update(&encoded[..PREFIX_BYTES]);
    hasher.update(metadata_json);
    hasher.update(&encoded[OBSERVATION_PAYLOAD_ENVELOPE_HEADER_BYTES..]);
    encoded[DIGEST_OFFSET..OBSERVATION_PAYLOAD_ENVELOPE_HEADER_BYTES]
        .copy_from_slice(&hasher.finalize());
}

fn copy_array<const N: usize>(encoded: &[u8]) -> [u8; N] {
    let mut value = [0_u8; N];
    value.copy_from_slice(encoded);
    value
}
