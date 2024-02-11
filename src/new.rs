pub mod systems;
pub mod for_user;

use bevy::prelude::*;
use bevy::ecs::schedule::ScheduleLabel;
use bevy::ecs::system::BoxedSystem;
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

pub struct RollbackPlugin {
    /// The [`Schedule`] in which rollback processing [`SystemSet`]s will be configured
    pub rollback_processing_schedule: Option<bevy::utils::intern::Interned<dyn ScheduleLabel>>,
}
impl Default for RollbackPlugin {
    fn default() -> Self {
        Self {
            rollback_processing_schedule: Some(Update.intern()), //TODO: this might better be FixedUpdate
            //or at least just for some systems (get local input and run rollback? actually not run rollback as we might need to do rollback after any network input)
        }
    }
}

//TODO: use *_system for names of systems probably?
impl Plugin for RollbackPlugin {
    fn build(&self, app: &mut App) {
        app.init_schedule(RollbackSchedule);    // user can app.add_schedule(custom_schedule)
        if let Some(schedule) = self.rollback_processing_schedule {
            app.configure_sets(schedule,
                (
                    RollbackProcessSet::HandleIO,  //TODO: the local input should be probably gathered at fixed frame intervals, just like the RollbackSchedule
                    RollbackProcessSet::RunRollbackSchedule,
                ).chain()
            )
            .add_systems(schedule,(
                systems::run_rollback_schedule_system.in_set(RollbackProcessSet::RunRollbackSchedule),

                apply_deferred.after(RollbackProcessSet::HandleIO).before(RollbackProcessSet::RunRollbackSchedule),
            ));
        }

        app.configure_sets(RollbackSchedule,(
            (
                RollbackSet::RespawnRemove,
                (RollbackSet::Restore, RollbackSet::RestoreInputs),
                RollbackSet::Update,
                RollbackSet::Save,
                RollbackSet::Despawn,
            ).chain(),
            (
                RollbackSet::RespawnRemove,
                RollbackSet::Restore,
            ).run_if(|snapshot: Option<Res<SnapshotToRestore>>| {
                if let Some(snapshot) = snapshot {
                    !snapshot.input_only
                }else{false}
            }),
            RollbackSet::RestoreInputs.run_if(resource_exists::<SnapshotToRestore>()),
        ))
        .add_systems(RollbackSchedule,(
            (
                systems::respawn,
                systems::restore_remove_nonexistent,
            ).in_set(RollbackSet::RespawnRemove),
            apply_deferred.after(RollbackSet::RespawnRemove).before(RollbackSet::Restore),
            //RollbackSet::Restore
            apply_deferred.after(RollbackSet::Restore).before(RollbackSet::Update),
            //RollbackSet::Update
            apply_deferred.after(RollbackSet::Update).before(RollbackSet::Save),
            systems::save_existence.in_set(RollbackSet::Save),
            apply_deferred.after(RollbackSet::Save).before(RollbackSet::Despawn),
            systems::despawn.in_set(RollbackSet::Despawn),

            systems::update_rollback_map.in_set(RollbackSet::Save),
        ));

        app.insert_resource(SnapshotInfo {
            last: 0,
            current: 0,
            snapshots: vec![Snapshot { frame: 0, modified: false };SNAPSHOTS_LEN],
        }).insert_resource(RollbackMap(HashMap::new())).insert_resource(DespawnedRollbackEntities {
            entities: Vec::new(),
        });
    }
}

#[derive(Component, Clone, Copy, Hash, PartialEq, Eq)]
pub struct RollbackID(pub u64);

pub const SNAPSHOTS_LEN: usize = 128;    //TODO: how to make this changable by the user of this library? maybe extern?

//TODO: use a flag to choose if the size is growable, size is fixed by default
#[derive(Component, Resource, Clone)]
pub struct Rollback<T>(pub [Option<T>;SNAPSHOTS_LEN]);   //this version has fixed size, it should be faster as there is no pointer dereference, question is if it matters or was it just premature optimization
impl<T> Default for Rollback<T> {
    fn default() -> Self {
        Self(std::array::from_fn(|_| None))     //this had to be done manualy because the #[derive(Default)] macro could not handle it with big SNAPSHOTS_LEN
    }
}

// use std::collections::VecDeque;
// struct Rollback<T>(VecDeque<T>);  //this version is dynamicaly growable during runtime

//TODO: split this enum into two enums, one will be pub and second not pub
#[derive(SystemSet, Hash, Debug, PartialEq, Eq, Clone)]
pub enum RollbackSet {
    /// Prune DespawnedRollbackEntities and other updates of the rollback states
    //PrepareStep,    //???

    /// Respawn rollback entities that were deleted and need to be restored and despawn those that should not exist
    RespawnRemove,
    /// Restore the state of rollback entities that are needed to be restored. Restore Resources.
    Restore,
    /// Restore inputs
    RestoreInputs,
    /// Run the game update step
    Update,
    /// Save the state of rollback entities and Resources (not inputs)
    Save,
    /// Despawn rollback entities that were marked for removal and save their state to DespawnedRollbackEntities
    Despawn,
}

#[derive(SystemSet, Hash, Debug, PartialEq, Eq, Clone)]
/// This [`SystemSet`] specifies the high level rollback steps. Those are:
/// 1. Getting all the [`Inputs`] (current or old/delayed) from all "players"
/// 2. Running the [`RollbackSchedule`] on them. This will replay the past history
/// if old inputs arrived and advance the simulation to the current _now_ frame
pub enum RollbackProcessSet {
    /// Get local input (and probably do network IO)
    HandleIO,
    /// Run the [`RollbackSchedule`]. This will contain the [`run_rollback_schedule_system`] by default
    RunRollbackSchedule,
    //server_only_send_new_spawns? // used by the server to send more stuff after rollback schedule, like new entities
}

#[derive(ScheduleLabel, PartialEq, Eq, Hash, Debug, Clone)]
pub struct RollbackSchedule;

#[derive(Resource)]
pub struct DespawnedRollbackEntities {
    entities: Vec<RollbackEntityEntry>,
}

pub struct RollbackEntityEntry {
    id: RollbackID,
    spawn_system: Option<RollbackSpawnMarker>,
    existence: Rollback<Existence>,
    rollback_data: Vec<Box<dyn RollbackStorage>>,
}

pub trait RollbackStorage: Send + Sync {
    fn restore(self: Box<Self>, entity_mut: &mut EntityWorldMut);  //TODO: can this be moved to RollbackRegistry?
}
impl<T: 'static + Clone + Send + Sync> RollbackStorage for Rollback<T> {
    fn restore(self: Box<Self>, entity_mut: &mut EntityWorldMut) {
        entity_mut.insert(*self);
    }
}

#[derive(Resource)]
pub struct RollbackRegistry {
    pub getters: Vec<Getter>,  //TODO: maybe it should be a HashMap<ComponentId, fn(...)->...> or similar for optimization
}

pub type Getter = fn(&mut EntityWorldMut) -> Option<Box<dyn RollbackStorage>>;

#[derive(Component)]
pub struct RollbackSpawnMarker(pub BoxedSystem<(), Entity>);   //TODO: maybe this should be made even more universal
impl RollbackSpawnMarker {
    pub fn new<Marker>(system: impl IntoSystem<(), Entity, Marker>) -> Self {
        RollbackSpawnMarker(Box::new(IntoSystem::into_system(system)))
    }
}

#[derive(Component)]
pub struct RollbackDespawnMarker;

#[derive(Clone, Default)]
pub struct Existence;

#[derive(Resource)]
pub struct RollbackMap(HashMap<RollbackID, Entity>);

#[derive(Resource)]
pub struct SnapshotToSave {
    /// The index in storage where this snapshot should be stored
    index: usize,
    frame: u64,
}

#[derive(Resource)]
pub struct SnapshotToRestore {
    /// The index in storage where this snapshot is located
    index: usize,
    frame: u64,
    /// Only inputs should be restored
    //TODO: this feels hacky, maybe put this somewhere else
    input_only: bool,
}

#[derive(Resource)]
pub struct SnapshotInfo {
    /// The last frame in storage
    pub last: u64,
    /// The currently loaded frame
    pub current: u64,
    pub snapshots: Vec<Snapshot>,   //TODO: this should probably not belong to here
}
impl SnapshotInfo {
    pub fn index(&self, frame: u64) -> usize { (frame%SNAPSHOTS_LEN as u64) as usize }  //TODO: &self is not needed now but in the future SNAPSHOTS_LEN could be part of the struct
}

#[derive(Clone)]
pub struct Snapshot {
    pub frame: u64,
    pub modified: bool,
}