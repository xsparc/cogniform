//! Compact hash-addressed asset reference contracts.

use std::str::FromStr;

use cogniform_protocol::{
    AssetMeshComponent, ComponentKind, ComponentValue, ConflictPolicy, ContentHash, CreateEntity,
    DeliverySemantic, IdempotencyKey, PatchBudget, RuntimeLimits, SceneOperation, ScenePatch,
    SceneRevision, SchemaVersion, StableEntityId, TransactionId,
};

#[test]
fn content_hash_uses_exact_lowercase_sha256_encoding() {
    let encoded = "0123456789abcdef".repeat(4);
    let hash = ContentHash::from_str(&encoded).unwrap();
    assert_eq!(hash.to_string(), encoded);
    assert_eq!(ContentHash::from_bytes(*hash.as_bytes()), hash);
    assert!(ContentHash::from_str(&encoded.to_uppercase()).is_err());
    assert!(ContentHash::from_str("00").is_err());
}

#[test]
fn asset_mesh_component_round_trips_without_bulk_asset_bytes() {
    let component = ComponentValue::AssetMesh(AssetMeshComponent {
        content_hash: ContentHash::from_bytes([0x5a; 32]),
        mesh_index: 7,
    });
    let encoded = serde_json::to_vec(&component).unwrap();
    assert!(encoded.len() < 160);
    assert_eq!(
        serde_json::from_slice::<ComponentValue>(&encoded).unwrap(),
        component
    );
    assert_eq!(component.kind(), ComponentKind::AssetMesh);
}

#[test]
fn asset_mesh_patch_has_stable_bounded_canonical_bytes() {
    let limits = RuntimeLimits::default();
    let patch = ScenePatch {
        schema_version: SchemaVersion::V1,
        transaction_id: TransactionId::new(1).unwrap(),
        idempotency_key: IdempotencyKey::new(2).unwrap(),
        base_revision: SceneRevision::INITIAL,
        conflict_policy: ConflictPolicy::RequireExactBase,
        delivery: DeliverySemantic::MustApply,
        declared_budget: PatchBudget::default(),
        operations: vec![SceneOperation::Create(CreateEntity {
            entity_id: StableEntityId::new(3).unwrap(),
            components: vec![ComponentValue::AssetMesh(AssetMeshComponent {
                content_hash: ContentHash::from_bytes([0x5a; 32]),
                mesh_index: 7,
            })],
        })],
    };
    let encoded = patch.to_canonical_json(&limits).unwrap();
    let expected = concat!(
        r#"{"schema_version":1,"transaction_id":"00000000000000000000000000000001","#,
        r#""idempotency_key":"00000000000000000000000000000002","base_revision":0,"#,
        r#""conflict_policy":"require_exact_base","delivery":{"mode":"must_apply"},"#,
        r#""declared_budget":{"max_operations":256,"max_components":2048,"max_text_bytes":16384,"#,
        r#""max_decoded_bytes":262144},"operations":[{"operation":"create","value":{"#,
        r#""entity_id":"00000000000000000000000000000003","components":[{"#,
        r#""component":"asset_mesh","value":{"content_hash":"#,
        r#""5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a","#,
        "\"mesh_index\":7}}]}}]}\n",
    );
    assert_eq!(encoded, expected.as_bytes());
    assert_eq!(ScenePatch::from_json(&encoded, &limits).unwrap(), patch);
}
