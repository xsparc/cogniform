//! Append-only replay, integrity, and corrupt-tail contracts.

use core::num::NonZeroU32;

use cogniform_protocol::{
    ComponentValue, ConflictPolicy, CreateEntity, DeliverySemantic, FiniteF32, FrameId,
    IdempotencyKey, LocalTransform, PatchBudget, PositiveF32, PositiveVec3, Quaternion,
    ReparentEntity, SceneOperation, ScenePatch, SceneRevision, SchemaVersion, SetComponent,
    StableEntityId, TransactionId, Vec3,
};
use cogniform_replay::{
    RecordedApplyError, RecordedWorld, ReplayConfig, ReplayIntegrityErrorKind, ReplayLog,
    ReplayTailErrorKind,
};
use cogniform_world::{AuthoritativeWorld, WorldConfig};

fn id(value: u128) -> StableEntityId {
    StableEntityId::new(value).unwrap()
}

fn patch(base: SceneRevision, nonce: u128, operations: Vec<SceneOperation>) -> ScenePatch {
    ScenePatch {
        schema_version: SchemaVersion::V1,
        transaction_id: TransactionId::new(nonce * 2).unwrap(),
        idempotency_key: IdempotencyKey::new((nonce * 2) + 1).unwrap(),
        base_revision: base,
        conflict_policy: ConflictPolicy::RequireExactBase,
        delivery: DeliverySemantic::MustApply,
        declared_budget: PatchBudget::default(),
        operations,
    }
}

fn create(entity_id: StableEntityId) -> SceneOperation {
    SceneOperation::Create(CreateEntity {
        entity_id,
        components: Vec::new(),
    })
}

fn local(x: f32) -> ComponentValue {
    ComponentValue::LocalTransform(LocalTransform {
        translation: Vec3 {
            x: FiniteF32::new(x).unwrap(),
            y: FiniteF32::new(0.0).unwrap(),
            z: FiniteF32::new(0.0).unwrap(),
        },
        rotation: Quaternion {
            x: FiniteF32::new(0.0).unwrap(),
            y: FiniteF32::new(0.0).unwrap(),
            z: FiniteF32::new(0.0).unwrap(),
            w: FiniteF32::new(1.0).unwrap(),
        },
        scale: PositiveVec3 {
            x: PositiveF32::new(1.0).unwrap(),
            y: PositiveF32::new(1.0).unwrap(),
            z: PositiveF32::new(1.0).unwrap(),
        },
    })
}

fn record_three() -> RecordedWorld {
    let mut recorded = RecordedWorld::default();
    let first = id(1);
    let second = id(2);
    recorded
        .apply_patch(
            &patch(
                recorded.world().revision(),
                1,
                vec![create(first), create(second)],
            ),
            FrameId::new(10).unwrap(),
        )
        .unwrap();
    recorded
        .apply_patch(
            &patch(
                recorded.world().revision(),
                2,
                vec![SceneOperation::Reparent(ReparentEntity {
                    entity_id: second,
                    parent_id: Some(first),
                })],
            ),
            FrameId::new(20).unwrap(),
        )
        .unwrap();
    recorded
        .apply_patch(
            &patch(
                recorded.world().revision(),
                3,
                vec![SceneOperation::SetComponent(SetComponent {
                    entity_id: first,
                    component: local(4.0),
                })],
            ),
            FrameId::new(30).unwrap(),
        )
        .unwrap();
    recorded
}

fn frame_ranges(encoded: &[u8]) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut offset = 8;
    while offset < encoded.len() {
        let body_len = u32::from_be_bytes(encoded[offset..offset + 4].try_into().unwrap()) as usize;
        let end = offset + 4 + body_len;
        ranges.push((offset, end));
        offset = end;
    }
    ranges
}

#[test]
fn accepted_events_round_trip_and_replay_to_the_exact_hash() {
    let mut recorded = record_three();
    let original_hash = recorded.world().logical_hash().unwrap();
    let original_snapshot = recorded.world().snapshot().unwrap();
    let verification = recorded.log().verify().unwrap();
    assert_eq!(verification.entry_count(), 3);
    assert_eq!(verification.final_scene_hash(), Some(original_hash));

    let repeated = recorded.log().entries()[2].patch().clone();
    let receipt = recorded
        .apply_patch(&repeated, FrameId::new(99).unwrap())
        .unwrap();
    assert_eq!(
        receipt.status,
        cogniform_protocol::ApplyStatus::IdempotentReplay
    );
    assert_eq!(recorded.log().len(), 3);

    let bytes = recorded.log().to_bytes();
    let loaded = ReplayLog::load_prefix(
        &bytes,
        ReplayConfig::default(),
        &WorldConfig::default().runtime_limits,
    );
    assert!(loaded.tail_error().is_none());
    assert_eq!(loaded.log().to_bytes(), bytes);
    let replayed = loaded.log().replay(WorldConfig::default()).unwrap();
    assert_eq!(replayed.logical_hash().unwrap(), original_hash);
    assert_eq!(replayed.snapshot().unwrap(), original_snapshot);
}

#[test]
fn restored_recorded_world_retains_evidence_and_appends_the_next_entry() {
    let original = record_three();
    let original_bytes = original.log().to_bytes();
    let original_hash = original.world().logical_hash().unwrap();
    let original_snapshot = original.world().snapshot().unwrap();
    let loaded = ReplayLog::load_prefix(
        &original_bytes,
        ReplayConfig::default(),
        &WorldConfig::default().runtime_limits,
    );
    let (log, tail) = loaded.into_parts();
    assert_eq!(tail, None);

    let mut restored = RecordedWorld::restore(WorldConfig::default(), log).unwrap();
    assert_eq!(restored.log().to_bytes(), original_bytes);
    assert_eq!(restored.world().logical_hash().unwrap(), original_hash);
    assert_eq!(restored.world().snapshot().unwrap(), original_snapshot);

    restored
        .apply_patch(
            &patch(restored.world().revision(), 4, vec![create(id(3))]),
            FrameId::new(40).unwrap(),
        )
        .unwrap();
    let continued_bytes = restored.log().to_bytes();
    assert!(continued_bytes.starts_with(&original_bytes));
    let verification = restored.log().verify().unwrap();
    assert_eq!(verification.entry_count(), 4);
    assert_eq!(verification.final_revision(), SceneRevision::new(4));
    assert_eq!(
        verification.final_scene_hash(),
        Some(restored.world().logical_hash().unwrap())
    );
}

#[test]
fn repeated_recording_is_byte_exact() {
    assert_eq!(
        record_three().log().to_bytes(),
        record_three().log().to_bytes()
    );
}

#[test]
fn exact_revision_prefixes_are_complete_bounded_replay_streams() {
    let recorded = record_three();
    let full_bytes = recorded.log().to_bytes();
    let empty_hash = AuthoritativeWorld::default().logical_hash().unwrap();

    for value in 0..=3 {
        let revision = SceneRevision::new(value);
        let first = recorded.log().to_bytes_through_revision(revision).unwrap();
        let second = recorded.log().to_bytes_through_revision(revision).unwrap();
        assert_eq!(first, second);
        assert!(full_bytes.starts_with(&first));

        let loaded = ReplayLog::load_prefix(
            &first,
            ReplayConfig::default(),
            &WorldConfig::default().runtime_limits,
        );
        assert_eq!(loaded.tail_error(), None);
        assert_eq!(loaded.log().verify().unwrap().final_revision(), revision);
        let replayed = loaded.log().replay(WorldConfig::default()).unwrap();
        let expected_hash = if value == 0 {
            empty_hash
        } else {
            recorded.log().entries()[usize::try_from(value - 1).unwrap()].new_scene_hash()
        };
        assert_eq!(replayed.logical_hash().unwrap(), expected_hash);
    }

    assert_eq!(
        recorded
            .log()
            .to_bytes_through_revision(SceneRevision::new(3))
            .unwrap(),
        full_bytes
    );
    let error = recorded
        .log()
        .to_bytes_through_revision(SceneRevision::new(4))
        .unwrap_err();
    assert_eq!(error.requested(), SceneRevision::new(4));
    assert_eq!(error.latest(), SceneRevision::new(3));
    assert_eq!(recorded.log().to_bytes(), full_bytes);
}

#[test]
fn truncated_or_modified_tail_preserves_only_the_verified_prefix() {
    let encoded = record_three().log().to_bytes();
    let ranges = frame_ranges(&encoded);

    let truncated = &encoded[..ranges[2].1 - 7];
    let loaded = ReplayLog::load_prefix(
        truncated,
        ReplayConfig::default(),
        &WorldConfig::default().runtime_limits,
    );
    assert_eq!(loaded.log().len(), 2);
    let tail = loaded.tail_error().unwrap();
    assert_eq!(tail.verified_entries(), 2);
    assert_eq!(tail.kind(), &ReplayTailErrorKind::Truncated);

    let mut modified = encoded.clone();
    modified[ranges[1].1 - 1] ^= 0x01;
    let loaded = ReplayLog::load_prefix(
        &modified,
        ReplayConfig::default(),
        &WorldConfig::default().runtime_limits,
    );
    assert_eq!(loaded.log().len(), 1);
    assert_eq!(
        loaded.tail_error().unwrap().kind(),
        &ReplayTailErrorKind::Integrity(ReplayIntegrityErrorKind::EntryHashMismatch)
    );
}

#[test]
fn every_single_byte_corruption_stops_before_the_unverified_entry() {
    let encoded = record_three().log().to_bytes();
    for index in 0..encoded.len() {
        let mut modified = encoded.clone();
        modified[index] ^= 0x01;
        let loaded = ReplayLog::load_prefix(
            &modified,
            ReplayConfig::default(),
            &WorldConfig::default().runtime_limits,
        );
        assert!(
            loaded.tail_error().is_some(),
            "byte {index} was not rejected"
        );
        assert!(loaded.log().len() < 3, "byte {index} entered the log");
        let verification = loaded.log().verify().unwrap();
        assert_eq!(
            verification.entry_count(),
            u32::try_from(loaded.log().len()).unwrap()
        );
        let recovered = loaded.log().replay(WorldConfig::default()).unwrap();
        assert_eq!(
            recovered.revision().get(),
            u64::try_from(loaded.log().len()).unwrap()
        );
    }
}

#[test]
fn missing_and_reordered_entries_break_the_chain() {
    let encoded = record_three().log().to_bytes();
    let ranges = frame_ranges(&encoded);

    let mut missing = encoded[..ranges[0].1].to_vec();
    missing.extend_from_slice(&encoded[ranges[2].0..ranges[2].1]);
    let loaded = ReplayLog::load_prefix(
        &missing,
        ReplayConfig::default(),
        &WorldConfig::default().runtime_limits,
    );
    assert_eq!(loaded.log().len(), 1);
    assert_eq!(
        loaded.tail_error().unwrap().kind(),
        &ReplayTailErrorKind::Integrity(ReplayIntegrityErrorKind::SequenceGap)
    );

    let mut reordered = encoded[..8].to_vec();
    reordered.extend_from_slice(&encoded[ranges[1].0..ranges[1].1]);
    reordered.extend_from_slice(&encoded[ranges[0].0..ranges[0].1]);
    let loaded = ReplayLog::load_prefix(
        &reordered,
        ReplayConfig::default(),
        &WorldConfig::default().runtime_limits,
    );
    assert!(loaded.log().is_empty());
    assert_eq!(
        loaded.tail_error().unwrap().kind(),
        &ReplayTailErrorKind::Integrity(ReplayIntegrityErrorKind::SequenceGap)
    );
}

#[test]
fn replay_capacity_rejects_before_world_mutation() {
    let replay_config = ReplayConfig {
        max_entries: NonZeroU32::new(1).unwrap(),
        ..ReplayConfig::default()
    };
    let mut recorded = RecordedWorld::new(WorldConfig::default(), replay_config).unwrap();
    recorded
        .apply_patch(
            &patch(recorded.world().revision(), 10, vec![create(id(10))]),
            FrameId::new(1).unwrap(),
        )
        .unwrap();
    let before_revision = recorded.world().revision();
    let before_hash = recorded.world().logical_hash().unwrap();
    let rejected = patch(recorded.world().revision(), 11, vec![create(id(11))]);
    assert!(matches!(
        recorded.apply_patch(&rejected, FrameId::new(2).unwrap()),
        Err(RecordedApplyError::EntryCapacityExceeded { limit: 1 })
    ));
    assert_eq!(recorded.world().revision(), before_revision);
    assert_eq!(recorded.world().logical_hash().unwrap(), before_hash);
    assert!(!recorded.world().contains(id(11)));
}

#[test]
fn total_log_bound_must_fit_the_mandatory_header() {
    let invalid = ReplayConfig {
        max_log_bytes: NonZeroU32::new(7).unwrap(),
        ..ReplayConfig::default()
    };
    let error = RecordedWorld::new(WorldConfig::default(), invalid)
        .err()
        .unwrap();
    assert_eq!(error.actual(), 7);
    assert_eq!(error.minimum(), 8);

    let loaded = ReplayLog::load_prefix(
        b"CNFRPL1\n",
        invalid,
        &WorldConfig::default().runtime_limits,
    );
    assert!(matches!(
        loaded.tail_error().unwrap().kind(),
        ReplayTailErrorKind::InvalidConfig(_)
    ));
}
