use crate::hotkey::next::{Hotkey, HotkeyContext, HotkeyExt};
use crate::identifier::Identifier;
use bevy::ecs::system::SystemState;
use bevy::{prelude::*, utils::HashMap};
use phichain_game::GameSet;

pub type ActionIdentifier = Identifier;

pub struct RegisteredAction {
    system: Box<dyn System<In = (), Out = ()>>,
    enable_hotkey: bool,
}

impl RegisteredAction {
    pub fn run(&mut self, world: &mut World) {
        self.system.run((), world);
    }
}

#[derive(Resource, Deref, Default)]
pub struct ActionRegistry(HashMap<ActionIdentifier, RegisteredAction>);

impl ActionRegistry {
    pub fn run_action(&mut self, world: &mut World, id: impl Into<ActionIdentifier>) {
        let id = id.into();
        if let Some(action) = self.0.get_mut(&id) {
            action.run(world);
        } else {
            error!("Failed to find action with id {}", id);
        }
    }
}

pub struct ActionPlugin;

impl Plugin for ActionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActionRegistry>()
            .register_action(
                "phichain.debug",
                || {
                    println!("Hello from Phichain!");
                },
                None,
            )
            .add_systems(Update, handle_action_hotkey_system.in_set(GameSet));
    }
}

pub trait ActionRegistrationExt {
    fn register_action<M1>(
        &mut self,
        id: impl Into<ActionIdentifier>,
        system: impl IntoSystem<(), (), M1>,
        hotkey: Option<Hotkey>,
    ) -> &mut Self;
}

impl ActionRegistrationExt for App {
    fn register_action<M1>(
        &mut self,
        id: impl Into<ActionIdentifier>,
        system: impl IntoSystem<(), (), M1>,
        hotkey: Option<Hotkey>,
    ) -> &mut Self {
        let id = id.into();

        self.world
            .resource_scope(|world, mut registry: Mut<ActionRegistry>| {
                registry.0.insert(
                    id.clone(),
                    RegisteredAction {
                        system: Box::new({
                            let mut sys = IntoSystem::into_system(system);
                            sys.initialize(world);
                            sys
                        }),
                        enable_hotkey: hotkey.is_some(),
                    },
                )
            });

        if let Some(hotkey) = hotkey {
            self.add_hotkey(id, hotkey);
        }

        self
    }
}

fn handle_action_hotkey_system(world: &mut World) {
    let mut state: SystemState<(HotkeyContext, Res<ActionRegistry>)> = SystemState::new(world);
    let (hotkey, registry) = state.get_mut(world);
    let mut actions_to_run = vec![];

    for (id, _) in registry.0.iter().filter(|(_, action)| action.enable_hotkey) {
        if hotkey.just_pressed(id.clone()) {
            actions_to_run.push(id.clone());
        }
    }

    if !actions_to_run.is_empty() {
        world.resource_scope(|world, mut registry: Mut<ActionRegistry>| {
            for action in actions_to_run {
                registry.run_action(world, action);
            }
        });
    }
}
