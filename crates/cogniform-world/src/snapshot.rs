use cogniform_protocol::{ComponentKind, ComponentValue, SceneRevision, StableEntityId};

/// Backend-neutral logical state for one entity.
#[derive(Debug, Clone, PartialEq)]
pub struct EntitySnapshot {
    entity_id: StableEntityId,
    components: Vec<ComponentValue>,
}

impl EntitySnapshot {
    pub(crate) fn new(entity_id: StableEntityId, components: Vec<ComponentValue>) -> Self {
        Self {
            entity_id,
            components,
        }
    }

    /// Returns the stable external identity.
    #[must_use]
    pub const fn entity_id(&self) -> StableEntityId {
        self.entity_id
    }

    /// Returns component values in versioned component-kind order.
    #[must_use]
    pub fn components(&self) -> &[ComponentValue] {
        &self.components
    }

    /// Returns a component by its stable kind.
    #[must_use]
    pub fn component(&self, kind: ComponentKind) -> Option<&ComponentValue> {
        self.components
            .binary_search_by_key(&kind, ComponentValue::kind)
            .ok()
            .map(|index| &self.components[index])
    }
}

/// Deterministically ordered logical view of the authoritative world.
#[derive(Debug, Clone, PartialEq)]
pub struct WorldSnapshot {
    revision: SceneRevision,
    entities: Vec<EntitySnapshot>,
}

impl WorldSnapshot {
    pub(crate) fn new(revision: SceneRevision, entities: Vec<EntitySnapshot>) -> Self {
        Self { revision, entities }
    }

    /// Returns the revision represented by this view.
    #[must_use]
    pub const fn revision(&self) -> SceneRevision {
        self.revision
    }

    /// Returns entities in stable-ID order.
    #[must_use]
    pub fn entities(&self) -> &[EntitySnapshot] {
        &self.entities
    }

    /// Returns the number of live logical entities.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entities.len()
    }

    /// Reports whether the logical world is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    /// Returns an entity by stable identity.
    #[must_use]
    pub fn entity(&self, entity_id: StableEntityId) -> Option<&EntitySnapshot> {
        self.entities
            .binary_search_by_key(&entity_id, |entity| entity.entity_id)
            .ok()
            .map(|index| &self.entities[index])
    }
}
