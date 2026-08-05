//! Controlled-adapter proof of the documented unattended MVP scenario.

#![cfg(any(target_os = "windows", target_os = "linux"))]

use cogniform_engine::{
    CanonicalScenarioConfig, LocalService, LocalServiceConfig, run_canonical_scenario,
};
use cogniform_protocol::{ObservationKind, SceneRevision};

#[test]
#[ignore = "requires an approved DX12 or Vulkan conformance adapter"]
fn canonical_scenario_preserves_revision_observation_and_replay_causality() {
    pollster::block_on(async {
        let mut service = LocalService::new(LocalServiceConfig::new(64, 64))
            .await
            .unwrap();
        let report =
            run_canonical_scenario(&mut service, CanonicalScenarioConfig::default()).unwrap();

        assert_eq!(report.initial_receipt.new_revision, SceneRevision::new(1));
        assert_eq!(
            report.update_receipt.previous_revision,
            SceneRevision::new(1)
        );
        assert_eq!(report.update_receipt.new_revision, SceneRevision::new(2));
        assert_eq!(report.queried_entities, 4);
        assert_eq!(report.color.kind, ObservationKind::Color);
        assert_eq!(report.entity_id.kind, ObservationKind::EntityId);
        assert_eq!(report.visibility.kind, ObservationKind::Visibility);
        for evidence in [report.color, report.entity_id, report.visibility] {
            assert_eq!(evidence.scene_revision, SceneRevision::new(2));
            assert_eq!(evidence.camera_id, report.camera_id);
            assert_eq!(evidence.revisions_behind, 0);
        }
        assert!(report.color.frame_id < report.entity_id.frame_id);
        assert!(report.entity_id.frame_id < report.visibility.frame_id);
        assert_eq!(report.center_color, [0, 0, 0, 255]);
        assert_eq!(report.center_entity_id, report.table_id);
        assert!(report.table_visible_pixels > 0);
        assert_eq!(report.logical_hash, report.replayed_logical_hash);
        assert_eq!(report.replay.entry_count(), 2);
        assert_eq!(report.replay.final_revision(), SceneRevision::new(2));
        assert_eq!(report.replay.final_scene_hash(), Some(report.logical_hash));
        assert_eq!(service.status().outstanding_observations, 0);
        assert_eq!(service.status().replay_entries, 2);
        assert_eq!(service.status().command_queue.depth, 0);
    });
}
