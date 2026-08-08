use core::num::NonZeroU32;
use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{
    CodecError, ComponentKind, ComponentValue, DiagnosticCode, FrameId, IdempotencyKey,
    ObservationId, PatchBudget, RuntimeLimits, SceneRevision, SceneText, SchemaVersion,
    StableEntityId, TransactionId, ValidationError,
    codec::{self, Validate},
};

const OBSERVATION_REQUEST_LOGICAL_BYTES: u64 = 44;

/// Base-revision behavior for a scene patch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictPolicy {
    /// Reject the patch unless its base revision is the current revision.
    RequireExactBase,
}

/// Admission behavior for bounded command or feedback queues.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum DeliverySemantic {
    /// Ordered durable work that fails admission when capacity is unavailable.
    MustApply,
    /// Replaceable work that supersedes an uncommitted item with the same key.
    LatestWins {
        /// Non-empty key defining the supersession scope.
        supersession_key: SceneText,
    },
    /// Work that may be dropped under configured pressure.
    BestEffort,
}

impl DeliverySemantic {
    pub(crate) fn text_bytes(&self) -> usize {
        match self {
            Self::LatestWins { supersession_key } => supersession_key.len_bytes(),
            Self::MustApply | Self::BestEffort => 0,
        }
    }

    pub(crate) fn logical_size_bytes(&self) -> u64 {
        match self {
            Self::LatestWins { supersession_key } => 1_u64
                .saturating_add(4)
                .saturating_add(u64::try_from(supersession_key.len_bytes()).unwrap_or(u64::MAX)),
            Self::MustApply | Self::BestEffort => 1,
        }
    }
}

/// Capacity and delivery semantics for one bounded queue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueueConfig {
    /// Maximum number of admitted entries.
    pub capacity: NonZeroU32,
    /// Behavior applied when the queue is under pressure.
    pub delivery: DeliverySemantic,
}

impl QueueConfig {
    /// Validates this queue configuration against runtime limits.
    pub fn validate_with_limits(&self, limits: &RuntimeLimits) -> Result<(), ValidationError> {
        Validate::validate(self, limits)
    }
}

impl Validate for QueueConfig {
    fn validate(&self, limits: &RuntimeLimits) -> Result<(), ValidationError> {
        if self.capacity.get() > limits.max_queue_capacity.get() {
            return Err(ValidationError::new(
                DiagnosticCode::QueueCapacityExceeded,
                "queue.capacity",
            ));
        }
        if u64::try_from(self.delivery.text_bytes()).unwrap_or(u64::MAX)
            > limits.max_text_bytes.get()
        {
            return Err(ValidationError::new(
                DiagnosticCode::TextLimitExceeded,
                "queue.delivery.supersession_key",
            ));
        }
        if 4_u64.saturating_add(self.delivery.logical_size_bytes()) > limits.max_decoded_bytes.get()
        {
            return Err(ValidationError::new(
                DiagnosticCode::DecodedSizeLimitExceeded,
                "queue",
            ));
        }
        Ok(())
    }
}

/// Payload for an entity-creation operation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateEntity {
    /// Stable identity assigned to the new entity.
    pub entity_id: StableEntityId,
    /// Initial component values in stable message order.
    pub components: Vec<ComponentValue>,
}

/// Payload for an entity-deletion operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeleteEntity {
    /// Stable identity of the entity to delete.
    pub entity_id: StableEntityId,
}

/// Payload for a component-set operation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetComponent {
    /// Stable identity of the entity to update.
    pub entity_id: StableEntityId,
    /// Complete typed component replacement.
    pub component: ComponentValue,
}

/// Payload for a component-removal operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoveComponent {
    /// Stable identity of the entity to update.
    pub entity_id: StableEntityId,
    /// Stable component kind to remove.
    pub component: ComponentKind,
}

/// Payload for a hierarchy-parent update.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReparentEntity {
    /// Stable identity of the child entity.
    pub entity_id: StableEntityId,
    /// New parent, or `None` to detach the entity from its parent.
    pub parent_id: Option<StableEntityId>,
}

/// Ordered version-one scene operation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "operation",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum SceneOperation {
    /// Create one stable entity.
    Create(CreateEntity),
    /// Delete one stable entity.
    Delete(DeleteEntity),
    /// Replace one component value.
    SetComponent(SetComponent),
    /// Remove one component value.
    RemoveComponent(RemoveComponent),
    /// Change or clear one parent relation.
    Reparent(ReparentEntity),
}

impl SceneOperation {
    fn validate(&self) -> Result<(), ValidationError> {
        match self {
            Self::Create(create) => {
                let mut kinds = BTreeSet::new();
                for component in &create.components {
                    component.validate()?;
                    if !kinds.insert(component.kind()) {
                        return Err(ValidationError::new(
                            DiagnosticCode::DuplicateComponent,
                            "operations[].create.components",
                        ));
                    }
                }
            }
            Self::SetComponent(set) => set.component.validate()?,
            Self::Reparent(reparent) if reparent.parent_id == Some(reparent.entity_id) => {
                return Err(ValidationError::new(
                    DiagnosticCode::SelfParent,
                    "operations[].reparent.parent_id",
                ));
            }
            Self::Delete(_) | Self::RemoveComponent(_) | Self::Reparent(_) => {}
        }
        Ok(())
    }

    fn component_count(&self) -> usize {
        match self {
            Self::Create(create) => create.components.len(),
            Self::SetComponent(_) => 1,
            Self::Delete(_) | Self::RemoveComponent(_) | Self::Reparent(_) => 0,
        }
    }

    fn text_bytes(&self) -> u64 {
        match self {
            Self::Create(create) => create.components.iter().fold(0_u64, |total, component| {
                total.saturating_add(u64::try_from(component.text_bytes()).unwrap_or(u64::MAX))
            }),
            Self::SetComponent(set) => {
                u64::try_from(set.component.text_bytes()).unwrap_or(u64::MAX)
            }
            Self::Delete(_) | Self::RemoveComponent(_) | Self::Reparent(_) => 0,
        }
    }

    fn logical_size_bytes(&self) -> u64 {
        const TAG_BYTES: u64 = 1;
        const ENTITY_ID_BYTES: u64 = 16;
        match self {
            Self::Create(create) => create
                .components
                .iter()
                .fold(TAG_BYTES + ENTITY_ID_BYTES + 4, |total, component| {
                    total.saturating_add(component.logical_size_bytes())
                }),
            Self::Delete(_) => TAG_BYTES + ENTITY_ID_BYTES,
            Self::SetComponent(set) => TAG_BYTES
                .saturating_add(ENTITY_ID_BYTES)
                .saturating_add(set.component.logical_size_bytes()),
            Self::RemoveComponent(_) => TAG_BYTES + ENTITY_ID_BYTES + 1,
            Self::Reparent(reparent) => {
                TAG_BYTES + ENTITY_ID_BYTES + 1 + u64::from(reparent.parent_id.is_some()) * 16
            }
        }
    }
}

/// Atomic, ordered scene mutation request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenePatch {
    /// Public schema version.
    pub schema_version: SchemaVersion,
    /// Transaction identity echoed by the receipt.
    pub transaction_id: TransactionId,
    /// Mandatory key for idempotent result lookup.
    pub idempotency_key: IdempotencyKey,
    /// Authoritative revision against which the request was prepared.
    pub base_revision: SceneRevision,
    /// Behavior when the base revision is stale.
    pub conflict_policy: ConflictPolicy,
    /// Admission behavior for the command queue.
    pub delivery: DeliverySemantic,
    /// Sender-declared resource budget.
    pub declared_budget: PatchBudget,
    /// Operations retained and applied in this exact order.
    pub operations: Vec<SceneOperation>,
}

impl ScenePatch {
    /// Validates schema invariants and runtime resource limits.
    pub fn validate_with_limits(&self, limits: &RuntimeLimits) -> Result<(), ValidationError> {
        Validate::validate(self, limits)
    }

    /// Encodes one validated canonical JSON line with a trailing LF.
    pub fn to_canonical_json(&self, limits: &RuntimeLimits) -> Result<Vec<u8>, CodecError> {
        codec::encode(self, limits)
    }

    /// Decodes bounded JSON and validates the resulting patch.
    pub fn from_json(encoded: &[u8], limits: &RuntimeLimits) -> Result<Self, CodecError> {
        codec::decode(encoded, limits)
    }
}

impl Validate for ScenePatch {
    fn validate(&self, limits: &RuntimeLimits) -> Result<(), ValidationError> {
        validate_schema(self.schema_version)?;

        let operation_count = u32::try_from(self.operations.len()).unwrap_or(u32::MAX);
        if operation_count == 0 {
            return Err(ValidationError::new(
                DiagnosticCode::EmptyPatch,
                "operations",
            ));
        }
        if operation_count > self.declared_budget.max_operations.get()
            || operation_count > limits.max_operations.get()
            || self.declared_budget.max_operations.get() > limits.max_operations.get()
        {
            return Err(ValidationError::new(
                DiagnosticCode::OperationLimitExceeded,
                "operations",
            ));
        }

        let mut component_count = 0_u64;
        let mut text_bytes = u64::try_from(self.delivery.text_bytes()).unwrap_or(u64::MAX);
        for operation in &self.operations {
            operation.validate()?;

            if let SceneOperation::Create(create) = operation {
                let per_entity = u32::try_from(create.components.len()).unwrap_or(u32::MAX);
                if per_entity > limits.max_components_per_entity.get() {
                    return Err(ValidationError::new(
                        DiagnosticCode::ComponentLimitExceeded,
                        "operations[].create.components",
                    ));
                }
            }

            component_count = component_count
                .saturating_add(u64::try_from(operation.component_count()).unwrap_or(u64::MAX));
            text_bytes = text_bytes.saturating_add(operation.text_bytes());
        }

        if component_count > u64::from(self.declared_budget.max_components.get())
            || component_count > u64::from(limits.max_components.get())
            || self.declared_budget.max_components.get() > limits.max_components.get()
        {
            return Err(ValidationError::new(
                DiagnosticCode::ComponentLimitExceeded,
                "operations[].components",
            ));
        }
        if text_bytes > self.declared_budget.max_text_bytes.get()
            || text_bytes > limits.max_text_bytes.get()
            || self.declared_budget.max_text_bytes.get() > limits.max_text_bytes.get()
        {
            return Err(ValidationError::new(
                DiagnosticCode::TextLimitExceeded,
                "operations[].text",
            ));
        }

        let logical_size = self.logical_size_bytes();
        if logical_size > self.declared_budget.max_decoded_bytes.get()
            || logical_size > limits.max_decoded_bytes.get()
            || self.declared_budget.max_decoded_bytes.get() > limits.max_decoded_bytes.get()
        {
            return Err(ValidationError::new(
                DiagnosticCode::DecodedSizeLimitExceeded,
                "declared_budget.max_decoded_bytes",
            ));
        }

        Ok(())
    }
}

impl ScenePatch {
    /// Returns aggregate scene-text bytes carried by this patch.
    #[must_use]
    pub fn text_bytes(&self) -> u64 {
        self.operations.iter().fold(
            u64::try_from(self.delivery.text_bytes()).unwrap_or(u64::MAX),
            |total, operation| total.saturating_add(operation.text_bytes()),
        )
    }

    /// Returns deterministic logical decoded bytes carried by this patch.
    #[must_use]
    pub fn logical_size_bytes(&self) -> u64 {
        self.operations.iter().fold(
            2_u64
                .saturating_add(16)
                .saturating_add(16)
                .saturating_add(8)
                .saturating_add(1)
                .saturating_add(self.delivery.logical_size_bytes())
                .saturating_add(24)
                .saturating_add(4),
            |total, operation| total.saturating_add(operation.logical_size_bytes()),
        )
    }
}

/// Outcome represented by an accepted apply receipt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApplyStatus {
    /// The patch was accepted and committed.
    Applied,
    /// The original accepted receipt was returned for a repeated idempotency key.
    IdempotentReplay,
}

/// Apply-stage timings carried as observational metadata, never hashed state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApplyTiming {
    /// Bounded-decode duration in microseconds.
    pub decode_micros: u64,
    /// Preflight-validation duration in microseconds.
    pub validate_micros: u64,
    /// Atomic-commit duration in microseconds.
    pub commit_micros: u64,
}

/// Severity assigned to one receipt diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverity {
    /// Informational result detail.
    Info,
    /// Non-fatal condition that callers should inspect.
    Warning,
    /// Error detail associated with a rejected operation or request.
    Error,
}

/// Structured bounded diagnostic attached to a result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Diagnostic {
    /// Stable machine-readable code.
    pub code: DiagnosticCode,
    /// Diagnostic severity.
    pub severity: DiagnosticSeverity,
    /// Bounded human-readable detail.
    pub message: SceneText,
    /// Zero-based operation index when the diagnostic is operation-specific.
    pub operation_index: Option<u32>,
    /// Stable entity identity when the diagnostic is entity-specific.
    pub entity_id: Option<StableEntityId>,
}

/// Receipt for one accepted patch or an idempotent replay of that result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApplyReceipt {
    /// Public schema version.
    pub schema_version: SchemaVersion,
    /// Transaction identity from the request.
    pub transaction_id: TransactionId,
    /// Idempotency key from the request.
    pub idempotency_key: IdempotencyKey,
    /// Whether this is the original apply result or its replay.
    pub status: ApplyStatus,
    /// Revision before the accepted commit.
    pub previous_revision: SceneRevision,
    /// Revision after the accepted commit.
    pub new_revision: SceneRevision,
    /// Number of operations accepted atomically.
    pub operation_count: NonZeroU32,
    /// Structured result diagnostics.
    pub diagnostics: Vec<Diagnostic>,
    /// Non-deterministic apply-stage timing metadata.
    pub timing: ApplyTiming,
    /// Earliest frame expected to include the committed revision.
    pub estimated_visible_frame: FrameId,
}

impl ApplyReceipt {
    /// Validates receipt causality and configured collection limits.
    pub fn validate_with_limits(&self, limits: &RuntimeLimits) -> Result<(), ValidationError> {
        Validate::validate(self, limits)
    }

    /// Encodes one validated canonical JSON line with a trailing LF.
    pub fn to_canonical_json(&self, limits: &RuntimeLimits) -> Result<Vec<u8>, CodecError> {
        codec::encode(self, limits)
    }

    /// Decodes bounded JSON and validates the resulting receipt.
    pub fn from_json(encoded: &[u8], limits: &RuntimeLimits) -> Result<Self, CodecError> {
        codec::decode(encoded, limits)
    }
}

impl Validate for ApplyReceipt {
    fn validate(&self, limits: &RuntimeLimits) -> Result<(), ValidationError> {
        validate_schema(self.schema_version)?;
        if self.previous_revision.checked_next().ok() != Some(self.new_revision) {
            return Err(ValidationError::new(
                DiagnosticCode::InvalidReceiptRevision,
                "new_revision",
            ));
        }
        if self.operation_count.get() > limits.max_operations.get() {
            return Err(ValidationError::new(
                DiagnosticCode::InvalidReceiptOperationCount,
                "operation_count",
            ));
        }
        if u32::try_from(self.diagnostics.len()).unwrap_or(u32::MAX) > limits.max_diagnostics.get()
        {
            return Err(ValidationError::new(
                DiagnosticCode::DiagnosticLimitExceeded,
                "diagnostics",
            ));
        }
        let text_bytes = self.diagnostics.iter().fold(0_u64, |total, diagnostic| {
            total.saturating_add(u64::try_from(diagnostic.message.len_bytes()).unwrap_or(u64::MAX))
        });
        if text_bytes > limits.max_text_bytes.get() {
            return Err(ValidationError::new(
                DiagnosticCode::TextLimitExceeded,
                "diagnostics[].message",
            ));
        }
        if self.logical_size_bytes() > limits.max_decoded_bytes.get() {
            return Err(ValidationError::new(
                DiagnosticCode::DecodedSizeLimitExceeded,
                "receipt",
            ));
        }
        Ok(())
    }
}

impl ApplyReceipt {
    fn logical_size_bytes(&self) -> u64 {
        self.diagnostics.iter().fold(
            2_u64 + 16 + 16 + 1 + 8 + 8 + 4 + 4 + 24 + 8,
            |total, diagnostic| {
                total
                    .saturating_add(2)
                    .saturating_add(1)
                    .saturating_add(4)
                    .saturating_add(
                        u64::try_from(diagnostic.message.len_bytes()).unwrap_or(u64::MAX),
                    )
                    .saturating_add(1 + u64::from(diagnostic.operation_index.is_some()) * 4)
                    .saturating_add(1 + u64::from(diagnostic.entity_id.is_some()) * 16)
            },
        )
    }
}

/// Kind of machine-readable observation represented by metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationKind {
    /// Linear or encoded color image.
    Color,
    /// Depth image.
    Depth,
    /// Quantized geometric surface-normal image.
    Normal,
    /// Exact stable-entity identity image.
    EntityId,
    /// Structured entity visibility summary without a pixel payload.
    Visibility,
}

/// Requested or delivered observation quality tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationQuality {
    /// Lowest-cost diagnostic output.
    Low,
    /// Balanced output.
    Medium,
    /// Highest configured fidelity.
    High,
}

/// One bounded exact-revision request for a machine-readable observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationRequest {
    /// Public schema version.
    pub schema_version: SchemaVersion,
    /// Public identity of the requested result.
    pub observation_id: ObservationId,
    /// Exact authoritative scene revision the caller expects to observe.
    pub scene_revision: SceneRevision,
    /// Stable extracted camera identity.
    pub camera_id: StableEntityId,
    /// Requested payload kind.
    pub kind: ObservationKind,
    /// Requested quality tier.
    pub quality: ObservationQuality,
}

impl ObservationRequest {
    /// Validates schema and logical-size invariants.
    pub fn validate_with_limits(&self, limits: &RuntimeLimits) -> Result<(), ValidationError> {
        Validate::validate(self, limits)
    }

    /// Encodes one validated canonical JSON line with a trailing LF.
    pub fn to_canonical_json(&self, limits: &RuntimeLimits) -> Result<Vec<u8>, CodecError> {
        codec::encode(self, limits)
    }

    /// Decodes bounded JSON and validates the resulting request.
    pub fn from_json(encoded: &[u8], limits: &RuntimeLimits) -> Result<Self, CodecError> {
        codec::decode(encoded, limits)
    }
}

impl Validate for ObservationRequest {
    fn validate(&self, limits: &RuntimeLimits) -> Result<(), ValidationError> {
        validate_schema(self.schema_version)?;
        if OBSERVATION_REQUEST_LOGICAL_BYTES > limits.max_decoded_bytes.get() {
            return Err(ValidationError::new(
                DiagnosticCode::DecodedSizeLimitExceeded,
                "observation_request",
            ));
        }
        Ok(())
    }
}

/// Non-zero image dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageDimensions {
    /// Image width in pixels.
    pub width: NonZeroU32,
    /// Image height in pixels.
    pub height: NonZeroU32,
}

impl ImageDimensions {
    /// Returns the checked pixel count.
    #[must_use]
    pub fn pixel_count(self) -> u64 {
        u64::from(self.width.get()) * u64::from(self.height.get())
    }
}

/// Explicit staleness measurement relative to the latest known world revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationStaleness {
    /// Latest authoritative revision known when the envelope was produced.
    pub latest_known_revision: SceneRevision,
    /// Exact difference between latest known and observed scene revisions.
    pub revisions_behind: u64,
}

/// Causal metadata for one asynchronous observation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationMetadata {
    /// Public schema version.
    pub schema_version: SchemaVersion,
    /// Identity of this observation request/result.
    pub observation_id: ObservationId,
    /// Fully extracted scene revision rendered by the source frame.
    pub scene_revision: SceneRevision,
    /// Source frame identity.
    pub frame_id: FrameId,
    /// Camera used to produce the observation.
    pub camera_id: StableEntityId,
    /// Observation kind.
    pub kind: ObservationKind,
    /// Pixel dimensions for image observations; absent for visibility summaries.
    pub dimensions: Option<ImageDimensions>,
    /// Requested or delivered quality tier.
    pub quality: ObservationQuality,
    /// Unix timestamp in microseconds when production completed.
    pub observed_at_unix_micros: u64,
    /// End-to-end production latency in microseconds.
    pub production_latency_micros: u64,
    /// Explicit revision staleness metadata.
    pub staleness: ObservationStaleness,
}

impl ObservationMetadata {
    /// Validates causal and pixel-budget invariants.
    pub fn validate_with_limits(&self, limits: &RuntimeLimits) -> Result<(), ValidationError> {
        Validate::validate(self, limits)
    }

    /// Encodes one validated canonical JSON line with a trailing LF.
    pub fn to_canonical_json(&self, limits: &RuntimeLimits) -> Result<Vec<u8>, CodecError> {
        codec::encode(self, limits)
    }

    /// Decodes bounded JSON and validates the resulting metadata.
    pub fn from_json(encoded: &[u8], limits: &RuntimeLimits) -> Result<Self, CodecError> {
        codec::decode(encoded, limits)
    }
}

impl Validate for ObservationMetadata {
    fn validate(&self, limits: &RuntimeLimits) -> Result<(), ValidationError> {
        validate_schema(self.schema_version)?;

        let expects_dimensions = self.kind != ObservationKind::Visibility;
        if expects_dimensions != self.dimensions.is_some() {
            return Err(ValidationError::new(
                DiagnosticCode::InvalidObservationDimensions,
                "dimensions",
            ));
        }
        if let Some(dimensions) = self.dimensions
            && (dimensions.width.get() > limits.max_observation_width.get()
                || dimensions.height.get() > limits.max_observation_height.get()
                || dimensions.pixel_count() > limits.max_observation_pixels.get())
        {
            return Err(ValidationError::new(
                DiagnosticCode::ObservationPixelLimitExceeded,
                "dimensions",
            ));
        }

        let latest = self.staleness.latest_known_revision.get();
        let observed = self.scene_revision.get();
        if latest.checked_sub(observed) != Some(self.staleness.revisions_behind) {
            return Err(ValidationError::new(
                DiagnosticCode::InvalidObservationStaleness,
                "staleness",
            ));
        }
        if self.logical_size_bytes() > limits.max_decoded_bytes.get() {
            return Err(ValidationError::new(
                DiagnosticCode::DecodedSizeLimitExceeded,
                "observation",
            ));
        }
        Ok(())
    }
}

impl ObservationMetadata {
    fn logical_size_bytes(&self) -> u64 {
        2_u64
            .saturating_add(16)
            .saturating_add(8)
            .saturating_add(8)
            .saturating_add(16)
            .saturating_add(1)
            .saturating_add(1 + u64::from(self.dimensions.is_some()) * 8)
            .saturating_add(1)
            .saturating_add(8)
            .saturating_add(8)
            .saturating_add(16)
    }
}

fn validate_schema(schema_version: SchemaVersion) -> Result<(), ValidationError> {
    if schema_version != SchemaVersion::V1 {
        return Err(ValidationError::new(
            DiagnosticCode::UnsupportedSchema,
            "schema_version",
        ));
    }
    Ok(())
}
