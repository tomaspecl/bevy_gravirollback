pub mod rollback_config_plugin;

pub mod schedule_plugin;
pub mod existence_plugin;
pub mod systems;
pub mod for_user;

use bevy::prelude::*;
use bevy::ecs::component::ComponentId;
use bevy::ecs::world::DeferredWorld;
use bevy::utils::HashMap;

pub mod prelude {
    pub use crate::*;
    pub use crate::systems::*;
    pub use crate::for_user::*;
    pub use crate::schedule_plugin::*;
    pub use crate::existence_plugin::*;
    pub use crate::rollback_config_plugin::*;
}

// *****************************
//TODO: check comments and names and update them
// *****************************

//TODO: do serialization for most structs, or perhaps Reflect is sufficient?

// A snapshot contains the state of rollback entities and the player inputs of a single game frame

// The rollback schedule gets new player inputs and combines them with the current state by running the Update.
// The Update generates a new state which will be saved.
// If some old saved state or input gets updated then the rollback schedule will load that past snapshot
// and rerun it up to the present state.


//TODO: ideas that could make it better
//bevy::ecs::system::SystemState can be used for caching access to certain data through &mut World -> speed up
//let components = world.inspect_entity(entity);     //can be used to get Components of an entity

// future ideal for how configurable and generic this plugin should be
// imagine a multiplayer game with multiple simulated multiplayer minigames with simulated networking delay
// you should be able to apply the outer rollback schedule to the rollback of the minigames
// so you should be able to rollback the rollbacks
// or said differently: have a simulation of some rollback enabled system (not in the sense of ECS system), and the simulation itself has rollback

//TODO: component change detection should work correctly even when changing frames -> maybe use marker Component to signal change?

//TODO: figure out how to remove the Default requirement for #[reflect(Resource)]

//TODO: use a flag to choose if the size is growable, size is fixed by default
#[derive(Component, Resource, Reflect, Deref, DerefMut, Clone)]
#[reflect(Resource)]
pub struct Rollback<T, const LEN: usize>(pub [T; LEN]);   //this version has fixed size, it should be faster as there is no pointer dereference, question is if it matters or was it just premature optimization
impl<T: Default, const LEN: usize> Default for Rollback<T, LEN> {
    fn default() -> Self {
        Self(std::array::from_fn(|_| T::default()))     //this had to be done manualy because the #[derive(Default)] macro could not handle it with big LEN
    }
}

// use std::collections::VecDeque;
// struct Rollback<T>(VecDeque<T>);  //this version is dynamicaly growable during runtime


#[derive(Component, Reflect, Clone, Copy, Hash, PartialEq, Eq, Debug)]
#[component(on_insert=rollback_id_on_insert,on_replace=rollback_id_on_replace)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
pub struct RollbackID(pub u64); //this should be user defined

//automatically update the RollbackMap when a new RollbackID component is added or removed
fn rollback_id_on_insert(mut world: DeferredWorld, entity: Entity, _component_id: ComponentId) {
    let id = world.entity(entity).get::<RollbackID>().unwrap().clone();
    world.resource_mut::<RollbackMap>().insert(entity,id);
}
fn rollback_id_on_replace(mut world: DeferredWorld, entity: Entity, _component_id: ComponentId) {
    world.resource_mut::<RollbackMap>().remove(entity);
}

#[derive(Resource, Reflect, Default)]
#[reflect(Resource)]
pub struct RollbackMap(pub HashMap<RollbackID, Entity>, pub HashMap<Entity, RollbackID>);   //TODO: this should be generic over RollbackID
impl RollbackMap {
    pub fn remove(&mut self, e: Entity) {
        self.print();
        println!("Removing from RollbackMap {e:?}");
        if let Some(r) = self.1.get(&e) {
            if let Some(e2) = self.0.get(r) {
                if *e2!=e {
                    panic!("Entity {e:?} RollbackID {r:?} had mapping to Entity {e2:?}");
                }
                self.0.remove(r);
                self.1.remove(&e);
            }else{
                panic!("Entity {e:?} did not have a mapping from RollbackID {r:?}");
            }
        }else{
            warn!("Entity {e:?} did not have a mapping to RollbackID");
        }
        self.print();
    }
    pub fn insert(&mut self, e: Entity, r: RollbackID) {
        self.print();
        println!("Adding to RollbackMap {e:?} {r:?}");
        match (self.0.get(&r), self.1.get(&e)) {
            (None, None) => {
                self.0.insert(r, e);
                self.1.insert(e, r);
            },
            (Some(e2), Some(r2)) => {
                //if *e2!=e || *r2!=r {
                    panic!("Can not add rollback mapping for Entity {e:?} RollbackID {r:?} because {e2:?}, {r2:?} already existed");
                //}
            },
            //TODO: these are misleading, e2 has a RollbackID with it, so its not incomplete but (e2, RollbackID for e2) entry already exists
            (Some(e2), None) => panic!("RollbackMap was incomplete Entity {e2:?} (insert with {e:?}, {r:?})"),
            (None, Some(r2)) => panic!("RollbackMap was incomplete RollbackID {r2:?} (insert with {e:?}, {r:?})"),
        }
        self.print();
    }
    pub fn print(&self) {
        //*
        let mut tmp = self.1.clone();
        let x = self.0.iter().map(|(r,e)| (r.clone(),e.clone(),tmp.remove(e))).collect::<Vec<(RollbackID, Entity, Option<RollbackID>)>>();
        println!("\tRollbackMap:");
        for (r,e,r2) in x {
            if let Some(r2) = r2 {
                if r==r2 {
                    println!("\tRollbackID {r:?} <=> Entity {e:?}");
                }else{
                    println!("\tRollbackID {r:?} -> Entity {e:?} -> {r2:?}");
                }
            }else{
                println!("\tRollbackID {r:?} -> Entity {e:?}");
            }
        }
        for (e,r) in tmp {
            println!("\tRollbackID {r:?} <- Entity {e:?}");
        }
        // */
    }
}

/// The currently loaded frame, or the frame that should be restored
#[derive(Resource, Reflect, Default, Clone, Copy, Debug)]
#[reflect(Resource)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
pub struct Frame(pub u64);

// /// The index into storage of currently loaded frame, or the frame that should be restored.
// /// Used for rollback restore/save systems to avoid recalculating this value inside them for no reason.
// #[derive(Resource, Reflect Default, Clone, Copy, Debug)]
// #[reflect(Resource)]
// pub struct Index<const LEN: usize>(pub usize);

/// The last frame in storage
#[derive(Resource, Reflect, Default, Clone, Copy, Debug)]
#[reflect(Resource)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
pub struct LastFrame(pub u64);

pub fn index<const LEN: usize>(frame: u64) -> usize {
    (frame%LEN as u64) as usize
}

#[derive(Reflect, Default, Clone, Copy, Debug)]
pub struct Modified(pub bool);

pub struct RollbackPlugin<const LEN: usize>;

//TODO: use *_system for names of systems probably?
impl<const LEN: usize> Plugin for RollbackPlugin<LEN> {
    fn build(&self, app: &mut App) {
        app
        .init_resource::<Frame>()
        //.init_resource::<Index>()
        .init_resource::<LastFrame>()
        .init_resource::<Rollback<Frame, LEN>>()
        .init_resource::<Rollback<Modified, LEN>>()

        .init_resource::<RollbackMap>();
    }
}
