//! Field-wise receive-limit negotiation contracts for CF041.

use core::num::{NonZeroU32, NonZeroU64};

use cogniform_local_session::LocalSessionLimits;
use cogniform_local_transport::LocalFrameConfig;

#[test]
fn negotiation_is_field_wise_bounded_and_round_trips_to_frame_policy() {
    let local_config = LocalFrameConfig::default();
    let mut peer = LocalSessionLimits::from_config(&local_config).unwrap();
    peer.max_control_message_bytes = NonZeroU64::new(8_192).unwrap();
    peer.max_observation_envelope_bytes = NonZeroU64::new(1_024).unwrap();
    peer.max_visibility_entries = NonZeroU32::new(12).unwrap();
    peer.runtime_limits.max_components = NonZeroU32::new(3).unwrap();
    peer.runtime_limits.max_components_per_entity = NonZeroU32::new(3).unwrap();
    peer.runtime_limits.max_query_entities = NonZeroU32::new(7).unwrap();

    let effective = peer.negotiate(&local_config).unwrap();
    assert_eq!(effective.max_control_message_bytes.get(), 8_192);
    assert_eq!(effective.max_observation_envelope_bytes.get(), 1_024);
    assert_eq!(effective.max_visibility_entries.get(), 12);
    assert_eq!(effective.runtime_limits.max_components.get(), 3);
    assert_eq!(effective.runtime_limits.max_components_per_entity.get(), 3);
    assert_eq!(effective.runtime_limits.max_query_entities.get(), 7);

    let effective_config = effective.to_frame_config().unwrap();
    assert_eq!(
        LocalSessionLimits::from_config(&effective_config).unwrap(),
        effective
    );
}

#[test]
fn negotiation_normalizes_nested_component_capacity_after_intersection() {
    let local_config = LocalFrameConfig::default();
    let mut peer = LocalSessionLimits::from_config(&local_config).unwrap();
    peer.runtime_limits.max_components = NonZeroU32::new(2).unwrap();
    peer.runtime_limits.max_components_per_entity = NonZeroU32::new(2).unwrap();

    let mut local_with_narrow_total = local_config;
    local_with_narrow_total.runtime_limits.max_components = NonZeroU32::new(1).unwrap();
    local_with_narrow_total
        .runtime_limits
        .max_components_per_entity = NonZeroU32::new(1).unwrap();

    let effective = peer.negotiate(&local_with_narrow_total).unwrap();
    assert_eq!(effective.runtime_limits.max_components.get(), 1);
    assert_eq!(effective.runtime_limits.max_components_per_entity.get(), 1);
    effective.validate().unwrap();
}

#[test]
fn negotiation_recaps_envelope_when_the_other_peer_has_narrower_bulk() {
    let config = LocalFrameConfig::default();
    let left = LocalSessionLimits::from_config(&config).unwrap();
    let mut right = left;
    right.max_bulk_bytes = NonZeroU64::new(512).unwrap();
    right.max_observation_envelope_bytes = NonZeroU64::new(512).unwrap();

    let mut left_with_narrow_envelope = left;
    left_with_narrow_envelope.max_observation_envelope_bytes = NonZeroU64::new(1_024).unwrap();
    let effective = left_with_narrow_envelope.intersect(&right).unwrap();
    assert_eq!(effective.max_bulk_bytes.get(), 512);
    assert_eq!(effective.max_observation_envelope_bytes.get(), 512);
}
