#[cfg(old)]
pub mod old;
pub mod rollback_config_plugin;

pub mod schedule_plugin;
pub mod existence_plugin;
pub mod systems;
pub mod for_user;

use bevy::prelude::*;
use bevy::ecs::component::ComponentId;
use bevy::ecs::world::DeferredWorld;
use bevy::utils::HashMap;

// *****************************
//TODO: check comments and names and update them
// *****************************

// A snapshot contains the state of rollback entities and the player inputs of a single game frame

// The rollback schedule gets new player inputs and combines them with the current state by running the Update.
// The Update generates a new state which will be saved.
// If some old saved state or input gets updated then the rollback schedule will load that past snapshot
// and rerun it up to the present state.


//TODO: ideas that could make it better
//bevy::ecs::system::SystemState can be used for caching access to certain data through &mut World -> speed up
//let components = world.inspect_entity(entity);     //can be used to get Components of an entity

//TODO: component change detection should work correctly even when changing frames -> maybe use marker Component to signal change?


pub const SNAPSHOTS_LEN: usize = 128;    //TODO: how to make this changable by the user of this library? maybe extern?
//maybe I can wrap the whole lib in a macro and let the user instanciate it with their specified SNAPSHOTS_LEN? this would be crazy
//maybe some configuration/compilation flag can be passed in Cargo.toml ?

//TODO: figure out how to remove the Default requirement for #[reflect(Resource)]

//TODO: use a flag to choose if the size is growable, size is fixed by default
#[derive(Component, Resource, Reflect, Clone)]
#[reflect(Resource)]
pub struct Rollback<T: Default /* ideally remove this bound, it is mainly for Reflect */>(pub [T; SNAPSHOTS_LEN]);   //this version has fixed size, it should be faster as there is no pointer dereference, question is if it matters or was it just premature optimization
impl<T: Default> Default for Rollback<T> {
    fn default() -> Self {
        Self(std::array::from_fn(|_| T::default()))     //this had to be done manualy because the #[derive(Default)] macro could not handle it with big SNAPSHOTS_LEN
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


#[derive(Resource, Reflect, Default)]
#[reflect(Resource)]
pub struct SnapshotInfo {
    /// The last frame in storage
    pub last: u64,
    /// The currently loaded frame, or the frame that should be restored
    pub current: u64,
    pub snapshots: Vec<Snapshot>,   //TODO: this should probably not belong to here
}
impl SnapshotInfo {
    /// Compute the index in storage of the specified frame
    pub fn index(&self, frame: u64) -> usize { (frame%SNAPSHOTS_LEN as u64) as usize }  //TODO: &self is not needed now but in the future SNAPSHOTS_LEN could be part of the struct
    /// Compute the index in storage of the current frame
    pub fn current_index(&self) -> usize { self.index(self.current) }
}

#[derive(Reflect, Clone)]
pub struct Snapshot {
    pub frame: u64,
    pub modified: bool,
}

pub struct RollbackPlugin;

//TODO: use *_system for names of systems probably?
impl Plugin for RollbackPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(SnapshotInfo {
            last: 0,
            current: 0,
            snapshots: vec![Snapshot { frame: 0, modified: false };SNAPSHOTS_LEN],
        });

        app.insert_resource(RollbackMap(HashMap::new(), HashMap::new()));
    }
}
