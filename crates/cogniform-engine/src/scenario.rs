use core::num::NonZeroU32;
use std::time::{Duration, Instant};

use cogniform_protocol::{
    ApplyReceipt, CameraComponent, ColorRgb, ColorRgba, ComponentValue, ConflictPolicy,
    CreateEntity, DeliverySemantic, FiniteF32, FrameId, IdempotencyKey, ImageDimensions,
    LightComponent, LightKind, LocalTransform, MaterialComponent, NameComponent, NonNegativeF32,
    ObservationId, ObservationKind, ObservationQuality, PatchBudget, PositiveF32, PositiveVec3,
    PrimitiveComponent, PrimitiveShape, Quaternion, SceneOperation, ScenePatch, SceneQuery,
    SceneRevision, SceneText, SchemaVersion, SetComponent, StableEntityId, TransactionId, UnitF32,
    Vec3,
};
use cogniform_replay::ReplayVerification;
use cogniform_world::LogicalSceneHash;

use crate::{
    GatewayAdmission, GatewayResponse, LocalService, LocalServiceError, Observation,
    ObservationPayload, ObservationRequest,
};

const ROOM_ID: u128 = 0x100;
const TABLE_ID: u128 = 0x200;
const LIGHT_ID: u128 = 0x300;
const CAMERA_ID: u128 = 0x400;
const INITIAL_TRANSACTION_ID: u128 = 0x1_000;
const INITIAL_IDEMPOTENCY_KEY: u128 = 0x1_001;
const UPDATE_TRANSACTION_ID: u128 = 0x2_000;
const UPDATE_IDEMPOTENCY_KEY: u128 = 0x2_001;
const COLOR_OBSERVATION_ID: u128 = 0x3_001;
const ENTITY_ID_OBSERVATION_ID: u128 = 0x3_002;
const VISIBILITY_OBSERVATION_ID: u128 = 0x3_003;
const EXPECTED_TABLE_COLOR: [u8; 4] = [55, 30, 11, 255];
const MAX_OBSERVATION_TIMEOUT: Duration = Duration::from_mins(1);

struct ScenarioReplayEvidence {
    verification: ReplayVerification,
    logical_hash: LogicalSceneHash,
    replayed_logical_hash: LogicalSceneHash,
    encoded_bytes: u64,
}

/// Bounded execution settings for the canonical unattended scenario.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CanonicalScenarioConfig {
    /// Maximum time to poll for each independently admitted observation.
    pub observation_timeout: Duration,
}

impl Default for CanonicalScenarioConfig {
    fn default() -> Self {
        Self {
            observation_timeout: Duration::from_secs(10),
        }
    }
}

/// Stable causal fields retained from one successful scenario observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObservationEvidence {
    /// Request identity.
    pub observation_id: ObservationId,
    /// Rendered authoritative revision.
    pub scene_revision: SceneRevision,
    /// Source frame identity.
    pub frame_id: FrameId,
    /// Camera used for the frame.
    pub camera_id: StableEntityId,
    /// Delivered observation kind.
    pub kind: ObservationKind,
    /// Image dimensions, absent for visibility summaries.
    pub dimensions: Option<ImageDimensions>,
    /// Difference from the latest authoritative revision at delivery.
    pub revisions_behind: u64,
}

impl ObservationEvidence {
    fn from_observation(observation: &Observation) -> Self {
        let metadata = observation.metadata();
        Self {
            observation_id: metadata.observation_id,
            scene_revision: metadata.scene_revision,
            frame_id: metadata.frame_id,
            camera_id: metadata.camera_id,
            kind: metadata.kind,
            dimensions: metadata.dimensions,
            revisions_behind: metadata.staleness.revisions_behind,
        }
    }
}

/// Compact proof that the canonical local MVP scenario completed successfully.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalScenarioReport {
    /// Stable room identity used by the scenario.
    pub room_id: StableEntityId,
    /// Stable table identity used by the scenario.
    pub table_id: StableEntityId,
    /// Stable light identity used by the scenario.
    pub light_id: StableEntityId,
    /// Stable camera identity used by the scenario.
    pub camera_id: StableEntityId,
    /// Receipt for atomically creating all four entities.
    pub initial_receipt: ApplyReceipt,
    /// Receipt for atomically moving and restyling the table.
    pub update_receipt: ApplyReceipt,
    /// Entities returned by the exact-revision logical query.
    pub queried_entities: u32,
    /// Causal proof for the color image.
    pub color: ObservationEvidence,
    /// Causal proof for the entity-ID image.
    pub entity_id: ObservationEvidence,
    /// Causal proof for the visibility summary.
    pub visibility: ObservationEvidence,
    /// Center color pixel from the updated table.
    pub center_color: [u8; 4],
    /// Center stable entity identity from the same revision.
    pub center_entity_id: StableEntityId,
    /// Number of frame pixels attributed to the table.
    pub table_visible_pixels: u64,
    /// Canonical hash of the live authoritative world.
    pub logical_hash: LogicalSceneHash,
    /// Canonical hash after replaying accepted events into a fresh world.
    pub replayed_logical_hash: LogicalSceneHash,
    /// Integrity verification of the accepted-event log.
    pub replay: ReplayVerification,
    /// Exact encoded replay stream size.
    pub replay_bytes: u64,
}

/// Failure to complete or prove one step of the canonical scenario.
#[derive(Debug)]
pub enum CanonicalScenarioError {
    /// The local service rejected or failed an operation.
    Service(LocalServiceError),
    /// The supplied service was not an empty initial world.
    NonEmptyService {
        /// Revision found before the scenario began.
        revision: SceneRevision,
    },
    /// The observation timeout was zero or exceeded the scenario bound.
    InvalidObservationTimeout,
    /// One admitted observation did not complete within its configured bound.
    ObservationTimedOut {
        /// Observation kind that timed out.
        kind: ObservationKind,
    },
    /// A service result or observation violated the canonical scenario contract.
    Contract {
        /// Stable, non-sensitive description of the failed invariant.
        reason: &'static str,
    },
    /// Live and replayed authoritative state produced different canonical hashes.
    ReplayHashMismatch {
        /// Hash of the live authoritative world.
        live: LogicalSceneHash,
        /// Hash produced by replaying into a fresh world.
        replayed: LogicalSceneHash,
    },
}

impl std::fmt::Display for CanonicalScenarioError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Service(error) => {
                write!(formatter, "canonical scenario service failure: {error}")
            }
            Self::NonEmptyService { revision } => write!(
                formatter,
                "canonical scenario requires revision 0, found revision {}",
                revision.get()
            ),
            Self::InvalidObservationTimeout => {
                formatter.write_str(
                    "canonical scenario observation timeout must be between 1 nanosecond and 60 seconds",
                )
            }
            Self::ObservationTimedOut { kind } => {
                write!(
                    formatter,
                    "canonical scenario {kind:?} observation timed out"
                )
            }
            Self::Contract { reason } => {
                write!(formatter, "canonical scenario contract failed: {reason}")
            }
            Self::ReplayHashMismatch { live, replayed } => write!(
                formatter,
                "canonical scenario replay hash mismatch: live {live}, replayed {replayed}"
            ),
        }
    }
}

impl std::error::Error for CanonicalScenarioError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Service(error) => Some(error),
            Self::NonEmptyService { .. }
            | Self::InvalidObservationTimeout
            | Self::ObservationTimedOut { .. }
            | Self::Contract { .. }
            | Self::ReplayHashMismatch { .. } => None,
        }
    }
}

impl From<LocalServiceError> for CanonicalScenarioError {
    fn from(value: LocalServiceError) -> Self {
        Self::Service(value)
    }
}

/// Runs the documented room, table, light, camera, observation, and replay flow.
///
/// The caller supplies a newly initialized local service. The function admits
/// bounded work, polls each observation independently, and returns only after
/// the live and replayed logical hashes match.
pub fn run_canonical_scenario(
    service: &mut LocalService,
    config: CanonicalScenarioConfig,
) -> Result<CanonicalScenarioReport, CanonicalScenarioError> {
    validate_scenario_config(config)?;
    let starting_revision = service.status().scene_revision;
    if starting_revision != SceneRevision::INITIAL {
        return Err(CanonicalScenarioError::NonEmptyService {
            revision: starting_revision,
        });
    }
    let room_id = entity_id(ROOM_ID);
    let table_id = entity_id(TABLE_ID);
    let light_id = entity_id(LIGHT_ID);
    let camera_id = entity_id(CAMERA_ID);

    let initial_receipt = submit_and_process_patch(
        service,
        initial_patch(room_id, table_id, light_id, camera_id),
    )?;
    if initial_receipt.previous_revision != SceneRevision::INITIAL
        || initial_receipt.new_revision != SceneRevision::new(1)
    {
        return contract("initial patch did not commit revision 1");
    }

    let update_receipt = submit_and_process_patch(
        service,
        update_patch(initial_receipt.new_revision, table_id),
    )?;
    let final_revision = update_receipt.new_revision;
    if update_receipt.previous_revision != initial_receipt.new_revision
        || final_revision != SceneRevision::new(2)
        || update_receipt.operation_count.get() != 2
    {
        return contract("table move and restyle did not commit atomically at revision 2");
    }

    let queried_entities = verify_query(service, final_revision, table_id)?;
    let color_observation = request_and_wait(
        service,
        observation_request(COLOR_OBSERVATION_ID, camera_id, ObservationKind::Color),
        final_revision,
        config.observation_timeout,
    )?;
    let center_color = center_color(&color_observation)?;
    if center_color
        .into_iter()
        .zip(EXPECTED_TABLE_COLOR)
        .any(|(actual, expected)| actual.abs_diff(expected) > 2)
    {
        return contract("color observation center pixel violates canonical point lighting");
    }

    let entity_observation = request_and_wait(
        service,
        observation_request(
            ENTITY_ID_OBSERVATION_ID,
            camera_id,
            ObservationKind::EntityId,
        ),
        final_revision,
        config.observation_timeout,
    )?;
    let center_entity_id = center_entity(&entity_observation)?;
    if center_entity_id != table_id {
        return contract("entity-ID observation center pixel is not the table");
    }

    let visibility_observation = request_and_wait(
        service,
        observation_request(
            VISIBILITY_OBSERVATION_ID,
            camera_id,
            ObservationKind::Visibility,
        ),
        final_revision,
        config.observation_timeout,
    )?;
    let table_visible_pixels = table_visibility(&visibility_observation, table_id)?;

    let replay = verify_replay_evidence(service, final_revision)?;

    Ok(CanonicalScenarioReport {
        room_id,
        table_id,
        light_id,
        camera_id,
        initial_receipt,
        update_receipt,
        queried_entities,
        color: ObservationEvidence::from_observation(&color_observation),
        entity_id: ObservationEvidence::from_observation(&entity_observation),
        visibility: ObservationEvidence::from_observation(&visibility_observation),
        center_color,
        center_entity_id,
        table_visible_pixels,
        logical_hash: replay.logical_hash,
        replayed_logical_hash: replay.replayed_logical_hash,
        replay: replay.verification,
        replay_bytes: replay.encoded_bytes,
    })
}

fn verify_replay_evidence(
    service: &LocalService,
    final_revision: SceneRevision,
) -> Result<ScenarioReplayEvidence, CanonicalScenarioError> {
    let verification = service.verify_replay()?;
    let logical_hash = service.logical_hash()?;
    let replayed_logical_hash = service.replayed_logical_hash()?;
    if logical_hash != replayed_logical_hash {
        return Err(CanonicalScenarioError::ReplayHashMismatch {
            live: logical_hash,
            replayed: replayed_logical_hash,
        });
    }
    if verification.entry_count() != 2
        || verification.final_revision() != final_revision
        || verification.final_scene_hash() != Some(logical_hash)
    {
        return contract("replay verification does not describe the final accepted revision");
    }
    Ok(ScenarioReplayEvidence {
        verification,
        logical_hash,
        replayed_logical_hash,
        encoded_bytes: u64::try_from(service.replay_bytes().len()).unwrap_or(u64::MAX),
    })
}

fn submit_and_process_patch(
    service: &mut LocalService,
    patch: ScenePatch,
) -> Result<ApplyReceipt, CanonicalScenarioError> {
    if !matches!(
        service.submit_patch(patch)?,
        GatewayAdmission::Queued { .. }
    ) {
        return contract("new canonical patch was not queued exactly once");
    }
    match service.process_next()? {
        Some(GatewayResponse::PatchApplied { receipt }) => Ok(receipt),
        Some(GatewayResponse::ImaginationProcessed { .. }) => {
            contract("explicit patch produced an imagination response")
        }
        None => contract("queued canonical patch was not available for processing"),
    }
}

fn verify_query(
    service: &LocalService,
    scene_revision: SceneRevision,
    table_id: StableEntityId,
) -> Result<u32, CanonicalScenarioError> {
    let result = service.query(&SceneQuery {
        schema_version: SchemaVersion::V1,
        scene_revision,
        entity_ids: Vec::new(),
        component_kinds: Vec::new(),
        limit: NonZeroU32::new(4).expect("canonical entity limit is non-zero"),
    })?;
    if result.entities.len() != 4 {
        return contract("exact-revision query did not return all four canonical entities");
    }
    if !result.entities.iter().map(|entity| entity.entity_id).eq([
        entity_id(ROOM_ID),
        entity_id(TABLE_ID),
        entity_id(LIGHT_ID),
        entity_id(CAMERA_ID),
    ]) {
        return contract("exact-revision query returned unexpected canonical identities");
    }
    let Some(table) = result
        .entities
        .iter()
        .find(|entity| entity.entity_id == table_id)
    else {
        return contract("exact-revision query did not return the table");
    };
    let expected_transform = table_transform();
    let expected_material = table_material();
    if !table.components.iter().any(
        |component| matches!(component, ComponentValue::LocalTransform(value) if *value == expected_transform),
    ) || !table.components.iter().any(
        |component| matches!(component, ComponentValue::Material(value) if *value == expected_material),
    ) {
        return contract("exact-revision query did not return the moved and restyled table");
    }
    Ok(u32::try_from(result.entities.len()).unwrap_or(u32::MAX))
}

fn request_and_wait(
    service: &mut LocalService,
    request: ObservationRequest,
    scene_revision: SceneRevision,
    timeout: Duration,
) -> Result<Observation, CanonicalScenarioError> {
    service.request_observation(request)?;
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(observation) = service.try_receive_observation()? {
            let metadata = observation.metadata();
            if metadata.observation_id != request.observation_id
                || metadata.kind != request.kind
                || metadata.camera_id != request.camera_id
                || metadata.scene_revision != scene_revision
                || metadata.staleness.revisions_behind != 0
            {
                return contract("observation metadata does not match its request and revision");
            }
            return Ok(observation);
        }
        if Instant::now() >= deadline {
            return Err(CanonicalScenarioError::ObservationTimedOut { kind: request.kind });
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn center_color(observation: &Observation) -> Result<[u8; 4], CanonicalScenarioError> {
    let Some(dimensions) = observation.metadata().dimensions else {
        return contract("color observation has no image dimensions");
    };
    let ObservationPayload::Color(pixels) = observation.payload() else {
        return contract("color request returned a different payload kind");
    };
    center_index(dimensions, pixels.len()).and_then(|index| {
        pixels
            .get(index)
            .copied()
            .ok_or(CanonicalScenarioError::Contract {
                reason: "color observation payload is shorter than its dimensions",
            })
    })
}

fn center_entity(observation: &Observation) -> Result<StableEntityId, CanonicalScenarioError> {
    let Some(dimensions) = observation.metadata().dimensions else {
        return contract("entity-ID observation has no image dimensions");
    };
    let ObservationPayload::EntityId(pixels) = observation.payload() else {
        return contract("entity-ID request returned a different payload kind");
    };
    let index = center_index(dimensions, pixels.len())?;
    pixels
        .get(index)
        .copied()
        .flatten()
        .ok_or(CanonicalScenarioError::Contract {
            reason: "entity-ID observation center pixel is background",
        })
}

fn center_index(
    dimensions: ImageDimensions,
    payload_len: usize,
) -> Result<usize, CanonicalScenarioError> {
    let width = usize::try_from(dimensions.width.get()).unwrap_or(usize::MAX);
    let height = usize::try_from(dimensions.height.get()).unwrap_or(usize::MAX);
    let expected = width.saturating_mul(height);
    if payload_len != expected {
        return contract("observation payload length does not match its dimensions");
    }
    Ok((height / 2).saturating_mul(width).saturating_add(width / 2))
}

fn table_visibility(
    observation: &Observation,
    table_id: StableEntityId,
) -> Result<u64, CanonicalScenarioError> {
    let ObservationPayload::Visibility(entities) = observation.payload() else {
        return contract("visibility request returned a different payload kind");
    };
    let Some(visible_pixels) = entities
        .iter()
        .find(|entity| entity.entity_id == table_id)
        .map(|entity| entity.visible_pixels)
    else {
        return contract("visibility summary does not contain the table");
    };
    if visible_pixels == 0 {
        return contract("visibility summary reports zero table pixels");
    }
    Ok(visible_pixels)
}

fn initial_patch(
    room_id: StableEntityId,
    table_id: StableEntityId,
    light_id: StableEntityId,
    camera_id: StableEntityId,
) -> ScenePatch {
    patch(
        SceneRevision::INITIAL,
        INITIAL_TRANSACTION_ID,
        INITIAL_IDEMPOTENCY_KEY,
        vec![
            SceneOperation::Create(CreateEntity {
                entity_id: room_id,
                components: vec![
                    name("room"),
                    ComponentValue::LocalTransform(transform(0.0, 0.0, -1.0)),
                    ComponentValue::Primitive(PrimitiveComponent {
                        shape: PrimitiveShape::Cuboid,
                        dimensions: positive_vec3(6.0, 4.0, 0.1),
                    }),
                    ComponentValue::Material(material(0.08, 0.10, 0.14)),
                ],
            }),
            SceneOperation::Create(CreateEntity {
                entity_id: table_id,
                components: vec![
                    name("table"),
                    ComponentValue::LocalTransform(transform(0.0, 0.0, 0.0)),
                    ComponentValue::Primitive(PrimitiveComponent {
                        shape: PrimitiveShape::Cuboid,
                        dimensions: positive_vec3(1.5, 0.8, 0.3),
                    }),
                    ComponentValue::Material(material(0.2, 0.6, 0.9)),
                ],
            }),
            SceneOperation::Create(CreateEntity {
                entity_id: light_id,
                components: vec![
                    name("light"),
                    ComponentValue::LocalTransform(transform(0.0, 1.5, 2.0)),
                    ComponentValue::Light(LightComponent {
                        kind: LightKind::Point,
                        color: ColorRgb {
                            r: unit(1.0),
                            g: unit(0.95),
                            b: unit(0.85),
                        },
                        intensity: non_negative(1_000.0),
                    }),
                ],
            }),
            SceneOperation::Create(CreateEntity {
                entity_id: camera_id,
                components: vec![
                    name("camera"),
                    ComponentValue::LocalTransform(transform(0.0, 0.0, 4.0)),
                    ComponentValue::Camera(CameraComponent {
                        vertical_fov_radians: positive(core::f32::consts::FRAC_PI_2),
                        near: positive(0.1),
                        far: positive(100.0),
                    }),
                ],
            }),
        ],
    )
}

fn update_patch(base_revision: SceneRevision, table_id: StableEntityId) -> ScenePatch {
    patch(
        base_revision,
        UPDATE_TRANSACTION_ID,
        UPDATE_IDEMPOTENCY_KEY,
        vec![
            SceneOperation::SetComponent(SetComponent {
                entity_id: table_id,
                component: ComponentValue::LocalTransform(table_transform()),
            }),
            SceneOperation::SetComponent(SetComponent {
                entity_id: table_id,
                component: ComponentValue::Material(table_material()),
            }),
        ],
    )
}

fn patch(
    base_revision: SceneRevision,
    transaction_id: u128,
    idempotency_key: u128,
    operations: Vec<SceneOperation>,
) -> ScenePatch {
    ScenePatch {
        schema_version: SchemaVersion::V1,
        transaction_id: TransactionId::new(transaction_id)
            .expect("canonical transaction identity is non-zero"),
        idempotency_key: IdempotencyKey::new(idempotency_key)
            .expect("canonical idempotency key is non-zero"),
        base_revision,
        conflict_policy: ConflictPolicy::RequireExactBase,
        delivery: DeliverySemantic::MustApply,
        declared_budget: PatchBudget::default(),
        operations,
    }
}

fn observation_request(
    observation_id: u128,
    camera_id: StableEntityId,
    kind: ObservationKind,
) -> ObservationRequest {
    ObservationRequest {
        observation_id: ObservationId::new(observation_id)
            .expect("canonical observation identity is non-zero"),
        camera_id,
        kind,
        quality: ObservationQuality::Low,
    }
}

fn entity_id(value: u128) -> StableEntityId {
    StableEntityId::new(value).expect("canonical entity identity is non-zero")
}

fn name(value: &str) -> ComponentValue {
    ComponentValue::Name(NameComponent {
        value: SceneText::new(value).expect("canonical entity name is valid"),
    })
}

fn transform(x: f32, y: f32, z: f32) -> LocalTransform {
    LocalTransform {
        translation: Vec3 {
            x: finite(x),
            y: finite(y),
            z: finite(z),
        },
        rotation: Quaternion {
            x: finite(0.0),
            y: finite(0.0),
            z: finite(0.0),
            w: finite(1.0),
        },
        scale: positive_vec3(1.0, 1.0, 1.0),
    }
}

fn table_transform() -> LocalTransform {
    transform(0.25, 0.0, 0.0)
}

fn table_material() -> MaterialComponent {
    material(0.9, 0.5, 0.2)
}

fn material(r: f32, g: f32, b: f32) -> MaterialComponent {
    MaterialComponent {
        base_color: ColorRgba {
            r: unit(r),
            g: unit(g),
            b: unit(b),
            a: unit(1.0),
        },
        metallic: unit(0.0),
        roughness: unit(0.5),
    }
}

fn positive_vec3(x: f32, y: f32, z: f32) -> PositiveVec3 {
    PositiveVec3 {
        x: positive(x),
        y: positive(y),
        z: positive(z),
    }
}

fn finite(value: f32) -> FiniteF32 {
    FiniteF32::new(value).expect("canonical finite value is valid")
}

fn positive(value: f32) -> PositiveF32 {
    PositiveF32::new(value).expect("canonical positive value is valid")
}

fn non_negative(value: f32) -> NonNegativeF32 {
    NonNegativeF32::new(value).expect("canonical non-negative value is valid")
}

fn unit(value: f32) -> UnitF32 {
    UnitF32::new(value).expect("canonical unit value is valid")
}

fn contract<T>(reason: &'static str) -> Result<T, CanonicalScenarioError> {
    Err(CanonicalScenarioError::Contract { reason })
}

fn validate_scenario_config(config: CanonicalScenarioConfig) -> Result<(), CanonicalScenarioError> {
    if config.observation_timeout.is_zero() || config.observation_timeout > MAX_OBSERVATION_TIMEOUT
    {
        Err(CanonicalScenarioError::InvalidObservationTimeout)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_patches_are_valid_and_atomic() {
        let room = entity_id(ROOM_ID);
        let table = entity_id(TABLE_ID);
        let light = entity_id(LIGHT_ID);
        let camera = entity_id(CAMERA_ID);
        let limits = cogniform_protocol::RuntimeLimits::default();
        let create = initial_patch(room, table, light, camera);
        assert_eq!(create.operations.len(), 4);
        create.validate_with_limits(&limits).unwrap();
        let update = update_patch(SceneRevision::new(1), table);
        assert_eq!(update.operations.len(), 2);
        update.validate_with_limits(&limits).unwrap();
    }

    #[test]
    fn zero_observation_timeout_is_rejected() {
        assert!(matches!(
            validate_scenario_config(CanonicalScenarioConfig {
                observation_timeout: Duration::ZERO,
            }),
            Err(CanonicalScenarioError::InvalidObservationTimeout)
        ));
    }

    #[test]
    fn excessive_observation_timeout_is_rejected() {
        assert!(matches!(
            validate_scenario_config(CanonicalScenarioConfig {
                observation_timeout: MAX_OBSERVATION_TIMEOUT + Duration::from_nanos(1),
            }),
            Err(CanonicalScenarioError::InvalidObservationTimeout)
        ));
    }
}
