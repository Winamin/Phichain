use crate::events::line::{DespawnLineEvent, SpawnLineEvent};
use crate::events::EditorEvent;
use crate::utils::entity::replace_with_empty;
use bevy::prelude::*;
use phichain_chart::serialization::LineWrapper;
use undo::Edit;

#[derive(Debug, Copy, Clone)]
pub struct CreateLine(Option<Entity>);

impl CreateLine {
    pub fn new() -> Self {
        Self(None)
    }
}

impl Edit for CreateLine {
    type Target = World;
    type Output = ();

    fn edit(&mut self, target: &mut Self::Target) -> Self::Output {
        let entity = SpawnLineEvent {
            line: LineWrapper::default(),
            parent: None,
            target: None,
        }
        .run(target);
        self.0 = Some(entity);
    }

    fn undo(&mut self, target: &mut Self::Target) -> Self::Output {
        if let Some(entity) = self.0 {
            target.send_event(DespawnLineEvent(entity));
        }
    }
}

#[derive(Debug, Clone)]
pub struct RemoveLine {
    entity: Entity,
    line: Option<(LineWrapper, Option<Entity>)>,
}

impl RemoveLine {
    pub fn new(entity: Entity) -> Self {
        Self { entity, line: None }
    }
}

impl Edit for RemoveLine {
    type Target = World;
    type Output = ();

    // To persist entity ID for each line, we do not despawn the line entity directly
    // Instead, we retain the entity, despawn all its children and remove all components
    // When undoing, we restore the line entity and its children
    fn edit(&mut self, target: &mut Self::Target) -> Self::Output {
        let parent = target.entity(self.entity).get::<Parent>().map(|x| x.get());
        self.line = Some((LineWrapper::serialize_line(target, self.entity), parent));
        replace_with_empty(target, self.entity);
    }

    fn undo(&mut self, target: &mut Self::Target) -> Self::Output {
        if let Some(ref line) = self.line {
            // restore line entity and its children
            SpawnLineEvent {
                line: line.0.clone(),
                parent: line.1,
                target: Some(self.entity),
            }
            .run(target);
        }
    }
}

/// Move a line as child of another line
#[derive(Debug, Clone)]
pub struct MoveLineAsChild {
    entity: Entity,
    prev_parent: Option<Entity>,
    /// Some = move as child of this line, None = move to root
    target: Option<Entity>,
}

impl MoveLineAsChild {
    pub fn new(entity: Entity, target: Option<Entity>) -> Self {
        Self {
            entity,
            prev_parent: None,
            target,
        }
    }
}

impl Edit for MoveLineAsChild {
    type Target = World;
    type Output = ();

    fn edit(&mut self, world: &mut Self::Target) -> Self::Output {
        self.prev_parent = world.entity(self.entity).get::<Parent>().map(|x| x.get());
        match self.target {
            None => {
                world.entity_mut(self.entity).remove_parent();
            }
            Some(target) => {
                world.entity_mut(self.entity).set_parent(target);
            }
        }
    }

    fn undo(&mut self, target: &mut Self::Target) -> Self::Output {
        target.entity_mut(self.entity).remove_parent();
        if let Some(prev_parent) = self.prev_parent {
            target.entity_mut(self.entity).set_parent(prev_parent);
        }
    }
}
